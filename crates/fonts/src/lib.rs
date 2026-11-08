//! NoteCraft's fonts, for every crate that draws or writes text (the egui canvas, PDF export):
//!
//! - the **installed fonts**, through [`craft_font_catalog`]: scanned on a background thread, so
//!   nothing waits for them; until the scan is done only the bundled faces are offered;
//! - the **bundled faces** ([`CRAFT_FONTS`]): the metric-compatible OFL substitutes for the Office
//!   fonts notes are usually written in (Carlito for Calibri, Caladea for Cambria, Liberation Sans,
//!   Serif and Mono for Arial, Times New Roman and Courier New), embedded at build time from
//!   [craft-fonts](https://github.com/storytold/craft-fonts) when `CRAFT_FONTS_DIR` is set
//!   (`build.rs`); empty otherwise;
//! - **substitutes** for a font that isn't there: the metric-compatible face first, then a face
//!   of the same kind (sans serif, serif, monospace) that is;
//! - fonts the **user adds** ([`FontLibrary::add_font_file`]): TrueType, OpenType, collections,
//!   WOFF and WOFF2 ([`web_font`]), for the web build, where the browser has no fonts to list.
//!
//! [`FontLibrary::resolve`] turns a run's font name and bold/italic into a face: the family's
//! real Bold, Italic or Bold Italic face when it has one, a variable face set to that weight, or
//! the nearest face with [`Resolved::fake_bold`] / [`Resolved::fake_italic`] telling the caller to
//! draw the missing style itself. Every face handed out parses ([`face_parses`]).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod web_font;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

pub use craft_font_catalog::{Catalog, FaceInfo, FontEnv, Match, Os, Source};

/// A font from the optional craft-fonts build input (empty unless built with `CRAFT_FONTS_DIR`).
#[derive(Clone, Copy, Debug)]
pub struct CraftFont {
    pub family: &'static str,
    pub style: &'static str,
    /// ISO 15924 scripts the font is for, e.g. `"Latn"`.
    pub scripts: &'static [&'static str],
    pub bytes: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/craft_fonts.rs"));

/// The font a run without one is drawn in (OneNote's default).
pub const DEFAULT_FONT: &str = "Calibri";
/// The font of the Code paragraph style.
pub const DEFAULT_MONO_FONT: &str = "Consolas";

/// Shared, immutable font file bytes (egui's `Blob`): an installed file, or bundled bytes
/// borrowed without a copy.
pub type FontBytes = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// Why a font file wasn't added.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum FontError {
    #[error("the font file is empty")]
    Empty,
    #[error("the font file is too large ({} MB; at most {} MB)", .bytes.div_ceil(1 << 20), web_font::MAX_FONT_FILE_BYTES >> 20)]
    TooLarge { bytes: usize },
    #[error("not a font file (TrueType, OpenType, TTC, WOFF or WOFF2)")]
    NotAFont,
    #[error("the web font couldn't be read: {0}")]
    BadWebFont(String),
    #[error("the font can't be used: {0}")]
    Unusable(String),
}

/// A face's data and the name egui and PDF writers key it by.
#[derive(Clone)]
pub struct FaceData {
    /// Unique per file, face index and variation settings, so faces shared by several families
    /// load once.
    pub key: String,
    pub bytes: FontBytes,
    pub index: u32,
    /// Variation axis settings to draw the face at (a named instance, or `wght` for bold from a
    /// variable face); empty for a static face.
    pub coords: Vec<([u8; 4], f32)>,
}

impl FaceData {
    pub fn bytes(&self) -> &[u8] {
        (*self.bytes).as_ref()
    }
}

impl std::fmt::Debug for FaceData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FaceData").field("key", &self.key).field("index", &self.index).field("coords", &self.coords).finish_non_exhaustive()
    }
}

/// How a font name and style resolved.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub face: FaceData,
    /// The family drawn with: the one asked for, or its substitute.
    pub family: String,
    /// Bold was asked for and the face isn't bold: draw it emboldened.
    pub fake_bold: bool,
    /// Italic was asked for and the face isn't italic: slant it.
    pub fake_italic: bool,
    /// The family asked for isn't available; `family` stands in for it.
    pub substituted: bool,
    /// Where the face comes from.
    pub source: Source,
    /// The face (as drawn, variation settings applied) is bold / italic.
    pub face_bold: bool,
    pub face_italic: bool,
    /// The font's licence lets a document embed it (OS/2 `fsType` isn't "restricted").
    pub embeddable: bool,
}

