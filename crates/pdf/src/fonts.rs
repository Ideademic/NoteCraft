//! Fonts for the PDF's text: which face draws each run, laid out and embedded from the same bytes.
//!
//! A run asks for a family (its font name, else [`DEFAULT_FONT`]; the Code style asks for
//! [`DEFAULT_MONO`]) in bold and/or italic. [`resolve`] finds a face for that through a
//! [`FontSource`]: the family itself, then metric-compatible alternatives (Calibri → Carlito,
//! Arial → Liberation Sans…), and finally the face egui bundles (Ubuntu Light, or Hack for
//! monospace names), so text always draws. When the face found isn't bold or italic and the run
//! is, the PDF fakes it (an outline stroke, a slant), as the canvas does without a real face.
//!
//! Each requested style becomes one epaint font family, named like the canvas's
//! (`nc:{family}:{b|n}{i|n}`), whose faces are the resolved face, then the bundled faces, then
//! fallbacks for scripts none of those cover. epaint takes each character from the first face of
//! the family that has it; [`FontSet::glyph`] makes the same choice, so the PDF draws every glyph
//! from the face the layout measured. Faces are embedded as subsets by Krilla.
//!
//! The source is swappable: [`SystemFonts`] reads the installed fonts (craft-font-catalog, which
//! the canvas uses too); [`BundledFonts`] only the bundled faces (tests, reproducible output).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use craft_pdf::write::{self, backend::text::Font as PdfFont};
use epaint::text::{FontData, FontDefinitions, FontFamily, FontTweak, Fonts, TextOptions, VariationCoords, ViewportKey};
use skrifa::MetadataProvider as _;

/// Font file bytes, shared without copying (the same type epaint's `FontData` holds).
pub type FontBytes = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// The family text uses when a run names none (OneNote's default).
pub const DEFAULT_FONT: &str = "Calibri";
/// The family of the Code paragraph style.
pub const DEFAULT_MONO: &str = "Consolas";
/// Most distinct styles one export lays out (hostile files can name thousands of fonts):
/// further ones draw with the default font's.
pub const MAX_STYLES: usize = 64;
/// Most fallback faces added for characters the fonts asked for don't have.
const MAX_SCRIPT_FALLBACKS: usize = 4;

/// Names of monospace families: these fall back to the bundled monospace face.
const MONO: [&str; 12] = [
    "consolas",
    "courier new",
    "courier",
    "lucida console",
    "menlo",
    "monaco",
    "cascadia code",
    "cascadia mono",
    "monospace",
    "sf mono",
    "dejavu sans mono",
    "liberation mono",
];

/// Installed families tried for characters the requested fonts lack (other scripts, symbols).
const SCRIPT_FALLBACKS: [&str; 14] = [
    "Noto Sans",
    "DejaVu Sans",
    "Arial Unicode MS",
    "Segoe UI",
    "Segoe UI Symbol",
    "Apple Symbols",
    "Noto Sans Symbols",
    "Noto Sans Symbols 2",
    "Noto Sans CJK SC",
    "PingFang SC",
    "Hiragino Sans",
    "Microsoft YaHei",
    "Noto Sans Arabic",
    "Noto Sans Hebrew",
];

/// One face of a font file.
#[derive(Clone)]
pub struct FaceData {
    /// Identifies the face: two with the same key are the same face.
    pub key: String,
    pub bytes: FontBytes,
    /// The face's index in its file (a collection).
    pub index: u32,
    /// A named instance's axis settings (variable fonts); empty for a static face.
    pub variations: Vec<([u8; 4], f32)>,
    /// The face itself is bold (weight 600 or more) / italic.
    pub bold: bool,
    pub italic: bool,
}

impl std::fmt::Debug for FaceData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaceData")
            .field("key", &self.key)
            .field("index", &self.index)
            .field("bold", &self.bold)
            .field("italic", &self.italic)
            .finish()
    }
}

/// Where faces come from.
pub trait FontSource: Send + Sync {
    /// The face of `family` nearest to bold/italic, or `None` when no such family is available
    /// (or its licence forbids embedding it).
    fn face(&self, family: &str, bold: bool, italic: bool) -> Option<FaceData>;
}

/// The installed fonts and fonts added this session, from craft-font-catalog's process catalog.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemFonts;

