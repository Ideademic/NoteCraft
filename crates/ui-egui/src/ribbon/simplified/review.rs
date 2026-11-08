//! Simplified Review: Spelling▾, Word Count | Language▾. The menus are the classic tab's.

use egui::Ui;
use notecraft_engine::ids;

use super::{Segment, labeled_split};
use crate::icons::Icon;
use crate::ribbon::review::{language_menu, spelling_menu};
use crate::widgets::{self, Btn};
use crate::{NoteApp, review, tl};

pub const SEGMENTS: [Segment; 2] = [Segment { key: "proofing", draw: proofing }, Segment { key: "language", draw: language }];

fn proofing(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::Spelling, tl!("Spelling"))
        .help(tl!("Check the spelling on this page, one word at a time."))
        .shortcut("F7")
        .checked(app.ui.review.spelling.is_some())
        .enabled(app.can(ids::PROOF_CHECK));
    if labeled_split(ui, app, b, spelling_menu) {
        crate::panes::spelling::toggle(app);
    }
    let b = Btn::new(Icon::WordCount, tl!("Word Count"))
        .help(tl!("Count the words, characters and paragraphs on this page, or in the selected text."))
        .enabled(app.can(ids::REVIEW_WORD_COUNT));
    if widgets::small(ui, app, b) {
        review::word_count(app);
    }
}

fn language(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::Language, tl!("Language"))
        .help(tl!("Choose the language the selected text, or the whole page, is checked in."))
        .enabled(app.can(ids::PROOF_SET_LANGUAGE));
    widgets::small_menu(ui, app, b, language_menu);
}
