//! Draw: Tools (Type, Lasso Select, Eraser, Pen, Pencil, Highlighter and a gallery of preset
//! pens), Shapes (a gallery of lines, shapes and graph axes, as in OneNote 2016), Colour &
//! Thickness, Convert (Ink to Shape, Ink to Text), Edit (Insert Space, Delete, Arrange, Rotate).
//!
//! The tools only set [`crate::canvas::CanvasState`]: `set_tool` and the pen styles in `pens`;
//! the canvas draws with them (a shape tile picks [`Tool::Shape`]). Ink to Shape is a mode in the
//! session (`draw.inkToShape`), shown as a checked toggle. Delete and recolour act on the lasso
//! selection through engine commands.

use egui::{Color32, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};
use notecraft_doc::{Color, InkTool};
use notecraft_engine::ids;
use serde_json::json;

use crate::canvas::{PenStyle, ShapeKind, Tool};
use crate::icons::{self, Icon};
use crate::theme::{RADIUS_SMALL, color32, model_color};
use crate::widgets::{self, Btn, CONTENT_H, color};
use crate::{NoteApp, tl};

/// A preset in the pen gallery.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub tool: InkTool,
    pub color: Color,
    /// Points.
    pub width: f32,
}

const fn preset(tool: InkTool, r: u8, g: u8, b: u8, width: f32) -> Preset {
    Preset { tool, color: Color::rgb(r, g, b), width }
}

/// The pen gallery: pens in bold colours and two weights, a pencil, highlighters.
pub const PRESETS: [Preset; 12] = [
    preset(InkTool::Pen, 0x00, 0x00, 0x00, 1.0),
    preset(InkTool::Pen, 0x2F, 0x6F, 0xD6, 1.0),
    preset(InkTool::Pen, 0xD6, 0x2D, 0x20, 1.0),
    preset(InkTool::Pen, 0x2E, 0xA0, 0x4F, 1.0),
    preset(InkTool::Pen, 0xC4, 0x4D, 0xF0, 2.0),
    preset(InkTool::Pen, 0x00, 0x00, 0x00, 3.0),
    preset(InkTool::Pencil, 0x59, 0x59, 0x59, 1.0),
    preset(InkTool::Pencil, 0x1B, 0x3F, 0x86, 1.5),
    preset(InkTool::Highlighter, 0xFF, 0xFF, 0x00, 12.0),
    preset(InkTool::Highlighter, 0x7F, 0xF2, 0x5A, 12.0),
    preset(InkTool::Highlighter, 0xFF, 0x8A, 0xD8, 12.0),
    preset(InkTool::Highlighter, 0x5A, 0xE6, 0xF2, 12.0),
];

/// Widths offered for pens and pencils, and for highlighters (points).
pub const PEN_WIDTHS: [f32; 7] = [0.5, 1.0, 1.5, 2.0, 3.0, 5.0, 8.0];
pub const HIGHLIGHTER_WIDTHS: [f32; 5] = [6.0, 9.0, 12.0, 16.0, 24.0];

fn tool_of(t: InkTool) -> Tool {
    match t {
        InkTool::Pen => Tool::Pen,
        InkTool::Pencil => Tool::Pencil,
        InkTool::Highlighter => Tool::Highlighter,
    }
}

/// Picks a preset: its tool, with its colour and width.
pub fn use_preset(app: &mut NoteApp, p: Preset) {
    *app.canvas.pens.style_mut(p.tool) = PenStyle { color: p.color, width: p.width };
    app.canvas.set_tool(tool_of(p.tool));
}

/// The drawing tool the colour and thickness menu changes: the current one, else the pen.
fn ink_target(app: &NoteApp) -> InkTool {
    app.canvas.tool.ink_tool().unwrap_or(InkTool::Pen)
}