impl FontSource for SystemFonts {
    fn face(&self, family: &str, bold: bool, italic: bool) -> Option<FaceData> {
        let catalog = craft_font_catalog::Catalog::global();
        let style = match (bold, italic) {
            (false, false) => "Regular",
            (true, false) => "Bold",
            (false, true) => "Italic",
            (true, true) => "Bold Italic",
        };
        let (info, _) = catalog.resolve(family, style)?;
        // OS/2 fsType: "restricted licence embedding" (bit 1 alone in the low nibble) forbids
        // putting the font in a document; such text falls back to a face that may be embedded.
        if info.fs_type & 0x000F == 0x0002 {
            log::info!("pdf: {} may not be embedded; using another font", info.family);
            return None;
        }
        let data = catalog.load(&info).ok()?;
        let key =
            format!("{}#{}#{:?}", info.path.as_ref().map_or_else(|| info.family.clone(), |p| p.display().to_string()), info.index, info.variations);
        let bytes: FontBytes = Arc::new(data.bytes);
        Some(FaceData { key, bytes, index: data.index, variations: info.variations.clone(), bold: info.weight >= 600.0, italic: info.italic })
    }
}

/// No installed fonts: everything draws with the bundled faces. Reproducible output.
#[derive(Clone, Copy, Debug, Default)]
pub struct BundledFonts;

impl FontSource for BundledFonts {
    fn face(&self, _: &str, _: bool, _: bool) -> Option<FaceData> {
        None
    }
}

/// Whether `family` names a monospace font.
pub fn is_mono(family: &str) -> bool {
    MONO.contains(&family.trim().to_ascii_lowercase().as_str())
}

/// Families that set text in the same widths (or close), tried when `family` isn't installed.
pub fn alternatives(family: &str) -> &'static [&'static str] {
    match family.trim().to_ascii_lowercase().as_str() {
        "calibri" | "calibri light" => &["Carlito"],
        "cambria" => &["Caladea"],
        "arial" | "helvetica" | "helvetica neue" | "arimo" => &["Liberation Sans", "Arimo", "Helvetica", "Arial"],
        "times new roman" | "times" | "tinos" => &["Liberation Serif", "Tinos", "Times New Roman", "Times"],
        "courier new" | "courier" | "cousine" => &["Liberation Mono", "Cousine", "Courier New", "Courier"],
        "consolas" | "menlo" | "monaco" | "cascadia mono" | "cascadia code" => {
            &["Consolas", "Menlo", "Cascadia Mono", "DejaVu Sans Mono", "Liberation Mono", "Courier New"]
        }
        "segoe ui" => &["Selawik", "Helvetica Neue", "Liberation Sans"],
        "georgia" => &["Gelasio"],
        _ => &[],
    }
}

/// The bundled faces: (proportional, monospace).
fn bundled() -> (FaceData, FaceData) {
    let defs = FontDefinitions::default();
    let get = |name: &str, key: &str| {
        let bytes: FontBytes = defs.font_data.get(name).map_or_else(|| Arc::new(Vec::<u8>::new()) as FontBytes, |d| d.font.clone());
        FaceData { key: key.to_owned(), bytes, index: 0, variations: Vec::new(), bold: false, italic: false }
    };
    (get("Ubuntu-Light", "bundled:Ubuntu-Light"), get("Hack", "bundled:Hack"))
}

/// The face for `family` in this style: the family, its alternatives, else `None` (the caller
/// uses the bundled face). The source decides which face of a family is nearest.
pub fn resolve(source: &dyn FontSource, family: &str, bold: bool, italic: bool) -> Option<FaceData> {
    std::iter::once(family).chain(alternatives(family).iter().copied()).find_map(|f| source.face(f, bold, italic))
}

/// A style of text: a family name and bold/italic, as runs ask for it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StyleKey {
    pub family: String,
    pub bold: bool,
    pub italic: bool,
}

impl StyleKey {
    pub fn new(family: &str, bold: bool, italic: bool) -> StyleKey {
        let family: String = family.trim().chars().filter(|c| !c.is_control()).take(64).collect();
        StyleKey { family: if family.is_empty() { DEFAULT_FONT.to_owned() } else { family }, bold, italic }
    }

