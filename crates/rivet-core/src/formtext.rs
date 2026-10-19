//! Drawing the text of a form field ourselves when it holds right-to-left text.
//!
//! PDFium writes a text field's appearance one glyph at a time (a `Td` and a
//! `Tj` per glyph), moving by advances that don't match the widths of the
//! glyphs it draws. Arabic letters join only when neighbouring glyphs touch
//! exactly, so the gaps this leaves split words into separate letters, at
//! every zoom and in every reader.
//!
//! So when PDFium has drawn such a value, the appearance is rebuilt: the
//! background and border PDFium drew are kept, and the text is replaced by
//! the same shaped outlines page text boxes use (`textlayout`). PDFium's own
//! drawing tells how the field wants its text: its font (when that font has
//! the letters), size and colour, alignment (`/Q`) and rotation (`/MK /R`).
//!
//! PDFium draws fields again on its own: after a value changes (every widget
//! of the field, on any page), and, in forms marked `/NeedAppearances`, every
//! time a page is loaded. [`fix_page`] runs before a page is drawn, so the
//! fields are always shown our way.

#![allow(unsafe_code)]

use std::sync::Arc;

use pdfium_render::prelude::*;

use crate::document::RawBindings;
use crate::fonts::{self, LoadedFont};
use crate::textlayout::{
    self, LINE_HEIGHT, Layout, TextAlign, TextDirection, TextStyle, VerticalAlign,
};

/// Space between a field's edge and its text, in points.
const PADDING: f32 = 2.0;
/// The size PDFium and Acrobat give "auto" text at most, in points.
const AUTO_SIZE_MAX: f32 = 12.0;
const AUTO_SIZE_MIN: f32 = 4.0;

// Values from PDFium's public headers, which pdfium-render doesn't re-export.
const FPDF_ANNOT_APPEARANCEMODE_NORMAL: i32 = 0;
const FPDF_ANNOT_WIDGET: i32 = 20;
const FPDF_FORMFIELD_TEXTFIELD: i32 = 6;
const FPDF_PAGEOBJ_TEXT: i32 = 1;
const FPDF_PAGEOBJ_PATH: i32 = 2;
const FPDF_SEGMENT_LINETO: i32 = 0;
const FPDF_SEGMENT_BEZIERTO: i32 = 1;
const FPDF_SEGMENT_MOVETO: i32 = 2;
// Text field flags (PDF 32000-1, table 228).
const FLAG_MULTILINE: i32 = 1 << 12;
const FLAG_PASSWORD: i32 = 1 << 13;
/// Each character in its own box: PDFium already places them one by one, as
/// such a field asks.
const FLAG_COMB: i32 = 1 << 24;

/// Whether a field's value needs drawing by us: it has letters of a
/// right-to-left script.
pub(crate) fn needs_redraw(value: &str) -> bool {
    crate::direction::has_rtl_letter(value)
}

/// What PDFium doesn't tell about a form, read from the file (see
/// `Document::form_facts`).
#[derive(Debug, Default)]
pub(crate) struct FormFacts {
    /// The form asks readers to draw its fields (`/NeedAppearances`).
    pub need_appearances: bool,
    /// Widgets turned by `/MK /R`: their rectangle (left, bottom, right, top)
    /// and how far they're turned (90, 180 or 270 degrees).
    pub turned: Vec<([f32; 4], u16)>,
}

impl FormFacts {
    /// How far the widget with rectangle `rect` is turned.
    fn rotation(&self, rect: &FS_RECTF) -> u16 {
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        self.turned
            .iter()
            .find(|(r, _)| {
                near(r[0], rect.left)
                    && near(r[1], rect.bottom)
                    && near(r[2], rect.right)
                    && near(r[3], rect.top)
            })
            .map_or(0, |&(_, rotation)| rotation)
    }
}

/// A field's value as drawn by [`fix_page`], to add as invisible text when
/// the field is flattened (outlines can't be searched or copied).
pub(crate) struct DrawnText {
    /// From the field's text frame (points, y upwards) to page space.
    pub matrix: [f32; 6],
    pub size: f32,
    pub glyphs: Vec<DrawnGlyph>,
}