pub fn show(ui: &mut Ui, app: &mut NoteApp) {
    widgets::group(ui, app, tl!("Tools"), Icon::Pen, tools);
    widgets::group(ui, app, tl!("Pens"), Icon::ColorThickness, pens_gallery);
    widgets::group(ui, app, tl!("Shapes"), Icon::Shapes, shapes_gallery);
    widgets::group(ui, app, tl!("Color"), Icon::ColorThickness, |ui, app| {
        let b = Btn::new(Icon::ColorThickness, tl!("Color &\nThickness"))
            .tip(tl!("Color & Thickness"))
            .help(tl!("Choose the ink colour and the line thickness."));
        widgets::big_menu(ui, app, b, color_thickness_menu);
    });
    widgets::group(ui, app, tl!("Convert"), Icon::InkToShape, |ui, app| {
        let b = Btn::new(Icon::InkToShape, tl!("Ink to\nShape"))
            .tip(tl!("Ink to Shape"))
            .help(tl!("Turn drawn shapes into neat ones as you draw. With ink selected, converts the selection."))
            .for_command(app, ids::DRAW_INK_TO_SHAPE)
            .checked(ink_to_shape_on(app));
        if widgets::big(ui, app, b) {
            app.act(ids::DRAW_INK_TO_SHAPE, json!({}));
        }
        let b = Btn::new(Icon::InkToText, tl!("Ink to\nText"))
            .tip(tl!("Ink to Text"))
            .enabled(false)
            .help(tl!("Handwriting recognition isn't part of NoteCraft yet."));
        widgets::big(ui, app, b);
    });
    widgets::group(ui, app, tl!("Edit"), Icon::Delete, edit);
}

fn tools(ui: &mut Ui, app: &mut NoteApp) {
    for (tool, icon, label, help) in [
        (Tool::Type, Icon::Type, tl!("Type"), tl!("Click on the page to type.")),
        (Tool::Lasso, Icon::Lasso, tl!("Lasso\nSelect"), tl!("Draw around ink to select it.")),
        (Tool::Eraser, Icon::Eraser, tl!("Eraser"), tl!("Erase whole strokes you touch.")),
    ] {
        let tip = label.replace('\n', " ");
        let b = Btn::new(icon, label).tip(&tip).help(help).checked(app.canvas.tool == tool);
        if widgets::big(ui, app, b) {
            app.canvas.set_tool(tool);
        }
    }
    for (ink, icon, label) in [
        (InkTool::Pen, Icon::Pen, tl!("Pen")),
        (InkTool::Pencil, Icon::Pencil, tl!("Pencil")),
        (InkTool::Highlighter, Icon::Highlighter, tl!("Highlighter")),
    ] {
        let tool = tool_of(ink);
        let b = Btn::new(icon, label).help(tl!("Draw on the page. The arrow picks the colour and thickness.")).checked(app.canvas.tool == tool);
        if widgets::big_split(ui, app, b, |ui, app| {
            app.canvas.set_tool(tool);
            style_menu(ui, app, ink);
        }) {
            app.canvas.set_tool(tool);
        }
    }
}