    /// The epaint family name (the canvas names its families the same way).
    fn epaint_name(&self) -> String {
        format!("nc:{}:{}{}", self.family, if self.bold { 'b' } else { 'n' }, if self.italic { 'i' } else { 'n' })
    }
}

/// A resolved style: its epaint family, its faces in fallback order, and what the PDF fakes.
pub struct Style {
    pub family: FontFamily,
    faces: Vec<usize>,
    /// The face has no bold (italic) but the text is: stroke (slant) the glyphs.
    pub fake_bold: bool,
    pub fake_italic: bool,
}

struct Face {
    data: FaceData,
    gids: HashMap<char, Option<u32>>,
    pdf: Option<PdfFont>,
    shaper: Option<harfrust::ShaperData>,
}

/// One shaped glyph, in ems (font units over units per em); offsets y down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shaped {
    pub gid: u32,
    pub x_advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,
    /// Byte offset of its cluster in the shaped text.
    pub cluster: usize,
}

impl Face {
    /// Opens a face for both layout and embedding; `None` when either can't read it.
    fn open(data: FaceData) -> Option<Face> {
        let bytes = (*data.bytes).as_ref();
        skrifa::FontRef::from_index(bytes, data.index).ok()?;
        let vars = (!data.variations.is_empty()).then_some(data.variations.as_slice());
        let pdf = match write::font(bytes, data.index, vars) {
            Ok(f) => f,
            Err(e) => {
                log::warn!("pdf: font {} can't be embedded: {e}", data.key);
                return None;
            }
        };
        Some(Face { data, gids: HashMap::new(), pdf: Some(pdf), shaper: None })
    }

    /// Shapes `text` with harfrust as epaint does (same direction handling, script guessed).
    fn shape(&mut self, text: &str, rtl: bool) -> Option<Vec<Shaped>> {
        let bytes = (*self.data.bytes).as_ref();
        let font = harfrust::FontRef::from_index(bytes, self.data.index).ok()?;
        let data = self.shaper.get_or_insert_with(|| harfrust::ShaperData::new(&font));
        let instance = (!self.data.variations.is_empty()).then(|| {
            harfrust::ShaperInstance::from_variations(
                &font,
                self.data.variations.iter().map(|(t, v)| harfrust::Variation { tag: harfrust::Tag::new(t), value: *v }),
            )
        });
        let shaper = data.shaper(&font).instance(instance.as_ref()).build();
        let upem = shaper.units_per_em();
        if upem <= 0 {
            return None;
        }
        let upem = upem as f32;
        let mut buffer = harfrust::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(if rtl { harfrust::Direction::RightToLeft } else { harfrust::Direction::LeftToRight });
        buffer.guess_segment_properties();
        let out = shaper.shape(buffer, harfrust::ShapeOptions::new());
        Some(
            out.glyph_infos()
                .iter()
                .zip(out.glyph_positions())
                .map(|(i, p)| Shaped {
                    gid: i.glyph_id,
                    x_advance: p.x_advance as f32 / upem,
                    x_offset: p.x_offset as f32 / upem,
                    y_offset: -(p.y_offset as f32) / upem,
                    cluster: i.cluster as usize,
                })
                .collect(),
        )
    }

    fn gid(&mut self, c: char) -> Option<u32> {
        if let Some(g) = self.gids.get(&c) {
            return *g;
        }
        let g = skrifa::FontRef::from_index((*self.data.bytes).as_ref(), self.data.index)
            .ok()
            .and_then(|f| f.charmap().map(c))
            .map(|g| g.to_u32())
            .filter(|g| *g != 0);
        if self.gids.len() < 1 << 16 {
            self.gids.insert(c, g);
        }
        g
    }
}

/// The fonts of one export: epaint's layout fonts and the faces to embed.
pub struct FontSet {
    faces: Vec<Face>,
    styles: Vec<Style>,
    by_key: HashMap<StyleKey, usize>,
    pub fonts: Fonts,
}

/// Pixels per point epaint lays out at: glyph positions snap to this grid (1/4 pt).
pub const LAYOUT_PPP: f32 = 4.0;

