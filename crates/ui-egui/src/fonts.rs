//! Real fonts in egui: one [`FontProvider`] over [`notecraft_fonts::FontLibrary`] (the installed
//! fonts, the bundled metric-compatible faces, fonts the user added).
//!
//! A run's font is the egui family `nc:{family}:{b|n}{i|n}@{generation}` ([`family_id`]): the
//! provider resolves it to the family's real Regular, Bold, Italic or Bold Italic face (or its
//! substitute), followed by egui's own faces for characters it lacks, and on the desktop the
//! system's fonts for other scripts. The generation changes when the installed fonts have been
//! read, a font is added, or the fallback fonts become ready, so egui asks again for families it
//! has already built. Nothing here waits: until the installed fonts are read, families resolve to
//! the bundled faces.
//!
//! A family named before the provider is active in a context would draw nothing, so callers ask
//! [`active`] first and draw with egui's own families until it is.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use egui::epaint::text::VariationCoords;
use egui::{FallbackRequest, FontData, FontDefinitions, FontFamily, FontInsert, FontProvider};
use notecraft_fonts::{Class, FontLibrary, Resolved};

pub use notecraft_fonts::{DEFAULT_FONT, DEFAULT_MONO_FONT};

/// Prefix of the families the provider answers.
pub const FAMILY_PREFIX: &str = "nc:";

/// The library every family resolves through.
pub fn library() -> &'static FontLibrary {
    FontLibrary::global()
}

/// The generation the family names carry now.
pub fn generation() -> u64 {
    library().generation()
}

/// The egui family that draws `family` in this style with its real face.
pub fn family_id(family: &str, bold: bool, italic: bool) -> FontFamily {
    FontFamily::Name(family_name(family, bold, italic, generation()).into())
}

fn family_name(family: &str, bold: bool, italic: bool, generation: u64) -> String {
    let family: String = family.trim().chars().filter(|c| !c.is_control()).take(128).collect();
    format!("{FAMILY_PREFIX}{family}:{}{}@{generation}", if bold { 'b' } else { 'n' }, if italic { 'i' } else { 'n' })
}

/// `nc:{family}:{b|n}{i|n}@{generation}` → (family, bold, italic).
pub fn parse_family(name: &str) -> Option<(&str, bool, bool)> {
    let rest = name.strip_prefix(FAMILY_PREFIX)?;
    let (rest, _generation) = rest.rsplit_once('@')?;
    let (family, style) = rest.rsplit_once(':')?;
    let mut s = style.chars();
    let bold = match s.next()? {
        'b' => true,
        'n' => false,
        _ => return None,
    };
    let italic = match s.next()? {
        'i' => true,
        'n' => false,
        _ => return None,
    };
    (s.next().is_none() && !family.is_empty()).then_some((family, bold, italic))
}

/// How a run is drawn: the egui family, and what the face lacks (drawn by the caller: a second
/// pass nudged sideways for bold, egui's slant for italic).
#[derive(Clone, Debug, PartialEq)]
pub struct Face {
    pub family: FontFamily,
    pub fake_bold: bool,
    pub fake_italic: bool,
    /// The family that draws it (`None`: egui's own face).
    pub drawn_with: Option<String>,
}

/// The face for `family` in this style. With `named` false (the provider isn't active yet), egui's
/// proportional or monospace face with every style faked.
pub fn face(family: &str, bold: bool, italic: bool, named: bool) -> Face {
    if !named {
        let family = if Class::of(family) == Class::Mono { FontFamily::Monospace } else { FontFamily::Proportional };
        return Face { family, fake_bold: bold, fake_italic: italic, drawn_with: None };
    }
    let r = library().resolve(family, bold, italic);
    Face {
        family: family_id(family, bold, italic),
        fake_bold: r.as_ref().map_or(bold, |r| r.fake_bold),
        fake_italic: r.as_ref().map_or(italic, |r| r.fake_italic),
        drawn_with: r.map(|r| r.family),
    }
}

