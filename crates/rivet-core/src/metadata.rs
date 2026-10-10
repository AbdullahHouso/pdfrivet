//! Document properties: reading them (with PDFium) and changing the
//! title, author, subject and keywords.
//!
//! PDFium can read metadata but not write it, so changes are applied when
//! saving: lopdf appends an *incremental update* to the bytes PDFium saved
//! (only the changed objects are added, nothing else is rewritten). Both places
//! readers look at are updated: the classic Info dictionary and, when present,
//! the XMP metadata stream that Acrobat prefers.

use lopdf::{Object, Stream, StringFormat};

use crate::incremental::{Pdf, text};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{Error, ErrorCode, Result};

/// The editable part of the document's description.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Metadata {
    pub title: String,
    pub author: String,
    pub subject: String,
    pub keywords: String,
}

/// What the document allows (as set by its author when protecting it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct Permissions {
    pub print: bool,
    pub copy: bool,
    pub modify: bool,
    pub fill_forms: bool,
    pub annotate: bool,
}

/// Everything shown in the Document properties dialog.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct DocProperties {
    pub metadata: Metadata,
    pub creator: String,
    pub producer: String,
    /// ISO 8601 dates (e.g. `2026-09-13T20:21:56+03:00`), when the PDF has them.
    pub created: Option<String>,
    pub modified: Option<String>,
    pub pdf_version: String,
    pub page_count: u32,
    pub file_name: String,
    pub folder: String,
    #[ts(type = "number")]
    pub file_size: u64,
    pub tagged: bool,
    pub encrypted: bool,
    pub permissions: Permissions,
    /// Title, author… can be changed (not for password-protected files yet).
    pub can_edit_metadata: bool,
    /// Opening the file needs a password (not only its restrictions).
    pub needs_open_password: bool,
    /// The protection can be changed or removed: the file isn't protected, or
    /// it was opened with (or unlocked by) its owner password.
    pub can_change_protection: bool,
    /// A change to the protection waits for the next save.
    pub protection_pending: bool,
}

/// Converts a PDF date (`D:20260913202156+03'00'`) to ISO 8601.
/// Missing parts default as the PDF specification says (month 01, time 00…).
pub fn pdf_date_to_iso(value: &str) -> Option<String> {
    let s = value.trim().trim_start_matches("D:");
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return None;
    }
    let part = |from: usize, len: usize, default: &str| -> String {
        digits.get(from..from + len).unwrap_or(default).to_owned()
    };
    let (year, month, day) = (part(0, 4, "0000"), part(4, 2, "01"), part(6, 2, "01"));
    let (hour, minute, second) = (part(8, 2, "00"), part(10, 2, "00"), part(12, 2, "00"));
    let rest = &s[digits.len()..];
    let zone = match rest.chars().next() {
        Some('Z') | None => "Z".to_owned(),
        Some(sign @ ('+' | '-')) => {
            let nums: String = rest[1..].chars().filter(char::is_ascii_digit).collect();
            let hh = nums.get(0..2).unwrap_or("00");
            let mm = nums.get(2..4).unwrap_or("00");
            format!("{sign}{hh}:{mm}")
        }
        _ => "Z".to_owned(),
    };
    Some(format!(
        "{year}-{month}-{day}T{hour}:{minute}:{second}{zone}"
    ))
}

/// The current time as (PDF date, ISO 8601), in UTC.
pub(crate) fn now() -> (String, String) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (h, m, s) = (rem / 3600, rem % 3600 / 60, rem % 60);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        format!("D:{year:04}{month:02}{day:02}{h:02}{m:02}{s:02}Z"),
        format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z"),
    )
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Replaces the element `<tag …>…</tag>` (or `<tag …/>`) with `replacement`,
/// or the attribute `tag="…"` with `tag="value"`. Returns false if neither exists.
fn replace_xmp_property(xml: &mut String, tag: &str, replacement: &str, value: &str) -> bool {
    let open = format!("<{tag}");
    let mut search = 0;
    while let Some(found) = xml[search..].find(&open) {
        let start = search + found;
        let after = xml[start + open.len()..].chars().next();
        if matches!(after, Some('>' | ' ' | '/' | '\n' | '\r' | '\t')) {
            let Some(gt) = xml[start..].find('>').map(|i| start + i) else {
                return false;
            };
            let end = if xml[..gt].ends_with('/') {
                gt + 1
            } else {
                let close = format!("</{tag}>");
                match xml[gt..].find(&close) {
                    Some(i) => gt + i + close.len(),
                    None => return false,
                }
            };
            xml.replace_range(start..end, replacement);
            return true;
        }
        search = start + open.len();
    }
    // Attribute form: <rdf:Description pdf:Keywords="…">
    let attr = format!(" {tag}=\"");
    if let Some(found) = xml.find(&attr) {
        let value_start = found + attr.len();
        if let Some(len) = xml[value_start..].find('"') {
            xml.replace_range(value_start..value_start + len, &escape_xml(value));
            return true;
        }
    }
    false
}

