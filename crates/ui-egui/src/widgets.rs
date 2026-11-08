//! Ribbon and pane widgets: groups, large and small ribbon buttons, split and menu buttons,
//! colour grids, tooltips with shortcuts. Flat: a button has no face until it's hovered, pressed
//! or toggled on, and then a flat fill with corners of 2. Use `craft_ui::ComboBox`, never
//! `egui::ComboBox` (clippy.toml).

pub mod color;

use egui::{Color32, CornerRadius, Rect, Response, Sense, Stroke, Ui, pos2, vec2};
use serde_json::Value;

use crate::NoteApp;
use crate::icons::{self, Icon};
use crate::theme::{RADIUS_SMALL, Tokens, regular};

/// Height of the ribbon content area (without group labels).
pub const CONTENT_H: f32 = 66.0;
/// Height of the group title row under the content.
pub const LABEL_H: f32 = 18.0;
/// Large buttons: centre of the 32 px icon and top of the first label line.
pub const BIG_ICON_Y: f32 = 19.0;
pub const BIG_LABEL_Y: f32 = 38.0;
pub const LABEL_PITCH: f32 = 13.0;
/// Small buttons.
pub const SMALL_H: f32 = 22.0;
/// The drop-down marker's size.
pub const DROPDOWN: f32 = 9.0;

/// A button's face: flat fills for hovered, pressed and toggled on.
pub fn bg(ui: &Ui, r: Rect, resp: &Response, checked: bool, enabled: bool, t: &Tokens) {
    let fill = if enabled && resp.is_pointer_button_down_on() {
        t.pressed
    } else if checked {
        t.checked
    } else if enabled && resp.hovered() {
        t.hover
    } else {
        return;
    };
    let radius = CornerRadius::same(RADIUS_SMALL);
    ui.painter().rect_filled(r, radius, fill);
    if checked {
        ui.painter().rect_stroke(r, radius, Stroke::new(1.0, t.checked_border), egui::StrokeKind::Inside);
    }
}

/// One ribbon button's look and behaviour.
#[derive(Clone, Debug)]
pub struct Btn<'a> {
    pub icon: Icon,
    pub label: &'a str,
    /// The tooltip's title (the label when empty).
    pub tip: &'a str,
    /// A line of help under the tooltip's title.
    pub help: &'a str,
    /// A shortcut shown in the tooltip (`Ctrl+B`).
    pub shortcut: String,
    pub enabled: bool,
    /// Greyed out because the feature isn't built yet: a click says "Coming soon" in a toast.
    pub coming_soon: bool,
    pub checked: bool,
    /// Draw the label (small buttons; a compact group drops it).
    pub show_label: bool,
    /// The fill colour bar under the icon (font colour, highlight).
    pub swatch: Option<Color32>,
}

impl<'a> Btn<'a> {
    pub fn new(icon: Icon, label: &'a str) -> Btn<'a> {
        Btn {
            icon,
            label,
            tip: "",
            help: "",
            shortcut: String::new(),
            enabled: true,
            coming_soon: false,
            checked: false,
            show_label: true,
            swatch: None,
        }
    }
    pub fn tip(mut self, tip: &'a str) -> Self {
        self.tip = tip;
        self
    }
    pub fn help(mut self, help: &'a str) -> Self {
        self.help = help;
        self
    }
    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = s.into();
        self
    }
    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }
    /// Not built yet: drawn disabled, and a click shows "Coming soon" in a toast. (Not for a
    /// control that is only disabled for now, such as Cut with nothing selected.)
    pub fn coming_soon(mut self) -> Self {
        self.enabled = false;
        self.coming_soon = true;
        self
    }
    pub fn checked(mut self, on: bool) -> Self {
        self.checked = on;
        self
    }
    pub fn icon_only(mut self) -> Self {
        self.show_label = false;
        self
    }
    pub fn swatch(mut self, c: Option<Color32>) -> Self {
        self.swatch = c;
        self
    }
    /// For command `id`: enabled when it can run, its shortcut, and its label as the tooltip
    /// when none is given.
    pub fn for_command(mut self, app: &NoteApp, id: &str) -> Self {
        self.enabled = app.can(id);
        if let Some(k) = notecraft_engine::registry().get(id).and_then(|s| s.shortcut) {
            self.shortcut = shortcut_text(k);
        }
        if !app.has_command(id) {
            self.coming_soon = true;
            if self.help.is_empty() {
                self.help = crate::tl!("Coming in a later version of NoteCraft.");
            }
        }
        self
    }
    fn title(&self) -> &str {
        if self.tip.is_empty() { self.label } else { self.tip }
    }
}