fn pens_gallery(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let cols = 6;
    let tile = vec2(26.0, 30.0);
    let (outer, _) = ui.allocate_exact_size(vec2(tile.x * cols as f32 + 4.0, CONTENT_H - 2.0), Sense::hover());
    // Paper behind the samples, so dark inks show in the dark theme too.
    let paper = if t.dark { Color32::from_rgb(0xE6, 0xE6, 0xE8) } else { Color32::WHITE };
    ui.painter().rect(outer, RADIUS_SMALL, paper, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    for (i, p) in PRESETS.iter().enumerate() {
        let (cx, cy) = (i % cols, i / cols);
        let r = Rect::from_min_size(outer.min + vec2(2.0 + tile.x * cx as f32, 2.0 + tile.y * cy as f32), tile);
        let resp = ui.interact(r, ui.scope_id().with(("pen_preset", i)), Sense::click());
        let name = format!("{} {} ({:.1} pt)", tool_name(p.tool), color::hex(color32(p.color)), p.width);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
        let current = tool_of(p.tool) == app.canvas.tool && app.canvas.pens.style(p.tool) == PenStyle { color: p.color, width: p.width };
        widgets::bg(ui, r.shrink(1.0), &resp, current, true, &t);
        paint_preset(ui, r.shrink(4.0), *p);
        let keyed = app.ui.keytips.control(r);
        if resp.on_hover_text(name).clicked() || keyed {
            use_preset(app, *p);
        }
    }
}

/// Columns of the Shapes gallery (three rows fill the group).
const SHAPE_COLS: usize = 6;

fn shapes_gallery(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    let rows = ShapeKind::ALL.len().div_ceil(SHAPE_COLS);
    let tile = vec2(26.0, ((CONTENT_H - 4.0) / rows as f32).floor());
    let (outer, _) = ui.allocate_exact_size(vec2(tile.x * SHAPE_COLS as f32 + 4.0, CONTENT_H - 2.0), Sense::hover());
    ui.painter().rect(outer, RADIUS_SMALL, t.canvas, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    shape_tiles(ui, app, outer.min + vec2(2.0, 2.0), SHAPE_COLS, tile, "shape_tile");
}

/// The shape tiles in a grid of `cols` from `origin`; a tile picks its shape tool. Returns
/// whether one was picked.
fn shape_tiles(ui: &mut Ui, app: &mut NoteApp, origin: egui::Pos2, cols: usize, tile: egui::Vec2, salt: &str) -> bool {
    let t = app.tokens.clone();
    let mut picked = false;
    for (i, kind) in ShapeKind::ALL.into_iter().enumerate() {
        let (cx, cy) = (i % cols.max(1), i / cols.max(1));
        let r = Rect::from_min_size(origin + vec2(tile.x * cx as f32, tile.y * cy as f32), tile);
        let resp = ui.interact(r, ui.scope_id().with((salt, i)), Sense::click());
        let name = shape_label(kind);
        let current = app.canvas.tool == Tool::Shape(kind);
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, current, name));
        widgets::bg(ui, r.shrink(1.0), &resp, current, true, &t);
        let inset = vec2((tile.x * 0.23).round(), (tile.y * 0.24).round());
        paint_shape_tile(ui, r.shrink2(inset), kind, t.icon);
        let keyed = app.ui.keytips.control(r);
        if resp.on_hover_text(name).clicked() || keyed {
            app.canvas.set_tool(Tool::Shape(kind));
            picked = true;
        }
    }
    picked
}

/// Columns of the Shapes drop-down.
const MENU_COLS: usize = 4;

/// The body of a Shapes drop-down (the simplified ribbon's): the shape tiles, larger; choosing
/// one picks its shape tool and closes the menu.
pub fn shapes_menu(ui: &mut Ui, app: &mut NoteApp) {
    widgets::menu_heading(ui, tl!("Shapes"));
    let tile = vec2(34.0, 30.0);
    let rows = ShapeKind::ALL.len().div_ceil(MENU_COLS);
    let (outer, _) = ui.allocate_exact_size(vec2(tile.x * MENU_COLS as f32, tile.y * rows as f32), Sense::hover());
    if shape_tiles(ui, app, outer.min, MENU_COLS, tile, "shape_menu_tile") {
        ui.close();
    }
}

/// Whether Ink to Shape is on (the engine's mode; the toggle's checked state).
pub fn ink_to_shape_on(app: &NoteApp) -> bool {
    app.session.ink_to_shape()
}

/// A shape's gallery tile: the shape itself, from the same geometry `draw.shape` inserts (lines
/// run from the bottom left to the top right).
fn paint_shape_tile(ui: &Ui, r: Rect, kind: ShapeKind, color: Color32) {
    let rect = if kind.is_line() { [r.min.x, r.max.y, r.width(), -r.height()] } else { [r.min.x, r.min.y, r.width(), r.height()] };
    for st in notecraft_engine::cmd::shapes::shape_strokes(kind, rect, false) {
        let pts: Vec<egui::Pos2> = st.into_iter().map(|[x, y]| pos2(x, y)).collect();
        ui.painter().add(Shape::line(pts, Stroke::new(1.2, color)));
    }
}