/// One glyph of a [`DrawnText`]: its origin in the text frame, how far it
/// advances, and the characters it shows (empty for the rest of a cluster).
pub(crate) struct DrawnGlyph {
    pub x: f32,
    pub y: f32,
    pub advance: f32,
    pub text: String,
}

/// Draws, on `page`, the right-to-left values of text fields that PDFium has
/// drawn (and that `wanted` picks, by full name) our way. Returns what was drawn.
///
/// # Safety
/// `form` and `page` must be live handles of `bindings`' library, used on the
/// PDFium thread; `page` belongs to `form`'s document.
pub(crate) unsafe fn fix_page<'f>(
    bindings: &dyn PdfiumLibraryBindings,
    form: FPDF_FORMHANDLE,
    page: FPDF_PAGE,
    wanted: &mut dyn FnMut(&str) -> bool,
    facts: &dyn Fn() -> &'f FormFacts,
) -> Vec<DrawnText> {
    let mut drawn = Vec::new();
    // SAFETY: valid handles (see above). Loading the page into the form
    // engine is what makes PDFium draw `/NeedAppearances` fields, so it
    // happens here, before we look; drawing the page reuses it.
    unsafe {
        bindings.FORM_OnAfterLoadPage(page, form);
        for i in 0..bindings.FPDFPage_GetAnnotCount(page) {
            let annot = bindings.FPDFPage_GetAnnot(page, i);
            if annot.is_null() {
                continue;
            }
            if let Some((value, flags)) = candidate(bindings, form, annot)
                && has_text(bindings, annot)
                && wanted(&field_name(bindings, form, annot))
                && let Some(theirs) = pdfium_appearance(bindings, annot, facts())
                && let Some(text) = redraw(
                    bindings,
                    form,
                    annot,
                    &value,
                    flags & FLAG_MULTILINE != 0,
                    &theirs,
                )
            {
                drawn.push(text);
            }
            bindings.FPDFPage_CloseAnnot(annot);
        }
    }
    drawn
}

/// [`fix_page`] for `page` of a document whose form is `form`.
pub(crate) fn prepare_page<'f>(
    pdfium: &Pdfium,
    form: &PdfForm,
    page: &PdfPage,
    wanted: &mut dyn FnMut(&str) -> bool,
    facts: &dyn Fn() -> &'f FormFacts,
) -> Vec<DrawnText> {
    let bindings = pdfium.bindings();
    // SAFETY: both handles come from objects the caller keeps alive for the
    // whole call, of the same document; we're on the PDFium thread.
    unsafe {
        fix_page(
            bindings,
            bindings.get_handle_from_form(form),
            bindings.get_handle_from_page(page),
            wanted,
            facts,
        )
    }
}

/// Has PDFium draw again, its own way, the right-to-left fields on `page`
/// that we drew (so [`fix_page`] can redraw them knowing how they're laid
/// out). Returns their names.
///
/// # Safety
/// See [`fix_page`].
pub(crate) unsafe fn redo_drawn(
    bindings: &dyn PdfiumLibraryBindings,
    form: FPDF_FORMHANDLE,
    page: FPDF_PAGE,
) -> Vec<String> {
    let mut names = Vec::new();
    // SAFETY: see `fix_page`. A field without an appearance is drawn by
    // PDFium when the page is loaded into the form engine again.
    unsafe {
        bindings.FORM_OnBeforeClosePage(page, form);
        for i in 0..bindings.FPDFPage_GetAnnotCount(page) {
            let annot = bindings.FPDFPage_GetAnnot(page, i);
            if annot.is_null() {
                continue;
            }
            if candidate(bindings, form, annot).is_some() && !has_text(bindings, annot) {
                names.push(field_name(bindings, form, annot));
                bindings.FPDFAnnot_SetAP(annot, FPDF_ANNOT_APPEARANCEMODE_NORMAL, std::ptr::null());
            }
            bindings.FPDFPage_CloseAnnot(annot);
        }
        bindings.FORM_OnAfterLoadPage(page, form);
    }
    names
}