/// The kind of face a family name suggests, for picking a substitute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Sans,
    Serif,
    Mono,
}

/// Font names that are monospaced (lower case).
const MONO_NAMES: &[&str] = &[
    "consolas",
    "courier new",
    "courier",
    "lucida console",
    "lucida sans typewriter",
    "menlo",
    "monaco",
    "cascadia code",
    "cascadia mono",
    "sf mono",
    "dejavu sans mono",
    "liberation mono",
    "cousine",
    "jetbrains mono",
    "source code pro",
    "fira code",
    "fira mono",
    "ubuntu mono",
    "noto sans mono",
    "roboto mono",
    "andale mono",
    "monospace",
];

/// Font names that are serif (lower case).
const SERIF_NAMES: &[&str] = &[
    "cambria",
    "times new roman",
    "times",
    "georgia",
    "garamond",
    "book antiqua",
    "palatino",
    "palatino linotype",
    "constantia",
    "century",
    "century schoolbook",
    "baskerville",
    "liberation serif",
    "tinos",
    "caladea",
    "dejavu serif",
    "noto serif",
    "source serif 4",
    "merriweather",
    "serif",
];

impl Class {
    /// The kind of face `family` names (sans serif when it isn't a known serif or monospace).
    pub fn of(family: &str) -> Class {
        let f = family.trim().to_ascii_lowercase();
        if MONO_NAMES.contains(&f.as_str()) || f.contains(" mono") || f.ends_with("mono") || f.contains("code") {
            Class::Mono
        } else if SERIF_NAMES.contains(&f.as_str()) || (f.contains("serif") && !f.contains("sans")) {
            Class::Serif
        } else {
            Class::Sans
        }
    }
}

/// Families that set text in the same widths (or close), tried first when `family` isn't there:
/// the metric-compatible faces craft-fonts bundles (Calibri → Carlito, Cambria → Caladea, Arial →
/// Liberation Sans…), then their look-alikes on other systems.
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

/// The first metric-compatible stand-in for `family` ([`alternatives`]).
pub fn metric_substitute(family: &str) -> Option<&'static str> {
    alternatives(family).first().copied()
}

/// Every family tried, in order, when `family` itself isn't there: its [`alternatives`], then
/// faces of the same kind ([`Class`]). The canvas and the PDF export both resolve through it, so
/// the screen and the PDF draw with the same face.
pub fn fallback_chain(family: &str) -> Vec<&'static str> {
    let own = family.trim();
    let mut out: Vec<&'static str> = Vec::new();
    for f in alternatives(family).iter().chain(class_fallbacks(Class::of(family))) {
        if !f.eq_ignore_ascii_case(own) && !out.contains(f) {
            out.push(f);
        }
    }
    out
}

/// Faces of the same kind to try when a font and its alternatives are all missing, in order:
/// bundled ones first, then families most systems have.
fn class_fallbacks(class: Class) -> &'static [&'static str] {
    match class {
        Class::Sans => &["Carlito", "Liberation Sans", "Arial", "Helvetica", "Segoe UI", "DejaVu Sans", "Noto Sans", "Ubuntu"],
        Class::Serif => &["Caladea", "Liberation Serif", "Times New Roman", "Times", "Georgia", "DejaVu Serif", "Noto Serif"],
        Class::Mono => &["Liberation Mono", "Consolas", "Menlo", "Courier New", "DejaVu Sans Mono", "Noto Sans Mono", "Ubuntu Mono"],
    }
}

/// Whether `bytes` holds a face at `index` that parses and has a character map: what egui needs
/// before it draws with it (it would refuse, or draw nothing, otherwise).
pub fn face_parses(bytes: &[u8], index: u32) -> bool {
    use skrifa::MetadataProvider as _;
    skrifa::FontRef::from_index(bytes, index).is_ok_and(|font| font.charmap().has_map())
}