/// A shape's name in the gallery.
pub fn shape_label(kind: ShapeKind) -> &'static str {
    match kind {
        ShapeKind::Line => tl!("Line"),
        ShapeKind::Arrow => tl!("Arrow"),
        ShapeKind::DoubleArrow => tl!("Double Arrow"),
        ShapeKind::Rectangle => tl!("Rectangle"),
        ShapeKind::RoundedRectangle => tl!("Rounded Rectangle"),
        ShapeKind::Ellipse => tl!("Ellipse"),
        ShapeKind::Triangle => tl!("Triangle"),
        ShapeKind::RightTriangle => tl!("Right Triangle"),
        ShapeKind::Parallelogram => tl!("Parallelogram"),
        ShapeKind::Trapezoid => tl!("Trapezoid"),
        ShapeKind::Diamond => tl!("Diamond"),
        ShapeKind::Pentagon => tl!("Pentagon"),
        ShapeKind::Hexagon => tl!("Hexagon"),
        ShapeKind::Axes2 => tl!("Graph: X-Y Axes"),
        ShapeKind::Axes2Arrows => tl!("Graph: Coordinate Plane"),
        ShapeKind::Axes3 => tl!("Graph: 3-D Axes"),
    }
}

fn tool_name(t: InkTool) -> &'static str {
    match t {
        InkTool::Pen => tl!("Pen"),
        InkTool::Pencil => tl!("Pencil"),
        InkTool::Highlighter => tl!("Highlighter"),
    }
}

/// A preset's tile: a sample stroke in its colour and weight, with the tool's nib at its end.
fn paint_preset(ui: &Ui, r: Rect, p: Preset) {
    let c = color32(p.color);
    let w = (p.width * 0.9).clamp(1.0, 7.0);
    let n = 14;
    let pts: Vec<egui::Pos2> = (0..=n)
        .map(|i| {
            let k = i as f32 / n as f32;
            pos2(r.min.x + 2.0 + k * (r.width() - 4.0), r.center().y + 2.0 + (k * std::f32::consts::TAU).sin() * r.height() * 0.22)
        })
        .collect();
    let fill = if p.tool == InkTool::Highlighter { crate::theme::mix(c, Color32::WHITE, 0.25) } else { c };
    ui.painter().add(Shape::line(pts, Stroke::new(w, fill)));
    let tip = pos2(r.max.x - 3.0, r.min.y + 4.0);
    let body = match p.tool {
        InkTool::Pen => c,
        InkTool::Pencil => icons::ink::YELLOW,
        InkTool::Highlighter => icons::ink::GREY,
    };
    ui.painter().circle_filled(tip, 3.0, body);
    ui.painter().circle_stroke(tip, 3.0, Stroke::new(0.8, Color32::from_rgb(0x22, 0x22, 0x26)));
}

/// Colour and thickness for one drawing tool.
fn style_menu(ui: &mut Ui, app: &mut NoteApp, ink: InkTool) {
    let t = app.tokens.clone();
    let style = app.canvas.pens.style(ink);
    widgets::menu_heading(ui, tl!("Thickness"));
    let widths: &[f32] = if ink == InkTool::Highlighter { &HIGHLIGHTER_WIDTHS } else { &PEN_WIDTHS };
    for w in widths {
        let (r, resp) = ui.allocate_exact_size(vec2(180.0, 20.0), Sense::click());
        let label = format!("{w} pt");
        resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, (style.width - w).abs() < 0.01, &label));
        if resp.hovered() || (style.width - w).abs() < 0.01 {
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
}