/// Updates the producer, dates and (when edited) title, author, subject and
/// keywords already present in an XMP packet. Properties the packet doesn't have are left to the Info dictionary.
pub fn update_xmp(xml: &str, edits: Option<&Metadata>, modified_iso: &str) -> String {
    let mut out = xml.to_owned();
    let producer = format!("<pdf:Producer>{PRODUCER}</pdf:Producer>");
    replace_xmp_property(&mut out, "pdf:Producer", &producer, PRODUCER);
    for tag in ["xmp:ModifyDate", "xmp:MetadataDate"] {
        let element = format!("<{tag}>{modified_iso}</{tag}>");
        replace_xmp_property(&mut out, tag, &element, modified_iso);
    }
    let Some(meta) = edits else {
        return out;
    };
    let alt = |v: &str| {
        format!(
            "<rdf:Alt><rdf:li xml:lang=\"x-default\">{}</rdf:li></rdf:Alt>",
            escape_xml(v)
        )
    };
    let title = format!("<dc:title>{}</dc:title>", alt(&meta.title));
    let creator = format!(
        "<dc:creator><rdf:Seq><rdf:li>{}</rdf:li></rdf:Seq></dc:creator>",
        escape_xml(&meta.author)
    );
    let description = format!("<dc:description>{}</dc:description>", alt(&meta.subject));
    let keywords = format!(
        "<pdf:Keywords>{}</pdf:Keywords>",
        escape_xml(&meta.keywords)
    );
    replace_xmp_property(&mut out, "dc:title", &title, &meta.title);
    replace_xmp_property(&mut out, "dc:creator", &creator, &meta.author);
    replace_xmp_property(&mut out, "dc:description", &description, &meta.subject);
    replace_xmp_property(&mut out, "pdf:Keywords", &keywords, &meta.keywords);
    out
}

/// What saved files name as their Producer: the program that wrote the PDF.
/// (The Creator, the program the document was made in, stays as it was.)
pub const PRODUCER: &str = concat!("PDFRivet ", env!("CARGO_PKG_VERSION"));

