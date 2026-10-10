//! Text strings stored in a PDF (titles, bookmarks, comments, form fields…)
//! that PDFium misreads because the app that wrote them broke the rules.
//!
//! The specification allows two encodings for these strings: UTF-16 with a
//! byte order mark, or PDFDocEncoding (a Latin-1 variant). Many apps instead
//! write plain UTF-8, for any language. PDFium reads those bytes as
//! PDFDocEncoding, so `جُمّل` shows as `Ø¬Ù‘Ù–Ù—` and `Мир` as `ÐœÐ¸Ñ€`.
//!
//! [`Texts::repair`] turns PDFium's text back into the bytes it came from
//! and, when those bytes are UTF-8, decodes them as UTF-8. Two bytes, 0x9F and
//! 0xAD, are undefined in PDFDocEncoding: PDFium turns both into U+0000 (drops
//! them at the end, shows them as spaces in bookmark titles), and UTF-8 uses
//! them a lot (Arabic `ح` is `D8 AD`, Russian `П` is `D0 9F`). For those, the
//! string is looked up in the file itself: every UTF-8 string it stores is
//! collected once (decrypted when the file is password-protected), and the
//! one matching PDFium's text at every other byte is used. When nothing
//! matches, or more than one string does, PDFium's text is kept as it was.

use std::{cell::OnceCell, collections::BTreeSet, path::Path};

use lopdf::{LoadOptions, Object};

use crate::incremental::Pdf;

/// PDFDocEncoding's bytes 0x80–0xFF as PDFium decodes them. 0x9F and 0xAD
/// are undefined and come back as U+0000.
const PDF_DOC_HIGH: [u16; 128] = [
    0x2022, 0x2020, 0x2021, 0x2026, 0x2014, 0x2013, 0x0192, 0x2044, //
    0x2039, 0x203A, 0x2212, 0x2030, 0x201E, 0x201C, 0x201D, 0x2018, //
    0x2019, 0x201A, 0x2122, 0xFB01, 0xFB02, 0x0141, 0x0152, 0x0160, //
    0x0178, 0x017D, 0x0131, 0x0142, 0x0153, 0x0161, 0x017E, 0x0000, //
    0x20AC, 0x00A1, 0x00A2, 0x00A3, 0x00A4, 0x00A5, 0x00A6, 0x00A7, //
    0x00A8, 0x00A9, 0x00AA, 0x00AB, 0x00AC, 0x0000, 0x00AE, 0x00AF, //
    0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x00B4, 0x00B5, 0x00B6, 0x00B7, //
    0x00B8, 0x00B9, 0x00BA, 0x00BB, 0x00BC, 0x00BD, 0x00BE, 0x00BF, //
    0x00C0, 0x00C1, 0x00C2, 0x00C3, 0x00C4, 0x00C5, 0x00C6, 0x00C7, //
    0x00C8, 0x00C9, 0x00CA, 0x00CB, 0x00CC, 0x00CD, 0x00CE, 0x00CF, //
    0x00D0, 0x00D1, 0x00D2, 0x00D3, 0x00D4, 0x00D5, 0x00D6, 0x00D7, //
    0x00D8, 0x00D9, 0x00DA, 0x00DB, 0x00DC, 0x00DD, 0x00DE, 0x00DF, //
    0x00E0, 0x00E1, 0x00E2, 0x00E3, 0x00E4, 0x00E5, 0x00E6, 0x00E7, //
    0x00E8, 0x00E9, 0x00EA, 0x00EB, 0x00EC, 0x00ED, 0x00EE, 0x00EF, //
    0x00F0, 0x00F1, 0x00F2, 0x00F3, 0x00F4, 0x00F5, 0x00F6, 0x00F7, //
    0x00F8, 0x00F9, 0x00FA, 0x00FB, 0x00FC, 0x00FD, 0x00FE, 0x00FF, //
];

/// The two bytes PDFium loses.
const LOST: [u8; 2] = [0x9F, 0xAD];