/// The fonts egui uses after the resolved face: its own proportional (or monospace) faces.
struct Defaults {
    proportional: Vec<FontInsert>,
    monospace: Vec<FontInsert>,
}

impl Defaults {
    fn new() -> Defaults {
        let defs = FontDefinitions::default();
        let chain = |family: &FontFamily| -> Vec<FontInsert> {
            defs.families
                .get(family)
                .into_iter()
                .flatten()
                .filter_map(|name| Some(FontInsert::new(name, (**defs.font_data.get(name)?).clone(), Vec::new())))
                .collect()
        };
        Defaults { proportional: chain(&FontFamily::Proportional), monospace: chain(&FontFamily::Monospace) }
    }
}

/// The installed fonts as egui fonts: answers the `nc:` families, and on the desktop finds a
/// system font for characters no face of a family has (other scripts), once that index is built.
pub struct Provider {
    lib: &'static FontLibrary,
    defaults: Defaults,
    fallback: Option<Arc<dyn FontProvider>>,
    fallback_ready: Arc<AtomicBool>,
}

impl Provider {
    fn new(lib: &'static FontLibrary) -> Provider {
        Provider { lib, defaults: Defaults::new(), fallback: None, fallback_ready: Arc::new(AtomicBool::new(false)) }
    }
}

/// egui font data for a resolved face.
fn insert_for(r: &Resolved) -> FontInsert {
    let mut data = FontData::from_blob(Arc::clone(&r.face.bytes), r.face.index);
    if !r.face.coords.is_empty() {
        // Tags as numbers: a hostile font's axis tag can't be a malformed tag here.
        data.tweak.coords = VariationCoords::new(r.face.coords.iter().map(|(t, v)| (u32::from_be_bytes(*t), *v)));
    }
    FontInsert::new(&r.face.key, data, Vec::new())
}

impl FontProvider for Provider {
    fn fonts_for_family(&self, family: &FontFamily) -> Vec<FontInsert> {
        let FontFamily::Name(name) = family else { return Vec::new() };
        let Some((family, bold, italic)) = parse_family(name) else { return Vec::new() };
        let mut out = Vec::new();
        let resolved = self.lib.resolve(family, bold, italic);
        if let Some(r) = &resolved {
            out.push(insert_for(r));
        }
        let mono = Class::of(resolved.as_ref().map_or(family, |r| r.family.as_str())) == Class::Mono;
        out.extend(if mono { self.defaults.monospace.iter() } else { self.defaults.proportional.iter() }.cloned());
        out
    }

    fn font_for(&self, request: &FallbackRequest<'_>) -> Option<FontInsert> {
        // Before the system index is built a lookup would wait for it: miss instead (the
        // generation changes when it is ready, so the canvas's families ask again).
        if !self.fallback_ready.load(Ordering::Acquire) {
            return None;
        }
        self.fallback.as_ref()?.font_for(request)
    }
}

