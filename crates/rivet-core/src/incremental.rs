//! Small changes to a saved PDF, written as an *incremental update*: the
//! changed objects and a new cross-reference section are appended to the
//! file, and nothing before them is touched.
//!
//! lopdf can do this too, but it parses every object first (0.7 s for a
//! 1,400-page file). This reads only the cross-reference sections and the few
//! objects asked for, so it costs the same for any file size. lopdf still
//! parses each object's syntax and decodes streams.

use std::collections::{HashMap, HashSet};

use lopdf::{Dictionary, Object, ObjectId, ObjectStream, Stream, StringFormat};

/// Where an object lives, per the cross-reference sections.
#[derive(Clone, Copy)]
enum Entry {
    /// At a byte offset of the file.
    Offset(usize),
    /// Inside an object stream (that stream's object number).
    InStream(u32),
}

/// The cross-reference sections of a PDF and the objects they point to.
pub(crate) struct Pdf<'a> {
    bytes: &'a [u8],
    entries: HashMap<u32, Entry>,
    /// The newest trailer (for a cross-reference stream, its dictionary).
    pub trailer: Dictionary,
    /// Where the newest cross-reference section starts.
    last_xref: usize,
    /// The newest section is a cross-reference stream, so the update must be one too.
    xref_stream: bool,
}

type Result<T> = std::result::Result<T, String>;

impl<'a> Pdf<'a> {
    pub fn read(bytes: &'a [u8]) -> Result<Self> {
        let at = rfind(bytes, b"startxref").ok_or("no startxref")?;
        let last_xref = read_number(&bytes[at + 9..]).ok_or("bad startxref")? as usize;
        let mut pdf = Pdf {
            bytes,
            entries: HashMap::new(),
            trailer: Dictionary::new(),
            last_xref,
            xref_stream: false,
        };
        // Newest section first; older sections only fill in objects not yet seen.
        let mut next = Some(last_xref);
        let mut seen = HashSet::new();
        let mut newest = true;
        while let Some(offset) = next.filter(|o| seen.insert(*o)) {
            let (trailer, is_stream) = pdf.read_section(offset)?;
            // A "hybrid" file also lists objects in a cross-reference stream.
            if let Ok(stm) = trailer.get(b"XRefStm").and_then(Object::as_i64) {
                pdf.read_section(stm as usize)?;
            }
            next = trailer
                .get(b"Prev")
                .and_then(Object::as_i64)
                .ok()
                .map(|p| p as usize);
            if newest {
                pdf.trailer = trailer;
                pdf.xref_stream = is_stream;
                newest = false;
            }
        }
        Ok(pdf)
    }

