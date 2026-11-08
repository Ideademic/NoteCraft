//! Software updates (desktop): a check at start when File › Options says so, File › About ›
//! Check for Updates, the update prompt and the self-update, all from `craft-update`
//! (craft-libs), as in WordCraft. NoteCraft's releases are published on GitHub
//! (`Ideademic/NoteCraft`) with suite-style asset names and `SHA256SUMS.txt`, which is what
//! craft-update reads. The web build has nothing to update: a reload gets the newest version.

use crate::NoteApp;

/// NoteCraft as craft-update knows it: a community app (not in the suite catalog), with its
/// 128 px icon and accent colour for the update prompt.
#[cfg(not(target_arch = "wasm32"))]
pub const UPDATE_APP: craft_update::App = craft_update::App {
    id: "notecraft",
    name: "NoteCraft",
    tagline: "Notebooks, sections and pages",
    repo: "Ideademic/NoteCraft",
    former_ids: &[],
    former_names: &[],
    icon_png: Some(include_bytes!("../../../assets/app-icon/hicolor/128x128/apps/com.ideademic.notecraft.png")),
    accent: Some([0xc4, 0x4d, 0xf0]),
    developer: "Ideademic",
};

/// The update flow's state, kept in [`NoteApp::updates`].
pub struct Updates {
    #[cfg(not(target_arch = "wasm32"))]
    updater: craft_update::updater::Updater,
    #[cfg(not(target_arch = "wasm32"))]
    result_dialog: craft_update::CheckResultDialog,
    /// The desktop app turns the check at start on (and the preference can turn it off); tests and
    /// the web build leave it off, so they never reach the network.
    startup_check: bool,
    check_requested: bool,
    /// The Check for Updates window is open.
    pub manual_window_open: bool,
}

// On the web every field is a plain default, but the desktop's aren't.
#[cfg_attr(target_arch = "wasm32", allow(clippy::derivable_impls))]
impl Default for Updates {
    fn default() -> Self {
        Updates {
            #[cfg(not(target_arch = "wasm32"))]
            updater: craft_update::updater::Updater::new(&UPDATE_APP, env!("CARGO_PKG_VERSION")),
            #[cfg(not(target_arch = "wasm32"))]
            result_dialog: craft_update::CheckResultDialog::new(&UPDATE_APP, env!("CARGO_PKG_VERSION")),
            startup_check: false,
            check_requested: false,
            manual_window_open: false,
        }
    }
}

impl Updates {
    /// Whether this build can update itself at all (the desktop app; not the web).
    pub const AVAILABLE: bool = cfg!(not(target_arch = "wasm32"));

    /// Allows the check at start (the desktop app calls this once; the preference still decides).
    pub fn enable_startup_check(&mut self) {
        self.startup_check = true;
    }

    /// Whether the check at start is allowed (see [`Updates::enable_startup_check`]).
    pub fn startup_check_enabled(&self) -> bool {
        self.startup_check
    }

    /// Answers checks from `source` instead of GitHub (tests and screenshots).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_source(mut self, source: craft_update::updater::CheckSource) -> Self {
        self.updater = craft_update::updater::Updater::new(&UPDATE_APP, env!("CARGO_PKG_VERSION")).with_source(source);
        self
    }

    /// What the last requested check came to.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn status(&self) -> &craft_update::updater::Status {
        self.updater.status()
    }

    /// Whether the prompt offering a newer release is showing.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn prompt_open(&self) -> bool {
        self.updater.prompt_open()
    }
}

/// File › About › Check for Updates: a check whose outcome shows in its own window.
pub fn check(app: &mut NoteApp) {
    if Updates::AVAILABLE {
        app.updates.check_requested = true;
        app.updates.manual_window_open = true;
    }
}

/// Once per frame, before drawing: starts a requested check (or the one at start) and picks up
/// finished ones.
pub fn tick(app: &mut NoteApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let at_start = app.ui.prefs.check_for_updates;
        let u = &mut app.updates;
        if std::mem::take(&mut u.check_requested) {
            u.updater.check_now(ctx);
        } else if u.startup_check && at_start {
            u.updater.check_at_start(ctx);
        }
        u.updater.tick(ctx);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (app, ctx);
}

