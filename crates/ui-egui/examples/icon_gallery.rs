//! Renders every NoteCraft icon into a labelled PNG grid: on a light and a dark ribbon face, and on
//! the purple title bar and File-menu rail.
//!
//! ```sh
//! cargo run -p notecraft-ui-egui --example icon_gallery -- /tmp/icons.png [filter]
//! cargo run -p notecraft-ui-egui --example icon_gallery -- /tmp/deck.png --deck
//! ```
//!
//! Each cell shows the icon at 32, 20, 16 and 12 pt, then at 16 pt disabled and flat, with its
//! name underneath. A second argument keeps the icons whose name contains it; `--deck` draws the
//! whole vendored DeckCraft set and NoteCraft's extra drawings instead.

use egui::{Align2, Color32, FontId, Painter, Rect, pos2, vec2};
use notecraft_ui_egui::icons::{self, Icon, Style, deck};

const CELL_W: f32 = 170.0;
const CELL_H: f32 = 56.0;
const PAD: f32 = 12.0;

struct Face {
    bg: Color32,
    ink: Color32,
    label: Color32,
}

/// One cell's icon drawn at `size` in `rect` in the given style.
type Painted = Box<dyn Fn(&Painter, Rect, Face2)>;

#[derive(Clone, Copy)]
struct Face2 {
    ink: Color32,
    disabled: bool,
    flat: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "icon_gallery.png".to_owned());
    let filter = args.next();
    let cells: Vec<(String, Painted)> = if filter.as_deref() == Some("--deck") {
        let mut v: Vec<(String, Painted)> = deck::Icon::ALL
            .iter()
            .map(|&i| {
                let f: Painted = Box::new(move |p: &Painter, r: Rect, s: Face2| {
                    let mode = if s.disabled {
                        deck::Mode::Disabled
                    } else if s.flat {
                        deck::Mode::Flat
                    } else {
                        deck::Mode::Colour
                    };
                    deck::paint_mode(p, r, i, s.ink, mode);
                });
                (i.name().to_owned(), f)
            })
            .collect();
        for &x in deck::extra::Extra::ALL {
            let f: Painted = Box::new(move |p: &Painter, r: Rect, s: Face2| {
                let mode = if s.disabled {
                    deck::Mode::Disabled
                } else if s.flat {
                    deck::Mode::Flat
                } else {
                    deck::Mode::Colour
                };
                deck::extra::paint_mode(p, r, x, s.ink, mode);
            });
            v.push((format!("+{x:?}"), f));
        }
        v
    } else {
        let mut v: Vec<(String, Painted)> = Icon::ALL
            .iter()
            .filter(|i| filter.as_deref().is_none_or(|f| format!("{i:?}").to_lowercase().contains(&f.to_lowercase())))
            .map(|&i| {
                let f: Painted = Box::new(move |p: &Painter, r: Rect, s: Face2| {
                    let style = if s.disabled {
                        Style::disabled(s.ink)
                    } else if s.flat {
                        Style::flat(s.ink)
                    } else {
                        Style::mono(s.ink)
                    };
                    icons::paint_styled(p, r, i, style);
                });
                (format!("{i:?}"), f)
            })
            .collect();
        let nb: Painted = Box::new(|p: &Painter, r: Rect, _| icons::paint_notebook(p, r, Color32::from_rgb(0x4A, 0x90, 0xD9)));
        v.push(("paint_notebook".to_owned(), nb));
        let mark: Painted = Box::new(|p: &Painter, r: Rect, _| icons::paint_app_mark(p, r));
        v.push(("paint_app_mark".to_owned(), mark));
        v
    };

    // Keep the image within the GPU's 8192 px texture limit.
    let cols = if cells.len() > 120 { 14 } else { 8 };
    let rows = cells.len().div_ceil(cols);
    let panel_h = rows as f32 * CELL_H + PAD * 2.0;
    let width = cols as f32 * CELL_W + PAD * 2.0;
    let faces = [
        Face { bg: Color32::from_rgb(0xff, 0xff, 0xff), ink: Color32::from_rgb(0x44, 0x44, 0x44), label: Color32::from_rgb(0x70, 0x70, 0x70) },
        Face { bg: Color32::from_rgb(0x2b, 0x2b, 0x2b), ink: Color32::from_rgb(0xe8, 0xe8, 0xe8), label: Color32::from_rgb(0xa8, 0xa8, 0xa8) },
        Face { bg: notecraft_ui_egui::theme::APP_COLOR, ink: Color32::WHITE, label: Color32::from_rgb(0xf4, 0xe4, 0xfc) },
    ];

    let mut harness =
        egui_kittest::Harness::builder().with_size(vec2(width, panel_h * faces.len() as f32)).with_pixels_per_point(2.0).wgpu().build_ui(|ui| {
            let painter = ui.painter();
            for (fi, face) in faces.iter().enumerate() {
                let top = fi as f32 * panel_h;
                painter.rect_filled(Rect::from_min_size(pos2(0.0, top), vec2(width, panel_h)), 0.0, face.bg);
                for (n, (name, f)) in cells.iter().enumerate() {
                    let x = PAD + (n % cols) as f32 * CELL_W;
                    let y = top + PAD + (n / cols) as f32 * CELL_H;
                    let on = Face2 { ink: face.ink, disabled: false, flat: false };
                    let mut cx = x + 4.0;
                    for size in [32.0, 20.0, 16.0, 12.0] {
                        f(painter, Rect::from_min_size(pos2(cx, y + 2.0 + (32.0 - size) / 2.0), vec2(size, size)), on);
                        cx += size + 6.0;
                    }
                    f(painter, Rect::from_min_size(pos2(cx, y + 10.0), vec2(16.0, 16.0)), Face2 { disabled: true, ..on });
                    cx += 22.0;
                    f(painter, Rect::from_min_size(pos2(cx, y + 10.0), vec2(16.0, 16.0)), Face2 { flat: true, ..on });
                    painter.text(pos2(x + CELL_W * 0.5 - 6.0, y + 44.0), Align2::CENTER_CENTER, name, FontId::proportional(10.0), face.label);
                }
            }
        });
    harness.run();
    let image = harness.render()?;
    if let Some(dir) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(dir)?;
    }
    image.save(&out)?;
    println!("wrote {} icons to {out}", cells.len());
    Ok(())
}
