//! Review: Proofing (Spelling▾ with Check Spelling as You Type, Word Count) and Language
//! (the proofing language of the selection or page). craft-proof has no thesaurus, so there is no
//! Thesaurus button.

use egui::Ui;
use notecraft_engine::ids;

use crate::icons::Icon;
use crate::widgets::{self, Btn};
use crate::{NoteApp, review, tl};

pub fn show(ui: &mut Ui, app: &mut NoteApp) {
    widgets::group(ui, app, tl!("Proofing"), Icon::Spelling, |ui, app| {
        let b = Btn::new(Icon::Spelling, tl!("Spelling"))
            .tip(tl!("Spelling"))
            .help(tl!("Check the spelling on this page, one word at a time."))
            .shortcut("F7")
            .checked(app.ui.review.spelling.is_some())
            .enabled(app.can(ids::PROOF_CHECK));
        if widgets::big_split(ui, app, b, spelling_menu) {
            crate::panes::spelling::toggle(app);
        }
        let b = Btn::new(Icon::WordCount, tl!("Word\nCount"))
            .tip(tl!("Word Count"))
            .help(tl!("Count the words, characters and paragraphs on this page, or in the selected text."))
            .enabled(app.can(ids::REVIEW_WORD_COUNT));
        if widgets::big(ui, app, b) {
            review::word_count(app);
        }
    });
    widgets::group(ui, app, tl!("Language"), Icon::Language, |ui, app| {
        let b = Btn::new(Icon::Language, tl!("Language"))
            .tip(tl!("Language"))
            .help(tl!("Choose the language the selected text, or the whole page, is checked in."))
            .enabled(app.can(ids::PROOF_SET_LANGUAGE));
        widgets::big_menu(ui, app, b, language_menu);
    });
}

/// Spelling▾: open the pane, and the as-you-type marks.
pub fn spelling_menu(ui: &mut Ui, app: &mut NoteApp) {
    if widgets::menu_item(ui, Some(Icon::Spelling), tl!("Spelling…"), app.can(ids::PROOF_CHECK)) {
        crate::panes::spelling::open(app);
    }
    ui.separator();
    let on = app.ui.prefs.check_spelling;
    if widgets::check_item(ui, on, tl!("Check Spelling as You Type")) {
        app.ui.prefs.check_spelling = !on;
    }
}

/// Language▾: the proofing language for the selection (or the page), and the default.
pub fn language_menu(ui: &mut Ui, app: &mut NoteApp) {
    review::language_items(ui, app);
}
