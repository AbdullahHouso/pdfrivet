//! Fonts for text boxes: a few that ship with PDFRivet (the same on every
//! computer) and the ones installed on this computer.
//!
//! Bundled: Rubik (a modern sans, variable weight) and Amiri (a classic
//! naskh, regular and bold). Both cover Arabic and Latin, so they're also the
//! fallback for letters a chosen font doesn't have. Installed fonts are found
//! by scanning the system's font folders the first time they're needed.
//!
//! Fonts are read with skrifa (names, character coverage, outlines) and
//! shaped with HarfRust; both come from the same font-reading crate.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use harfrust::font::{Blob, Variation};
use serde::{Deserialize, Serialize};
use skrifa::instance::Location;
use skrifa::raw::FileRef;
use skrifa::string::StringId;
use skrifa::{FontRef, MetadataProvider, Tag};
use ts_rs::TS;

static RUBIK: &[u8] = include_bytes!("../fonts/Rubik-Variable.ttf");
static AMIRI: &[u8] = include_bytes!("../fonts/Amiri-Regular.ttf");
static AMIRI_BOLD: &[u8] = include_bytes!("../fonts/Amiri-Bold.ttf");

/// The bundled family used when nothing else fits.
pub const DEFAULT_FAMILY: &str = "Rubik";

/// A font chosen for a text box.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct TextFont {
    pub family: String,
    /// One of PDFRivet's own fonts (otherwise installed on the computer).
    #[serde(default)]
    pub bundled: bool,
}

impl Default for TextFont {
    fn default() -> Self {
        Self {
            family: DEFAULT_FAMILY.to_owned(),
            bundled: true,
        }
    }
}

/// A font family that can be chosen, for the font picker.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, rename_all = "camelCase")]
pub struct FontInfo {
    pub family: String,
    pub bundled: bool,
    /// It has Arabic letters.
    pub arabic: bool,
}

/// A loaded font face, ready to shape and draw with.
pub(crate) struct LoadedFont {
    data: FontData,
    index: u32,
    /// The weight to use on a variable font's `wght` axis.
    weight: Option<f32>,
    /// Bold was asked for but the family has no bold face: thicken the outlines.
    pub(crate) fake_bold: bool,
    /// The face at its chosen weight, for shaping.
    shaping: harfrust::Font,
}

#[derive(Clone)]
enum FontData {
    Static(&'static [u8]),
    Owned(Arc<Vec<u8>>),
}

impl LoadedFont {
    fn new(data: FontData, index: u32, weight: Option<f32>, fake_bold: bool) -> Option<Self> {
        let blob: Blob = match &data {
            FontData::Static(b) => (*b).into(),
            FontData::Owned(b) => Blob::from(b.clone() as Arc<dyn AsRef<[u8]> + Send + Sync>),
        };
        let base = harfrust::Font::new(blob, index)?;
        let shaping = match weight {
            Some(w) => base
                .instance_builder()
                .variations([Variation::new(Tag::new(b"wght"), w)])
                .build(),
            None => base,
        };
        let font = Self {
            data,
            index,
            weight,
            fake_bold,
            shaping,
        };
        // Skrifa must read it too (for outlines).
        font.try_font_ref()?;
        Some(font)
    }

    fn bytes(&self) -> &[u8] {
        match &self.data {
            FontData::Static(b) => b,
            FontData::Owned(b) => b,
        }
    }

    fn try_font_ref(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(self.bytes(), self.index).ok()
    }

    /// The face, to read names, coverage, metrics and outlines.
    pub(crate) fn font_ref(&self) -> FontRef<'_> {
        self.try_font_ref().expect("checked when loaded")
    }

    /// Where on its variation axes the face is drawn (its weight).
    pub(crate) fn location(&self) -> Location {
        let font = self.font_ref();
        match self.weight {
            Some(w) => font.axes().location([("wght", w)]),
            None => Location::default(),
        }
    }