/// A text field with a right-to-left value we can draw: its value and flags.
///
/// # Safety
/// `annot` must be an open annotation of a page of `form`'s document.
unsafe fn candidate(
    bindings: &dyn PdfiumLibraryBindings,
    form: FPDF_FORMHANDLE,
    annot: FPDF_ANNOTATION,
) -> Option<(String, i32)> {
    // SAFETY: as above.
    unsafe {
        if bindings.FPDFAnnot_GetSubtype(annot) != FPDF_ANNOT_WIDGET
            || bindings.FPDFAnnot_GetFormFieldType(form, annot) != FPDF_FORMFIELD_TEXTFIELD
        {
            return None;
        }
        let flags = bindings.FPDFAnnot_GetFormFieldFlags(form, annot);
        if flags & (FLAG_PASSWORD | FLAG_COMB) != 0 {
            return None;
        }
        let value = wide_string(|buffer, length| {
            bindings.FPDFAnnot_GetFormFieldValue(form, annot, buffer, length as _) as usize
        });
        needs_redraw(&value).then_some((value, flags))
    }
}

/// The field's fully qualified name.
///
/// # Safety
/// See [`candidate`].
pub(crate) unsafe fn field_name(
    bindings: &dyn PdfiumLibraryBindings,
    form: FPDF_FORMHANDLE,
    annot: FPDF_ANNOTATION,
) -> String {
    // SAFETY: as above; the helper sizes the buffer from PDFium's answer.
    wide_string(|buffer, length| unsafe {
        bindings.FPDFAnnot_GetFormFieldName(form, annot, buffer, length as _) as usize
    })
}

/// Reads a UTF-16 string from a PDFium call that answers with its length in
/// bytes (terminating NUL included) and fills a buffer of that size.
fn wide_string(mut call: impl FnMut(*mut u16, usize) -> usize) -> String {
    let needed = call(std::ptr::null_mut(), 0);
    if needed < 2 {
        return String::new();
    }
    let mut buffer = vec![0u16; needed / 2];
    call(buffer.as_mut_ptr(), needed);
    String::from_utf16_lossy(&buffer)
        .trim_end_matches('\0')
        .to_owned()
}

/// Whether the appearance of `annot` has any text in it.
///
/// # Safety
/// See [`candidate`].
unsafe fn has_text(bindings: &dyn PdfiumLibraryBindings, annot: FPDF_ANNOTATION) -> bool {
    // SAFETY: objects belong to the annotation's appearance and are only read.
    unsafe {
        (0..bindings.FPDFAnnot_GetObjectCount(annot)).any(|i| {
            let object = bindings.FPDFAnnot_GetObject(annot, i);
            !object.is_null() && bindings.FPDFPageObj_GetType(object) == FPDF_PAGEOBJ_TEXT
        })
    }
}

/// What PDFium's appearance of a field holds, in the field's text frame:
/// PDFium draws a turned field unturned, and turns the whole drawing.
struct PdfiumAppearance {
    /// The background and border, as content.
    chrome: String,
    /// The fonts its text is in, in order.
    fonts: Vec<FPDF_FONT>,
    /// The box around its text: left, bottom, right, top.
    text_bounds: (f32, f32, f32, f32),
    /// How far the field is turned (`/MK /R`): 0, 90, 180 or 270 degrees.
    rotation: u16,
    /// The text frame's width and height (the field's, swapped when it's
    /// turned a quarter).
    frame: (f32, f32),
}

