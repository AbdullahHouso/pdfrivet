//! Laying out a text box: shaping, right-to-left order and line breaks.
//!
//! Text in a PDF is drawn glyph by glyph at positions the writer chose, so
//! everything a word processor does has to happen here:
//! 1. **Direction.** Each paragraph (line of the text) gets its direction from
//!    its first strong letter, unless the box forces right-to-left or
//!    left-to-right (Unicode bidi algorithm, `unicode-bidi`).
//! 2. **Fonts.** Each letter uses the chosen font, or a bundled one that has it.
//! 3. **Shaping** (HarfRust, the HarfBuzz project's Rust port): Arabic letters take their
//!    joined forms, لا becomes one ligature, marks sit on their letters.
//! 4. **Line breaks** at Unicode break opportunities (`unicode-linebreak`) when
//!    the box has a width; a word longer than the box is split.
//! 5. Each line is **shaped again on its own**, so the letter before a break
//!    takes its final form, then its pieces are put in visual order.
//!
//! The result is glyphs at positions in the box (points, from its top-left
//! corner, y downwards), drawn as outlines by [`Layout::to_pdf_path`].

use std::ops::Range;
use std::sync::Arc;

use harfrust::{Buffer, Direction, ShapeOptions};
use serde::{Deserialize, Serialize};
use skrifa::instance::Size;
use skrifa::outline::OutlinePen;
use skrifa::{GlyphId, MetadataProvider};
use ts_rs::TS;
use unicode_bidi::{BidiInfo, Level};

use crate::fonts::{self, LoadedFont, TextFont};

/// Line height, as a multiple of the font size (the editor uses the same).
pub const LINE_HEIGHT: f32 = 1.3;

/// Space between a text box's edge and its text, in points (the editor uses the same).
pub const TEXT_PADDING: f32 = 2.0;

/// A text box's size in points, padding included.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct TextBoxSize {
    pub width: f32,
    pub height: f32,
}

/// The size a text box needs for `text` (its set width and height, or what the text takes).
pub fn measure(text: &str, style: &TextStyle) -> TextBoxSize {
    let l = layout(text, style);
    TextBoxSize {
        width: l.width + 2.0 * TEXT_PADDING,
        height: l.height + 2.0 * TEXT_PADDING,
    }
}

/// Where lines sit across the box.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename_all = "lowercase")]
pub enum TextAlign {
    /// On each paragraph's start side: right for Arabic, left for English.
    #[default]
    Auto,
    Left,
    Center,
    Right,
}

/// Where the text sits in a box taller than it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename_all = "lowercase")]
pub enum VerticalAlign {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// The text's base direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, rename_all = "lowercase")]
pub enum TextDirection {
    /// Each paragraph from its first strong letter.
    #[default]
    Auto,
    Rtl,
    Ltr,
}

/// How a text box's text looks and where it goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct TextStyle {
    #[serde(default)]
    pub font: TextFont,
    /// Font size in points.
    pub size: f32,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub align: TextAlign,
    #[serde(default)]
    pub valign: VerticalAlign,
    #[serde(default)]
    pub direction: TextDirection,
    /// Lines wrap at this width (points); `None`: the box grows with the text.
    #[serde(default)]
    pub width: Option<f32>,
    /// The box's height (points) when it's taller than the text; `None`: it fits the text.
    #[serde(default)]
    pub height: Option<f32>,
}

/// One glyph placed in the box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlacedGlyph {
    /// Index into [`Layout::fonts`].
    pub font: usize,
    pub glyph: u32,
    /// Left edge of the glyph's origin and its baseline, in points from the
    /// box's top-left corner (y downwards).
    pub x: f32,
    pub y: f32,
}

/// Laid-out text.
pub(crate) struct Layout {
    pub fonts: Vec<Arc<LoadedFont>>,
    pub glyphs: Vec<PlacedGlyph>,
    /// The box's size in points.
    pub width: f32,
    pub height: f32,
    pub size: f32,
    /// Each line's width, top to bottom.
    #[cfg(test)]
    pub line_widths: Vec<f32>,
    /// Whether the first paragraph runs right to left (a box that grows with
    /// its text grows leftwards then).
    pub rtl: bool,
}

/// A piece of one line in one font and direction, in logical order.
struct Piece {
    range: Range<usize>,
    font: usize,
    rtl: bool,
}

struct Shaped {
    glyph: u32,
    cluster: usize,
    advance: f32,
    dx: f32,
    dy: f32,
}

