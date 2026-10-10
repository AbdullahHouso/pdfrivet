//! The text of a page: where every character is (for selecting text) and the
//! text itself (for copying).
//!
//! Selection works on PDFium's character order, the way Chrome's PDF viewer
//! does: the UI gets every character's box once per page, finds the character
//! under the pointer and draws the selection itself. A range of character
//! indexes is then turned back into text here. This stays correct for Arabic
//! and other right-to-left text, where an invisible HTML text layer and the
//! browser's own selection go wrong.
//!
//! pdfium-render doesn't expose the text page handle, so
//! [`TextPage`] calls PDFium directly (`unsafe`), following the same rules as
//! `forms.rs`: handles come from live pdfium-render objects that outlive the
//! calls, and everything runs on the Engine's single PDFium thread.

#![allow(unsafe_code)]

use std::marker::PhantomData;

use crate::document::RawBindings;
use pdfium_render::prelude::*;
use serde::Deserialize;
use ts_rs::TS;

use crate::geometry::{Affine, PageGeometry};

/// Character flag: PDFium inserted this character (a space or line break that
/// isn't in the file, added so copied text reads naturally).
pub const CHAR_GENERATED: u32 = 1;
/// Character flag: the character has no box on the page (nothing to select).
pub const CHAR_NO_BOX: u32 = 2;

/// One character of a page's text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextChar {
    /// The character's box as fractions of the page (top-left origin, before view rotation).
    /// Boxes are "loose": as tall as the font's line, so selections look even.
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    /// Unicode code point.
    pub code: u32,
    /// `CHAR_*` flags.
    pub flags: u32,
}

/// Every character of a page, in PDFium's text order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageText {
    pub chars: Vec<TextChar>,
}

impl PageText {
    /// Size of one character in [`PageText::to_bytes`].
    pub const BYTES_PER_CHAR: usize = 24;

    /// The compact binary form sent to the UI (all values little-endian):
    /// the character count (`u32`), 4 reserved bytes, then per character its
    /// box as four `f32` (left, top, right, bottom), its code point (`u32`)
    /// and its flags (`u32`). Every value is 4-byte aligned, so the UI can read
    /// it with typed arrays without copying.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.chars.len() * Self::BYTES_PER_CHAR);
        out.extend_from_slice(&(self.chars.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        for c in &self.chars {
            for v in [c.left, c.top, c.right, c.bottom] {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&c.code.to_le_bytes());
            out.extend_from_slice(&c.flags.to_le_bytes());
        }
        out
    }
}

/// A stretch of text across pages: from character `start` on `startPage` up to
/// (not including) character `end` on `endPage`. An `end` past the last
/// character means "to the end of that page".
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct TextRange {
    pub start_page: u32,
    pub start: u32,
    pub end_page: u32,
    pub end: u32,
}

/// Reads every character of a page with its box.
pub(crate) fn read(pdfium: &Pdfium, page: &PdfPage) -> PageText {
    let (Some(text), Some(transform)) = (
        TextPage::load(pdfium, page),
        PageGeometry::new(page)
            .as_ref()
            .and_then(PageGeometry::affine),
    ) else {
        return PageText::default();
    };
    let chars = raw_chars(&text)
        .into_iter()
        .map(|c| to_text_char(&c, &transform))
        .collect();
    PageText { chars }
}

/// A character as PDFium reports it, with its box in PDF points.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RawChar {
    pub code: u32,
    pub generated: bool,
    /// (left, bottom, right, top)
    pub rect: Option<(f32, f32, f32, f32)>,
}

/// Every character of a page in PDFium's order, with its ligature mistake fixed
/// (see [`fix_reversed_ligatures`]).
pub(crate) fn raw_chars(text: &TextPage) -> Vec<RawChar> {
    let mut chars: Vec<RawChar> = (0..text.count())
        .map(|i| RawChar {
            code: text.unicode(i),
            generated: text.is_generated(i),
            rect: text.loose_box(i),
        })
        .collect();
    fix_reversed_ligatures(&mut chars);
    chars
}

/// PDFium puts right-to-left text into reading order by reversing it, which
/// also reverses the letters of a ligature: the lam-alef in "للاختبار" comes
/// out as "ال". The letters of one ligature share one glyph, so they have the
/// same box; a run of right-to-left letters with the same box is turned back.
fn fix_reversed_ligatures(chars: &mut [RawChar]) {
    let mut i = 0;
    while i < chars.len() {
        let mut end = i + 1;
        if chars[i].rect.is_some() && is_rtl(chars[i].code) {
            while end < chars.len() && chars[end].rect == chars[i].rect && is_rtl(chars[end].code) {
                end += 1;
            }
            chars[i..end].reverse();
        }
        i = end;
    }
}

