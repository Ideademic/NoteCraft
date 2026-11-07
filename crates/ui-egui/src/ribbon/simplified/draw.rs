//! Simplified Draw: Type, Lasso Select, Eraser | the pen strip (the pens used last) and Pens▾ (the
//! whole preset gallery) | Color & Thickness▾ | Shapes▾ | Ink to Shape | Delete.
//!
//! As on the classic tab, the tools only set [`crate::canvas::CanvasState`] (`set_tool` and the
//! pen styles); Ink to Shape and Delete run engine commands. Shapes▾ and the Ink to Shape state
//! come from the classic Draw tab ([`shapes_menu`], [`ink_to_shape_on`]).

use egui::{Color32, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use notecraft_doc::InkTool;
use notecraft_engine::ids;
use serde_json::json;

use super::Segment;
use crate::canvas::{PenStyle, Tool};
use crate::icons::Icon;
use crate::ribbon::draw::{HIGHLIGHTER_WIDTHS, PEN_WIDTHS, PRESETS, Preset, ink_to_shape_on, shapes_menu, use_preset};
use crate::theme::{RADIUS_SMALL, color32, model_color};
use crate::widgets::{self, Btn, color};
use crate::{NoteApp, tl};

pub const SEGMENTS: [Segment; 6] = [
    Segment { key: "tools", draw: tools },
    Segment { key: "pens", draw: pens },
    Segment { key: "color", draw: color_thickness },
    Segment { key: "shapes", draw: shapes },
    Segment { key: "convert", draw: convert },
    Segment { key: "edit", draw: edit },
];

/// How many pens the strip shows.
pub const STRIP_PENS: usize = 5;

fn tool_of(t: InkTool) -> Tool {
    match t {
        InkTool::Pen => Tool::Pen,
        InkTool::Pencil => Tool::Pencil,
        InkTool::Highlighter => Tool::Highlighter,
    }
}

fn tool_name(t: InkTool) -> &'static str {
    match t {
        InkTool::Pen => tl!("Pen"),
        InkTool::Pencil => tl!("Pencil"),
        InkTool::Highlighter => tl!("Highlighter"),
    }
}

fn tool_icon(t: InkTool) -> Icon {
    match t {
        InkTool::Pen => Icon::Pen,
        InkTool::Pencil => Icon::Pencil,
        InkTool::Highlighter => Icon::Highlighter,
    }
}

fn recent_id() -> egui::Id {
    egui::Id::unique("nc_simple_recent_pens")
}

/// The pens on the strip, most recently picked first (a black, blue and red pen, a pencil and a
/// yellow highlighter to start with).
pub fn recent_pens(ctx: &egui::Context) -> Vec<Preset> {
    ctx.data(|d| d.get_temp::<Vec<Preset>>(recent_id()))
        .unwrap_or_else(|| [0usize, 1, 2, 6, 8].iter().filter_map(|i| PRESETS.get(*i).copied()).collect())
}

/// Picks a pen and puts it first on the strip.
fn pick(ctx: &egui::Context, app: &mut NoteApp, p: Preset) {
    use_preset(app, p);
    let mut recent = recent_pens(ctx);
    recent.retain(|q| *q != p);
    recent.insert(0, p);
    recent.truncate(STRIP_PENS);
    ctx.data_mut(|d| d.insert_temp(recent_id(), recent));
}

fn is_current(app: &NoteApp, p: Preset) -> bool {
    tool_of(p.tool) == app.canvas.tool && app.canvas.pens.style(p.tool) == PenStyle { color: p.color, width: p.width }
}

fn preset_name(p: Preset) -> String {
    format!("{} {} ({:.1} pt)", tool_name(p.tool), color::hex(color32(p.color)), p.width)
}

fn tools(ui: &mut Ui, app: &mut NoteApp) {
    for (tool, icon, label, help) in [
        (Tool::Type, Icon::Type, tl!("Type"), tl!("Click on the page to type.")),
        (Tool::Lasso, Icon::Lasso, tl!("Lasso Select"), tl!("Draw around ink to select it.")),
        (Tool::Eraser, Icon::Eraser, tl!("Eraser"), tl!("Erase whole strokes you touch.")),
    ] {
        let b = Btn::new(icon, label).icon_only().help(help).checked(app.canvas.tool == tool);
        if widgets::small(ui, app, b) {
            app.canvas.set_tool(tool);
        }
    }
}

fn pens(ui: &mut Ui, app: &mut NoteApp) {
    let ctx = ui.ctx().clone();
    for p in recent_pens(&ctx) {
        let name = preset_name(p);
        let b = Btn::new(tool_icon(p.tool), &name).icon_only().swatch(Some(color32(p.color))).checked(is_current(app, p));
        if widgets::small(ui, app, b) {
            pick(&ctx, app, p);
        }
    }
    let b = Btn::new(Icon::Pen, tl!("Pens")).icon_only().help(tl!("All the pens, pencils and highlighters."));
    widgets::small_menu(ui, app, b, pens_gallery);
}