/// The system fonts for other scripts (desktop): built on a background thread; the provider
/// uses them once built and the generation moves on.
fn start_fallback(p: &mut Provider) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(fallback) = craft_ui::fonts::system_font_provider("en") {
        p.fallback = Some(Arc::clone(&fallback));
        let ready = Arc::clone(&p.fallback_ready);
        let lib = p.lib;
        let spawned = std::thread::Builder::new().name("nc-font-fallback".into()).spawn(move || {
            // The first lookup waits for the index; later ones don't.
            let _ = fallback.font_for(&FallbackRequest { cluster: "\u{4E2D}", family: &FontFamily::Proportional });
            ready.store(true, Ordering::Release);
            lib.bump();
        });
        if spawned.is_err() {
            p.fallback = None;
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = p;
}

#[derive(Clone, Copy)]
struct Installed {
    /// The pass the provider was added in; it is active from the next one.
    pass: u64,
    generation: u64,
}

fn installed_id() -> egui::Id {
    egui::Id::unique("nc_fonts_installed")
}

/// Install the provider in `ctx` (once), start reading the installed fonts, and repaint when what
/// resolves has changed. Call every frame, before drawing.
pub fn sync(ctx: &egui::Context) {
    let generation = generation();
    match ctx.data(|d| d.get_temp::<Installed>(installed_id())) {
        None => {
            let lib = library();
            lib.start();
            let mut p = Provider::new(lib);
            start_fallback(&mut p);
            ctx.add_font_provider(Arc::new(p));
            // (Read before `data_mut`: the context is locked inside it.)
            let pass = ctx.cumulative_pass_nr();
            ctx.data_mut(|d| d.insert_temp(installed_id(), Installed { pass, generation }));
            ctx.request_repaint();
        }
        Some(i) if i.generation != generation => {
            ctx.data_mut(|d| d.insert_temp(installed_id(), Installed { generation, ..i }));
            ctx.request_repaint();
        }
        Some(_) => {}
    }
    if !library().is_ready() {
        // Look again soon: the scan finishing changes what resolves.
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

/// Whether the `nc:` families draw in `ctx` (the provider was added in an earlier pass).
pub fn active(ctx: &egui::Context) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<Installed>(installed_id())).is_some_and(|i| i.pass < pass)
}

/// What the canvas keys its layout on: the fonts' generation, or `None` while it draws with
/// egui's own faces.
pub fn state(ctx: &egui::Context) -> Option<u64> {
    active(ctx).then(generation)
}

/// Add a font file the user picked (TTF, OTF, TTC, WOFF, WOFF2) for this session; the families
/// it adds, or why it wasn't added.
pub fn add_font_file(bytes: &[u8]) -> Result<Vec<String>, notecraft_fonts::FontError> {
    library().add_font_file(bytes)
}

/// Every family the font box offers (never waits for the scan).
pub fn families() -> Arc<[String]> {
    library().families()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn family_names_round_trip() {
        for (f, b, i) in [("Calibri", false, false), ("Segoe UI", true, false), ("a:b@c", false, true), ("Times New Roman", true, true)] {
            let name = family_name(f, b, i, 7);
            assert_eq!(parse_family(&name), Some((f, b, i)), "{name}");
        }
        assert_eq!(family_name("Calibri", true, false, 3), "nc:Calibri:bn@3");
        for bad in ["Calibri", "nc:Calibri", "nc:Calibri:bn", "nc::nn@1", "nc:X:xx@1", "nc:X:bnn@1", "nc:X:b@1"] {
            assert_eq!(parse_family(bad), None, "{bad}");
        }
        // Control characters don't reach egui's family names.
        assert_eq!(parse_family(&family_name("A\u{0}B\n", false, false, 1)), Some(("AB", false, false)));
    }

    #[test]
    fn unknown_families_still_have_egui_faces() {
        let p = Provider::new(library());
        let inserts = p.fonts_for_family(&FontFamily::Name(family_name("No Such Font Anywhere", false, false, 0).into()));
        assert!(inserts.iter().any(|f| f.name == "Ubuntu-Light"), "egui's face follows: {:?}", inserts.iter().map(|f| &f.name).collect::<Vec<_>>());
        assert!(p.fonts_for_family(&FontFamily::Proportional).is_empty(), "egui's own families are left to egui");
        assert!(p.fonts_for_family(&FontFamily::Name("other".into())).is_empty());
        let mono = p.fonts_for_family(&FontFamily::Name(family_name("Nope Mono", false, false, 0).into()));
        assert!(mono.iter().any(|f| f.name == "Hack"));
    }

    #[test]
    fn without_the_provider_faces_fall_back_to_egui() {
        let f = face("Consolas", true, false, false);
        assert_eq!(f.family, FontFamily::Monospace);
        assert!(f.fake_bold && !f.fake_italic);
        assert_eq!(face("Calibri", false, true, false).family, FontFamily::Proportional);
    }
}
