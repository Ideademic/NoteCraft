//! Named window states for screenshots (the `ui_shot` example) and tests: each sets up the UI
//! the way a person would have it at that moment.

use serde_json::json;

use super::NoteApp;
use crate::backstage::BackstagePage;
use crate::ribbon::RibbonTab;

/// Every named state.
pub const SHOTS: [&str; 12] = [
    "main",
    "tab-insert",
    "tab-draw",
    "tab-view",
    "backstage-info",
    "backstage-new",
    "backstage-open",
    "backstage-export",
    "backstage-options",
    "backstage-about",
    "panes-collapsed",
    "busy-notebook",
];

/// The ribbon's layouts and display options (`ribbon-display-menu` also needs the options arrow
/// clicked; the `ui_shot` example does that).
pub const RIBBON_SHOTS: [&str; 8] = [
    "simplified-home",
    "simplified-insert",
    "simplified-draw",
    "simplified-view",
    "ribbon-display-menu",
    "ribbon-full-screen",
    "ribbon-full-screen-revealed",
    "ribbon-tabs-only",
];

/// Options' language picker (open; the `ui_shot` example clicks it) and the Check for Updates
/// window answered "up to date" without the network.
pub const PLATFORM_SHOTS: [&str; 2] = ["options-language-menu", "update-up-to-date"];

/// The states of [`PLATFORM_SHOTS`]; `false` for other names.
fn prepare_platform(app: &mut NoteApp, shot: &str) -> bool {
    match shot {
        "options-language-menu" => crate::backstage::open(app, BackstagePage::Options),
        "update-up-to-date" => {
            crate::backstage::open(app, BackstagePage::About);
            #[cfg(not(target_arch = "wasm32"))]
            {
                app.updates = crate::updates::Updates::default().with_source(std::sync::Arc::new(|| Ok(None)));
            }
            crate::updates::check(app);
        }
        _ => return false,
    }
    true
}

/// The ribbon states of [`RIBBON_SHOTS`]; `false` for other names.
fn prepare_ribbon(app: &mut NoteApp, shot: &str) -> bool {
    use crate::ribbon::display::{RibbonDisplay, RibbonLayout};
    let simplified = |app: &mut NoteApp, tab| {
        app.ui.ribbon_layout = RibbonLayout::Simplified;
        app.ui.ribbon_tab = tab;
    };
    match shot {
        "simplified-home" => simplified(app, RibbonTab::Home),
        "simplified-insert" => simplified(app, RibbonTab::Insert),
        "simplified-draw" => simplified(app, RibbonTab::Draw),
        "simplified-view" => simplified(app, RibbonTab::View),
        "ribbon-display-menu" => simplified(app, RibbonTab::Home),
        "ribbon-full-screen" => app.ui.ribbon_display = RibbonDisplay::FullScreen,
        "ribbon-full-screen-revealed" => {
            simplified(app, RibbonTab::Home);
            app.ui.ribbon_display = RibbonDisplay::FullScreen;
            app.ui.ribbon_peek = true;
        }
        "ribbon-tabs-only" => app.ui.ribbon_display = RibbonDisplay::TabsOnly,
        _ => return false,
    }
    true
}

/// Puts `app` in the state named `shot` (unknown names leave it as it is).
pub fn prepare(app: &mut NoteApp, shot: &str) {
    if prepare_ribbon(app, shot) || prepare_platform(app, shot) {
        return;
    }
    match shot {
        "tab-insert" => app.ui.ribbon_tab = RibbonTab::Insert,
        "tab-draw" => app.ui.ribbon_tab = RibbonTab::Draw,
        "tab-view" => app.ui.ribbon_tab = RibbonTab::View,
        "backstage-info" => crate::backstage::open(app, BackstagePage::Info),
        "backstage-new" => crate::backstage::open(app, BackstagePage::New),
        "backstage-open" => {
            for (name, path, color) in [
                ("Recipes", "/Users/me/Documents/Recipes.onepkg", "#f28b8b"),
                ("Work Notes", "/Users/me/Documents/NoteCraft Notebooks/Work Notes", "#9cd87e"),
            ] {
                app.session.recent.add(path.into(), name);
                app.ui.prefs.remember(name, path, Some(color.into()));
            }
            crate::backstage::open(app, BackstagePage::Open);
        }
        "backstage-export" => crate::backstage::open(app, BackstagePage::Export),
        "backstage-options" => crate::backstage::open(app, BackstagePage::Options),
        "backstage-about" => crate::backstage::open(app, BackstagePage::About),
        "panes-collapsed" => {
            app.ui.notebook_pane_open = false;
            app.ui.page_list_open = false;
        }
        "busy-notebook" => busy(app),
        _ => {}
    }
}

/// A notebook with a few sections, a section group, pages and subpages.
fn busy(app: &mut NoteApp) {
    let run = |app: &mut NoteApp, id: &str, p: serde_json::Value| {
        let _ = app.run(id, p);
    };
    use notecraft_engine::ids;
    let first = app.session.current.section;
    for name in ["Meetings", "Research", "Travel"] {
        run(app, ids::SECTION_NEW, json!({ "name": name }));
    }
    run(app, ids::SECTION_GROUP_NEW, json!({ "name": "Archive" }));
    run(app, ids::NOTEBOOK_NEW, json!({ "name": "Recipes" }));
    if let Some(nb) = app.session.notebooks.first().map(|n| n.id) {
        run(app, ids::NAV_NOTEBOOK, json!({ "id": nb }));
    }
    if let Some(s) = first {
        run(app, ids::NAV_SECTION, json!({ "id": s }));
    }
    for title in ["Project kickoff", "Agenda", "Action items", "Reading list"] {
        run(app, ids::PAGE_NEW, json!({ "title": title }));
    }
    run(app, ids::PAGE_INDENT, json!({}));
    if let Some(p) = app.session.current_section().and_then(|s| s.pages.first()).map(|p| p.id) {
        run(app, ids::NAV_PAGE, json!({ "id": p }));
    }
    app.ui.search = String::new();
}