/// Arabic, Hebrew, Syriac, Thaana, N'Ko… and their presentation forms.
fn is_rtl(code: u32) -> bool {
    matches!(code, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

fn to_text_char(c: &RawChar, transform: &Affine) -> TextChar {
    let mut flags = 0;
    if c.generated {
        flags |= CHAR_GENERATED;
    }
    let rect = c
        .rect
        .map(|(l, b, r, t)| transform.rect(l, b, r, t))
        .filter(|r| r.right > r.left && r.bottom > r.top);
    let (left, top, right, bottom) = match rect {
        Some(r) => (r.left, r.top, r.right, r.bottom),
        None => {
            flags |= CHAR_NO_BOX;
            (0.0, 0.0, 0.0, 0.0)
        }
    };
    TextChar {
        left,
        top,
        right,
        bottom,
        code: c.code,
        flags,
    }
}

/// The text of characters `start..end` on a page (clamped to the page).
pub(crate) fn text_of(pdfium: &Pdfium, page: &PdfPage, start: u32, end: u32) -> String {
    let Some(text) = TextPage::load(pdfium, page) else {
        return String::new();
    };
    let chars = raw_chars(&text);
    let end = (end as usize).min(chars.len());
    let start = (start as usize).min(end);
    let text: String = chars[start..end]
        .iter()
        .filter_map(|c| char::from_u32(c.code))
        .collect();
    clean(&text)
}

/// Tidies extracted text for the clipboard: one kind of line break, and no
/// control characters PDFium uses internally.
pub(crate) fn clean(text: &str) -> String {
    text.replace("\r\n", "\n")
        .chars()
        .map(|c| if c == '\r' { '\n' } else { c })
        .filter(|&c| c == '\n' || c == '\t' || !c.is_control())
        .collect()
}

/// PDFium's text information for one page. Closed when dropped.
pub(crate) struct TextPage<'p> {
    bindings: &'p dyn PdfiumLibraryBindings,
    handle: FPDF_TEXTPAGE,
    // The text page is only valid while the page it was loaded from is.
    _page: PhantomData<&'p PdfPage<'p>>,
}

impl<'p> TextPage<'p> {
    pub fn load(pdfium: &'p Pdfium, page: &'p PdfPage) -> Option<Self> {
        let bindings = pdfium.bindings();
        // SAFETY: the page handle belongs to a live PdfPage that outlives this
        // TextPage (tied by the lifetime); we're on the PDFium thread.
        let handle = unsafe { bindings.FPDFText_LoadPage(bindings.get_handle_from_page(page)) };
        (!handle.is_null()).then_some(Self {
            bindings,
            handle,
            _page: PhantomData,
        })
    }

    pub fn count(&self) -> i32 {
        // SAFETY: valid text page handle (see `load`).
        unsafe { self.bindings.FPDFText_CountChars(self.handle) }.max(0)
    }

    pub fn unicode(&self, index: i32) -> u32 {
        // SAFETY: valid handle; PDFium checks the index range itself.
        unsafe { self.bindings.FPDFText_GetUnicode(self.handle, index) }
    }

    pub fn is_generated(&self, index: i32) -> bool {
        // SAFETY: as above; returns -1 for a bad index, 1 for generated.
        unsafe { self.bindings.FPDFText_IsGenerated(self.handle, index) == 1 }
    }

    /// The loose box of a character in PDF points: (left, bottom, right, top).
    pub fn loose_box(&self, index: i32) -> Option<(f32, f32, f32, f32)> {
        let mut rect = FS_RECTF {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        };
        // SAFETY: `rect` is a valid, writable FS_RECTF for the duration of the call.
        let ok = unsafe {
            self.bindings
                .FPDFText_GetLooseCharBox(self.handle, index, &mut rect)
        };
        (ok != 0).then_some((rect.left, rect.bottom, rect.right, rect.top))
    }
}

impl Drop for TextPage<'_> {
    fn drop(&mut self) {
        // SAFETY: matches FPDFText_LoadPage in `load`; closed exactly once.
        unsafe { self.bindings.FPDFText_ClosePage(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(code: char, rect: (f32, f32, f32, f32)) -> RawChar {
        RawChar {
            code: code as u32,
            generated: false,
            rect: Some(rect),
        }
    }

    #[test]
    fn puts_ligature_letters_back_in_order() {
        let glyph = (10.0, 0.0, 20.0, 10.0);
        // "ل" + reversed lam-alef ("ا", "ل" sharing one box) + "خ".
        let mut chars = vec![
            raw('ل', (20.0, 0.0, 25.0, 10.0)),
            raw('ا', glyph),
            raw('ل', glyph),
            raw('خ', (5.0, 0.0, 10.0, 10.0)),
        ];
        fix_reversed_ligatures(&mut chars);
        let text: String = chars
            .iter()
            .map(|c| char::from_u32(c.code).unwrap())
            .collect();
        assert_eq!(text, "للاخ");
        // Latin ligatures ("fi") are already in order and stay as they are.
        let mut latin = vec![raw('f', glyph), raw('i', glyph)];
        fix_reversed_ligatures(&mut latin);
        assert_eq!(latin[0].code, 'f' as u32);
    }

    #[test]
    fn clean_keeps_line_breaks_and_drops_control_characters() {
        assert_eq!(clean("one\r\ntwo\rthree\u{2}"), "one\ntwo\nthree");
        assert_eq!(clean("a\tb"), "a\tb");
    }

    #[test]
    fn binary_form_has_a_header_and_fixed_size_characters() {
        let text = PageText {
            chars: vec![TextChar {
                left: 0.1,
                top: 0.2,
                right: 0.3,
                bottom: 0.4,
                code: 'ب' as u32,
                flags: CHAR_GENERATED,
            }],
        };
        let bytes = text.to_bytes();
        assert_eq!(bytes.len(), 8 + PageText::BYTES_PER_CHAR);
        assert_eq!(u32::from_le_bytes(bytes[0..4].try_into().unwrap()), 1);
        assert_eq!(f32::from_le_bytes(bytes[8..12].try_into().unwrap()), 0.1);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            'ب' as u32
        );
        assert_eq!(
            u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
            CHAR_GENERATED
        );
    }
}