impl FontSet {
    /// Resolves every style in `wanted` (the first [`MAX_STYLES`]) and builds the layout fonts.
    /// `text` is every character the export draws, to find faces for scripts the requested
    /// fonts lack.
    pub fn new(source: &dyn FontSource, wanted: &[StyleKey], text: &HashSet<char>) -> FontSet {
        let (prop, mono) = bundled();
        let mut faces: Vec<Face> = Vec::new();
        let mut index_of: HashMap<String, usize> = HashMap::new();
        let mut add = |faces: &mut Vec<Face>, data: FaceData| -> Option<usize> {
            if let Some(i) = index_of.get(&data.key) {
                return Some(*i);
            }
            let key = data.key.clone();
            let face = Face::open(data)?;
            faces.push(face);
            let i = faces.len() - 1;
            index_of.insert(key, i);
            Some(i)
        };
        let prop_i = add(&mut faces, prop);
        let mono_i = add(&mut faces, mono);
        let mut wanted: Vec<StyleKey> = wanted.to_vec();
        wanted.sort();
        wanted.dedup();
        wanted.truncate(MAX_STYLES);
        let default = StyleKey::new(DEFAULT_FONT, false, false);
        if !wanted.contains(&default) {
            wanted.insert(0, default);
        }
        let mut resolved: Vec<(StyleKey, Option<usize>, bool, bool)> = Vec::new();
        for key in &wanted {
            let face = resolve(source, &key.family, key.bold, key.italic).and_then(|d| {
                let (b, i) = (d.bold, d.italic);
                add(&mut faces, d).map(|f| (f, b, i))
            });
            let (face, b, i) = match face {
                Some((f, b, i)) => (Some(f), b, i),
                None => (None, false, false),
            };
            resolved.push((key.clone(), face, key.bold && !b, key.italic && !i));
        }
        // Faces for characters none of the chosen faces (nor the bundled ones) have.
        let mut extra: Vec<usize> = Vec::new();
        let mut missing: Vec<char> = text.iter().copied().filter(|c| !c.is_control() && !c.is_whitespace()).collect();
        missing.sort_unstable();
        let mut primary: Vec<usize> = resolved.iter().filter_map(|r| r.1).collect();
        primary.extend(prop_i);
        primary.extend(mono_i);
        primary.sort_unstable();
        primary.dedup();
        missing.retain(|c| !primary.iter().any(|&f| faces.get_mut(f).and_then(|face| face.gid(*c)).is_some()));
        if !missing.is_empty() {
            for fam in SCRIPT_FALLBACKS {
                if missing.is_empty() || extra.len() >= MAX_SCRIPT_FALLBACKS {
                    break;
                }
                let Some(data) = source.face(fam, false, false) else { continue };
                let Some(fi) = add(&mut faces, data) else { continue };
                let before = missing.len();
                missing.retain(|c| faces.get_mut(fi).and_then(|face| face.gid(*c)).is_none());
                if missing.len() < before && !extra.contains(&fi) {
                    extra.push(fi);
                }
            }
        }
        let mut defs = FontDefinitions::empty();
        for (i, f) in faces.iter().enumerate() {
            let mut data = FontData::from_blob(f.data.bytes.clone(), f.data.index);
            if !f.data.variations.is_empty() {
                data =
                    data.tweak(FontTweak { coords: VariationCoords::new(f.data.variations.iter().map(|(t, v)| (*t, *v))), ..FontTweak::default() });
            }
            defs.font_data.insert(face_name(i), Arc::new(data));
        }
        let names = |list: &[usize]| list.iter().map(|i| face_name(*i)).collect::<Vec<_>>();
        let chain = |first: Option<usize>, mono: bool| {
            let mut v: Vec<usize> = Vec::new();
            v.extend(first);
            let (a, b) = if mono { (mono_i, prop_i) } else { (prop_i, mono_i) };
            for f in [a, b].into_iter().flatten().chain(extra.iter().copied()) {
                if !v.contains(&f) {
                    v.push(f);
                }
            }
            v
        };
        let prop_chain = chain(None, false);
        let mono_chain = chain(None, true);
        defs.families.insert(FontFamily::Proportional, names(&prop_chain));
        defs.families.insert(FontFamily::Monospace, names(&mono_chain));
        let mut styles = Vec::new();
        let mut by_key = HashMap::new();
        for (key, face, fake_bold, fake_italic) in resolved {
            let list = chain(face, is_mono(&key.family));
            let family = FontFamily::Name(key.epaint_name().into());
            defs.families.insert(family.clone(), names(&list));
            by_key.insert(key, styles.len());
            styles.push(Style { family, faces: list, fake_bold, fake_italic });
        }
        let options = TextOptions { font_hinting: false, subpixel_binning: false, max_texture_side: 4096, ..TextOptions::default() };
        let mut fonts = Fonts::new(options, defs);
        fonts.begin_pass(options, ViewportKey::default());
        FontSet { faces, styles, by_key, fonts }
    }