    /// Reads one cross-reference section: its entries and its trailer.
    fn read_section(&mut self, offset: usize) -> Result<(Dictionary, bool)> {
        let rest = self.bytes.get(offset..).ok_or("xref offset out of range")?;
        let start = skip_space(rest);
        if rest[start..].starts_with(b"xref") {
            let table = &rest[start + 4..];
            let end = find(table, b"trailer").ok_or("no trailer")?;
            let mut words = table[..end]
                .split(|b| b.is_ascii_whitespace())
                .filter(|w| !w.is_empty());
            while let (Some(first), Some(count)) = (words.next(), words.next()) {
                let first = parse_u64(first).ok_or("bad xref")?;
                let count = parse_u64(count).ok_or("bad xref")?;
                for number in first..first + count {
                    let (Some(at), Some(_gen), Some(kind)) =
                        (words.next(), words.next(), words.next())
                    else {
                        return Err("short xref".into());
                    };
                    if kind == b"n" {
                        let at = parse_u64(at).ok_or("bad xref")? as usize;
                        self.entries
                            .entry(number as u32)
                            .or_insert(Entry::Offset(at));
                    }
                }
            }
            let trailer = parse_object(&table[end + 7..])
                .and_then(|o| o.as_dict().ok().cloned())
                .ok_or("bad trailer")?;
            return Ok((trailer, false));
        }
        // A cross-reference stream: rows of (type, field 2, field 3), widths from /W.
        let stream = self.stream_at(offset)?;
        let data = stream
            .decompressed_content()
            .unwrap_or_else(|_| stream.content.clone());
        let widths: Vec<usize> = stream
            .dict
            .get(b"W")
            .and_then(Object::as_array)
            .map_err(|e| e.to_string())?
            .iter()
            .map(|w| w.as_i64().unwrap_or(0) as usize)
            .collect();
        let [w0, w1, w2] = widths[..] else {
            return Err("bad /W".into());
        };
        let size = stream
            .dict
            .get(b"Size")
            .and_then(Object::as_i64)
            .unwrap_or(0);
        let index: Vec<i64> = match stream.dict.get(b"Index").and_then(Object::as_array) {
            Ok(items) => items.iter().filter_map(|i| i.as_i64().ok()).collect(),
            Err(_) => vec![0, size],
        };
        let field = |row: &[u8], from: usize, width: usize| {
            row[from..from + width]
                .iter()
                .fold(0u64, |n, b| (n << 8) | u64::from(*b))
        };
        let row_len = w0 + w1 + w2;
        let mut rows = data.chunks_exact(row_len.max(1));
        for pair in index.as_chunks::<2>().0 {
            for number in pair[0]..pair[0] + pair[1] {
                let Some(row) = rows.next() else { break };
                let kind = if w0 == 0 { 1 } else { field(row, 0, w0) };
                let entry = match kind {
                    1 => Entry::Offset(field(row, w0, w1) as usize),
                    2 => Entry::InStream(field(row, w0, w1) as u32),
                    _ => continue,
                };
                self.entries.entry(number as u32).or_insert(entry);
            }
        }
        Ok((stream.dict, true))
    }

    /// The object `number` (generation 0, or as listed), if the file has it.
    pub fn object(&self, number: u32) -> Option<Object> {
        match *self.entries.get(&number)? {
            Entry::Offset(at) => {
                let body = object_body(self.bytes.get(at..)?)?;
                let object = parse_object(body)?;
                if object.as_dict().is_ok() && stream_keyword(body).is_some() {
                    return self.stream_at(at).ok().map(Object::Stream);
                }
                Some(object)
            }
            Entry::InStream(container) => {
                let Object::Stream(stream) = self.object(container)? else {
                    return None;
                };
                let objects = ObjectStream::new(&stream).ok()?.objects;
                objects.get(&(number, 0)).cloned()
            }
        }
    }

    /// Follows a reference (or returns a direct object as it is).
    pub fn resolve(&self, object: &Object) -> Option<Object> {
        match object {
            Object::Reference((number, _)) => self.object(*number),
            other => Some(other.clone()),
        }
    }

    /// The stream object at byte `at` (its dictionary and raw, still encoded, bytes).
    fn stream_at(&self, at: usize) -> Result<Stream> {
        let body = after_obj_header(&self.bytes[at..]).ok_or("bad object header")?;
        let dict = parse_object(object_body(&self.bytes[at..]).unwrap_or(body))
            .and_then(|o| o.as_dict().ok().cloned())
            .ok_or("bad stream dictionary")?;
        let data_start = stream_keyword(body).ok_or("no stream data")?;
        let length = dict.get(b"Length").map_err(|e| e.to_string())?;
        let length = self
            .resolve(length)
            .and_then(|l| l.as_i64().ok())
            .ok_or("bad stream length")? as usize;
        let content = body
            .get(data_start..data_start + length)
            .ok_or("stream past the end of the file")?;
        Ok(Stream::new(dict, content.to_vec()).with_compression(false))
    }