/// Reads PDFium's appearance of a field. `None` when there's no text in it
/// (nothing to replace, or it's ours already), or when it holds something
/// this can't carry over (pictures, shadings, drawing outside the field).
///
/// # Safety
/// See [`candidate`].
unsafe fn pdfium_appearance(
    bindings: &dyn PdfiumLibraryBindings,
    annot: FPDF_ANNOTATION,
    facts: &FormFacts,
) -> Option<PdfiumAppearance> {
    // SAFETY: see above.
    let rect = unsafe { rect_of(bindings, annot) }?;
    let rotation = facts.rotation(&rect);
    let (width, height) = (rect.right - rect.left, rect.top - rect.bottom);
    let frame = if rotation % 180 == 90 {
        (height, width)
    } else {
        (width, height)
    };
    let mut out = PdfiumAppearance {
        chrome: String::new(),
        fonts: Vec::new(),
        text_bounds: (f32::MAX, f32::MAX, f32::MIN, f32::MIN),
        rotation,
        frame,
    };
    let mut any_text = false;
    // SAFETY: objects belong to the annotation's appearance and stay valid
    // while the annotation handle is open; they are only read.
    unsafe {
        for i in 0..bindings.FPDFAnnot_GetObjectCount(annot) {
            let object = bindings.FPDFAnnot_GetObject(annot, i);
            if object.is_null() {
                return None;
            }
            match bindings.FPDFPageObj_GetType(object) {
                FPDF_PAGEOBJ_TEXT => {
                    any_text = true;
                    let font = bindings.FPDFTextObj_GetFont(object);
                    if !font.is_null() && !out.fonts.contains(&font) {
                        out.fonts.push(font);
                    }
                    let (mut l, mut b, mut r, mut t) = (0.0, 0.0, 0.0, 0.0);
                    if bindings.FPDFPageObj_GetBounds(object, &mut l, &mut b, &mut r, &mut t) != 0 {
                        let bounds = &mut out.text_bounds;
                        *bounds = (
                            bounds.0.min(l),
                            bounds.1.min(b),
                            bounds.2.max(r),
                            bounds.3.max(t),
                        );
                    }
                }
                FPDF_PAGEOBJ_PATH => out.chrome.push_str(&path_content(bindings, object, frame)?),
                _ => return None,
            }
        }
    }
    any_text.then_some(out)
}

/// The field's rectangle on the page.
///
/// # Safety
/// See [`candidate`].
unsafe fn rect_of(
    bindings: &dyn PdfiumLibraryBindings,
    annot: FPDF_ANNOTATION,
) -> Option<FS_RECTF> {
    let mut rect = FS_RECTF {
        left: 0.0,
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
    };
    // SAFETY: `rect` is a plain struct the call fills in.
    (unsafe { bindings.FPDFAnnot_GetRect(annot, &mut rect) } != 0).then_some(rect)
}

fn identity() -> FS_MATRIX {
    FS_MATRIX {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    }
}

/// From a field's text frame (`width` × `height` after turning) to the field's
/// own space, for a field turned by `rotation` (as PDFium turns its appearance).
fn frame_matrix(rotation: u16, width: f32, height: f32) -> [f32; 6] {
    match rotation {
        90 => [0.0, 1.0, -1.0, 0.0, width, 0.0],
        180 => [-1.0, 0.0, 0.0, -1.0, width, height],
        270 => [0.0, -1.0, 1.0, 0.0, 0.0, height],
        _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    }
}

/// Replaces the text in the appearance of the text field `annot` with shaped
/// outlines of `value`, keeping the rest of PDFium's appearance `theirs`.
///
/// # Safety
/// See [`candidate`].
unsafe fn redraw(
    bindings: &dyn PdfiumLibraryBindings,
    form: FPDF_FORMHANDLE,
    annot: FPDF_ANNOTATION,
    value: &str,
    multiline: bool,
    theirs: &PdfiumAppearance,
) -> Option<DrawnText> {
    // SAFETY: see above.
    let rect = unsafe { rect_of(bindings, annot) }?;
    let (width, height) = (rect.right - rect.left, rect.top - rect.bottom);
    let rotation = theirs.rotation;
    let (fw, fh) = theirs.frame;
    if fw <= 2.0 * PADDING || fh <= 2.0 * PADDING {
        return None;
    }
    let (mut size, mut color) = (0.0f32, (0u32, 0u32, 0u32));
    // SAFETY: plain out-parameters. Both read the field's /DA, inherited from
    // its parents or the form when the widget has none.
    unsafe {
        bindings.FPDFAnnot_GetFontSize(form, annot, &mut size);
        if bindings.FPDFAnnot_GetFontColor(form, annot, &mut color.0, &mut color.1, &mut color.2)
            == 0
        {
            color = (0, 0, 0);
        }
    }
    let align = alignment(theirs, fw);
    // SAFETY: the fonts come from the annotation's appearance, still open.
    let font = unsafe { field_font(bindings, &theirs.fonts, value) };
    let layout = fit(value, font, size, multiline, align, fw, fh);

    // Where the layout's top-left corner goes in the text frame. One line
    // grows with its text, so it is placed by hand.
    let x = if multiline {
        PADDING
    } else {
        match align {
            TextAlign::Left => PADDING,
            TextAlign::Right => fw - PADDING - layout.width,
            TextAlign::Center => (fw - layout.width) / 2.0,
            TextAlign::Auto if layout.rtl => fw - PADDING - layout.width,
            TextAlign::Auto => PADDING,
        }
    };
    let top = fh - PADDING;
    let path = layout.to_pdf_path(x, top);
    let m = frame_matrix(rotation, width, height);
    let c = |v: u32| v as f32 / 255.0;
    // Into the text frame (as PDFium's appearance is drawn), then the
    // background and border it drew, then the text, kept inside the field.
    let content = format!(
        "q 1 0 0 1 {:.3} {:.3} cm {} {} {} {} {:.3} {:.3} cm {}q 1 1 {:.3} {:.3} re W n \
         {:.3} {:.3} {:.3} rg {path}f Q Q",
        rect.left,
        rect.bottom,
        m[0],
        m[1],
        m[2],
        m[3],
        m[4],
        m[5],
        theirs.chrome,
        fw - 2.0,
        fh - 2.0,
        c(color.0),
        c(color.1),
        c(color.2),
    );
    let wide: Vec<u16> = content.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    if unsafe { bindings.FPDFAnnot_SetAP(annot, FPDF_ANNOT_APPEARANCEMODE_NORMAL, wide.as_ptr()) }
        == 0
    {
        return None;
    }
    Some(DrawnText {
        matrix: [m[0], m[1], m[2], m[3], m[4] + rect.left, m[5] + rect.bottom],
        size: layout.size,
        glyphs: drawn_glyphs(&layout, x, top),
    })
}