    /// The style for `key`; the default font's when it wasn't resolved (past [`MAX_STYLES`]).
    pub fn style(&self, key: &StyleKey) -> usize {
        self.by_key.get(key).or_else(|| self.by_key.get(&StyleKey::new(DEFAULT_FONT, key.bold, key.italic))).copied().unwrap_or(0)
    }

    pub fn style_info(&self, style: usize) -> Option<&Style> {
        self.styles.get(style)
    }

    /// The face and glyph that draw `c` in `style`: the first face of its chain that has it,
    /// as epaint chose when it measured the text. `None` when no face has it.
    pub fn glyph(&mut self, style: usize, c: char) -> Option<(usize, u32)> {
        let faces = self.styles.get(style)?.faces.clone();
        faces.into_iter().find_map(|f| self.faces.get_mut(f)?.gid(c).map(|g| (f, g)))
    }

    /// `text` shaped with face `face` (left to right unless `rtl`): the glyphs the canvas draws.
    pub fn shape(&mut self, face: usize, text: &str, rtl: bool) -> Option<Vec<Shaped>> {
        self.faces.get_mut(face)?.shape(text, rtl)
    }

    /// The embedded font of face `face`.
    pub fn pdf_font(&self, face: usize) -> Option<PdfFont> {
        self.faces.get(face)?.pdf.clone()
    }

    /// Clears epaint's glyph atlas between pages when it fills up.
    pub fn next_page(&mut self) {
        let options = TextOptions { font_hinting: false, subpixel_binning: false, max_texture_side: 4096, ..TextOptions::default() };
        self.fonts.begin_pass(options, ViewportKey::default());
    }

    /// The faces in use, for tests and diagnostics: (key, real bold, real italic).
    pub fn face_keys(&self) -> Vec<&str> {
        self.faces.iter().map(|f| f.data.key.as_str()).collect()
    }
}

fn face_name(i: usize) -> String {
    format!("nc-pdf-face-{i}")
}

/// Every family a run of the notes might name, as an ordered set (for [`FontSet::new`]).
pub fn dedup_keys(keys: impl IntoIterator<Item = StyleKey>) -> Vec<StyleKey> {
    let set: BTreeMap<StyleKey, ()> = keys.into_iter().map(|k| (k, ())).collect();
    set.into_keys().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_resolve_to_the_bundled_faces_without_system_fonts() {
        let text: HashSet<char> = "Hello".chars().collect();
        let mut set = FontSet::new(&BundledFonts, &[StyleKey::new("Calibri", true, false), StyleKey::new("Consolas", false, false)], &text);
        let bold = set.style(&StyleKey::new("Calibri", true, false));
        assert!(set.style_info(bold).is_some_and(|s| s.fake_bold && !s.fake_italic));
        let (face, gid) = set.glyph(bold, 'H').expect("a glyph");
        assert!(gid > 0);
        assert!(set.pdf_font(face).is_some());
        let mono = set.style(&StyleKey::new("Consolas", false, false));
        let (mface, _) = set.glyph(mono, 'H').expect("a glyph");
        assert_ne!(face, mface, "monospace names use the monospace face first");
        assert!(set.glyph(bold, '\u{10FFFF}').is_none());
        // An unknown style draws with the default font's.
        assert_eq!(set.style(&StyleKey::new("Nope", false, false)), set.style(&StyleKey::new(DEFAULT_FONT, false, false)));
    }

    #[test]
    fn keys_are_bounded_and_clean() {
        let k = StyleKey::new(&format!("  {}\u{0}x  ", "a".repeat(200)), false, true);
        assert_eq!(k.family.chars().count(), 64);
        assert_eq!(StyleKey::new("  ", false, false).family, DEFAULT_FONT);
        assert!(is_mono("Courier New") && !is_mono("Calibri"));
        assert_eq!(alternatives("CALIBRI"), &["Carlito"]);
    }
}
