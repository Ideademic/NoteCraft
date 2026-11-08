//! Simplified History: Next Unread, Mark as Read▾ | Recent Edits▾ | Page Versions▾, Notebook
//! Recycle Bin▾. The menus are the classic tab's.

use egui::Ui;
use notecraft_engine::ids;
use serde_json::json;

use super::{Segment, labeled_split};
use crate::icons::Icon;
use crate::ribbon::history::{bin_menu, mark_read_menu, next_unread, recent_edits_menu, toggle_bin, versions_menu};
use crate::widgets::{self, Btn};
use crate::{NoteApp, tl};

pub const SEGMENTS: [Segment; 3] =
    [Segment { key: "unread", draw: unread }, Segment { key: "authors", draw: authors }, Segment { key: "history", draw: history }];

fn unread(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::Unread, tl!("Next Unread"))
        .help(tl!("Go to the next page that changed since you last saw it."))
        .enabled(app.can(ids::HISTORY_NEXT_UNREAD));
    if widgets::small(ui, app, b) {
        next_unread(app);
    }
    let b = Btn::new(Icon::MarkRead, tl!("Mark as Read"))
        .help(tl!("Mark this page as read, or as unread again."))
        .enabled(app.can(ids::HISTORY_TOGGLE_READ));
    if labeled_split(ui, app, b, mark_read_menu) {
        app.act(ids::HISTORY_TOGGLE_READ, json!({}));
    }
}

fn authors(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::RecentEdits, tl!("Recent Edits")).help(tl!("List the pages changed recently, newest first."));
    widgets::small_menu(ui, app, b, recent_edits_menu);
}

fn history(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::PageVersions, tl!("Page Versions"))
        .help(tl!("Show the earlier versions NoteCraft kept of each page, under the page in the page list."))
        .checked(app.session.show_versions);
    if labeled_split(ui, app, b, versions_menu) {
        app.act(ids::HISTORY_SHOW_VERSIONS, json!({}));
    }
    let b = Btn::new(Icon::RecycleBin, tl!("Notebook Recycle Bin"))
        .help(tl!("Deleted pages and sections stay here for 60 days. Restore them or delete them for good."))
        .checked(app.ui.bin_view.is_some())
        .enabled(app.can(ids::HISTORY_RECYCLE_BIN));
    if labeled_split(ui, app, b, bin_menu) {
        toggle_bin(app);
    }
}