    /// Appends `objects` (new, or replacing existing ones with the same number
    /// and generation) and a cross-reference section for them, in the format
    /// the file already uses. `new_info` makes one of them the Info dictionary.
    pub fn append(&self, objects: &[(ObjectId, Object)], new_info: Option<ObjectId>) -> Vec<u8> {
        let mut out = self.bytes.to_vec();
        if !out.ends_with(b"\n") {
            out.push(b'\n');
        }
        let mut offsets: Vec<(ObjectId, usize)> = Vec::new();
        for ((number, generation), object) in objects {
            offsets.push(((*number, *generation), out.len()));
            out.extend_from_slice(format!("{number} {generation} obj\n").as_bytes());
            write_object(&mut out, object);
            out.extend_from_slice(b"\nendobj\n");
        }
        let highest = objects.iter().map(|((n, _), _)| *n).max().unwrap_or(0);
        let old_size = self
            .trailer
            .get(b"Size")
            .and_then(Object::as_i64)
            .unwrap_or(0);
        let mut trailer = Dictionary::new();
        for key in [&b"Root"[..], b"Info", b"ID"] {
            if let Ok(value) = self.trailer.get(key) {
                trailer.set(key, value.clone());
            }
        }
        if let Some(id) = new_info {
            trailer.set("Info", Object::Reference(id));
        }
        trailer.set("Prev", Object::Integer(self.last_xref as i64));
        let xref_at = out.len();
        if self.xref_stream {
            // The stream lists itself too, as the next object number.
            let own = (old_size.max(i64::from(highest) + 1)) as u32;
            offsets.push(((own, 0), xref_at));
            offsets.sort_unstable();
            let mut rows = Vec::new();
            let mut index = Vec::new();
            for ((number, generation), at) in &offsets {
                index.extend([Object::Integer(i64::from(*number)), Object::Integer(1)]);
                rows.push(1u8);
                rows.extend_from_slice(&(*at as u32).to_be_bytes());
                rows.extend_from_slice(&generation.to_be_bytes());
            }
            trailer.set("Type", Object::Name(b"XRef".to_vec()));
            trailer.set("Size", Object::Integer(i64::from(own) + 1));
            trailer.set("W", Object::Array(vec![1.into(), 4.into(), 2.into()]));
            trailer.set("Index", Object::Array(index));
            out.extend_from_slice(format!("{own} 0 obj\n").as_bytes());
            write_object(&mut out, &Object::Stream(Stream::new(trailer, rows)));
            out.extend_from_slice(b"\nendobj\n");
        } else {
            offsets.sort_unstable();
            // Starting with the head of the free list (object 0), as Acrobat does,
            // keeps readers that expect it from guessing at the numbering.
            out.extend_from_slice(b"xref\n0 1\n0000000000 65535 f\r\n");
            for ((number, generation), at) in &offsets {
                // Each entry is exactly 20 bytes, as the format requires.
                let entry = format!("{number} 1\n{at:010} {generation:05} n\r\n");
                out.extend_from_slice(entry.as_bytes());
            }
            trailer.set(
                "Size",
                Object::Integer(old_size.max(i64::from(highest) + 1)),
            );
            out.extend_from_slice(b"trailer\n");
            write_object(&mut out, &Object::Dictionary(trailer));
            out.push(b'\n');
        }
        out.extend_from_slice(format!("startxref\n{xref_at}\n%%EOF\n").as_bytes());
        out
    }

    /// An object number not used yet (for a new object).
    pub fn next_number(&self) -> u32 {
        let size = self
            .trailer
            .get(b"Size")
            .and_then(Object::as_i64)
            .unwrap_or(0) as u32;
        size.max(self.entries.keys().max().map_or(0, |n| n + 1))
    }
}

/// Parses one object with lopdf's parser. (lopdf only exposes it through
/// object streams, so the text is wrapped in a one-object stream.)
fn parse_object(text: &[u8]) -> Option<Object> {
    let mut content = b"1 0 ".to_vec();
    content.extend_from_slice(text);
    let mut dict = Dictionary::new();
    dict.set("N", 1);
    dict.set("First", 4);
    let stream = Stream::new(dict, content);
    ObjectStream::new(&stream).ok()?.objects.remove(&(1, 0))
}