/// Lays out a field's value: at the field's size, or (size 0, "auto") as big
/// as fits, up to 12 points like PDFium and Acrobat.
fn fit(
    value: &str,
    font: Option<Arc<LoadedFont>>,
    size: f32,
    multiline: bool,
    align: TextAlign,
    fw: f32,
    fh: f32,
) -> Layout {
    let (inner_w, inner_h) = (fw - 2.0 * PADDING, fh - 2.0 * PADDING);
    let lay = |size: f32| {
        let style = TextStyle {
            font: fonts::TextFont::default(),
            size,
            bold: false,
            align: if multiline { align } else { TextAlign::Left },
            valign: if multiline {
                VerticalAlign::Top
            } else {
                VerticalAlign::Middle
            },
            direction: TextDirection::Auto,
            width: multiline.then_some(inner_w),
            height: Some(inner_h),
        };
        match &font {
            Some(font) => textlayout::layout_in(value, &style, font.clone()),
            None => textlayout::layout(value, &style),
        }
    };
    if size > 0.0 {
        return lay(size);
    }
    if !multiline {
        let size = (inner_h / LINE_HEIGHT).clamp(AUTO_SIZE_MIN, AUTO_SIZE_MAX);
        let layout = lay(size);
        if layout.width <= inner_w {
            return layout;
        }
        // Too long for the field: smaller, until it fits.
        return lay((size * inner_w / layout.width).max(AUTO_SIZE_MIN));
    }
    let mut size = AUTO_SIZE_MAX;
    loop {
        let layout = lay(size);
        if layout.height <= inner_h + 0.01 || size <= AUTO_SIZE_MIN {
            return layout;
        }
        size = (size * 0.85).max(AUTO_SIZE_MIN);
    }
}

/// How PDFium aligned the text (it follows the field's `/Q`): centred, or
/// against the right edge. Otherwise each paragraph goes on its own start side
/// (right for Arabic): `/Q 0`, left, is the default most forms never change.
fn alignment(theirs: &PdfiumAppearance, fw: f32) -> TextAlign {
    let (l, _, r, _) = theirs.text_bounds;
    if l > r {
        return TextAlign::Auto;
    }
    let (left, right) = (l, fw - r);
    if left < 0.0 || right < 0.0 {
        return TextAlign::Auto;
    }
    let tolerance = 1.5f32.max(0.05 * fw);
    if (left - right).abs() <= tolerance && left > PADDING + 1.0 {
        TextAlign::Center
    } else if left > right + tolerance {
        TextAlign::Right
    } else {
        TextAlign::Auto
    }
}

