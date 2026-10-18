//! Drawing the text of a form field ourselves when it holds right-to-left text.
//!
//! PDFium writes a field's appearance one glyph at a time, each with its own
//! move (`Td`) and its own rounded advance. Arabic joins only work when
//! neighbouring glyphs touch exactly, and a renderer that places every glyph on
//! its own pixel grid opens small gaps between them, so a word falls apart into
//! letters at normal zoom (more so the smaller the field).
//!
//! So after PDFium has committed such a value, the appearance is rebuilt: the
//! background and border PDFium drew are kept, and the text is replaced by the
//! same shaped outlines page text boxes use (`textlayout`), which join at any
//! zoom and in every reader.

#![allow(unsafe_code)]

use pdfium_render::prelude::*;

use crate::fonts::TextFont;
use crate::textlayout::{self, LINE_HEIGHT, TextAlign, TextDirection, TextStyle, VerticalAlign};

/// Space between a field's edge and its text, in points.
const PADDING: f32 = 2.0;
/// The size PDFium and Acrobat give "auto" text at most, in points.
const AUTO_SIZE_MAX: f32 = 12.0;
const AUTO_SIZE_MIN: f32 = 4.0;

// Values from PDFium's public headers (fpdf_edit.h), which pdfium-render
// doesn't re-export.
const FPDF_ANNOT_APPEARANCEMODE_NORMAL: i32 = 0;
const FPDF_PAGEOBJ_TEXT: i32 = 1;
const FPDF_PAGEOBJ_PATH: i32 = 2;
const FPDF_SEGMENT_LINETO: i32 = 0;
const FPDF_SEGMENT_BEZIERTO: i32 = 1;
const FPDF_SEGMENT_MOVETO: i32 = 2;

/// Whether a field's value needs [`redraw`]: it has letters of a
/// right-to-left script.
pub(crate) fn needs_redraw(value: &str) -> bool {
    crate::direction::has_rtl_letter(value)
}