/// Lays out `text` in `style`. Newlines start new paragraphs.
pub(crate) fn layout(text: &str, style: &TextStyle) -> Layout {
    let size = style.size.max(1.0);
    let primary = fonts::load(&style.font, style.bold);
    let mut all = vec![primary];
    all.extend(fonts::fallbacks(style.bold));
    let line_height = size * LINE_HEIGHT;
    let forced = match style.direction {
        TextDirection::Auto => None,
        TextDirection::Rtl => Some(Level::rtl()),
        TextDirection::Ltr => Some(Level::ltr()),
    };
    // Baseline inside a line: centred like CSS does, from the main font's metrics.
    let baseline = {
        let font = &all[0];
        let metrics = font.font_ref().metrics(Size::unscaled(), &font.location());
        let s = size / font.units_per_em();
        let ascent = metrics.ascent * s;
        let descent = -metrics.descent * s;
        (line_height - (ascent + descent)) / 2.0 + ascent
    };

    // Lines: (glyphs placed from x = 0, width, right-to-left paragraph).
    let mut lines: Vec<(Vec<PlacedGlyph>, f32, bool)> = Vec::new();
    let mut first_rtl = None;
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    for para in normalized.split('\n') {
        let bidi = BidiInfo::new(para, forced);
        let base_rtl = bidi
            .paragraphs
            .first()
            .map_or(forced.is_some_and(|l| l.is_rtl()), |p| p.level.is_rtl());
        first_rtl.get_or_insert(base_rtl);
        if para.is_empty() {
            lines.push((Vec::new(), 0.0, base_rtl));
            continue;
        }
        let font_of = choose_fonts(para, &all);
        let breaks = match style.width {
            Some(width) => break_lines(para, &bidi, &font_of, &all, size, width.max(size)),
            None => std::iter::once(0..para.len()).collect(),
        };
        for range in breaks {
            let range = trim_end(para, range);
            let (glyphs, width) = shape_line(para, range, &bidi, &font_of, &all, size);
            lines.push((glyphs, width, base_rtl));
        }
    }

    let text_width = lines.iter().map(|l| l.1).fold(0.0, f32::max);
    let width = style.width.unwrap_or(text_width);
    let text_height = line_height * lines.len().max(1) as f32;
    let height = style.height.map_or(text_height, |h| h.max(text_height));
    let top = match style.valign {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Middle => (height - text_height) / 2.0,
        VerticalAlign::Bottom => height - text_height,
    };
    let mut glyphs = Vec::new();
    #[cfg(test)]
    let mut line_widths = Vec::new();
    for (i, (line, line_width, rtl)) in lines.into_iter().enumerate() {
        let free = width - line_width;
        let x = match style.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => free / 2.0,
            TextAlign::Right => free,
            TextAlign::Auto if rtl => free,
            TextAlign::Auto => 0.0,
        };
        let y = top + i as f32 * line_height + baseline;
        glyphs.extend(line.into_iter().map(|g| PlacedGlyph {
            x: g.x + x,
            y: g.y + y,
            ..g
        }));
        #[cfg(test)]
        line_widths.push(line_width);
    }
    Layout {
        fonts: all,
        glyphs,
        width,
        height,
        size,
        #[cfg(test)]
        line_widths,
        rtl: first_rtl.unwrap_or(false),
    }
}

/// The font for each character (by byte index): the first font that has it.
/// Spaces and combining marks stay in the font of the letter before them.
fn choose_fonts(text: &str, fonts: &[Arc<LoadedFont>]) -> Vec<usize> {
    let mut out = vec![0; text.len()];
    let mut previous = 0;
    for (i, c) in text.char_indices() {
        let follows = c.is_whitespace() || is_mark(c) || c.is_control();
        let font = if follows {
            previous
        } else {
            (0..fonts.len())
                .find(|&f| fonts[f].has_char(c))
                .unwrap_or(0)
        };
        out[i..i + c.len_utf8()].fill(font);
        previous = font;
    }
    out
}

/// Combining marks (Arabic harakat, accents) and joiners.
fn is_mark(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F | 0x0610..=0x061A | 0x064B..=0x065F | 0x0670 | 0x06D6..=0x06ED
        | 0x08D3..=0x08FF | 0x200C | 0x200D | 0xFE20..=0xFE2F)
}

/// Splits a range into pieces of one font and one direction (logical order).
fn pieces(text: &str, range: Range<usize>, levels: &[Level], font_of: &[usize]) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    for (i, c) in text[range.clone()].char_indices() {
        let at = range.start + i;
        let (font, rtl) = (font_of[at], levels[at].is_rtl());
        match out.last_mut() {
            Some(p) if p.font == font && p.rtl == rtl => p.range.end = at + c.len_utf8(),
            _ => out.push(Piece {
                range: at..at + c.len_utf8(),
                font,
                rtl,
            }),
        }
    }
    out
}