/// The font to draw a value in: the field's own font (or the one PDFium chose
/// for its letters) when it has every right-to-left letter of the value;
/// `None` for PDFRivet's fonts. Letters it lacks come from the fallbacks.
///
/// # Safety
/// `fonts` must be valid font handles of `bindings`' library.
unsafe fn field_font(
    bindings: &dyn PdfiumLibraryBindings,
    fonts: &[FPDF_FONT],
    value: &str,
) -> Option<Arc<LoadedFont>> {
    let letters: Vec<char> = value
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect();
    let mut best: Option<(usize, Arc<LoadedFont>)> = None;
    for &font in fonts {
        // SAFETY: as above.
        let Some(loaded) = (unsafe { load_font(bindings, font) }) else {
            continue;
        };
        let rtl_covered = letters
            .iter()
            .filter(|&&c| crate::direction::is_rtl_letter(c))
            .all(|&c| loaded.has_char(c));
        if !rtl_covered {
            continue;
        }
        let covered = letters.iter().filter(|&&c| loaded.has_char(c)).count();
        if best.as_ref().is_none_or(|(n, _)| covered > *n) {
            best = Some((covered, loaded));
        }
    }
    best.map(|(_, font)| font)
}

/// A PDF font as one we can shape with: its embedded program (TrueType or
/// OpenType), or the installed font of its family.
///
/// # Safety
/// `font` must be a valid font handle of `bindings`' library.
unsafe fn load_font(
    bindings: &dyn PdfiumLibraryBindings,
    font: FPDF_FONT,
) -> Option<Arc<LoadedFont>> {
    // SAFETY: buffers are sized by PDFium's own answers; calls only write into them.
    unsafe {
        if bindings.FPDFFont_GetIsEmbedded(font) != 0 {
            let mut length = 0;
            if bindings.FPDFFont_GetFontData(font, std::ptr::null_mut(), 0, &mut length) == 0
                || length == 0
            {
                return None;
            }
            let mut data = vec![0u8; length];
            if bindings.FPDFFont_GetFontData(font, data.as_mut_ptr(), length, &mut length) == 0 {
                return None;
            }
            data.truncate(length);
            return fonts::from_bytes(data);
        }
        let length = bindings.FPDFFont_GetFamilyName(font, std::ptr::null_mut(), 0);
        if length < 2 {
            return None;
        }
        let mut name = vec![0u8; length];
        bindings.FPDFFont_GetFamilyName(font, name.as_mut_ptr().cast(), length);
        let family = String::from_utf8_lossy(&name)
            .trim_end_matches('\0')
            .to_owned();
        installed(&family)
    }
}

/// The installed font for a family, or for the standard PDF fonts, one that
/// looks like it.
fn installed(family: &str) -> Option<Arc<LoadedFont>> {
    let alikes: &[&str] = match family.to_lowercase().as_str() {
        "helvetica" | "helv" => &["Arial", "Liberation Sans"],
        "times" | "times-roman" | "times roman" => &["Times New Roman", "Liberation Serif"],
        "courier" | "cour" => &["Courier New", "Liberation Mono"],
        _ => &[],
    };
    fonts::find_installed(family).or_else(|| alikes.iter().find_map(|f| fonts::find_installed(f)))
}

/// The layout's glyphs in the text frame, each with the characters it shows.
fn drawn_glyphs(layout: &Layout, left: f32, top: f32) -> Vec<DrawnGlyph> {
    let mut starts: Vec<usize> = layout.glyphs.iter().map(|g| g.cluster).collect();
    starts.sort_unstable();
    starts.dedup();
    let mut given = vec![false; starts.len()];
    layout
        .glyphs
        .iter()
        .map(|g| {
            let at = starts.binary_search(&g.cluster).unwrap_or(0);
            let text = if given[at] {
                String::new()
            } else {
                given[at] = true;
                let end = starts.get(at + 1).copied().unwrap_or(layout.text.len());
                layout
                    .text
                    .get(g.cluster..end)
                    .unwrap_or_default()
                    .replace('\n', "")
            };
            DrawnGlyph {
                x: left + g.x,
                y: top - g.y,
                advance: g.advance,
                text,
            }
        })
        .collect()
}

