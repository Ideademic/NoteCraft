//! Interface languages. Strings in the code stay English and are the lookup keys; the craft-i18n
//! catalogs map them to display text when the UI is drawn. Command ids, notebook content, file
//! names and the CLI never see translated text. A string without a translation shows in English,
//! so coverage can grow one entry at a time and a missing translation never fails a build.
//!
//! The catalogs live in [storytold/craft-i18n](https://github.com/storytold/craft-i18n)
//! (`locales/<locale>/common.tsv` and `notecraft.tsv`), not in this repository. `build.rs` embeds
//! them from `CRAFT_I18N_DIR`; without it (the default: NoteCraft has no catalog there yet) the
//! interface is English only. Lookup, fallback (`pt-br` → `pt` → English, app rows over shared
//! ones), language names and system-locale matching are the `craft-i18n` crate's (craft-libs).
//!
//! The `language` preference ([`crate::Prefs::language`]) is [`AUTO`] by default: the interface
//! follows the operating system's language when there is a catalog for it, English otherwise.
//! File › Options › Language picks one by hand. `NOTECRAFT_LANGUAGE` overrides the system's.
//!
//! - [`tl!`](crate::tl) / [`t`]: a string in the current language; [`tr`]: in a given one.
//! - [`fmt`] fills `{name}` placeholders after a lookup (translators may reorder them);
//!   [`fmt_plain`] does it without bidi isolate marks, for text that is saved.

use std::cell::Cell;
use std::sync::{LazyLock, OnceLock};

use craft_i18n::I18n;

pub use craft_i18n::{fmt, fmt_plain};

include!(concat!(env!("OUT_DIR"), "/craft_i18n.rs"));

/// The embedded catalogs. Every embedded locale is offered except right-to-left ones: the window
/// isn't mirrored yet, so a right-to-left interface would read wrongly. Built on first use; a
/// locale is parsed on its first lookup.
static I18N: LazyLock<I18n> = LazyLock::new(|| catalogs(CRAFT_I18N));

/// The catalogs of `embedded`, offering its left-to-right locales.
fn catalogs(embedded: &'static craft_i18n::Embedded) -> I18n {
    let offered: Vec<&str> = embedded.iter().map(|(code, ..)| *code).filter(|c| craft_i18n::direction(c) != craft_i18n::Direction::Rtl).collect();
    let (i18n, problems) = I18n::new(embedded, &offered);
    problems.iter().for_each(|p| log::warn!("interface catalogs: {p}"));
    i18n
}

/// The `language` preference that follows the system.
pub const AUTO: &str = "auto";

/// A language the interface can be shown in: English, or an offered locale whose catalogs are
/// embedded.
#[derive(Clone, Copy)]
pub struct Lang {
    lang: craft_i18n::Lang,
    /// The catalogs it is looked up in: the embedded ones ([`I18N`]), or a test fixture's.
    catalogs: Option<&'static I18n>,
}

impl std::fmt::Debug for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lang({})", self.code())
    }
}

impl PartialEq for Lang {
    fn eq(&self, other: &Self) -> bool {
        self.code() == other.code()
    }
}

impl Eq for Lang {}

impl Lang {
    pub const EN: Lang = Lang { lang: craft_i18n::Lang::EN, catalogs: None };

    fn i18n(self) -> &'static I18n {
        self.catalogs.unwrap_or_else(|| LazyLock::force(&I18N))
    }

    /// Lowercase BCP 47 code (`en`, `pt-br`): also the `language` preference's value.
    pub fn code(self) -> &'static str {
        self.i18n().code(self.lang)
    }

    /// The language's name in itself (its `@id language.name` row), for the Options picker.
    pub fn name(self) -> &'static str {
        self.i18n().name(self.lang)
    }

    /// A language by its code (any case): English or an offered, embedded locale.
    pub fn from_code(code: &str) -> Option<Lang> {
        I18N.from_code(code).filter(|l| *l != craft_i18n::Lang::PSEUDO).map(|lang| Lang { lang, catalogs: None })
    }

    /// Resolves the `language` preference: a language code, or [`AUTO`] (and anything unknown,
    /// such as a code from a newer version or a catalog no longer embedded) to follow the system.
    pub fn from_pref(pref: &str) -> Lang {
        Lang::from_code(pref).unwrap_or_else(system_lang)
    }

    /// Every language the interface can be shown in, English first (English only in a build
    /// without craft-i18n).
    pub fn all() -> Vec<Lang> {
        I18N.languages().into_iter().map(|lang| Lang { lang, catalogs: None }).collect()
    }
}