/// Shapes `text[range]` in one font and direction, with the letters around
/// it (inside `context`) so joining is right at the edges.
fn shape(
    text: &str,
    range: Range<usize>,
    context: Range<usize>,
    rtl: bool,
    font: &LoadedFont,
    size: f32,
) -> Vec<Shaped> {
    let scale = size / font.units_per_em();
    let mut buffer = Buffer::new();
    buffer.push_str(&text[range.clone()]);
    buffer.set_pre_context(&text[context.start..range.start]);
    buffer.set_post_context(&text[range.end..context.end]);
    buffer.guess_segment_properties();
    buffer.set_direction(if rtl {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    });
    if harfrust::shape(&font.shaper(), &mut buffer, ShapeOptions::new()).is_err() {
        return Vec::new();
    }
    buffer
        .glyph_infos()
        .iter()
        .zip(buffer.glyph_positions())
        .map(|(info, pos)| Shaped {
            glyph: info.glyph_id,
            cluster: range.start + info.cluster as usize,
            advance: pos.x_advance as f32 * scale,
            dx: pos.x_offset as f32 * scale,
            dy: pos.y_offset as f32 * scale,
        })
        .collect()
}

/// Width of each character, by byte index (a cluster's width goes to its first byte).
fn advances(
    text: &str,
    bidi: &BidiInfo,
    font_of: &[usize],
    fonts: &[Arc<LoadedFont>],
    size: f32,
) -> Vec<f32> {
    let mut widths = vec![0.0; text.len() + 1];
    for piece in pieces(text, 0..text.len(), &bidi.levels, font_of) {
        for g in shape(
            text,
            piece.range.clone(),
            0..text.len(),
            piece.rtl,
            &fonts[piece.font],
            size,
        ) {
            widths[g.cluster] += g.advance;
        }
    }
    widths
}

/// Where lines end when they must fit in `max` points.
fn break_lines(
    text: &str,
    bidi: &BidiInfo,
    font_of: &[usize],
    fonts: &[Arc<LoadedFont>],
    size: f32,
    max: f32,
) -> Vec<Range<usize>> {
    let widths = advances(text, bidi, font_of, fonts, size);
    let mut prefix = vec![0.0f32; text.len() + 1];
    for i in 0..text.len() {
        prefix[i + 1] = prefix[i] + widths[i];
    }
    let measure = |r: Range<usize>| {
        let r = trim_end(text, r);
        prefix[r.end] - prefix[r.start]
    };
    let boundaries: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();

    let mut lines = Vec::new();
    let mut start = 0;
    let mut last_fit: Option<usize> = None;
    for (at, _) in unicode_linebreak::linebreaks(text) {
        if at <= start {
            continue;
        }
        if measure(start..at) <= max + 0.01 {
            last_fit = Some(at);
            continue;
        }
        // This piece doesn't fit: break at the last place that did.
        if let Some(fit) = last_fit.take() {
            lines.push(start..fit);
            start = fit;
        }
        // A word longer than the line: split it where it overflows.
        while measure(start..at) > max + 0.01 {
            let split = boundaries
                .iter()
                .copied()
                .filter(|&b| b > start && b < at)
                .take_while(|&b| measure(start..b) <= max + 0.01)
                .last();
            let Some(split) = split else { break };
            lines.push(start..split);
            start = split;
        }
        last_fit = Some(at);
    }
    lines.push(start..text.len());
    lines
}

/// Leaves out spaces at the end of a line (they don't take room at a break).
fn trim_end(text: &str, r: Range<usize>) -> Range<usize> {
    let trimmed = text[r.clone()].trim_end().len();
    r.start..r.start + trimmed
}

/// Shapes one line on its own and puts its pieces in visual order. Returns
/// glyphs placed from x = 0 on a baseline at y = 0, and the line's width.
fn shape_line(
    text: &str,
    line: Range<usize>,
    bidi: &BidiInfo,
    font_of: &[usize],
    fonts: &[Arc<LoadedFont>],
    size: f32,
) -> (Vec<PlacedGlyph>, f32) {
    if line.is_empty() {
        return (Vec::new(), 0.0);
    }
    let Some(para) = bidi.paragraphs.first() else {
        return (Vec::new(), 0.0);
    };
    let (levels, runs) = bidi.visual_runs(para, line.clone());
    let mut x = 0.0;
    let mut out = Vec::new();
    for run in runs {
        let mut parts = pieces(text, run.clone(), &levels, font_of);
        if levels[run.start].is_rtl() {
            parts.reverse();
        }
        for part in parts {
            for g in shape(
                text,
                part.range.clone(),
                line.clone(),
                part.rtl,
                &fonts[part.font],
                size,
            ) {
                out.push(PlacedGlyph {
                    font: part.font,
                    glyph: g.glyph,
                    x: x + g.dx,
                    y: -g.dy,
                });
                x += g.advance;
            }
        }
    }
    (out, x)
}