/// Pens▾: every preset as a tile with a sample stroke.
fn pens_gallery(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let ctx = ui.ctx().clone();
    widgets::menu_heading(ui, tl!("Pens"));
    let cols = 6;
    let tile = vec2(30.0, 30.0);
    let rows = PRESETS.len().div_ceil(cols);
    let (outer, _) = ui.allocate_exact_size(vec2(tile.x * cols as f32 + 4.0, tile.y * rows as f32 + 4.0), Sense::hover());
    // Paper behind the samples, so dark inks show in the dark theme too.
    let paper = if t.dark { Color32::from_rgb(0xE6, 0xE6, 0xE8) } else { Color32::WHITE };
    ui.painter().rect(outer, RADIUS_SMALL, paper, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    for (i, p) in PRESETS.iter().enumerate() {
        let (cx, cy) = (i % cols, i / cols);
        let r = Rect::from_min_size(outer.min + vec2(2.0 + tile.x * cx as f32, 2.0 + tile.y * cy as f32), tile);
        let resp = ui.interact(r, ui.scope_id().with(("nc_simple_pen", i)), Sense::click());
        let name = preset_name(*p);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
        widgets::bg(ui, r.shrink(1.0), &resp, is_current(app, *p), true, &t);
        paint_sample(ui, r.shrink(5.0), *p);
        if resp.on_hover_text(name).clicked() {
            pick(&ctx, app, *p);
            ui.close();
        }
    }
}

/// A preset's sample: a wave in its colour and weight.
fn paint_sample(ui: &Ui, r: Rect, p: Preset) {
    let c = color32(p.color);
    let w = (p.width * 0.9).clamp(1.0, 7.0);
    let n = 14;
    let pts: Vec<egui::Pos2> = (0..=n)
        .map(|i| {
            let k = i as f32 / n as f32;
            pos2(r.min.x + k * r.width(), r.center().y + (k * std::f32::consts::TAU).sin() * r.height() * 0.25)
        })
        .collect();
    let fill = if p.tool == InkTool::Highlighter { crate::theme::mix(c, Color32::WHITE, 0.25) } else { c };
    ui.painter().add(Shape::line(pts, Stroke::new(w, fill)));
}

/// The drawing tool Color & Thickness changes: the current one, else the pen.
fn ink_target(app: &NoteApp) -> InkTool {
    app.canvas.tool.ink_tool().unwrap_or(InkTool::Pen)
}

fn color_thickness(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::ColorThickness, tl!("Color & Thickness")).icon_only().help(tl!("Choose the ink colour and the line thickness."));
    widgets::small_menu(ui, app, b, color_thickness_menu);
}

fn color_thickness_menu(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let ink = ink_target(app);
    app.canvas.set_tool(tool_of(ink));
    ui.horizontal(|ui| {
        for k in InkTool::ALL {
            if ui.selectable_label(k == ink, tool_name(k)).clicked() {
                app.canvas.set_tool(tool_of(k));
            }
        }
    });
    ui.separator();
    let ink = ink_target(app);
    let style = app.canvas.pens.style(ink);
    widgets::menu_heading(ui, tl!("Thickness"));
    let widths: &[f32] = if ink == InkTool::Highlighter { &HIGHLIGHTER_WIDTHS } else { &PEN_WIDTHS };
    for w in widths {
        let (r, resp) = ui.allocate_exact_size(vec2(180.0, 20.0), Sense::click());
        let label = format!("{w} pt");
        let on = (style.width - w).abs() < 0.01;
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, &label));
        if resp.hovered() || on {
            ui.painter().rect_filled(r, RADIUS_SMALL, if resp.hovered() { t.hover } else { t.checked });
        }
        ui.painter().text(pos2(r.min.x + 6.0, r.center().y), egui::Align2::LEFT_CENTER, &label, crate::theme::regular(11.5), t.text);
        ui.painter().line_segment(
            [pos2(r.min.x + 56.0, r.center().y), pos2(r.max.x - 8.0, r.center().y)],
            Stroke::new(w.clamp(0.5, 12.0), color32(style.color)),
        );
        if resp.clicked() {
            app.canvas.pens.style_mut(ink).width = *w;
            ui.close();
        }
    }
    ui.separator();
    widgets::menu_heading(ui, tl!("Color"));
    let palette: &[Color32] = if ink == InkTool::Highlighter { &color::HIGHLIGHT } else { &color::INK };
    if let Some(c) = color::grid(ui, palette, 8, Some(color32(style.color))) {
        app.canvas.pens.style_mut(ink).color = model_color(c);
    }
    if !app.canvas.selection.strokes.is_empty() {
        ui.separator();
        if widgets::menu_item(ui, Some(Icon::ColorThickness), tl!("Recolor Selected Ink"), app.can(ids::DRAW_RECOLOR)) {
            let c = app.canvas.pens.style(ink).color;
            app.act(ids::DRAW_RECOLOR, json!({ "color": c.to_hex() }));
        }
    }
}

fn shapes(ui: &mut Ui, app: &mut NoteApp) {
    let current = matches!(app.canvas.tool, Tool::Shape(_));
    let b = Btn::new(Icon::Shapes, tl!("Shapes")).checked(current).help(tl!("Draw a line, a shape or graph axes."));
    widgets::small_menu(ui, app, b, shapes_menu);
}

fn convert(ui: &mut Ui, app: &mut NoteApp) {
    let b = Btn::new(Icon::InkToShape, tl!("Ink to Shape"))
        .help(tl!("Turn drawn shapes into neat ones as you draw. With ink selected, converts the selection."))
        .for_command(app, ids::DRAW_INK_TO_SHAPE)
        .checked(ink_to_shape_on(app));
    if widgets::small(ui, app, b) {
        app.act(ids::DRAW_INK_TO_SHAPE, json!({}));
    }
}

fn edit(ui: &mut Ui, app: &mut NoteApp) {
    let has_sel = !app.canvas.selection.strokes.is_empty() || !app.session.ink_selection.is_empty();
    let b = Btn::new(Icon::Delete, tl!("Delete")).icon_only().help(tl!("Delete the selected ink.")).for_command(app, ids::DRAW_DELETE_SELECTION);
    let b = Btn { enabled: b.enabled && has_sel, ..b };
    if widgets::small(ui, app, b) {
        // The canvas mirrors its lasso selection into `Session::ink_selection`, which the command
        // deletes.
        app.act(ids::DRAW_DELETE_SELECTION, json!({}));
        app.canvas.selection.strokes.clear();
    }
}