/// After the frame's panels: the update prompt and the Check for Updates window.
pub fn show(app: &mut NoteApp, ctx: &egui::Context) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use craft_update::CheckResultResponse;
        use craft_update::updater::{Action, Status};
        // The restart is scheduled before the app closes, so no update starts over unsaved work
        // (autosave clears it within seconds).
        let clean = !app.session.is_dirty();
        match app.updates.updater.show_if(ctx, clean) {
            Action::OpenUrl(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            Action::Close => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Action::None => {}
        }
        let u = &mut app.updates;
        if u.manual_window_open {
            match u.result_dialog.show(ctx, u.updater.status()) {
                CheckResultResponse::None => {}
                CheckResultResponse::Close => {
                    u.manual_window_open = false;
                    u.updater.dismiss_status();
                }
                CheckResultResponse::CheckAgain => u.updater.check_now(ctx),
            }
            // An update was found: the prompt takes over from the window. (A check asked for this
            // frame starts at the next tick: until then the status is still idle.)
            if matches!(u.updater.status(), Status::Idle) && !u.check_requested {
                u.manual_window_open = false;
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (app, ctx);
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::sync::Arc;

    use craft_update::updater::Status;

    use super::*;

    fn app() -> NoteApp {
        NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default())
    }

    #[test]
    fn the_community_app_entry() {
        assert_eq!(UPDATE_APP.id, "notecraft");
        assert_eq!(UPDATE_APP.repo, "Ideademic/NoteCraft");
        assert_eq!(UPDATE_APP.developer, "Ideademic");
        assert_eq!(UPDATE_APP.accent, Some([0xc4, 0x4d, 0xf0]));
        assert_eq!(UPDATE_APP.releases_url(), "https://github.com/Ideademic/NoteCraft/releases");
        let icon = UPDATE_APP.icon_png.unwrap_or_default();
        assert_eq!(icon.get(1..4), Some(&b"PNG"[..]));
        assert!(craft_update::App::by_id("notecraft").is_none(), "not in the suite catalog: the entry is ours");
    }

    #[test]
    fn release_assets_are_named_as_craft_update_reads_them() {
        // The names `.github/workflows/release.yml` publishes.
        let names = [
            "notecraft-0.2.0-macos-universal.dmg",
            "notecraft-0.2.0-windows-x64.msi",
            "notecraft-0.2.0-windows-arm64-portable.zip",
            "notecraft-0.2.0-linux-x86_64.AppImage",
            "notecraft-0.2.0-linux-aarch64.tar.gz",
            "notecraft-0.2.0-freebsd-x86_64.tar.gz",
        ];
        for n in names {
            let parsed = craft_update::platform::parse_asset_name(n, &["notecraft"]);
            assert!(parsed.is_some_and(|a| a.version == "0.2.0"), "{n}");
        }
        for other in ["notecraft-cli-0.2.0-macos-universal.zip", "notecraft-web-0.2.0.zip", "SHA256SUMS.txt"] {
            assert!(craft_update::platform::parse_asset_name(other, &["notecraft"]).is_none(), "{other}");
        }
    }

    #[test]
    fn startup_check_is_off_unless_the_app_turns_it_on() {
        let mut a = app();
        assert!(!a.updates.startup_check_enabled(), "tests and the web never check at start");
        assert!(a.ui.prefs.check_for_updates, "the preference defaults to on");
        // Ticking without the app's go-ahead starts nothing.
        let ctx = egui::Context::default();
        tick(&mut a, &ctx);
        assert_eq!(a.updates.status(), &Status::Idle);
        a.updates.enable_startup_check();
        assert!(a.updates.startup_check_enabled());
    }

    /// Ticks until the requested check is answered.
    fn wait(a: &mut NoteApp, done: impl Fn(&Status) -> bool) {
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !done(a.updates.status()) {
            assert!(std::time::Instant::now() < deadline, "the check never ended: {:?}", a.updates.status());
            tick(a, &ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn a_requested_check_reports_up_to_date_or_the_failure() {
        let mut a = app();
        a.updates = Updates::default().with_source(Arc::new(|| Ok(None)));
        check(&mut a);
        assert!(a.updates.manual_window_open);
        wait(&mut a, |s| matches!(s, Status::UpToDate));
        assert!(!a.updates.prompt_open());

        let mut a = app();
        a.updates = Updates::default().with_source(Arc::new(|| Err("offline".into())));
        check(&mut a);
        wait(&mut a, |s| matches!(s, Status::Failed(why) if why == "offline"));
    }

    #[test]
    fn a_newer_release_opens_the_prompt() {
        let json = r#"{"tag_name":"v99.0.0","name":"NoteCraft v99.0.0","prerelease":false,"draft":false,"published_at":"2026-11-01T00:00:00Z","body":"","assets":[]}"#;
        let release = craft_update::parse_github_release(json, "Ideademic/NoteCraft");
        let Ok(release) = release else { panic!("fixture parses: {release:?}") };
        let mut a = app();
        a.updates = Updates::default().with_source(Arc::new(move || Ok(Some(release.clone()))));
        check(&mut a);
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !a.updates.prompt_open() {
            assert!(std::time::Instant::now() < deadline, "no prompt: {:?}", a.updates.status());
            tick(&mut a, &ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(a.updates.status(), &Status::Idle);
    }
}