/// Marks a saved PDF (`bytes`) as written by PDFRivet now, and writes changed
/// title, author… (`edits`), as an incremental update. Updates the Info
/// dictionary and, when the document has one, the XMP metadata. Returns the
/// new bytes and the modification date written (none if left as it was).
///
/// Password-protected files are left as they are: their metadata would have
/// to be encrypted, which isn't supported yet (changing their title or author
/// is refused before this).
pub(crate) fn stamp(
    bytes: Vec<u8>,
    edits: Option<&Metadata>,
    created: bool,
) -> Result<(Vec<u8>, Option<String>)> {
    let failed = |e: &dyn std::fmt::Display| Error::new(ErrorCode::SaveFailed, e.to_string());
    let pdf = Pdf::read(&bytes).map_err(|e| failed(&e))?;
    if pdf.trailer.has(b"Encrypt") {
        if edits.is_some() {
            return Err(failed(
                &"cannot change the description of a password-protected PDF yet",
            ));
        }
        return Ok((bytes, None));
    }
    let (pdf_now, iso_now) = now();
    let mut changed: Vec<(lopdf::ObjectId, Object)> = Vec::new();

    // Info dictionary: update the existing one, or add a new one.
    let existing = pdf.trailer.get(b"Info").and_then(Object::as_reference).ok();
    let mut info = existing
        .and_then(|(number, _)| pdf.object(number))
        .and_then(|o| o.as_dict().ok().cloned())
        .unwrap_or_default();
    if let Some(meta) = edits {
        for (key, value) in [
            ("Title", &meta.title),
            ("Author", &meta.author),
            ("Subject", &meta.subject),
            ("Keywords", &meta.keywords),
        ] {
            if value.trim().is_empty() {
                info.remove(key.as_bytes());
            } else {
                info.set(key, text(value.trim()));
            }
        }
    }
    info.set("Producer", text(PRODUCER));
    // A document PDFRivet made (merged, from pictures…) was also created in it.
    if created {
        info.set("Creator", text(PRODUCER));
        if !info.has(b"CreationDate") {
            info.set(
                "CreationDate",
                Object::String(pdf_now.clone().into_bytes(), StringFormat::Literal),
            );
        }
    }
    info.set(
        "ModDate",
        Object::String(pdf_now.clone().into_bytes(), StringFormat::Literal),
    );
    let info_id = existing.unwrap_or_else(|| (pdf.next_number(), 0));
    changed.push((info_id, Object::Dictionary(info)));

    // XMP metadata, if the document has it.
    let xmp = pdf
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|root| pdf.resolve(root))
        .and_then(|catalog| {
            catalog
                .as_dict()
                .ok()?
                .get(b"Metadata")
                .ok()?
                .as_reference()
                .ok()
        });
    if let Some(id) = xmp
        && let Some(Object::Stream(stream)) = pdf.object(id.0)
        && let Ok(xml) = String::from_utf8(
            stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone()),
        )
    {
        let mut dict = stream.dict.clone();
        dict.remove(b"Filter");
        dict.remove(b"DecodeParms");
        let updated = update_xmp(&xml, edits, &iso_now).into_bytes();
        changed.push((id, Object::Stream(Stream::new(dict, updated))));
    }

    // A file that had no Info dictionary gets one in the new trailer.
    let new_info = existing.is_none().then_some(info_id);
    Ok((pdf.append(&changed, new_info), Some(pdf_now)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_pdf_dates() {
        assert_eq!(
            pdf_date_to_iso("D:20260913202156+03'00'").as_deref(),
            Some("2026-09-13T20:21:56+03:00")
        );
        assert_eq!(
            pdf_date_to_iso("D:20261007Z").as_deref(),
            Some("2026-10-07T00:00:00Z")
        );
        assert_eq!(
            pdf_date_to_iso("D:2026").as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
        assert_eq!(pdf_date_to_iso("garbage"), None);
    }

    #[test]
    fn current_time_looks_right() {
        let (pdf, iso) = now();
        assert!(pdf.starts_with("D:20") && pdf.ends_with('Z') && pdf.len() == 17);
        assert_eq!(pdf_date_to_iso(&pdf).as_deref(), Some(iso.as_str()));
    }

    #[test]
    fn updates_existing_xmp_properties() {
        let xml = r#"<rdf:Description xmlns:dc="x" pdf:Keywords="old">
<dc:title><rdf:Alt><rdf:li xml:lang="x-default">Old title</rdf:li></rdf:Alt></dc:title>
<dc:creator/>
<xmp:ModifyDate>2020-01-01T00:00:00Z</xmp:ModifyDate>
</rdf:Description>"#;
        let meta = Metadata {
            title: "عنوان <جديد>".into(),
            author: "Rivet".into(),
            subject: String::new(),
            keywords: "a, b".into(),
        };
        let out = update_xmp(xml, Some(&meta), "2026-10-07T12:00:00Z");
        assert!(out.contains("عنوان &lt;جديد&gt;"));
        assert!(!out.contains("Old title"));
        assert!(out.contains("<rdf:li>Rivet</rdf:li>"));
        assert!(out.contains(r#"pdf:Keywords="a, b""#));
        assert!(out.contains("<xmp:ModifyDate>2026-10-07T12:00:00Z</xmp:ModifyDate>"));
        // No description element existed, so none is invented.
        assert!(!out.contains("dc:description"));
    }

    #[test]
    fn stamps_a_file_without_an_info_dictionary() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/basic.pdf"
        );
        let mut doc = lopdf::Document::load(fixture).unwrap();
        doc.trailer.remove(b"Info");
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();

        let (out, date) = stamp(bytes, None, false).unwrap();
        assert!(date.is_some());
        let doc = lopdf::Document::load_mem(&out).unwrap();
        let info = doc
            .trailer
            .get(b"Info")
            .and_then(|r| doc.dereference(r))
            .unwrap()
            .1;
        let producer = info.as_dict().unwrap().get(b"Producer").unwrap();
        assert_eq!(lopdf::decode_text_string(producer).unwrap(), PRODUCER);
        assert_eq!(doc.get_pages().len(), 3);
    }
}
