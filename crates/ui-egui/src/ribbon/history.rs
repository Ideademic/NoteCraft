//! History: Unread (Next Unread, Mark as Read▾), Authors (Recent Edits▾), History (Page
//! Versions▾, Notebook Recycle Bin▾). NoteCraft keeps no authors, so Find by Author and Hide
//! Authors are left out.

use egui::Ui;
use notecraft_engine::ids;
use serde_json::json;

use crate::icons::Icon;
use crate::panes::history::{PERIODS, ask_delete_versions, ask_empty, open_bin, period_label};
use crate::widgets::{self, Btn};
use crate::{NoteApp, tl};

pub fn show(ui: &mut Ui, app: &mut NoteApp) {
    widgets::group(ui, app, tl!("Unread"), Icon::Unread, |ui, app| {
        let b = Btn::new(Icon::Unread, tl!("Next\nUnread"))
            .tip(tl!("Next Unread"))
            .help(tl!("Go to the next page that changed since you last saw it."))
            .enabled(app.can(ids::HISTORY_NEXT_UNREAD));
        if widgets::big(ui, app, b) {
            next_unread(app);
        }
        let b = Btn::new(Icon::MarkRead, tl!("Mark as\nRead"))
            .tip(tl!("Mark as Read"))
            .help(tl!("Mark this page as read, or as unread again."))
            .enabled(app.can(ids::HISTORY_TOGGLE_READ));
        if widgets::big_split(ui, app, b, mark_read_menu) {
            app.act(ids::HISTORY_TOGGLE_READ, json!({}));
        }
    });
    widgets::group(ui, app, tl!("Authors"), Icon::RecentEdits, |ui, app| {
        let b =
            Btn::new(Icon::RecentEdits, tl!("Recent\nEdits")).tip(tl!("Recent Edits")).help(tl!("List the pages changed recently, newest first."));
        widgets::big_menu(ui, app, b, recent_edits_menu);
    });
    widgets::group(ui, app, tl!("History"), Icon::History, |ui, app| {
        let b = Btn::new(Icon::PageVersions, tl!("Page\nVersions"))
            .tip(tl!("Page Versions"))
            .help(tl!("Show the earlier versions NoteCraft kept of each page, under the page in the page list."))
            .checked(app.session.show_versions);
        if widgets::big_split(ui, app, b, versions_menu) {
            app.act(ids::HISTORY_SHOW_VERSIONS, json!({}));
        }
        let b = Btn::new(Icon::RecycleBin, tl!("Notebook\nRecycle Bin"))
            .tip(tl!("Notebook Recycle Bin"))
            .help(tl!("Deleted pages and sections stay here for 60 days. Restore them or delete them for good."))
            .checked(app.ui.bin_view.is_some())
            .enabled(app.can(ids::HISTORY_RECYCLE_BIN));
        if widgets::big_split(ui, app, b, bin_menu) {
            toggle_bin(app);
        }
    });
}

/// Next Unread, saying so when there is nothing unread.
pub fn next_unread(app: &mut NoteApp) {
    app.ui.bin_view = None;
    if let Ok(out) = app.run(ids::HISTORY_NEXT_UNREAD, json!({}))
        && out.value.is_null()
    {
        app.set_status(tl!("There are no unread pages."), false);
    }
}

/// Opens or closes the recycle bin view.
pub fn toggle_bin(app: &mut NoteApp) {
    if app.ui.bin_view.is_some() {
        crate::panes::history::close_bin(app);
    } else {
        open_bin(app);
    }
}

pub fn mark_read_menu(ui: &mut Ui, app: &mut NoteApp) {
    let unread = app.session.current.page.is_some_and(|p| app.session.is_unread(p));
    if widgets::menu_item(
        ui,
        Some(Icon::MarkRead),
        if unread { tl!("Mark as Read") } else { tl!("Mark as Unread") },
        app.can(ids::HISTORY_TOGGLE_READ),
    ) {
        app.act(ids::HISTORY_TOGGLE_READ, json!({}));
    }
    ui.separator();
    if widgets::menu_item(ui, None, tl!("Mark Section as Read"), app.session.current.section.is_some()) {
        app.act(ids::HISTORY_MARK_READ, json!({ "scope": "section" }));
    }
    if widgets::menu_item(ui, None, tl!("Mark Notebook as Read"), app.session.current.notebook.is_some()) {
        app.act(ids::HISTORY_MARK_READ, json!({ "scope": "notebook" }));
    }
}

pub fn recent_edits_menu(ui: &mut Ui, app: &mut NoteApp) {
    for (key, _) in PERIODS {
        if widgets::check_item(ui, app.ui.recent_edits.as_deref() == Some(key), period_label(key)) {
            app.ui.recent_edits = Some(key.to_owned());
        }
    }
    if app.ui.recent_edits.is_some() {
        ui.separator();
        if widgets::menu_item(ui, Some(Icon::Close), tl!("Close Recent Edits"), true) {
            app.ui.recent_edits = None;
        }
    }
}

pub fn versions_menu(ui: &mut Ui, app: &mut NoteApp) {
    let shown = app.session.show_versions;
    if widgets::check_item(ui, shown, tl!("Page Versions")) {
        app.act(ids::HISTORY_SHOW_VERSIONS, json!({ "value": !shown }));
    }
    if widgets::menu_item(ui, None, tl!("Hide Page Versions"), shown) {
        app.act(ids::HISTORY_SHOW_VERSIONS, json!({ "value": false }));
    }
    ui.separator();
    if widgets::menu_item(ui, Some(Icon::Delete), tl!("Delete All Versions in Section"), app.session.current.section.is_some()) {
        ask_delete_versions(app, "section");
    }
    if widgets::menu_item(ui, Some(Icon::Delete), tl!("Delete All Versions in Notebook"), app.session.current.notebook.is_some()) {
        ask_delete_versions(app, "notebook");
    }
    ui.separator();
    disable_history_item(ui, app);
}

pub fn bin_menu(ui: &mut Ui, app: &mut NoteApp) {
    if widgets::check_item(ui, app.ui.bin_view.is_some(), tl!("Notebook Recycle Bin")) {
        toggle_bin(app);
    }
    let nb = app.session.current.notebook;
    let has = app.session.current_notebook().is_some_and(|n| !n.recycle_bin.is_empty());
    if widgets::menu_item(ui, Some(Icon::Delete), tl!("Empty Recycle Bin"), has)
        && let Some(nb) = nb
    {
        ask_empty(app, nb);
    }
    ui.separator();
    disable_history_item(ui, app);
}

fn disable_history_item(ui: &mut Ui, app: &mut NoteApp) {
    let off = app.session.current_notebook().is_some_and(|n| n.history_disabled);
    if widgets::check_item(ui, off, tl!("Disable History for this Notebook")) {
        app.act(ids::HISTORY_DISABLE, json!({ "value": !off }));
    }
}
