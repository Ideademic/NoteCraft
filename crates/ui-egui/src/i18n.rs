//! Interface strings. Strings in the code stay English and are the lookup keys; translations
//! come later from craft-i18n catalogs (wired as in WordCraft: a `build.rs` embedding
//! `CRAFT_I18N_DIR`). Until NoteCraft has a catalog, [`tl!`](crate::tl) returns the English.

pub use craft_i18n::{fmt, fmt_plain};

/// The interface string for `s` in the current language (English for now).
#[macro_export]
macro_rules! tl {
    ($s:literal) => {
        $crate::i18n::t($s)
    };
}

/// The interface string for `s` (English passthrough until catalogs are wired).
pub fn t(s: &'static str) -> &'static str {
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn english_passthrough() {
        assert_eq!(crate::tl!("New Page"), "New Page");
    }
}