/// The range of the face's `wght` axis, if it has one.
fn weight_axis(bytes: &[u8], index: u32) -> Option<(f32, f32)> {
    use skrifa::MetadataProvider as _;
    let font = skrifa::FontRef::from_index(bytes, index).ok()?;
    let axes = font.axes();
    let axis = axes.iter().find(|a| a.tag() == skrifa::Tag::new(b"wght"))?;
    Some((axis.min_value(), axis.max_value()))
}

/// A bundled face, as the manifest names it.
#[derive(Clone, Copy, Debug)]
struct Bundled {
    family: &'static str,
    weight: f32,
    italic: bool,
    bytes: &'static [u8],
}

fn style_bits(style: &str) -> (f32, bool) {
    let s = style.to_ascii_lowercase();
    let weight = if s.contains("bold") { if s.contains("semi") || s.contains("demi") { 600.0 } else { 700.0 } } else { 400.0 };
    (weight, s.contains("italic") || s.contains("oblique"))
}

fn style_name(bold: bool, italic: bool) -> &'static str {
    match (bold, italic) {
        (false, false) => "Regular",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (true, true) => "Bold Italic",
    }
}

/// Weight from which a face counts as bold.
const BOLD_WEIGHT: f32 = 600.0;

/// Most resolutions kept (a document can name any number of fonts).
const MAX_CACHED: usize = 4096;

type CacheKey = (String, bool, bool);

/// The fonts NoteCraft can draw with. One per process ([`FontLibrary::global`]); tests make their
/// own over an empty catalog.
pub struct FontLibrary {
    catalog: Arc<Catalog>,
    bundled: Vec<Bundled>,
    /// Resolutions of the current generation.
    cache: Mutex<(u64, HashMap<CacheKey, Option<Resolved>>)>,
    /// Families offered, of the current generation.
    families: Mutex<Option<(u64, Arc<[String]>)>>,
    /// Polls for installed and removed fonts (desktop) while the library lives.
    watch: Mutex<Option<craft_font_catalog::Watch>>,
    /// Changes not counted by the catalog (the fallback fonts for other scripts becoming ready).
    extra: AtomicU64,
}