fn color_thickness_menu(ui: &mut Ui, app: &mut NoteApp) {
    let ink = ink_target(app);
    app.canvas.set_tool(tool_of(ink));
    ui.horizontal(|ui| {
        for t in InkTool::ALL {
            if ui.selectable_label(t == ink, tool_name(t)).clicked() {
                app.canvas.set_tool(tool_of(t));
            }
        }
    });
    ui.separator();
    let ink = ink_target(app);
    style_menu(ui, app, ink);
    if !app.canvas.selection.strokes.is_empty() {
        ui.separator();
        if widgets::menu_item(ui, Some(Icon::ColorThickness), tl!("Recolor Selected Ink"), app.can(ids::DRAW_RECOLOR)) {
            let c = app.canvas.pens.style(ink).color;
            app.act(ids::DRAW_RECOLOR, json!({ "color": c.to_hex() }));
        }
    }
}

fn edit(ui: &mut Ui, app: &mut NoteApp) {
    widgets::rows(ui, app, |ui, app| {
        let b = Btn::new(Icon::InsertSpace, tl!("Insert Space"))
            .checked(app.canvas.tool == Tool::InsertSpace)
            .enabled(app.session.current_page().is_some())
            .help(tl!("Drag a line down the page to push what is below it further down (or up to take space away)."));
        if widgets::small(ui, app, b) {
            crate::ribbon::editing::insert_space(app);
        }
        let b = Btn::new(Icon::Delete, tl!("Delete"))
            .help(tl!("Delete the selected ink or items."))
            .enabled(crate::ribbon::editing::can_delete_selection(app));
        if widgets::small(ui, app, b) {
            crate::ribbon::editing::delete_selection(app);
        }
        let b = Btn::new(Icon::Arrange, tl!("Arrange"))
            .enabled(crate::ribbon::editing::can_arrange(app))
            .help(tl!("Bring the selected items or ink to the front, or send them back."));
        widgets::small_menu(ui, app, b, crate::ribbon::editing::arrange_menu);
    });
    widgets::rows(ui, app, |ui, app| {
        let b = Btn::new(Icon::Rotate, tl!("Rotate"))
            .enabled(crate::ribbon::editing::can_rotate(app))
            .help(tl!("Turn or flip the selected pictures or ink."));
        widgets::small_menu(ui, app, b, crate::ribbon::editing::rotate_menu);
        let one = crate::ribbon::editing::one_picture(app);
        let b = Btn::new(Icon::Crop, tl!("Crop")).enabled(one).help(tl!("Trim the edges of the selected picture."));
        if widgets::small(ui, app, b) {
            crate::ribbon::editing::crop(app);
        }
        let b = Btn::new(Icon::AltText, tl!("Alt Text")).enabled(one).help(tl!("Describe the selected picture for people who can't see it."));
        if widgets::small(ui, app, b) {
            crate::ribbon::editing::alt_text(app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_set_the_tool_and_its_style() {
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        for p in PRESETS {
            use_preset(&mut app, p);
            assert_eq!(app.canvas.tool.ink_tool(), Some(p.tool));
            assert_eq!(app.canvas.current_pen(), Some(PenStyle { color: p.color, width: p.width }));
            assert!(p.width >= notecraft_doc::ink::MIN_WIDTH && p.width <= notecraft_doc::ink::MAX_WIDTH);
        }
        for w in PEN_WIDTHS.iter().chain(&HIGHLIGHTER_WIDTHS) {
            assert!(*w >= notecraft_doc::ink::MIN_WIDTH && *w <= notecraft_doc::ink::MAX_WIDTH);
        }
    }

    #[test]
    fn every_shape_has_a_label_and_draws_in_its_tile() {
        let r = Rect::from_min_size(pos2(10.0, 10.0), vec2(14.0, 11.0));
        for kind in ShapeKind::ALL {
            assert!(!shape_label(kind).is_empty());
            let rect = if kind.is_line() { [r.min.x, r.max.y, r.width(), -r.height()] } else { [r.min.x, r.min.y, r.width(), r.height()] };
            let strokes = notecraft_engine::cmd::shapes::shape_strokes(kind, rect, false);
            assert!(!strokes.is_empty(), "{kind:?}");
            // Arrowheads may poke a little past the tile's inner box, never past the tile.
            assert!(strokes.iter().flatten().all(|[x, y]| r.expand(6.0).contains(pos2(*x, *y))), "{kind:?}");
        }
    }
}