/// A registry shortcut (`Mod+Shift+N`, `Mod+Y / Mod+Shift+Z`) as shown on this platform.
pub fn shortcut_text(k: &str) -> String {
    let first = k.split(" / ").next().unwrap_or(k);
    let mac = cfg!(target_os = "macos");
    first
        .split('+')
        .map(|part| match part {
            // The interface face has no ⌘ ⌥ ⇧ glyphs: the keys are spelled out.
            "Mod" if mac => "Cmd",
            "Mod" => "Ctrl",
            "Alt" if mac => "Option",
            other => other,
        })
        .collect::<Vec<_>>()
        .join("+")
}

/// The tooltip: the title (with its shortcut) and a line of help.
pub fn tooltip(resp: Response, b: &Btn<'_>) -> Response {
    let title = if b.shortcut.is_empty() { b.title().to_owned() } else { format!("{} ({})", b.title(), b.shortcut) };
    if b.help.is_empty() {
        return resp.on_hover_text(title).on_disabled_hover_text(b.title());
    }
    let help = b.help.to_owned();
    let title2 = title.clone();
    let help2 = help.clone();
    resp.on_hover_ui(move |ui| {
        ui.set_max_width(260.0);
        ui.label(egui::RichText::new(title).strong());
        ui.label(help);
    })
    .on_disabled_hover_ui(move |ui| {
        ui.set_max_width(260.0);
        ui.label(egui::RichText::new(title2).strong());
        ui.label(help2);
    })
}

fn icon_style(b: &Btn<'_>, t: &Tokens) -> icons::Style {
    if b.enabled { icons::Style::new(t.icon, t.accent) } else { icons::Style::disabled(t.icon) }
}

fn sense(enabled: bool) -> Sense {
    if enabled { Sense::click() } else { Sense::hover() }
}

/// A ribbon button's sense: clickable when enabled, or to say it's coming soon.
fn btn_sense(b: &Btn<'_>) -> Sense {
    sense(b.enabled || b.coming_soon)
}

/// Whether a button was used: clicked while enabled, or its KeyTip typed. A click on a button
/// that is [coming soon](Btn::coming_soon) shows a toast instead.
fn used(app: &mut NoteApp, resp: &Response, b: &Btn<'_>) -> bool {
    let keyed = app.ui.keytips.control(resp.rect);
    let hit = resp.clicked() || keyed;
    if hit && !b.enabled && b.coming_soon {
        app.coming_soon();
    }
    b.enabled && hit
}

/// One centred label line (top at `y`); `marker` puts the drop-down marker after it.
pub fn label_line(ui: &Ui, cx: f32, y: f32, text: &str, color: Color32, marker: bool) {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), regular(11.5), color);
    let extra = if marker { DROPDOWN + 3.0 } else { 0.0 };
    let size = galley.size();
    let x = cx - (size.x + extra) / 2.0;
    ui.painter().galley(pos2(x, y), galley, color);
    if marker {
        let c = pos2(x + size.x + 3.0 + DROPDOWN / 2.0, y + size.y / 2.0);
        icons::paint_styled(ui.painter(), Rect::from_center_size(c, vec2(DROPDOWN, DROPDOWN)), Icon::Dropdown, icons::Style::mono(color));
    }
}

fn label_width(ui: &Ui, label: &str) -> f32 {
    label.split('\n').map(|l| crate::theme::text_width(ui.ctx(), l, regular(11.5))).fold(0.0, f32::max)
}

fn paint_big(ui: &Ui, r: Rect, b: &Btn<'_>, t: &Tokens, marker: bool) {
    let ic = Rect::from_center_size(pos2(r.center().x, r.min.y + BIG_ICON_Y), vec2(32.0, 32.0));
    icons::paint_styled(ui.painter(), ic, b.icon, icon_style(b, t));
    let color = if b.enabled { t.text } else { t.text_disabled };
    let lines: Vec<&str> = b.label.split('\n').collect();
    let mut y = r.min.y + BIG_LABEL_Y;
    for (i, l) in lines.iter().enumerate() {
        label_line(ui, r.center().x, y, l, color, marker && i + 1 == lines.len());
        y += LABEL_PITCH;
    }
}