    /// The face prepared for HarfRust.
    pub(crate) fn shaper(&self) -> harfrust::ShaperFont<'_, '_> {
        harfrust::ShaperFont::new(&self.shaping)
    }

    pub(crate) fn units_per_em(&self) -> f32 {
        f32::from(self.shaping.units_per_em().max(1))
    }

    pub(crate) fn has_char(&self, c: char) -> bool {
        self.font_ref().charmap().map(c).is_some()
    }
}

/// The bundled font files, by the name the webview asks for (`/font/<name>`).
pub fn bundled_file(name: &str) -> Option<&'static [u8]> {
    match name {
        "rubik" => Some(RUBIK),
        "amiri" => Some(AMIRI),
        "amiri-bold" => Some(AMIRI_BOLD),
        _ => None,
    }
}

fn bundled(family: &str, bold: bool) -> Option<LoadedFont> {
    let (data, weight) = match family {
        "Rubik" => (RUBIK, Some(if bold { 700.0 } else { 400.0 })),
        "Amiri" => (if bold { AMIRI_BOLD } else { AMIRI }, None),
        _ => return None,
    };
    LoadedFont::new(FontData::Static(data), 0, weight, false)
}

const BUNDLED: [&str; 2] = ["Rubik", "Amiri"];

/// One face of an installed font.
struct SystemFace {
    family: String,
    path: PathBuf,
    index: u32,
    weight: f32,
    italic: bool,
    arabic: bool,
}

/// The installed fonts, found the first time they're needed.
fn system() -> &'static [SystemFace] {
    static FACES: OnceLock<Vec<SystemFace>> = OnceLock::new();
    FACES.get_or_init(|| {
        let mut faces = Vec::new();
        for dir in font_dirs() {
            scan_dir(&dir, 0, &mut faces);
        }
        faces
    })
}

/// Where each OS keeps fonts (for everyone, and for the user).
fn font_dirs() -> Vec<PathBuf> {
    let home = |p: &str| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(p));
    let mut dirs: Vec<Option<PathBuf>> = Vec::new();
    if cfg!(windows) {
        dirs.push(
            std::env::var_os("WINDIR")
                .map(|w| PathBuf::from(w).join("Fonts"))
                .or_else(|| Some(PathBuf::from(r"C:\Windows\Fonts"))),
        );
        dirs.push(
            std::env::var_os("LOCALAPPDATA")
                .map(|l| PathBuf::from(l).join(r"Microsoft\Windows\Fonts")),
        );
    } else if cfg!(target_os = "macos") {
        dirs.push(Some("/System/Library/Fonts".into()));
        dirs.push(Some("/Library/Fonts".into()));
        dirs.push(home("Library/Fonts"));
    } else {
        dirs.push(Some("/usr/share/fonts".into()));
        dirs.push(Some("/usr/local/share/fonts".into()));
        dirs.push(home(".local/share/fonts"));
        dirs.push(home(".fonts"));
    }
    dirs.into_iter().flatten().collect()
}

fn scan_dir(dir: &std::path::Path, depth: usize, out: &mut Vec<SystemFace>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if depth < 4 {
                scan_dir(&path, depth + 1, out);
            }
            continue;
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc") {
            scan_file(&path, out);
        }
    }
}