/// The bytes after `12 0 obj`.
fn after_obj_header(bytes: &[u8]) -> Option<&[u8]> {
    let head = &bytes[..bytes.len().min(64)];
    let at = find(head, b"obj")?;
    Some(&bytes[at + 3..])
}

/// The bytes between `12 0 obj` and `endobj`, so reading one object never
/// scans or copies the rest of a big file.
fn object_body(bytes: &[u8]) -> Option<&[u8]> {
    let body = after_obj_header(bytes)?;
    Some(find(body, b"endobj").map_or(body, |end| &body[..end]))
}

/// Where a stream's data starts, after the `stream` keyword and its line end.
/// (Only used on dictionaries of stream objects: the keyword follows `>>`.)
fn stream_keyword(body: &[u8]) -> Option<usize> {
    let mut from = 0;
    while let Some(i) = find(&body[from..], b"stream") {
        let at = from + i;
        let before = body[..at].iter().rev().find(|b| !b.is_ascii_whitespace());
        if before == Some(&b'>') {
            let mut start = at + 6;
            if body.get(start) == Some(&b'\r') {
                start += 1;
            }
            if body.get(start) == Some(&b'\n') {
                start += 1;
            }
            return Some(start);
        }
        from = at + 6;
    }
    None
}

/// Writes an object in PDF syntax. Strings are written as hex, which needs no escaping.
fn write_object(out: &mut Vec<u8>, object: &Object) {
    match object {
        Object::Null => out.extend_from_slice(b"null"),
        Object::Boolean(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Object::Integer(i) => out.extend_from_slice(i.to_string().as_bytes()),
        Object::Real(r) => out.extend_from_slice(format!("{r}").as_bytes()),
        Object::Name(name) => write_name(out, name),
        Object::String(bytes, _) => {
            out.push(b'<');
            for b in bytes {
                out.extend_from_slice(format!("{b:02X}").as_bytes());
            }
            out.push(b'>');
        }
        Object::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                write_object(out, item);
            }
            out.push(b']');
        }
        Object::Dictionary(dict) => write_dict(out, dict),
        Object::Stream(stream) => {
            let mut dict = stream.dict.clone();
            dict.set("Length", stream.content.len() as i64);
            write_dict(out, &dict);
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(&stream.content);
            out.extend_from_slice(b"\nendstream");
        }
        Object::Reference((number, generation)) => {
            out.extend_from_slice(format!("{number} {generation} R").as_bytes());
        }
    }
}

fn write_dict(out: &mut Vec<u8>, dict: &Dictionary) {
    out.extend_from_slice(b"<<");
    for (key, value) in dict.iter() {
        write_name(out, key);
        out.push(b' ');
        write_object(out, value);
    }
    out.extend_from_slice(b">>");
}

fn write_name(out: &mut Vec<u8>, name: &[u8]) {
    out.push(b'/');
    for &b in name {
        let delimiter = b"()<>[]{}/%#".contains(&b);
        if (b'!'..=b'~').contains(&b) && !delimiter {
            out.push(b);
        } else {
            out.extend_from_slice(format!("#{b:02X}").as_bytes());
        }
    }
}

pub(crate) fn text(value: &str) -> Object {
    let mut object = lopdf::text_string(value);
    if let Object::String(_, format) = &mut object {
        *format = StringFormat::Hexadecimal;
    }
    object
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

fn skip_space(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len())
}

fn parse_u64(word: &[u8]) -> Option<u64> {
    std::str::from_utf8(word).ok()?.parse().ok()
}