impl FontLibrary {
    /// The process's library: the installed fonts ([`Catalog::global`]) and [`CRAFT_FONTS`].
    pub fn global() -> &'static FontLibrary {
        static GLOBAL: OnceLock<FontLibrary> = OnceLock::new();
        GLOBAL.get_or_init(|| FontLibrary::new(Arc::clone(Catalog::global()), CRAFT_FONTS))
    }

    /// A library over `catalog` with these bundled faces (only faces that parse are kept).
    pub fn new(catalog: Arc<Catalog>, bundled: &[CraftFont]) -> FontLibrary {
        let bundled = bundled
            .iter()
            .filter(|f| face_parses(f.bytes, 0))
            .map(|f| {
                let (weight, italic) = style_bits(f.style);
                Bundled { family: f.family, weight, italic, bytes: f.bytes }
            })
            .collect();
        FontLibrary {
            catalog,
            bundled,
            cache: Mutex::new((u64::MAX, HashMap::new())),
            families: Mutex::new(None),
            watch: Mutex::new(None),
            extra: AtomicU64::new(0),
        }
    }

    /// The catalog of installed and added fonts.
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    /// Start reading the installed fonts without waiting: on a background thread on the desktop,
    /// which then looks for fonts installed or removed every few seconds (the generation moves on
    /// when it finds some); on the web (no font folders, no threads) the scan finds nothing and is
    /// done at once.
    pub fn start(&self) {
        if cfg!(target_arch = "wasm32") {
            if !self.catalog.is_ready() {
                self.catalog.rescan();
            }
            return;
        }
        self.catalog.scan_in_background();
        let mut watch = self.watch.lock().unwrap_or_else(PoisonError::into_inner);
        if watch.is_none() {
            *watch = Some(self.catalog.watch(std::time::Duration::from_secs(5), |_| {}));
        }
    }

    /// Read the installed fonts now if they haven't been (waits for a scan already running): for
    /// work that must use them, such as a PDF export from the command line.
    pub fn wait_until_ready(&self) {
        let _ = self.catalog.families();
    }

    /// Whether the installed fonts have been read (until then only bundled faces resolve).
    pub fn is_ready(&self) -> bool {
        self.catalog.is_ready()
    }

    /// Counts changes to what resolves: each scan, each font added, each [`Self::bump`]. A
    /// drawing cache keyed on it stays right.
    pub fn generation(&self) -> u64 {
        self.catalog.generation().wrapping_add(self.extra.load(Ordering::Acquire))
    }

    /// Count a change the catalog doesn't see.
    pub fn bump(&self) {
        self.extra.fetch_add(1, Ordering::AcqRel);
    }

    /// The bundled families, in manifest order.
    pub fn bundled_families(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for b in &self.bundled {
            if !out.contains(&b.family) {
                out.push(b.family);
            }
        }
        out
    }

    /// Every family that can be picked: the installed ones (once read), the added ones and the
    /// bundled ones, sorted ignoring case. Never waits for the scan. Hidden system families
    /// (macOS's `.SF NS`, `LastResort`) are left out.
    pub fn families(&self) -> Arc<[String]> {
        let generation = self.generation();
        let mut slot = self.families.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((g, list)) = slot.as_ref()
            && *g == generation
        {
            return Arc::clone(list);
        }
        let mut all: Vec<String> = self.bundled_families().into_iter().map(str::to_owned).collect();
        if self.catalog.is_ready() {
            all.extend(self.catalog.families().iter().filter(|f| !f.starts_with('.') && !f.is_empty() && f.as_str() != "LastResort").cloned());
        }
        all.sort_by_cached_key(|f| (f.to_lowercase(), f.clone()));
        all.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        let list: Arc<[String]> = all.into();
        *slot = Some((generation, Arc::clone(&list)));
        list
    }

    /// Whether `family` is installed, added or bundled (by any of its names). Never waits.
    pub fn has_family(&self, family: &str) -> bool {
        self.bundled.iter().any(|b| b.family.eq_ignore_ascii_case(family.trim()))
            || (self.catalog.is_ready() && self.catalog.family(family).is_some())
    }

    /// The face to draw `family` with in the asked style: the family's own face (installed,
    /// added or bundled), else its metric substitute, else a face of the same kind; `None` when
    /// nothing fits (draw with the toolkit's own face). Never waits for the scan; may read the
    /// face's file the first time. Cached per generation.
    pub fn resolve(&self, family: &str, bold: bool, italic: bool) -> Option<Resolved> {
        let family = family.trim();
        let generation = self.generation();
        let key: CacheKey = (family.to_lowercase(), bold, italic);
        {
            let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
            if cache.0 != generation {
                *cache = (generation, HashMap::new());
            }
            if let Some(r) = cache.1.get(&key) {
                return r.clone();
            }
        }
        let found = self.resolve_uncached(family, bold, italic);
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if cache.0 == generation {
            if cache.1.len() >= MAX_CACHED {
                cache.1.clear();
            }
            cache.1.insert(key, found.clone());
        }
        found
    }

    fn resolve_uncached(&self, family: &str, bold: bool, italic: bool) -> Option<Resolved> {
        if family.is_empty() {
            return self.resolve_uncached(DEFAULT_FONT, bold, italic);
        }
        if let Some(r) = self.own(family, bold, italic) {
            return Some(r);
        }
        for sub in fallback_chain(family) {
            if let Some(mut r) = self.own(sub, bold, italic) {
                r.substituted = true;
                return Some(r);
            }
        }
        None
    }

    /// `family`'s own face (no substitutes): installed or added first, then bundled. Never waits
    /// for the scan. For callers that walk [`fallback_chain`] themselves (the PDF export, which
    /// also skips faces whose licence forbids embedding).
    pub fn own(&self, family: &str, bold: bool, italic: bool) -> Option<Resolved> {
        if self.catalog.is_ready()
            && let Some(r) = self.installed(family, bold, italic)
        {
            return Some(r);
        }
        self.bundled_face(family, bold, italic)
    }

    fn installed(&self, family: &str, bold: bool, italic: bool) -> Option<Resolved> {
        let (info, _) = self.catalog.resolve(family, style_name(bold, italic))?;
        // A name that is a single face's full or PostScript name ("Arial Bold") brings its own
        // style; the family's other faces are looked up by family name.
        let data = self.catalog.load(&info).ok()?;
        let bytes: FontBytes = Arc::new(data.bytes);
        let raw: &[u8] = (*bytes).as_ref();
        if !face_parses(raw, data.index) {
            log::warn!("font {} {} doesn't parse; skipped", info.family, info.style);
            return None;
        }
        let mut coords = info.variations.clone();
        let mut is_bold = info.weight >= BOLD_WEIGHT;
        if bold
            && !is_bold
            && info.variable
            && let Some((lo, hi)) = weight_axis(raw, data.index)
            && hi >= BOLD_WEIGHT
        {
            // A variable family's bold: its own face at weight 700.
            coords.retain(|(t, _)| t != b"wght");
            coords.push((*b"wght", 700f32.clamp(lo, hi)));
            is_bold = true;
        } else if !bold
            && is_bold
            && info.variable
            && let Some((lo, hi)) = weight_axis(raw, data.index)
            && lo <= 400.0
        {
            coords.retain(|(t, _)| t != b"wght");
            coords.push((*b"wght", 400f32.clamp(lo, hi)));
            is_bold = false;
        }
        let place = match &info.path {
            Some(p) => p.display().to_string(),
            None => format!("added/{}/{}/{}", info.family, info.style, info.postscript_name.as_deref().unwrap_or("")),
        };
        let key = format!("ncf:{place}#{}{}", data.index, coords_key(&coords));
        Some(Resolved {
            face: FaceData { key, bytes, index: data.index, coords },
            family: info.family.clone(),
            fake_bold: bold && !is_bold,
            fake_italic: italic && !info.italic,
            substituted: false,
            source: info.source,
            face_bold: is_bold,
            face_italic: info.italic,
            embeddable: info.fs_type & 0x000F != 0x0002,
        })
    }

    /// `family`'s bundled face nearest to the style, if it is one of the bundled families.
    pub fn bundled_face(&self, family: &str, bold: bool, italic: bool) -> Option<Resolved> {
        let faces: Vec<&Bundled> = self.bundled.iter().filter(|b| b.family.eq_ignore_ascii_case(family)).collect();
        let want = if bold { 700.0 } else { 400.0 };
        let best = faces.iter().min_by(|a, b| {
            let score = |f: &Bundled| (f.weight - want).abs() + if f.italic != italic { 1000.0 } else { 0.0 };
            score(a).total_cmp(&score(b))
        })?;
        let bytes: FontBytes = Arc::new(best.bytes);
        Some(Resolved {
            face: FaceData {
                key: format!("ncf:bundled/{}/{}{}", best.family, best.weight, if best.italic { "i" } else { "" }),
                bytes,
                index: 0,
                coords: Vec::new(),
            },
            family: best.family.to_owned(),
            fake_bold: bold && best.weight < BOLD_WEIGHT,
            fake_italic: italic && !best.italic,
            substituted: false,
            source: Source::Bundled,
            face_bold: best.weight >= BOLD_WEIGHT,
            face_italic: best.italic,
            embeddable: true,
        })
    }

    /// Add a font file the user picked (TTF, OTF, TTC, WOFF, WOFF2) for this session; returns
    /// the families it adds. Waits for the first scan of the installed fonts if it is still
    /// running.
    pub fn add_font_file(&self, bytes: &[u8]) -> Result<Vec<String>, FontError> {
        let sfnt = web_font::decode_font_file(bytes)?;
        let faces =
            self.catalog.add_bytes(sfnt, Source::User, &craft_font_catalog::Limits::default()).map_err(|e| FontError::Unusable(e.to_string()))?;
        let mut families: Vec<String> = Vec::new();
        for f in faces {
            if !families.contains(&f.family) {
                families.push(f.family);
            }
        }
        Ok(families)
    }
}

