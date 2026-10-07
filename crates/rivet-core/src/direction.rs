//! Guessing a document's reading direction from its text, so two-page spreads
//! of Arabic (or Hebrew, Persian, Urdu…) documents can start on the right.

use pdfium_render::prelude::*;

/// How many pages to look at. The first pages are enough and keep this fast.
const PAGES_TO_SAMPLE: u32 = 5;
/// Stop reading after this many letters.
const ENOUGH_LETTERS: usize = 2_000;

/// True for letters of right-to-left scripts (Hebrew, Arabic, Syriac, Thaana,
/// N'Ko and their presentation forms).
fn is_rtl_letter(c: char) -> bool {
    matches!(c as u32,
        0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF)
        && c.is_alphabetic()
}

/// Decides from text: right-to-left if most letters come from RTL scripts.
pub(crate) fn is_rtl_text(text: &str) -> bool {
    let (mut rtl, mut ltr) = (0usize, 0usize);
    for c in text.chars().filter(|c| c.is_alphabetic()) {
        if is_rtl_letter(c) {
            rtl += 1;
        } else {
            ltr += 1;
        }
        if rtl + ltr >= ENOUGH_LETTERS {
            break;
        }
    }
    rtl > ltr
}

/// Looks at the first pages of a document. Scanned documents without text read
/// as left-to-right (the user can still flip the order in the app).
pub(crate) fn detect(document: &PdfDocument) -> bool {
    let pages = document.pages();
    let mut sample = String::new();
    for index in 0..(pages.len().max(0) as u32).min(PAGES_TO_SAMPLE) {
        if let Ok(page) = pages.get(index as PdfPageIndex)
            && let Ok(text) = page.text()
        {
            sample.push_str(&text.all());
        }
        if sample.chars().filter(|c| c.is_alphabetic()).count() >= ENOUGH_LETTERS {
            break;
        }
    }
    is_rtl_text(&sample)
}

#[cfg(test)]
mod tests {
    use super::is_rtl_text;

    #[test]
    fn decides_by_the_majority_of_letters() {
        assert!(is_rtl_text("مرحبا بكم في ريفت، هذا ملف PDF"));
        assert!(!is_rtl_text("The quick brown fox — ريفت"));
        assert!(is_rtl_text("שלום עולם"));
        assert!(!is_rtl_text("123 456 !?"));
    }
}