fn read_number(bytes: &[u8]) -> Option<u64> {
    let start = skip_space(bytes);
    let len = bytes[start..]
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count();
    parse_u64(&bytes[start..start + len])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny PDF with a classic cross-reference table and an Info dictionary.
    fn classic() -> Vec<u8> {
        let dict = |entries: Vec<(&str, Object)>| {
            let mut d = Dictionary::new();
            for (k, v) in entries {
                d.set(k, v);
            }
            d
        };
        let mut doc = lopdf::Document::with_version("1.7");
        let pages = doc.new_object_id();
        let media_box = Object::Array(vec![0.into(), 0.into(), 100.into(), 100.into()]);
        let page = doc.add_object(dict(vec![
            ("Type", Object::Name(b"Page".to_vec())),
            ("Parent", pages.into()),
            ("MediaBox", media_box),
        ]));
        let kids = dict(vec![
            ("Type", Object::Name(b"Pages".to_vec())),
            ("Kids", Object::Array(vec![page.into()])),
            ("Count", 1.into()),
        ]);
        doc.objects.insert(pages, Object::Dictionary(kids));
        let catalog = doc.add_object(dict(vec![
            ("Type", Object::Name(b"Catalog".to_vec())),
            ("Pages", pages.into()),
        ]));
        let info = doc.add_object(dict(vec![
            ("Producer", Object::string_literal("Old (producer)")),
            ("Custom", Object::string_literal("kept")),
        ]));
        doc.trailer.set("Root", catalog);
        doc.trailer.set("Info", info);
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    #[test]
    fn reads_and_appends_to_a_classic_table() {
        let bytes = classic();
        let pdf = Pdf::read(&bytes).unwrap();
        let info_id = pdf.trailer.get(b"Info").unwrap().as_reference().unwrap();
        let info_number = info_id.0;
        let mut info = pdf.object(info_number).unwrap().as_dict().unwrap().clone();
        assert_eq!(
            info.get(b"Producer").unwrap().as_str().unwrap(),
            b"Old (producer)"
        );
        info.set("Producer", text("PDFRivet"));
        let out = pdf.append(&[(info_id, Object::Dictionary(info))], None);

        // lopdf (a full, independent reader) sees the change and everything else.
        let doc = lopdf::Document::load_mem(&out).unwrap();
        let info = doc
            .trailer
            .get(b"Info")
            .and_then(|r| doc.dereference(r))
            .unwrap()
            .1;
        let info = info.as_dict().unwrap();
        assert_eq!(
            lopdf::decode_text_string(info.get(b"Producer").unwrap()).unwrap(),
            "PDFRivet"
        );
        assert_eq!(info.get(b"Custom").unwrap().as_str().unwrap(), b"kept");
        assert_eq!(doc.get_pages().len(), 1);

        // And so does this reader, for a second update on top.
        let again = Pdf::read(&out).unwrap();
        assert!(again.object(info_number).is_some());
    }

    #[test]
    fn appends_to_a_cross_reference_stream() {
        let mut doc = lopdf::Document::load_mem(&classic()).unwrap();
        let mut bytes = Vec::new();
        doc.save_modern(&mut bytes).unwrap();
        let pdf = Pdf::read(&bytes).unwrap();
        assert!(pdf.xref_stream);
        let info_id = pdf.trailer.get(b"Info").unwrap().as_reference().unwrap();
        let info_number = info_id.0;
        let mut info = pdf.object(info_number).unwrap().as_dict().unwrap().clone();
        info.set("Producer", text("PDFRivet"));
        let out = pdf.append(&[(info_id, Object::Dictionary(info))], None);

        let doc = lopdf::Document::load_mem(&out).unwrap();
        let info = doc
            .trailer
            .get(b"Info")
            .and_then(|r| doc.dereference(r))
            .unwrap()
            .1;
        let producer = info.as_dict().unwrap().get(b"Producer").unwrap();
        assert_eq!(lopdf::decode_text_string(producer).unwrap(), "PDFRivet");
        assert_eq!(doc.get_pages().len(), 1);
    }

    #[test]
    fn writes_names_and_strings_safely() {
        let mut out = Vec::new();
        write_object(&mut out, &Object::Name(b"A B#".to_vec()));
        out.push(b' ');
        write_object(&mut out, &Object::string_literal("(x)"));
        assert_eq!(out, b"/A#20B#23 <287829>");
    }
}