fn coords_key(coords: &[([u8; 4], f32)]) -> String {
    coords.iter().map(|(t, v)| format!(";{}={v}", String::from_utf8_lossy(t))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real font for tests: egui's Ubuntu Light (family "Ubuntu", style "Light").
    pub(crate) fn test_font() -> &'static [u8] {
        epaint_default_fonts::UBUNTU_LIGHT
    }

    pub(crate) fn empty_library(bundled: &[CraftFont]) -> FontLibrary {
        let catalog = Catalog::new(FontEnv { os: Os::None, ..FontEnv::default() }).with_font_service(false);
        let lib = FontLibrary::new(Arc::new(catalog), bundled);
        lib.catalog().rescan();
        lib
    }

    const BUNDLED: &[CraftFont] = &[
        CraftFont { family: "Testsans", style: "Regular", scripts: &["Latn"], bytes: epaint_default_fonts::UBUNTU_LIGHT },
        CraftFont { family: "Testsans", style: "Bold", scripts: &["Latn"], bytes: epaint_default_fonts::HACK_REGULAR },
        CraftFont { family: "Broken", style: "Regular", scripts: &["Latn"], bytes: b"not a font" },
    ];

    #[test]
    fn names_classify_and_substitute() {
        assert_eq!(Class::of("Consolas"), Class::Mono);
        assert_eq!(Class::of("Fira Code"), Class::Mono);
        assert_eq!(Class::of("Georgia"), Class::Serif);
        assert_eq!(Class::of("PT Serif"), Class::Serif);
        assert_eq!(Class::of("Noto Sans"), Class::Sans);
        assert_eq!(metric_substitute("calibri"), Some("Carlito"));
        assert_eq!(metric_substitute("Times New Roman"), Some("Liberation Serif"));
        assert_eq!(metric_substitute("Wingdings"), None);
        let chain = fallback_chain("Calibri");
        assert_eq!(chain.first(), Some(&"Carlito"));
        assert!(!chain.contains(&"Calibri") && chain.contains(&"Liberation Sans"), "{chain:?}");
        assert_eq!(fallback_chain("Consolas").first().copied(), Some("Menlo"), "the family itself is skipped");
        assert!(fallback_chain("Garamond").contains(&"Caladea"), "a serif falls back to serifs");
        assert_eq!(style_bits("Bold Italic"), (700.0, true));
        assert_eq!(style_bits("SemiBold"), (600.0, false));
    }

    #[test]
    fn bundled_faces_resolve_with_real_and_fake_styles() {
        let lib = empty_library(BUNDLED);
        assert_eq!(lib.bundled_families(), vec!["Testsans"], "the face that doesn't parse is dropped");
        let regular = lib.resolve("Testsans", false, false).unwrap();
        assert!(!regular.fake_bold && !regular.fake_italic && !regular.substituted);
        let bold = lib.resolve("testsans", true, false).unwrap();
        assert!(!bold.fake_bold, "a bold face exists");
        assert_ne!(bold.face.key, regular.face.key);
        let italic = lib.resolve("Testsans", false, true).unwrap();
        assert!(italic.fake_italic, "no italic face: slanted");
        assert_eq!(italic.face.key, regular.face.key);
        // A missing sans font falls back to a sans face that is there.
        assert!(lib.resolve("Nonexistent Sans", false, false).is_none(), "no fallback family is bundled here");
        assert!(lib.families().iter().any(|f| f == "Testsans"));
        assert!(lib.has_family("TESTSANS") && !lib.has_family("Broken"));
    }

    #[test]
    fn added_fonts_resolve_and_bump_the_generation() {
        let lib = empty_library(&[]);
        let before = lib.generation();
        assert!(lib.resolve("Ubuntu", false, false).is_none());
        let families = lib.add_font_file(test_font()).unwrap();
        assert_eq!(families, vec!["Ubuntu".to_owned()]);
        assert_ne!(lib.generation(), before);
        let r = lib.resolve("Ubuntu", false, false).unwrap();
        assert_eq!(r.source, Source::User);
        assert!(face_parses(r.face.bytes(), r.face.index));
        // Light is the only face: bold is drawn emboldened.
        assert!(lib.resolve("Ubuntu", true, false).unwrap().fake_bold);
        assert!(lib.families().iter().any(|f| f == "Ubuntu"));
        // WOFF works the same; junk is refused.
        assert!(lib.add_font_file(&crate::web_font::tests::woff1(test_font())).is_ok());
        assert!(lib.add_font_file(b"junk").is_err());
        // A missing font uses a substitute of its kind when one is there.
        let sub = lib.resolve("Calibri", false, false);
        assert!(sub.is_some_and(|s| s.substituted && s.family == "Ubuntu"), "Ubuntu is in the sans fallbacks");
    }

    #[test]
    fn the_installed_fonts_are_read_without_waiting() {
        // The process's own library: `families` and `resolve` answer at once, scanned or not.
        let lib = FontLibrary::global();
        lib.start();
        let _ = lib.families();
        let _ = lib.resolve(DEFAULT_FONT, true, true);
        let _ = lib.generation();
    }
}