impl Layout {
    /// The glyphs' outlines as a PDF path, for a box whose top-left corner is
    /// at (`left`, `top`) in PDF points (y upwards). Fill it with `f`.
    pub(crate) fn to_pdf_path(&self, left: f32, top: f32) -> String {
        let mut out = String::new();
        // Each font's outlines and weight, read once.
        let fonts: Vec<_> = self
            .fonts
            .iter()
            .map(|f| {
                (
                    f.font_ref().outline_glyphs(),
                    f.location(),
                    f.units_per_em(),
                )
            })
            .collect();
        for g in &self.glyphs {
            let (outlines, location, upem) = &fonts[g.font];
            let Some(glyph) = outlines.get(GlyphId::new(g.glyph)) else {
                continue;
            };
            let mut pen = PathWriter {
                out: &mut out,
                x: left + g.x,
                y: top - g.y,
                scale: self.size / upem,
                last: (0.0, 0.0),
            };
            let _ = glyph.draw((Size::unscaled(), location), &mut pen);
        }
        out
    }

    /// The text needs thickened outlines (bold asked for, no bold face).
    pub(crate) fn fake_bold(&self) -> bool {
        self.fonts.first().is_some_and(|f| f.fake_bold)
    }
}

/// Writes a glyph outline as PDF path operators.
struct PathWriter<'a> {
    out: &'a mut String,
    x: f32,
    y: f32,
    scale: f32,
    /// The current point, in font units.
    last: (f32, f32),
}

impl PathWriter<'_> {
    fn point(&mut self, x: f32, y: f32) {
        use std::fmt::Write;
        let _ = write!(
            self.out,
            "{:.2} {:.2} ",
            self.x + x * self.scale,
            self.y + y * self.scale
        );
    }
}

