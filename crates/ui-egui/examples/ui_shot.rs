//! Headless screenshots of the whole NoteCraft window, rendered offscreen with wgpu (works with
//! a locked screen or no display).
//!
//! `cargo run -p notecraft-ui-egui --example ui_shot -- OUT_DIR [light|dark|all] [NAME_PREFIX]`
//!
//! Writes `main-light.png`, `main-dark.png`, one per ribbon tab (`tab-insert-light.png`…), the
//! backstage pages, the collapsed panes, and the ribbon's layouts and display options
//! (`simplified-home-light.png`, `ribbon-full-screen-light.png`…), at 1440×900 pt and 2× scale.

use notecraft_engine::Session;
use notecraft_ui_egui::{Appearance, NoteApp, Services};

/// An optional third argument: only the shots whose names start with it (`history`).
fn args_filter() -> Option<String> {
    std::env::args().nth(3)
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = std::path::PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let which = args.next().unwrap_or_else(|| "all".into());
    std::fs::create_dir_all(&out).expect("output folder");
    let themes: Vec<(Appearance, &str)> = match which.as_str() {
        "light" => vec![(Appearance::Light, "light")],
        "dark" => vec![(Appearance::Dark, "dark")],
        _ => vec![(Appearance::Light, "light"), (Appearance::Dark, "dark")],
    };
    for (appearance, name) in themes {
        let filter = args_filter();
        let shots = notecraft_ui_egui::app::shots::SHOTS
            .iter()
            .chain(notecraft_ui_egui::app::shots::RIBBON_SHOTS.iter())
            .chain(notecraft_ui_egui::app::shots::HISTORY_SHOTS.iter())
            .filter(|s| filter.as_ref().is_none_or(|f| s.starts_with(f.as_str())));
        for &shot in shots {
            let mut app = NoteApp::new(Session::new_default(), Services::default());
            app.ui.prefs.appearance = appearance;
            notecraft_ui_egui::app::shots::prepare(&mut app, shot);
            let mut harness = egui_kittest::Harness::builder()
                // kittest puts an 8 pt margin round the root ui (eframe doesn't): render that
                // much bigger and crop it off, so the shot is the window as it looks.
                .with_size(egui::vec2(1440.0 + 16.0, 900.0 + 16.0))
                .with_pixels_per_point(2.0)
                .wgpu()
                .build_ui_state(
                    |ui, app: &mut NoteApp| {
                        let ctx = ui.ctx().clone();
                        app.logic(&ctx);
                        app.ui(ui);
                    },
                    app,
                );
            harness.run_steps(8);
            // Ribbon display options: the menu is open in its shot.
            if shot == "ribbon-display-menu" {
                use egui_kittest::kittest::Queryable;
                harness.get_by_label(notecraft_ui_egui::ribbon::display::OPTIONS_LABEL).click();
                harness.run_steps(4);
            }
            let path = out.join(format!("{shot}-{name}.png"));
            match harness.render() {
                Ok(img) => {
                    let img = image::imageops::crop_imm(&img, 16, 16, 2880, 1800).to_image();
                    img.save(&path).expect("save png");
                    println!("{}", path.display());
                }
                Err(e) => eprintln!("{shot}: render failed: {e}"),
            }
        }
    }
}