/// One path object as content, in the field's text frame (`size`). `None`
/// when it draws outside the field (it isn't the field's own drawing).
///
/// # Safety
/// `object` must be a valid path object of `bindings`' library.
unsafe fn path_content(
    bindings: &dyn PdfiumLibraryBindings,
    object: FPDF_PAGEOBJECT,
    size: (f32, f32),
) -> Option<String> {
    let mut matrix = identity();
    let (mut fill_mode, mut stroked) = (0, 0);
    let (mut fr, mut fg, mut fb, mut fa) = (0u32, 0u32, 0u32, 0u32);
    let (mut sr, mut sg, mut sb, mut sa) = (0u32, 0u32, 0u32, 0u32);
    let mut stroke_width = 1.0f32;
    // SAFETY: the caller guarantees `object` is valid; all out-parameters are
    // plain locals.
    let (cap, join, dashes, phase) = unsafe {
        // Objects of an appearance are usually unmoved; if there's no matrix, assume so.
        bindings.FPDFPageObj_GetMatrix(object, &mut matrix);
        bindings.FPDFPath_GetDrawMode(object, &mut fill_mode, &mut stroked);
        bindings.FPDFPageObj_GetFillColor(object, &mut fr, &mut fg, &mut fb, &mut fa);
        bindings.FPDFPageObj_GetStrokeColor(object, &mut sr, &mut sg, &mut sb, &mut sa);
        bindings.FPDFPageObj_GetStrokeWidth(object, &mut stroke_width);
        let count = bindings.FPDFPageObj_GetDashCount(object).max(0) as usize;
        let mut dashes = vec![0.0f32; count];
        if count > 0 && bindings.FPDFPageObj_GetDashArray(object, dashes.as_mut_ptr(), count) == 0 {
            dashes.clear();
        }
        let mut phase = 0.0f32;
        bindings.FPDFPageObj_GetDashPhase(object, &mut phase);
        (
            bindings.FPDFPageObj_GetLineCap(object),
            bindings.FPDFPageObj_GetLineJoin(object),
            dashes,
            phase,
        )
    };
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
        inside.then_some((px, py))
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
        if cap >= 0 {
            out.push_str(&format!("{cap} J "));
        }
        if join >= 0 {
            out.push_str(&format!("{join} j "));
        }
        if !dashes.is_empty() {
            let pattern: Vec<String> = dashes.iter().map(|d| format!("{d:.3}")).collect();
            out.push_str(&format!("[{}] {phase:.3} d ", pattern.join(" ")));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_right_to_left_letters_need_a_redraw() {
        assert!(needs_redraw("تجربة"));
        assert!(needs_redraw("Rivet ريفت 123"));
        assert!(!needs_redraw("Rivet 123"));
        assert!(!needs_redraw(""));
    }

    #[test]
    fn turned_frames_land_on_the_field() {
        let (w, h) = (200.0, 20.0);
        for rotation in [0, 90, 180, 270] {
            let (fw, fh) = if rotation % 180 == 90 { (h, w) } else { (w, h) };
            let m = frame_matrix(rotation, w, h);
            let corners = [(0.0, 0.0), (fw, 0.0), (0.0, fh), (fw, fh)]
                .map(|(x, y)| (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]));
            for (x, y) in corners {
                assert!(
                    (-1e-4..=w + 1e-4).contains(&x) && (-1e-4..=h + 1e-4).contains(&y),
                    "{rotation}"
                );
            }
        }
    }

    #[test]
    fn reads_alignment_from_where_pdfium_put_the_text() {
        let at = |l: f32, r: f32| PdfiumAppearance {
            chrome: String::new(),
            fonts: Vec::new(),
            text_bounds: (l, 5.0, r, 15.0),
            rotation: 0,
            frame: (200.0, 20.0),
        };
        assert_eq!(alignment(&at(80.0, 120.0), 200.0), TextAlign::Center);
        assert_eq!(alignment(&at(150.0, 198.0), 200.0), TextAlign::Right);
        assert_eq!(alignment(&at(2.0, 60.0), 200.0), TextAlign::Auto);
        // Text wider than the field (scrolled): nothing to go by.
        assert_eq!(alignment(&at(-30.0, 190.0), 200.0), TextAlign::Auto);
    }
}