/// A large ribbon button: a 32 px icon over a (possibly two-line, `\n`) label. Returns whether
/// it was used.
pub fn big(ui: &mut Ui, app: &mut NoteApp, b: Btn<'_>) -> bool {
    let t = app.tokens.clone();
    let w = (label_width(ui, b.label) + 12.0).max(44.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, CONTENT_H), btn_sense(&b));
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, b.enabled, b.checked, b.label.replace('\n', " ")));
    bg(ui, r, &resp, b.checked, b.enabled, &t);
    paint_big(ui, r, &b, &t, false);
    let go = used(app, &resp, &b);
    tooltip(resp, &b);
    go
}

/// A large button that opens a menu.
pub fn big_menu(ui: &mut Ui, app: &mut NoteApp, b: Btn<'_>, menu: impl FnOnce(&mut Ui, &mut NoteApp)) {
    let t = app.tokens.clone();
    let w = (label_width(ui, b.label) + 22.0).max(44.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, CONTENT_H), btn_sense(&b));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, b.enabled, b.label.replace('\n', " ")));
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    bg(ui, r, &resp, b.checked || open, b.enabled, &t);
    paint_big(ui, r, &b, &t, true);
    let keyed = app.ui.keytips.control(resp.rect);
    if keyed && b.enabled {
        egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&resp));
    }
    if b.coming_soon && !b.enabled && (keyed || resp.clicked()) {
        app.coming_soon();
    }
    let resp = tooltip(resp, &b);
    if b.enabled {
        popup_menu(&resp, app, menu);
    }
}

/// A large split button: the top runs the button, the label part opens a menu.
pub fn big_split(ui: &mut Ui, app: &mut NoteApp, b: Btn<'_>, menu: impl FnOnce(&mut Ui, &mut NoteApp)) -> bool {
    let t = app.tokens.clone();
    let w = (label_width(ui, b.label) + 22.0).max(46.0);
    let (r, _) = ui.allocate_exact_size(vec2(w, CONTENT_H), Sense::hover());
    let top = Rect::from_min_max(r.min, pos2(r.max.x, r.min.y + BIG_LABEL_Y - 2.0));
    let bottom = Rect::from_min_max(pos2(r.min.x, top.max.y), r.max);
    let id = ui.scope_id().with(("big_split", b.label));
    let tresp = ui.interact(top, id.with("top"), btn_sense(&b));
    let bresp = ui.interact(bottom, id.with("menu"), sense(b.enabled));
    tresp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, b.enabled, b.checked, b.label.replace('\n', " ")));
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&bresp));
    if b.enabled && (tresp.hovered() || bresp.hovered()) && !b.checked && !open {
        ui.painter().rect_filled(r, CornerRadius::same(RADIUS_SMALL), t.hover);
        let half = if tresp.hovered() { top } else { bottom };
        ui.painter().rect_filled(half, CornerRadius::same(RADIUS_SMALL), t.pressed);
        ui.painter().rect_filled(half.shrink(1.0), CornerRadius::same(RADIUS_SMALL), t.hover);
    }
    bg(ui, r, &tresp, b.checked || open, b.enabled, &t);
    paint_big(ui, r, &b, &t, true);
    let go = used(app, &tresp, &b);
    let resp = tooltip(tresp, &b);
    let _ = resp;
    let bresp = bresp.on_hover_text(crate::i18n::fmt_plain(crate::tl!("{name} options"), &[("name", b.title())]));
    if b.enabled {
        popup_menu(&bresp, app, menu);
    }
    go
}

/// A small ribbon button: a 16 px icon with an optional label. Returns whether it was used.
pub fn small(ui: &mut Ui, app: &mut NoteApp, b: Btn<'_>) -> bool {
    let t = app.tokens.clone();
    let show_label = b.show_label && !b.label.is_empty() && !crate::ribbon_fit::compact(ui.ctx());
    let text_w = if show_label { crate::theme::text_width(ui.ctx(), b.label, regular(11.5)) + 6.0 } else { 0.0 };
    if b.show_label && !b.label.is_empty() {
        crate::ribbon_fit::add_saving(ui.ctx(), text_w);
    }
    let (r, resp) = ui.allocate_exact_size(vec2(24.0 + text_w, SMALL_H), btn_sense(&b));
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, b.enabled, b.checked, b.title()));
    bg(ui, r, &resp, b.checked, b.enabled, &t);
    let ic = Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(16.0, 16.0));
    icons::paint_styled(ui.painter(), ic, b.icon, icon_style(&b, &t));
    if let Some(sw) = b.swatch {
        let sw = if b.enabled { sw } else { t.text_disabled };
        ui.painter().rect_filled(Rect::from_min_max(pos2(ic.min.x, ic.max.y - 2.5), pos2(ic.max.x, ic.max.y + 0.5)), 0.0, sw);
    }
    if show_label {
        let color = if b.enabled { t.text } else { t.text_disabled };
        ui.painter().text(pos2(r.min.x + 24.0, r.center().y), egui::Align2::LEFT_CENTER, b.label, regular(11.5), color);
    }
    let go = used(app, &resp, &b);
    tooltip(resp, &b);
    go
}