/// Reads the faces of one font file: names, weight, style and Arabic coverage.
fn scan_file(path: &std::path::Path, out: &mut Vec<SystemFace>) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    // SAFETY: the file is only read, for the length of this call. If another
    // program changed it meanwhile, parsing would see odd bytes (and reject
    // them), not crash: skrifa checks every offset it reads.
    #[allow(unsafe_code)]
    let Ok(map) = (unsafe { memmap2::Mmap::map(&file) }) else {
        return;
    };
    let faces: Vec<(u32, FontRef)> = match FileRef::new(&map) {
        Ok(FileRef::Font(f)) => vec![(0, f)],
        Ok(FileRef::Collection(c)) => (0..c.len())
            .filter_map(|i| c.get(i).ok().map(|f| (i, f)))
            .collect(),
        Err(_) => return,
    };
    for (index, font) in faces {
        let name = |id| {
            font.localized_strings(id)
                .english_or_first()
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty())
        };
        let Some(family) =
            name(StringId::TYPOGRAPHIC_FAMILY_NAME).or_else(|| name(StringId::FAMILY_NAME))
        else {
            continue;
        };
        if family.starts_with('.') {
            continue;
        }
        let attributes = font.attributes();
        out.push(SystemFace {
            family,
            path: path.to_owned(),
            index,
            weight: attributes.weight.value(),
            italic: attributes.style != skrifa::attribute::Style::Normal,
            arabic: font.charmap().map('\u{0628}').is_some(),
        });
    }
}

type Key = (TextFont, bool);
fn cache() -> &'static Mutex<HashMap<Key, Arc<LoadedFont>>> {
    static CACHE: OnceLock<Mutex<HashMap<Key, Arc<LoadedFont>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Loads a font (bold if asked). A font that can't be found is replaced by
/// the default bundled font, so text can always be drawn.
pub(crate) fn load(font: &TextFont, bold: bool) -> Arc<LoadedFont> {
    let key = (font.clone(), bold);
    if let Some(found) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return found;
    }
    let loaded = Arc::new(
        if font.bundled {
            bundled(&font.family, bold)
        } else {
            load_system(&font.family, bold)
        }
        .or_else(|| bundled(DEFAULT_FAMILY, bold))
        .expect("the default font is bundled"),
    );
    if let Ok(mut c) = cache().lock() {
        c.insert(key, loaded.clone());
    }
    loaded
}

fn load_system(family: &str, bold: bool) -> Option<LoadedFont> {
    let target = if bold { 700.0 } else { 400.0 };
    let face = system()
        .iter()
        .filter(|f| f.family.eq_ignore_ascii_case(family))
        .min_by(|a, b| {
            let score =
                |f: &SystemFace| (f.weight - target).abs() + if f.italic { 1000.0 } else { 0.0 };
            score(a).total_cmp(&score(b))
        })?;
    let data = std::fs::read(&face.path).ok()?;
    LoadedFont::new(
        FontData::Owned(Arc::new(data)),
        face.index,
        None,
        bold && face.weight < 600.0,
    )
}

/// Fonts to fall back on for letters the chosen one lacks.
pub(crate) fn fallbacks(bold: bool) -> Vec<Arc<LoadedFont>> {
    BUNDLED
        .iter()
        .map(|family| {
            load(
                &TextFont {
                    family: (*family).to_owned(),
                    bundled: true,
                },
                bold,
            )
        })
        .collect()
}

/// Every font that can be chosen: the bundled ones first, then the installed
/// ones by name. Hidden system fonts (names starting with a dot) are left out.
pub fn list() -> Vec<FontInfo> {
    static SYSTEM_LIST: OnceLock<Vec<FontInfo>> = OnceLock::new();
    let mut fonts: Vec<FontInfo> = BUNDLED
        .iter()
        .map(|family| FontInfo {
            family: (*family).to_owned(),
            bundled: true,
            arabic: true,
        })
        .collect();
    fonts.extend(
        SYSTEM_LIST
            .get_or_init(|| {
                // One entry per family; it has Arabic if its regular face (or any face) does.
                let mut families: HashMap<String, bool> = HashMap::new();
                for face in system() {
                    *families.entry(face.family.clone()).or_default() |= face.arabic;
                }
                let mut list: Vec<FontInfo> = families
                    .into_iter()
                    .map(|(family, arabic)| FontInfo {
                        family,
                        bundled: false,
                        arabic,
                    })
                    .collect();
                list.sort_by_key(|f| f.family.to_lowercase());
                list
            })
            .iter()
            .cloned(),
    );
    fonts
}