impl OutlinePen for PathWriter<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.point(x, y);
        self.out.push_str("m ");
        self.last = (x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.point(x, y);
        self.out.push_str("l ");
        self.last = (x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        // PDF has only cubic curves: raise the quadratic one.
        let (x0, y0) = self.last;
        let c1 = (x0 + (x1 - x0) * 2.0 / 3.0, y0 + (y1 - y0) * 2.0 / 3.0);
        let c2 = (x + (x1 - x) * 2.0 / 3.0, y + (y1 - y) * 2.0 / 3.0);
        self.curve_to(c1.0, c1.1, c2.0, c2.1, x, y);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.point(x1, y1);
        self.point(x2, y2);
        self.point(x, y);
        self.out.push_str("c ");
        self.last = (x, y);
    }

    fn close(&mut self) {
        self.out.push_str("h ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(family: &str) -> TextStyle {
        TextStyle {
            font: TextFont {
                family: family.into(),
                bundled: true,
            },
            size: 20.0,
            bold: false,
            align: TextAlign::Auto,
            valign: VerticalAlign::Top,
            direction: TextDirection::Auto,
            width: None,
            height: None,
        }
    }

    fn ids(l: &Layout) -> Vec<u32> {
        l.glyphs.iter().map(|g| g.glyph).collect()
    }

    fn glyph_of(l: &Layout, c: char) -> u32 {
        l.fonts[0].font_ref().charmap().map(c).unwrap().to_u32()
    }

    #[test]
    fn arabic_letters_join() {
        for family in ["Rubik", "Amiri"] {
            let s = style(family);
            let alone = layout("ب", &s);
            let joined = layout("ببب", &s);
            let isolated = ids(&alone)[0];
            let forms = ids(&joined);
            assert_eq!(forms.len(), 3, "{family}");
            assert!(
                forms.iter().all(|&g| g != isolated && g != 0),
                "{family}: {forms:?}"
            );
            // Visual order: the end of the word (final form) is on the left.
            assert_ne!(forms[0], forms[2], "{family}");
            // لا is shaped as a pair (one ligature in Rubik, two special forms in Amiri),
            // not as the two letters on their own.
            let lam_alef = ids(&layout("لا", &s));
            let apart = [ids(&layout("ل", &s))[0], ids(&layout("ا", &s))[0]];
            assert!(
                !lam_alef.iter().any(|g| apart.contains(g)),
                "{family}: {lam_alef:?}"
            );
        }
    }

    #[test]
    fn mixed_text_follows_the_paragraph_direction() {
        let mut s = style("Rubik");
        let text = "مرحبا PDF 2026";
        let auto = layout(text, &s);
        assert!(auto.rtl);
        let p = glyph_of(&auto, 'P');
        let x_of = |l: &Layout, g: u32| l.glyphs.iter().find(|x| x.glyph == g).unwrap().x;
        let arabic_min = |l: &Layout| {
            l.glyphs
                .iter()
                .filter(|g| g.glyph != p && x_of(l, p) != g.x)
                .map(|g| g.x)
                .fold(f32::MAX, f32::min)
        };
        // Right to left: "PDF 2026" comes before (left of) the Arabic word.
        let arabic_right = auto.glyphs.iter().map(|g| g.x).fold(0.0, f32::max);
        assert!(x_of(&auto, p) < arabic_right);
        assert!(x_of(&auto, glyph_of(&auto, '2')) > x_of(&auto, p));
        // Forced left to right: the Arabic word comes first, on the left.
        s.direction = TextDirection::Ltr;
        let ltr = layout(text, &s);
        assert!(!ltr.rtl);
        assert!(arabic_min(&ltr) < x_of(&ltr, glyph_of(&ltr, 'P')));
        assert!(x_of(&ltr, glyph_of(&ltr, 'P')) > ltr.width / 3.0);
    }

    #[test]
    fn wraps_and_reshapes_each_line() {
        let mut s = style("Amiri");
        let word = layout("بببب", &s);
        s.width = Some(word.width * 1.2);
        let two = layout("بببب بببب", &s);
        assert_eq!(two.line_widths.len(), 2);
        // Each line is the word on its own: same glyphs as shaping it alone.
        assert_eq!(ids(&two)[..4], ids(&word)[..]);
        assert!(
            two.line_widths
                .iter()
                .all(|&w| w <= word.width * 1.2 + 0.01)
        );
        // Longer than a line: split.
        s.width = Some(30.0);
        let long = layout("abcdefghijklmnop", &s);
        assert!(long.line_widths.len() > 2);
        assert!(long.line_widths.iter().all(|&w| w <= 30.01));
    }

    #[test]
    fn places_lines_across_and_down_the_box() {
        let mut s = style("Rubik");
        s.width = Some(300.0);
        s.height = Some(200.0);
        let left = layout("abc", &s);
        let first_x = |l: &Layout| l.glyphs.iter().map(|g| g.x).fold(f32::MAX, f32::min);
        assert!(first_x(&left) < 1.0);
        // Arabic starts on the right on its own.
        let arabic = layout("مرحبا", &s);
        assert!(first_x(&arabic) > 200.0);
        s.align = TextAlign::Center;
        let centred = layout("abc", &s);
        assert!((first_x(&centred) - (300.0 - centred.line_widths[0]) / 2.0).abs() < 1.0);
        s.align = TextAlign::Right;
        assert!((first_x(&layout("abc", &s)) - (300.0 - left.line_widths[0])).abs() < 1.0);
        // Vertically: the baseline moves down by the free space (or half of it).
        let top_y = left.glyphs[0].y;
        s.valign = VerticalAlign::Middle;
        let free = 200.0 - 20.0 * LINE_HEIGHT;
        assert!((layout("abc", &s).glyphs[0].y - top_y - free / 2.0).abs() < 0.5);
        s.valign = VerticalAlign::Bottom;
        assert!((layout("abc", &s).glyphs[0].y - top_y - free).abs() < 0.5);
        assert_eq!(layout("abc", &s).height, 200.0);
    }

    #[test]
    fn grows_with_the_text_and_draws_outlines() {
        let s = style("Rubik");
        let one = layout("Hello", &s);
        let two = layout("Hello\nWorld, longer", &s);
        assert!(two.width > one.width);
        assert!((two.height - 2.0 * 20.0 * LINE_HEIGHT).abs() < 0.01);
        let path = one.to_pdf_path(100.0, 700.0);
        assert!(path.contains(" m ") && path.contains(" c ") || path.contains(" l "));
        // An unknown font falls back to a bundled one, so text still draws.
        let mut missing = s.clone();
        missing.font = TextFont {
            family: "No Such Font".into(),
            bundled: false,
        };
        assert!(
            layout("مرحبا Hello", &missing)
                .glyphs
                .iter()
                .all(|g| g.glyph != 0)
        );
    }
}
