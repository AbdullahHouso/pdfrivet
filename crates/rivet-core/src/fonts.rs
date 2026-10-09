//! Fonts for text boxes: a few that ship with PDFRivet (the same on every
//! computer) and the ones installed on this computer.
//!
//! Bundled: Rubik (a modern sans, variable weight) and Amiri (a classic
//! naskh, regular and bold). Both cover Arabic and Latin, so they're also the
//! fallback for letters a chosen font doesn't have. System fonts are found
//! with `fontdb` the first time they're needed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use rustybuzz::ttf_parser::Tag;
use serde::{Deserialize, Serialize};
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
}

enum FontData {
    Static(&'static [u8]),
    Owned(Arc<Vec<u8>>),
}

impl LoadedFont {
    fn bytes(&self) -> &[u8] {
        match &self.data {
            FontData::Static(b) => b,
            FontData::Owned(b) => b,
        }
    }

    /// The face to shape and draw with (cheap: tables are read on demand).
    pub(crate) fn face(&self) -> rustybuzz::Face<'_> {
        let mut face = rustybuzz::Face::from_slice(self.bytes(), self.index)
            .or_else(|| rustybuzz::Face::from_slice(RUBIK, 0))
            .expect("the bundled font parses");
        if let Some(weight) = self.weight {
            face.set_variations(&[rustybuzz::Variation {
                tag: Tag::from_bytes(b"wght"),
                value: weight,
            }]);
        }
        face
    }

    pub(crate) fn has_char(&self, c: char) -> bool {
        self.face().glyph_index(c).is_some()
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
    Some(LoadedFont {
        data: FontData::Static(data),
        index: 0,
        weight,
        fake_bold: false,
    })
}

const BUNDLED: [&str; 2] = ["Rubik", "Amiri"];

/// The installed fonts, found the first time they're needed.
fn system() -> &'static fontdb::Database {
    static DB: OnceLock<fontdb::Database> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        db
    })
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
    let db = system();
    let id = db.query(&fontdb::Query {
        families: &[fontdb::Family::Name(family)],
        weight: if bold {
            fontdb::Weight::BOLD
        } else {
            fontdb::Weight::NORMAL
        },
        ..Default::default()
    })?;
    let info = db.face(id)?;
    let data = db.with_face_data(id, |data, _| data.to_vec())?;
    Some(LoadedFont {
        data: FontData::Owned(Arc::new(data)),
        index: info.index,
        weight: None,
        fake_bold: bold && info.weight.0 < 600,
    })
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
                let db = system();
                let mut families: HashMap<String, fontdb::ID> = HashMap::new();
                for face in db.faces() {
                    let Some((name, _)) = face.families.first() else {
                        continue;
                    };
                    if name.is_empty() || name.starts_with('.') {
                        continue;
                    }
                    // Prefer the regular face to check what the family covers.
                    let regular = face.weight == fontdb::Weight::NORMAL
                        && face.style == fontdb::Style::Normal;
                    if regular || !families.contains_key(name) {
                        families.insert(name.clone(), face.id);
                    }
                }
                let mut list: Vec<FontInfo> = families
                    .into_iter()
                    .map(|(family, id)| FontInfo {
                        arabic: db
                            .with_face_data(id, |data, index| {
                                rustybuzz::ttf_parser::Face::parse(data, index)
                                    .is_ok_and(|f| f.glyph_index('\u{0628}').is_some())
                            })
                            .unwrap_or(false),
                        family,
                        bundled: false,
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