/// One byte of a string as far as PDFium's text tells.
#[derive(Clone, Copy, PartialEq)]
enum Slot {
    Byte(u8),
    /// 0x9F or 0xAD.
    Lost,
    /// A space, or 0x9F or 0xAD (bookmark titles show lost bytes as spaces).
    SpaceOrLost,
}

/// The bytes PDFium decoded `value` from, if it was decoded as
/// PDFDocEncoding. Returns `None` for text PDFium decoded otherwise (UTF-16,
/// or UTF-8 with a byte order mark).
fn pdf_doc_bytes(value: &str, spaces_may_be_lost: bool) -> Option<Vec<Slot>> {
    value
        .chars()
        .map(|c| match u32::from(c) {
            0 => Some(Slot::Lost),
            0x20 if spaces_may_be_lost => Some(Slot::SpaceOrLost),
            ascii @ 1..0x80 => Some(Slot::Byte(ascii as u8)),
            other => PDF_DOC_HIGH
                .iter()
                .position(|&h| u32::from(h) == other)
                .map(|i| Slot::Byte(0x80 + i as u8)),
        })
        .collect()
}

/// Whether the bytes could be UTF-8 with lost bytes in place of some
/// continuation bytes (and maybe missing from the end).
fn could_be_utf8(slots: &[Slot]) -> bool {
    let mut at = 0;
    while at < slots.len() {
        let follow = match slots[at] {
            Slot::Byte(0..0x80) | Slot::SpaceOrLost => 0,
            Slot::Byte(0xC2..=0xDF) => 1,
            Slot::Byte(0xE0..=0xEF) => 2,
            Slot::Byte(0xF0..=0xF4) => 3,
            _ => return false,
        };
        for next in 1..=follow {
            match slots.get(at + next) {
                None => return true,
                Some(Slot::Byte(0x80..=0xBF) | Slot::Lost | Slot::SpaceOrLost) => {}
                Some(_) => return false,
            }
        }
        at += follow + 1;
    }
    true
}

/// A stored string PDFium would show as `slots` (lost bytes dropped from
/// the end included).
fn matches(stored: &[u8], slots: &[Slot]) -> bool {
    stored.len() >= slots.len()
        && stored[slots.len()..].iter().all(|b| LOST.contains(b))
        && stored.iter().zip(slots).all(|(b, slot)| match slot {
            Slot::Byte(s) => b == s,
            Slot::Lost => LOST.contains(b),
            Slot::SpaceOrLost => *b == b' ' || LOST.contains(b),
        })
}

/// A string that may need looking up: UTF-8 (no byte order mark, as that is
/// read correctly) with a byte PDFium loses.
fn worth_keeping(bytes: &[u8]) -> bool {
    !bytes.starts_with(b"\xFE\xFF")
        && !bytes.starts_with(b"\xFF\xFE")
        && !bytes.starts_with(b"\xEF\xBB\xBF")
        && bytes.iter().any(|b| LOST.contains(b))
        && std::str::from_utf8(bytes).is_ok()
}

/// Every string worth keeping in the loaded objects.
fn collect(doc: &lopdf::Document) -> Vec<Vec<u8>> {
    fn visit(object: &Object, found: &mut BTreeSet<Vec<u8>>) {
        match object {
            Object::String(bytes, _) if worth_keeping(bytes) => {
                found.insert(bytes.clone());
            }
            Object::Array(items) => items.iter().for_each(|o| visit(o, found)),
            Object::Dictionary(dict) => dict.iter().for_each(|(_, o)| visit(o, found)),
            Object::Stream(stream) => stream.dict.iter().for_each(|(_, o)| visit(o, found)),
            _ => {}
        }
    }
    let mut found = BTreeSet::new();
    doc.objects.values().for_each(|o| visit(o, &mut found));
    found.into_iter().collect()
}

