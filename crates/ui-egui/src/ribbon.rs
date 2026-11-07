//! The ribbon: the tab row (File, Home, Insert, Draw, View) and the selected tab's groups.
//! Phase 2 (shell agent): groups, large/small buttons, galleries, resizing to fit, KeyTips.

pub mod draw;
pub mod home;
pub mod insert;
pub mod view;

use crate::NoteApp;
use crate::tl;

/// Ribbon tabs. File opens the backstage instead of a tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RibbonTab {
    File,
    #[default]
    Home,
    Insert,
    Draw,
    View,
}

impl RibbonTab {
    pub const ALL: [RibbonTab; 5] = [RibbonTab::File, RibbonTab::Home, RibbonTab::Insert, RibbonTab::Draw, RibbonTab::View];

    pub fn label(self) -> &'static str {
        match self {
            RibbonTab::File => tl!("File"),
            RibbonTab::Home => tl!("Home"),
            RibbonTab::Insert => tl!("Insert"),
            RibbonTab::Draw => tl!("Draw"),
            RibbonTab::View => tl!("View"),
        }
    }
}

/// The tab row and the selected tab's content.
pub fn show(ui: &mut egui::Ui, app: &mut NoteApp) {
    ui.horizontal(|ui| {
        for tab in RibbonTab::ALL {
            if ui.selectable_label(app.ui.ribbon_tab == tab && tab != RibbonTab::File, tab.label()).clicked() {
                if tab == RibbonTab::File {
                    app.ui.backstage = true;
                } else {
                    app.ui.ribbon_tab = tab;
                }
            }
        }
    });
    if app.ui.ribbon_collapsed {
        return;
    }
    ui.horizontal(|ui| match app.ui.ribbon_tab {
        RibbonTab::File | RibbonTab::Home => home::show(ui, app),
        RibbonTab::Insert => insert::show(ui, app),
        RibbonTab::Draw => draw::show(ui, app),
        RibbonTab::View => view::show(ui, app),
    });
}