/// A small split button: the icon part runs the button, the arrow opens `menu`.
pub fn small_split(ui: &mut Ui, app: &mut NoteApp, b: Btn<'_>, menu: impl FnOnce(&mut Ui, &mut NoteApp)) -> bool {
    let t = app.tokens.clone();
    let (r, resp) = ui.allocate_exact_size(vec2(24.0, SMALL_H), btn_sense(&b));
    let (ar, aresp) = ui.allocate_exact_size(vec2(12.0, SMALL_H), sense(b.enabled));
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, b.enabled, b.checked, b.title()));
    bg(ui, r, &resp, b.checked, b.enabled, &t);
    bg(ui, ar, &aresp, false, b.enabled, &t);
    if b.enabled && (resp.hovered() || aresp.hovered()) {
        ui.painter().rect_stroke(r.union(ar), CornerRadius::same(RADIUS_SMALL), Stroke::new(1.0, t.checked_border), egui::StrokeKind::Inside);
    }
    let ic = Rect::from_center_size(r.center(), vec2(16.0, 16.0));
    icons::paint_styled(ui.painter(), ic, b.icon, icon_style(&b, &t));
    if let Some(sw) = b.swatch {
        let sw = if b.enabled { sw } else { t.text_disabled };
        ui.painter().rect_filled(Rect::from_min_max(pos2(ic.min.x, ic.max.y - 2.5), pos2(ic.max.x, ic.max.y + 0.5)), 0.0, sw);
    }
    let mono = if b.enabled { t.icon } else { t.text_disabled };
    icons::paint_styled(ui.painter(), Rect::from_center_size(ar.center(), vec2(DROPDOWN, DROPDOWN)), Icon::Dropdown, icons::Style::mono(mono));
    let go = used(app, &resp, &b);
    tooltip(resp, &b);
    let aresp = aresp.on_hover_text(crate::i18n::fmt_plain(crate::tl!("{name} options"), &[("name", b.title())]));
    if b.enabled {
        popup_menu(&aresp, app, menu);
    }
    go
}

/// A small button that opens a menu (icon, label, marker).
pub fn small_menu(ui: &mut Ui, app: &mut NoteApp, b: Btn<'_>, menu: impl FnOnce(&mut Ui, &mut NoteApp)) {
    let t = app.tokens.clone();
    let show_label = b.show_label && !b.label.is_empty() && !crate::ribbon_fit::compact(ui.ctx());
    let text_w = if show_label { crate::theme::text_width(ui.ctx(), b.label, regular(11.5)) + 6.0 } else { 0.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(24.0 + text_w + 12.0, SMALL_H), btn_sense(&b));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, b.enabled, b.title()));
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    bg(ui, r, &resp, b.checked || open, b.enabled, &t);
    icons::paint_styled(ui.painter(), Rect::from_center_size(pos2(r.min.x + 12.0, r.center().y), vec2(16.0, 16.0)), b.icon, icon_style(&b, &t));
    let color = if b.enabled { t.text } else { t.text_disabled };
    if show_label {
        ui.painter().text(pos2(r.min.x + 24.0, r.center().y), egui::Align2::LEFT_CENTER, b.label, regular(11.5), color);
    }
    icons::paint_styled(
        ui.painter(),
        Rect::from_center_size(pos2(r.max.x - 7.0, r.center().y), vec2(DROPDOWN, DROPDOWN)),
        Icon::Dropdown,
        icons::Style::mono(color),
    );
    let keyed = app.ui.keytips.control(resp.rect);
    if keyed && b.enabled {
        egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&resp));
    }
    if b.coming_soon && !b.enabled && (keyed || resp.clicked()) {
        app.coming_soon();
    }
    let resp = tooltip(resp, &b);
    if b.enabled {
        popup_menu(&resp, app, menu);
    }
}