/// Replaces the text in the appearance of the text field `annot` with shaped
/// outlines of `value`. Returns false (leaving PDFium's appearance alone) when
/// the appearance holds anything this can't carry over.
///
/// # Safety
/// `annot` must be an open annotation handle of `bindings`' library, used on
/// the PDFium thread.
pub(crate) unsafe fn redraw(
    bindings: &dyn PdfiumLibraryBindings,
    annot: FPDF_ANNOTATION,
    value: &str,
    multiline: bool,
) -> bool {
    let mut rect = FS_RECTF {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    // SAFETY: the caller guarantees `annot` is open; `rect` is a plain struct.
    if unsafe { bindings.FPDFAnnot_GetRect(annot, &mut rect) } == 0 {
        return false;
    }
    let (width, height) = (rect.right - rect.left, rect.top - rect.bottom);
    if width <= 2.0 * PADDING || height <= 2.0 * PADDING {
        return false;
    }
    // SAFETY: as above.
    let Some(chrome) = (unsafe { chrome(bindings, annot, &rect) }) else {
        return false;
    };
    // SAFETY: as above.
    let da = unsafe { default_appearance(bindings, annot) };

    let size = match da.size {
        Some(size) if size > 0.0 => size,
        _ if multiline => AUTO_SIZE_MAX,
        _ => ((height - 2.0 * PADDING) / LINE_HEIGHT).clamp(AUTO_SIZE_MIN, AUTO_SIZE_MAX),
    };
    let inner_width = width - 2.0 * PADDING;
    let style = TextStyle {
        font: TextFont::default(),
        size,
        bold: false,
        align: TextAlign::Auto,
        valign: if multiline {
            VerticalAlign::Top
        } else {
            VerticalAlign::Middle
        },
        direction: TextDirection::Auto,
        width: multiline.then_some(inner_width),
        height: Some(height - 2.0 * PADDING),
    };
    let layout = textlayout::layout(value, &style);
    // One line grows with its text, so it is placed by hand: from the right
    // edge for right-to-left text, from the left otherwise.
    let x = if multiline || !layout.rtl {
        rect.left + PADDING
    } else {
        rect.right - PADDING - layout.width
    };
    let path = layout.to_pdf_path(x, rect.top - PADDING);
    let (r, g, b) = da.color;
    let content = format!(
        "{chrome}q {:.3} {:.3} {:.3} {:.3} re W n {r:.3} {g:.3} {b:.3} rg {path}f Q",
        rect.left + 1.0,
        rect.bottom + 1.0,
        width - 2.0,
        height - 2.0,
    );
    let wide: Vec<u16> = content.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    unsafe { bindings.FPDFAnnot_SetAP(annot, FPDF_ANNOT_APPEARANCEMODE_NORMAL, wide.as_ptr()) != 0 }
}

/// The field's background and border as page-space content: PDFium's own
/// drawing minus its text. `None` when the appearance holds something else
/// (images, shadings…) or is drawn outside the field, so it is left alone.
///
/// # Safety
/// See [`redraw`].
unsafe fn chrome(
    bindings: &dyn PdfiumLibraryBindings,
    annot: FPDF_ANNOTATION,
    rect: &FS_RECTF,
) -> Option<String> {
    let (width, height) = (rect.right - rect.left, rect.top - rect.bottom);
    let mut out = String::new();
    // SAFETY: objects belong to the annotation's appearance and stay valid while
    // the annotation handle is open; they are only read.
    unsafe {
        for i in 0..bindings.FPDFAnnot_GetObjectCount(annot) {
            let object = bindings.FPDFAnnot_GetObject(annot, i);
            if object.is_null() {
                return None;
            }
            match bindings.FPDFPageObj_GetType(object) {
                FPDF_PAGEOBJ_TEXT => {}
                FPDF_PAGEOBJ_PATH => out.push_str(&path_content(
                    bindings,
                    object,
                    (rect.left, rect.bottom),
                    (width, height),
                )?),
                _ => return None,
            }
        }
    }
    Some(out)
}

/// One path object as content, moved to where the field is on the page.
///
/// # Safety
/// `object` must be a valid path object of `bindings`' library.
unsafe fn path_content(
    bindings: &dyn PdfiumLibraryBindings,
    object: FPDF_PAGEOBJECT,
    origin: (f32, f32),
    size: (f32, f32),
) -> Option<String> {
    let mut matrix = FS_MATRIX {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };
    let (mut fill_mode, mut stroked) = (0, 0);
    let (mut fr, mut fg, mut fb, mut fa) = (0u32, 0u32, 0u32, 0u32);
    let (mut sr, mut sg, mut sb, mut sa) = (0u32, 0u32, 0u32, 0u32);
    let mut stroke_width = 1.0f32;
    // SAFETY: the caller guarantees `object` is valid; all out-parameters are
    // plain locals.
    unsafe {
        // Objects of an appearance are usually unmoved; if there's no matrix, assume so.
        bindings.FPDFPageObj_GetMatrix(object, &mut matrix);
        bindings.FPDFPath_GetDrawMode(object, &mut fill_mode, &mut stroked);
        bindings.FPDFPageObj_GetFillColor(object, &mut fr, &mut fg, &mut fb, &mut fa);
        bindings.FPDFPageObj_GetStrokeColor(object, &mut sr, &mut sg, &mut sb, &mut sa);
        bindings.FPDFPageObj_GetStrokeWidth(object, &mut stroke_width);
    }
    let fill = fill_mode != 0;
    let stroked = stroked != 0;
    if !fill && !stroked {
        return Some(String::new());
    }

    let point = |x: f32, y: f32| -> Option<(f32, f32)> {
        let px = matrix.a * x + matrix.c * y + matrix.e;
        let py = matrix.b * x + matrix.d * y + matrix.f;
        // Inside the field (with a point of slack), or it isn't the field's own drawing.
        let inside = px >= -1.0 && px <= size.0 + 1.0 && py >= -1.0 && py <= size.1 + 1.0;
        inside.then_some((px + origin.0, py + origin.1))
    };

    let mut out = String::new();
    let c = |v: u32| v as f32 / 255.0;
    if fill {
        out.push_str(&format!("{:.3} {:.3} {:.3} rg ", c(fr), c(fg), c(fb)));
    }
    if stroked {
        out.push_str(&format!(
            "{:.3} {:.3} {:.3} RG {stroke_width:.3} w ",
            c(sr),
            c(sg),
            c(sb)
        ));
    }
    // SAFETY: as above; segments belong to the path object.
    unsafe {
        let mut curve: Vec<(f32, f32)> = Vec::new();
        for i in 0..bindings.FPDFPath_CountSegments(object) {
            let segment = bindings.FPDFPath_GetPathSegment(object, i);
            if segment.is_null() {
                return None;
            }
            let (mut x, mut y) = (0.0f32, 0.0f32);
            if bindings.FPDFPathSegment_GetPoint(segment, &mut x, &mut y) == 0 {
                return None;
            }
            let (x, y) = point(x, y)?;
            match bindings.FPDFPathSegment_GetType(segment) {
                FPDF_SEGMENT_MOVETO => out.push_str(&format!("{x:.3} {y:.3} m ")),
                FPDF_SEGMENT_LINETO => out.push_str(&format!("{x:.3} {y:.3} l ")),
                FPDF_SEGMENT_BEZIERTO => {
                    curve.push((x, y));
                    if let [a, b, c] = curve[..] {
                        out.push_str(&format!(
                            "{:.3} {:.3} {:.3} {:.3} {:.3} {:.3} c ",
                            a.0, a.1, b.0, b.1, c.0, c.1
                        ));
                        curve.clear();
                    }
                }
                _ => return None,
            }
            if bindings.FPDFPathSegment_GetClose(segment) != 0 {
                out.push_str("h ");
            }
        }
    }
    out.push_str(match (fill_mode, stroked) {
        (1, true) => "B* ",
        (_, true) if fill => "B ",
        (_, true) => "S ",
        (1, false) => "f* ",
        _ => "f ",
    });
    Some(format!("q {out}Q "))
}

/// What a field's `/DA` (default appearance) string says: the font size
/// (0 means "fit the field") and the text colour.
struct DefaultAppearance {
    size: Option<f32>,
    color: (f32, f32, f32),
}

/// # Safety
/// See [`redraw`].
unsafe fn default_appearance(
    bindings: &dyn PdfiumLibraryBindings,
    annot: FPDF_ANNOTATION,
) -> DefaultAppearance {
    let key = "DA";
    // SAFETY: the buffer is sized by PDFium's own answer; the call only writes into it.
    let text = unsafe {
        let needed = bindings.FPDFAnnot_GetStringValue(annot, key, std::ptr::null_mut(), 0);
        if needed < 2 {
            String::new()
        } else {
            let mut buffer = vec![0u16; needed as usize / 2];
            bindings.FPDFAnnot_GetStringValue(annot, key, buffer.as_mut_ptr(), needed);
            String::from_utf16_lossy(&buffer)
        }
    };
    parse_default_appearance(text.trim_end_matches('\0'))
}

fn parse_default_appearance(da: &str) -> DefaultAppearance {
    let tokens: Vec<&str> = da.split_whitespace().collect();
    let numbers = |end: usize, count: usize| -> Option<Vec<f32>> {
        let start = end.checked_sub(count)?;
        tokens[start..end].iter().map(|t| t.parse().ok()).collect()
    };
    let mut out = DefaultAppearance {
        size: None,
        color: (0.0, 0.0, 0.0),
    };
    for (i, token) in tokens.iter().enumerate() {
        match *token {
            "Tf" => out.size = numbers(i, 1).map(|n| n[0]),
            "g" => {
                if let Some(n) = numbers(i, 1) {
                    out.color = (n[0], n[0], n[0]);
                }
            }
            "rg" => {
                if let Some(n) = numbers(i, 3) {
                    out.color = (n[0], n[1], n[2]);
                }
            }
            "k" => {
                if let Some(n) = numbers(i, 4) {
                    let k = 1.0 - n[3];
                    out.color = ((1.0 - n[0]) * k, (1.0 - n[1]) * k, (1.0 - n[2]) * k);
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_size_and_colour_from_da() {
        let da = parse_default_appearance("/Helv 12 Tf 0.1 0.2 0.3 rg");
        assert_eq!(da.size, Some(12.0));
        assert_eq!(da.color, (0.1, 0.2, 0.3));
        let auto = parse_default_appearance("0 g /Arial 0 Tf");
        assert_eq!(auto.size, Some(0.0));
        let none = parse_default_appearance("");
        assert_eq!((none.size, none.color), (None, (0.0, 0.0, 0.0)));
    }

    #[test]
    fn only_right_to_left_letters_need_a_redraw() {
        assert!(needs_redraw("تجربة"));
        assert!(needs_redraw("Rivet ريفت 123"));
        assert!(!needs_redraw("Rivet 123"));
        assert!(!needs_redraw(""));
    }
}