/// While loading, keeps stream dictionaries but not their data (page
/// contents, images…), except object streams, which hold other objects.
/// (lopdf keeps the changed `object` for most objects, and the returned one
/// for those inside object streams.)
fn without_stream_data(
    id: lopdf::ObjectId,
    object: &mut Object,
) -> Option<(lopdf::ObjectId, Object)> {
    if let Object::Stream(stream) = object
        && !stream.dict.has_type(b"ObjStm")
    {
        *object = Object::Dictionary(std::mem::take(&mut stream.dict));
    }
    Some((id, object.clone()))
}

/// Repairs text strings of one document (see the module documentation).
pub(crate) struct Texts<'a> {
    /// The strings worth keeping, collected the first time one is needed.
    pub stored: &'a OnceCell<Vec<Vec<u8>>>,
    /// The file's bytes, when they are in memory; otherwise it is read from `path`.
    pub bytes: Option<&'a [u8]>,
    pub path: &'a Path,
    pub password: Option<&'a str>,
}

impl Texts<'_> {
    /// `value` as its author meant it (see the module documentation).
    pub fn repair(&self, value: String) -> String {
        self.repair_with(value, false, None)
    }

    /// An Info dictionary entry (`Title`, `Author`…) as its author meant it.
    /// Shown as soon as a file opens, so the entry is read from the file
    /// directly first, which is much quicker than collecting every string.
    pub fn repair_info(&self, value: String, key: &[u8]) -> String {
        self.repair_with(value, false, Some(key))
    }

    /// A bookmark title as its author meant it. PDFium shows lost bytes in
    /// bookmark titles as spaces.
    pub fn repair_title(&self, value: String) -> String {
        self.repair_with(value, true, None)
    }

    fn repair_with(
        &self,
        value: String,
        spaces_may_be_lost: bool,
        info_key: Option<&[u8]>,
    ) -> String {
        let Some(slots) = pdf_doc_bytes(&value, spaces_may_be_lost) else {
            return value;
        };
        let plain = |slot: &Slot| match slot {
            Slot::Byte(b) => Some(*b),
            Slot::SpaceOrLost => Some(b' '),
            Slot::Lost => None,
        };
        // Nothing lost: the bytes are all there.
        if let Some(bytes) = slots.iter().map(plain).collect::<Option<Vec<u8>>>() {
            if bytes.is_ascii() {
                return value;
            }
            if let Ok(text) = String::from_utf8(bytes) {
                return text;
            }
        }
        // Only look in the file when lost bytes would make this UTF-8.
        if !could_be_utf8(&slots) {
            return value;
        }
        if let Some(stored) = info_key.and_then(|key| self.info_string(key))
            && worth_keeping(&stored)
            && matches(&stored, &slots)
        {
            return String::from_utf8(stored).unwrap_or(value);
        }
        let mut found = self
            .stored()
            .iter()
            .filter(|stored| matches(stored, &slots));
        match (found.next(), found.next()) {
            (Some(only), None) => String::from_utf8(only.clone()).unwrap_or(value),
            _ => value,
        }
    }

    /// A form field's full name: its own name and its parents', joined by dots.
    /// Each part is a separate string in the file, so each is repaired alone.
    pub fn repair_dotted(&self, value: String) -> String {
        let whole = self.repair(value.clone());
        if whole != value || !value.contains('.') {
            return whole;
        }
        value
            .split('.')
            .map(|part| self.repair(part.to_owned()))
            .collect::<Vec<_>>()
            .join(".")
    }

    /// An Info dictionary string as stored in the file, read without loading
    /// the rest. None for password-protected files (their strings are encrypted).
    fn info_string(&self, key: &[u8]) -> Option<Vec<u8>> {
        let read;
        let bytes = match self.bytes {
            Some(bytes) => bytes,
            None => {
                read = std::fs::read(self.path).ok()?;
                &read
            }
        };
        let pdf = Pdf::read(bytes).ok()?;
        if pdf.trailer.has(b"Encrypt") {
            return None;
        }
        let info = pdf.resolve(pdf.trailer.get(b"Info").ok()?)?;
        let value = pdf.resolve(info.as_dict().ok()?.get(key).ok()?)?;
        value.as_str().ok().map(<[u8]>::to_vec)
    }

    fn stored(&self) -> &[Vec<u8>] {
        self.stored.get_or_init(|| {
            let options = LoadOptions {
                password: Some(self.password.unwrap_or_default().to_owned()),
                filter: Some(without_stream_data),
                ..LoadOptions::default()
            };
            let doc = match self.bytes {
                Some(bytes) => lopdf::Document::load_mem_with_options(bytes, options),
                None => lopdf::Document::load_with_options(self.path, options),
            };
            doc.map(|doc| collect(&doc)).unwrap_or_default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What PDFium shows for UTF-8 bytes stored without a byte order mark.
    pub(crate) fn as_pdfium(text: &str) -> String {
        let shown: String = text
            .bytes()
            .map(|b| match b {
                0..0x80 => char::from(b),
                _ => char::from_u32(PDF_DOC_HIGH[usize::from(b - 0x80)].into()).unwrap(),
            })
            .collect();
        shown.trim_end_matches('\0').to_owned()
    }

    fn texts<'a>(stored: &'a OnceCell<Vec<Vec<u8>>>, strings: &[&str]) -> Texts<'a> {
        let _ = stored.set(
            strings
                .iter()
                .map(|s| s.as_bytes().to_vec())
                .filter(|s| worth_keeping(s))
                .collect(),
        );
        Texts {
            stored,
            bytes: None,
            path: Path::new("/nonexistent.pdf"),
            password: None,
        }
    }

    #[test]
    fn repairs_utf8_in_any_language() {
        let cell = OnceCell::new();
        let samples = [
            "جُمّل.pdf",
            "محمد حسن",
            "أين الملح",
            "ماذا؟",
            "Привет, мир",
            "Παράδειγμα",
            "שלום עולם",
            "日本語のタイトル",
            "中文标题",
            "한국어 제목",
            "हिन्दी शीर्षक",
            "ภาษาไทย",
            "فارسی گزارش",
            "اردو عنوان",
            "Türkçe başlık",
            "Tiếng Việt",
            "emoji 📄✅",
        ];
        let texts = texts(&cell, &samples);
        for sample in samples {
            assert_eq!(texts.repair(as_pdfium(sample)), sample);
        }
    }

    #[test]
    fn leaves_correct_text_alone() {
        let cell = OnceCell::new();
        let texts = texts(&cell, &[]);
        for text in ["plain", "Café résumé", "عنوان", "“quoted” – dash", "a\0b"] {
            assert_eq!(texts.repair(text.to_owned()), text);
        }
    }

    #[test]
    fn keeps_pdfium_text_when_the_file_is_ambiguous() {
        // ح (D8 AD) and ؟ (D8 9F) look the same to PDFium.
        let cell = OnceCell::new();
        let texts = texts(&cell, &["ح", "؟"]);
        let shown = as_pdfium("ح");
        assert_eq!(texts.repair(shown.clone()), shown);
    }

    #[test]
    fn repairs_bookmark_titles_with_lost_bytes_shown_as_spaces() {
        let cell = OnceCell::new();
        let samples = ["محمد حسن", "Привет", "emoji 📄", "حح ح"];
        let texts = texts(&cell, &samples);
        for sample in samples {
            let shown = as_pdfium(sample).replace('\0', " ");
            assert_eq!(texts.repair_title(shown), sample);
        }
        // Real spaces in correct text stay.
        assert_eq!(texts.repair_title("a b".into()), "a b");
    }

    #[test]
    fn repairs_dotted_field_names() {
        let cell = OnceCell::new();
        let texts = texts(&cell, &["الحقل", "صحيح"]);
        let shown = format!("{}.{}", as_pdfium("الحقل"), as_pdfium("صحيح"));
        assert_eq!(texts.repair_dotted(shown), "الحقل.صحيح");
    }
}