/// The drop-down under a button, filled by `menu`.
pub fn popup_menu(resp: &Response, app: &mut NoteApp, menu: impl FnOnce(&mut Ui, &mut NoteApp)) {
    egui::Popup::menu(resp).show(|ui| {
        ui.set_min_width(160.0);
        menu(ui, app);
    });
}

/// A ribbon group: content, a centred title below and a divider on the right. Folds to fit the
/// window ([`crate::ribbon_fit`]).
pub fn group(ui: &mut Ui, app: &mut NoteApp, title: &str, icon: Icon, add: impl FnOnce(&mut Ui, &mut NoteApp)) {
    let t = app.tokens.clone();
    let start = ui.cursor().min;
    let level = crate::ribbon_fit::level(ui.ctx(), title);
    if level == crate::ribbon_fit::COLLAPSED {
        collapsed_group(ui, app, title, icon, add);
    } else {
        let label_w = crate::theme::text_width(ui.ctx(), title, regular(11.0));
        let inner = ui.vertical(|ui| {
            ui.set_min_width(label_w + 10.0);
            crate::ribbon_fit::set_compact(ui.ctx(), level == crate::ribbon_fit::COMPACT);
            ui.allocate_ui_with_layout(vec2(ui.available_width(), CONTENT_H), egui::Layout::left_to_right(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
                add(ui, app);
            });
            crate::ribbon_fit::end_compact(ui.ctx());
        });
        let r = inner.response.rect;
        let row = Rect::from_min_max(pos2(start.x, start.y + CONTENT_H), pos2(r.max.x, start.y + CONTENT_H + LABEL_H));
        ui.painter().text(row.center(), egui::Align2::CENTER_CENTER, title, regular(11.0), t.group_label);
    }
    ui.add_space(4.0);
    let x = ui.cursor().min.x;
    ui.painter().line_segment([pos2(x, start.y + 4.0), pos2(x, start.y + CONTENT_H + LABEL_H - 4.0)], Stroke::new(1.0, t.border));
    ui.add_space(5.0);
    crate::ribbon_fit::record(ui.ctx(), title, level, ui.cursor().min.x - start.x);
}

/// A group folded into one drop-down button that opens the whole group.
fn collapsed_group(ui: &mut Ui, app: &mut NoteApp, title: &str, icon: Icon, add: impl FnOnce(&mut Ui, &mut NoteApp)) {
    let t = app.tokens.clone();
    let text_w = crate::theme::text_width(ui.ctx(), title, regular(11.5));
    let (r, resp) = ui.allocate_exact_size(vec2((text_w + DROPDOWN + 14.0).max(48.0), CONTENT_H + LABEL_H), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, title));
    let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
    bg(ui, r, &resp, open, true, &t);
    icons::paint_styled(
        ui.painter(),
        Rect::from_center_size(pos2(r.center().x, r.min.y + BIG_ICON_Y), vec2(32.0, 32.0)),
        icon,
        icons::Style::new(t.icon, t.accent),
    );
    label_line(ui, r.center().x, r.min.y + BIG_LABEL_Y, title, t.text, true);
    let resp = resp.on_hover_text(title);
    egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.vertical(|ui| {
            ui.horizontal_top(|ui| {
                ui.set_min_height(CONTENT_H);
                ui.spacing_mut().item_spacing = vec2(1.0, 1.0);
                add(ui, app);
            });
            ui.vertical_centered(|ui| ui.label(egui::RichText::new(title).font(regular(11.0)).color(t.group_label)));
        });
    });
}

/// Small buttons stacked in rows inside a group (two or three rows of [`SMALL_H`]).
pub fn rows(ui: &mut Ui, app: &mut NoteApp, add: impl FnOnce(&mut Ui, &mut NoteApp)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
        add(ui, app);
    });
}

/// One row of small buttons.
pub fn row(ui: &mut Ui, app: &mut NoteApp, add: impl FnOnce(&mut Ui, &mut NoteApp)) {
    ui.horizontal(|ui| {
        ui.set_height(SMALL_H);
        ui.spacing_mut().item_spacing = vec2(1.0, 0.0);
        add(ui, app);
    });
}