/// The canonical `language` preference for `pref`: [`AUTO`] or an offered code. `None` for
/// anything else, so callers keep the current one.
pub fn normalize_pref(pref: &str) -> Option<&'static str> {
    if pref.eq_ignore_ascii_case(AUTO) {
        return Some(AUTO);
    }
    Lang::from_code(pref).map(Lang::code)
}

/// The system's language (looked up once): `NOTECRAFT_LANGUAGE`, then the platform's preferred
/// languages (macOS and Windows settings, `LC_ALL`/`LC_MESSAGES`/`LANG`, the browser's on the
/// web). English when none has a catalog.
pub fn system_lang() -> Lang {
    // Tests drive the UI by its English labels whatever the machine's language is.
    if cfg!(test) {
        return Lang::EN;
    }
    static SYSTEM: OnceLock<Lang> = OnceLock::new();
    *SYSTEM.get_or_init(|| {
        let forced = std::env::var("NOTECRAFT_LANGUAGE").ok().and_then(|v| I18N.match_tag(&v));
        Lang { lang: forced.unwrap_or_else(|| I18N.system_lang()), catalogs: None }
    })
}

thread_local! {
    // Per thread: parallel test harnesses must not change each other's language.
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::EN) };
}

/// Sets the language the interface is drawn in (the app does this every frame from its
/// preference).
pub fn set_current(lang: Lang) {
    CURRENT.set(lang);
}

/// The language the interface is drawn in.
pub fn current() -> Lang {
    CURRENT.get()
}

/// The interface string for `s` in the current language ([`tr`] with [`current`]).
#[macro_export]
macro_rules! tl {
    ($s:literal) => {
        $crate::i18n::t($s)
    };
}

/// `s` in the current language; English when there is no translation.
pub fn t(s: &'static str) -> &'static str {
    tr(current(), s)
}

/// `s` in `lang`; English when there is no translation.
pub fn tr(lang: Lang, s: &'static str) -> &'static str {
    lang.i18n().t(lang.lang, s)
}

/// A test language whose NoteCraft catalog is `source` (catalog format), so lookup tests don't
/// depend on the embedded catalogs.
#[cfg(test)]
pub(crate) fn fixture_lang(code: &'static str, source: &'static str) -> Lang {
    let embedded: &'static craft_i18n::Embedded = Box::leak(Box::new([(code, "", source)]));
    let i18n: &'static I18n = Box::leak(Box::new(catalogs(embedded)));
    let lang = i18n.from_code(code).unwrap_or(craft_i18n::Lang::EN);
    Lang { lang, catalogs: Some(i18n) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_passthrough() {
        assert_eq!(crate::tl!("New Page"), "New Page");
        assert_eq!(Lang::EN.code(), "en");
        assert_eq!(Lang::EN.name(), "English");
        assert_eq!(Lang::all().first(), Some(&Lang::EN));
    }

    #[test]
    fn translations_fall_back_to_english() {
        let fx = fixture_lang("x-fixture", "\tNew Page\tPage-FX\n\tbroken row without tabs\n");
        assert_eq!(tr(fx, "New Page"), "Page-FX");
        assert_eq!(tr(fx, "No such label"), "No such label", "missing: the English");
        assert_eq!(tr(Lang::EN, "New Page"), "New Page");
        // The current language is per thread and drives `tl!`.
        set_current(fx);
        assert_eq!(crate::tl!("New Page"), "Page-FX");
        assert_eq!(crate::tl!("Untitled"), "Untitled");
        set_current(Lang::EN);
        assert_eq!(crate::tl!("New Page"), "New Page");
    }

    #[test]
    fn right_to_left_catalogs_are_not_offered() {
        let rtl = fixture_lang("ar", "\tNew Page\tصفحة\n");
        assert_eq!(rtl, Lang::EN, "falls back to English until the window can be mirrored");
        let ltr = fixture_lang("de", "\tNew Page\tNeue Seite\n");
        assert_eq!(tr(ltr, "New Page"), "Neue Seite");
    }

    #[test]
    fn preferences_resolve() {
        assert_eq!(Lang::from_pref(AUTO), system_lang());
        assert_eq!(Lang::from_pref("en"), Lang::EN);
        assert_eq!(Lang::from_pref("xx-unknown"), system_lang(), "unknown codes follow the system");
        assert_eq!(Lang::from_pref(""), system_lang());
        assert_eq!(normalize_pref("AUTO"), Some(AUTO));
        assert_eq!(normalize_pref("EN"), Some("en"));
        assert_eq!(normalize_pref("pseudo"), None);
        assert_eq!(normalize_pref("klingon"), None);
    }
}
