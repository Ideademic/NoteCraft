//! File › Options › Language and Updates, and File › About › Check for Updates.

use egui::Ui;

use super::{dim, subheading};
use crate::i18n::{AUTO, Lang};
use crate::updates::{self, Updates};
use crate::widgets::flat_button;
use crate::{NoteApp, tl};

/// The accessible name of the language picker.
pub const LANGUAGE_LABEL: &str = "Interface language";
/// The About page's button (and the Options page's).
pub const CHECK_LABEL: &str = "Check for Updates";

/// The picker's entry for [`AUTO`]: the system's language, named.
fn auto_label() -> String {
    crate::i18n::fmt_plain(tl!("Use the system language ({name})"), &[("name", crate::i18n::system_lang().name())])
}

/// Options › Language: the system's language (the default) or one of the embedded ones.
pub fn language(ui: &mut Ui, app: &mut NoteApp) {
    subheading(ui, tl!("Language"));
    let auto = !Lang::all().iter().any(|l| l.code() == app.ui.prefs.language) || app.ui.prefs.language == AUTO;
    let shown = if auto { auto_label() } else { Lang::from_pref(&app.ui.prefs.language).name().to_owned() };
    let mut pick: Option<String> = None;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Interface language:")).font(crate::theme::regular(12.5)).color(app.tokens.text_dim));
        let r = craft_ui::ComboBox::from_id_salt("nc_language").width(260.0).selected_text(shown).show_ui(ui, |ui| {
            if ui.selectable_label(auto, auto_label()).clicked() {
                pick = Some(AUTO.to_owned());
            }
            for lang in Lang::all() {
                if ui.selectable_label(!auto && lang.code() == app.ui.prefs.language, lang.name()).clicked() {
                    pick = Some(lang.code().to_owned());
                }
            }
        });
        r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, crate::i18n::t(LANGUAGE_LABEL)));
    });
    if let Some(code) = pick {
        app.ui.prefs.language = code;
    }
    if Lang::all().len() <= 1 {
        dim(ui, tl!("NoteCraft is in English for now. Translations will come from the Craft apps' shared translation project."));
    }
}

/// Options › Updates (desktop): check at start, or now.
pub fn updates(ui: &mut Ui, app: &mut NoteApp) {
    if !Updates::AVAILABLE {
        return;
    }
    subheading(ui, tl!("Updates"));
    ui.checkbox(&mut app.ui.prefs.check_for_updates, tl!("Check for updates when NoteCraft starts"));
    dim(ui, tl!("NoteCraft asks GitHub for its newest release and offers to install it. Nothing else is sent."));
    ui.add_space(4.0);
    if flat_button(ui, crate::i18n::t(CHECK_LABEL), false, true) {
        updates::check(app);
    }
}

/// About › Check for Updates (desktop); the web build is always the version the page serves.
pub fn about_updates(ui: &mut Ui, app: &mut NoteApp) {
    ui.add_space(8.0);
    if Updates::AVAILABLE {
        if flat_button(ui, crate::i18n::t(CHECK_LABEL), true, true) {
            updates::check(app);
        }
    } else {
        dim(ui, tl!("This is the web version: reloading the page gets the newest release."));
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    use super::*;
    use crate::backstage::{self, BackstagePage};

    fn harness(app: NoteApp) -> Harness<'static, NoteApp> {
        Harness::builder().with_size(egui::vec2(1440.0, 1100.0)).build_ui_state(
            |ui, app: &mut NoteApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        )
    }

    fn app_at(page: BackstagePage) -> NoteApp {
        let mut a = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        backstage::open(&mut a, page);
        a
    }

    #[test]
    fn options_offer_the_language_and_updates() {
        let mut h = harness(app_at(BackstagePage::Options));
        h.run_steps(3);
        let _ = h.get_by_label("Language");
        h.get_by_label(LANGUAGE_LABEL).click();
        h.run_steps(3);
        h.get_by_label("English").click();
        h.run_steps(3);
        assert_eq!(h.state().ui.prefs.language, "en");
        h.get_by_label(LANGUAGE_LABEL).click();
        h.run_steps(3);
        h.get_by_label(&auto_label()).click();
        h.run_steps(3);
        assert_eq!(h.state().ui.prefs.language, AUTO);
        // The check at start can be turned off.
        h.get_by_label("Check for updates when NoteCraft starts").click();
        h.run_steps(2);
        assert!(!h.state().ui.prefs.check_for_updates);
        assert!(!h.state().prefs().check_for_updates, "kept with the settings");
    }

    #[test]
    fn about_checks_for_updates_without_the_network() {
        let mut a = app_at(BackstagePage::About);
        a.updates = Updates::default().with_source(std::sync::Arc::new(|| Ok(None)));
        let mut h = harness(a);
        h.run_steps(3);
        h.get_by_label(CHECK_LABEL).click();
        h.run_steps(3);
        assert!(h.state().updates.manual_window_open);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !matches!(h.state().updates.status(), craft_update::updater::Status::UpToDate) {
            assert!(std::time::Instant::now() < deadline, "{:?}", h.state().updates.status());
            std::thread::sleep(std::time::Duration::from_millis(5));
            h.step();
        }
        h.run_steps(2);
        assert!(h.state().updates.manual_window_open, "the window says so");
    }

    #[test]
    fn language_and_update_prefs_round_trip() {
        let p = crate::Prefs { language: "de".into(), check_for_updates: false, ..crate::Prefs::default() };
        let json = p.to_json();
        assert!(json.contains("\"language\": \"de\"") && json.contains("\"checkForUpdates\": false"), "{json}");
        assert_eq!(crate::Prefs::from_json(&json), p, "a code without a catalog here is kept");
        let d = crate::Prefs::from_json("{}");
        assert_eq!((d.language.as_str(), d.check_for_updates), (AUTO, true));
        for junk in [r#"{"language": ""}"#, r#"{"language": "<script>"}"#, &format!(r#"{{"language": "{}"}}"#, "x".repeat(100))] {
            assert_eq!(crate::Prefs::from_json(junk).language, AUTO, "{junk}");
        }
        // An unknown language shows the interface in the system's.
        let mut a = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        a.load_prefs(p);
        a.logic(&egui::Context::default());
        assert_eq!(crate::i18n::current(), crate::i18n::system_lang());
    }
}