/// A menu item with an icon. Returns whether it was clicked (and closes the menu then).
pub fn menu_item(ui: &mut Ui, icon: Option<Icon>, text: &str, enabled: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width().max(160.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0), sense(enabled));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, text));
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(RADIUS_SMALL), t.hover);
    }
    let color = if enabled { t.text } else { t.text_disabled };
    if let Some(icon) = icon {
        let style = if enabled { icons::Style::new(t.icon, t.accent) } else { icons::Style::disabled(t.icon) };
        icons::paint_styled(ui.painter(), Rect::from_center_size(pos2(r.min.x + 14.0, r.center().y), vec2(16.0, 16.0)), icon, style);
    }
    ui.painter().text(pos2(r.min.x + 30.0, r.center().y), egui::Align2::LEFT_CENTER, text, regular(12.5), color);
    let clicked = enabled && resp.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// A menu item with a check mark when `on`.
pub fn check_item(ui: &mut Ui, on: bool, text: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let w = ui.available_width().max(160.0);
    let (r, resp) = ui.allocate_exact_size(vec2(w, 24.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, text));
    if resp.hovered() {
        ui.painter().rect_filled(r, CornerRadius::same(RADIUS_SMALL), t.hover);
    }
    if on {
        let c = pos2(r.min.x + 14.0, r.center().y);
        ui.painter().line(vec![c + vec2(-5.0, 0.0), c + vec2(-1.5, 3.5), c + vec2(5.0, -4.0)], Stroke::new(1.6, t.accent_text));
    }
    ui.painter().text(pos2(r.min.x + 30.0, r.center().y), egui::Align2::LEFT_CENTER, text, regular(12.5), t.text);
    let clicked = resp.clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// A heading inside a menu.
pub fn menu_heading(ui: &mut Ui, text: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(2.0);
    ui.label(egui::RichText::new(text).font(regular(11.5)).color(t.text_dim));
}

/// A flat text button (backstage, dialogs): `primary` fills it with the accent.
pub fn flat_button(ui: &mut Ui, text: &str, primary: bool, enabled: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let w = crate::theme::text_width(ui.ctx(), text, regular(13.0)) + 28.0;
    let (r, resp) = ui.allocate_exact_size(vec2(w.max(80.0), 30.0), sense(enabled));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, text));
    let radius = CornerRadius::same(RADIUS_SMALL);
    let (fill, stroke, color) = match (primary, enabled) {
        (_, false) => (t.panel, t.border, t.text_disabled),
        (true, true) if resp.is_pointer_button_down_on() => (t.nav_selected, t.nav_selected, t.on_nav),
        (true, true) if resp.hovered() => (t.nav_hover, t.nav_hover, t.on_nav),
        (true, true) => (t.nav, t.nav, t.on_nav),
        (false, true) if resp.is_pointer_button_down_on() => (t.pressed, t.checked_border, t.text),
        (false, true) if resp.hovered() => (t.hover, t.checked_border, t.text),
        (false, true) => (t.input, t.input_border, t.text),
    };
    ui.painter().rect(r, radius, fill, Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, text, regular(13.0), color);
    enabled && resp.clicked()
}

/// The params that aim a formatting command at what the canvas is editing: the engine's
/// selection model `{"page", "outline", "start": {"para", "char"}, "end": {…}}` (see
/// `crates/engine/COMMANDS.md`), merged into `extra`.
///
/// The canvas owns the caret and selection. Until it exposes them (`canvas::selection_params`),
/// this falls back to the note container being edited ([`crate::canvas::CanvasState::editing`]),
/// which the engine reads as the whole outline. Every formatting button builds its params here,
/// so switching to the canvas's selection is this one function.
pub fn selection_params(app: &NoteApp, extra: Value) -> Value {
    let mut v = match extra {
        Value::Object(m) => m,
        _ => serde_json::Map::new(),
    };
    if let Some(Value::Object(sel)) = crate::canvas::selection_params(&app.canvas) {
        // The caret's container and range: {outline, start, end, page}.
        for (k, val) in sel {
            v.entry(k).or_insert(val);
        }
    } else if let Some(id) = app.canvas.editing {
        v.entry("outline").or_insert_with(|| serde_json::json!(id));
    }
    if let Some(page) = app.session.current.page {
        v.entry("page").or_insert_with(|| serde_json::json!(page));
    }
    Value::Object(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_read_naturally() {
        if cfg!(target_os = "macos") {
            assert_eq!(shortcut_text("Mod+Shift+N"), "Cmd+Shift+N");
        } else {
            assert_eq!(shortcut_text("Mod+Shift+N"), "Ctrl+Shift+N");
            assert_eq!(shortcut_text("Mod+Y / Mod+Shift+Z"), "Ctrl+Y");
        }
    }
}
