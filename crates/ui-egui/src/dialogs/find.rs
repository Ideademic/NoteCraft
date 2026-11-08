//! Find on the page (Ctrl+F): a bar over the page with the words to find, how many matches
//! there are and which one is current ("3 of 10"), Previous and Next (Enter, Shift+Enter),
//! Match Case and Whole Word, and Replace / Replace All (one undo step) when the replace row is
//! open. Every match is highlighted on the page ([`crate::canvas::find`]). Esc or the close
//! button closes it. The engine does the finding (`text.find`) and replacing (`text.replace`).

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use notecraft_engine::ids;
use serde_json::{Value, json};

use crate::canvas::find::{Highlights, Match};
use crate::icons::{self, Icon};
use crate::theme::{RADIUS_SMALL, regular};
use crate::{NoteApp, tl};

/// Height of the bar (and of the replace row).
pub const BAR_H: f32 = 34.0;

/// The bar's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FindBar {
    pub query: String,
    pub replace: String,
    pub match_case: bool,
    pub whole_word: bool,
    /// The replace row is open.
    pub show_replace: bool,
    /// Index of the current match.
    pub current: usize,
    /// Give the query box the keyboard next frame.
    pub focus: bool,
    /// What the matches were found for: (query, case, whole word, revision, page) and them.
    cache: Option<((String, bool, bool, u64, Option<notecraft_doc::Guid>), Vec<Match>)>,
}

impl FindBar {
    /// A bar looking for `query`, its box focused.
    pub fn new(query: &str) -> FindBar {
        FindBar { query: query.to_owned(), focus: true, ..FindBar::default() }
    }
}

fn query_id() -> egui::Id {
    egui::Id::unique("nc_find_query")
}

/// Opens the bar (Ctrl+F), with the selected text as the words to find; again, it just takes
/// the keyboard.
pub fn open(app: &mut NoteApp) {
    let selected = selected_text(app);
    match app.ui.find.as_mut() {
        Some(f) => {
            if let Some(t) = selected {
                f.query = t;
            }
            f.focus = true;
        }
        None => app.ui.find = Some(FindBar::new(&selected.unwrap_or_default())),
    }
    app.ui.backstage = false;
    app.ui.full_page_view = false;
}

/// Closes the bar and its highlights.
pub fn close(app: &mut NoteApp) {
    app.ui.find = None;
    app.canvas.find = None;
}

/// The selection, when it is a few words on one line.
fn selected_text(app: &NoteApp) -> Option<String> {
    let c = app.canvas.caret.as_ref().filter(|c| !c.is_collapsed())?;
    let (a, b) = c.range();
    if a.para != b.para {
        return None;
    }
    let o = app.session.current_page()?.outline(c.outline)?;
    let t: String = o.paragraphs.get(a.para)?.text().chars().skip(a.char).take(b.char.saturating_sub(a.char)).collect();
    (!t.trim().is_empty() && t.chars().count() <= 200).then_some(t)
}

/// The matches on the current page (cached until the words, options or page change).
pub fn matches(app: &mut NoteApp) -> Vec<Match> {
    let Some(f) = app.ui.find.as_ref() else { return Vec::new() };
    let key = (f.query.clone(), f.match_case, f.whole_word, app.session.revision, app.session.current.page);
    if let Some((k, m)) = &f.cache
        && *k == key
    {
        return m.clone();
    }
    let found = if f.query.is_empty() || app.session.current_page().is_none() {
        Vec::new()
    } else {
        let params = json!({"query": f.query, "matchCase": f.match_case, "wholeWord": f.whole_word});
        match app.session.run(ids::TEXT_FIND, &params) {
            Ok(out) => {
                out.value.get("matches").and_then(Value::as_array).map(|a| a.iter().filter_map(Match::from_json).collect()).unwrap_or_default()
            }
            Err(_) => Vec::new(),
        }
    };
    if let Some(f) = app.ui.find.as_mut() {
        f.cache = Some((key, found.clone()));
        if f.current >= found.len() {
            f.current = 0;
        }
    }
    found
}

/// Moves to the next (or previous) match.
pub fn step(app: &mut NoteApp, forward: bool) {
    let n = matches(app).len();
    let Some(f) = app.ui.find.as_mut() else { return };
    if n == 0 {
        return;
    }
    f.current = if forward { (f.current + 1) % n } else { (f.current + n - 1) % n };
    if let Some(h) = app.canvas.find.as_mut() {
        h.current = Some(f.current);
        h.reveal = true;
    }
}

/// Replaces the current match (`all`: every match, in one undo step).
pub fn replace(app: &mut NoteApp, all: bool) {
    let found = matches(app);
    let Some(f) = app.ui.find.clone() else { return };
    if f.query.is_empty() || found.is_empty() {
        return;
    }
    let mut params = json!({"query": f.query, "with": f.replace, "matchCase": f.match_case, "wholeWord": f.whole_word});
    if !all && let (Some(m), Value::Object(p)) = (found.get(f.current.min(found.len() - 1)), &mut params) {
        p.insert("at".into(), m.to_json());
    }
    app.canvas.caret = None;
    app.canvas.editing = None;
    if let Ok(out) = app.run(ids::TEXT_REPLACE, params) {
        let n = out.value.get("replaced").and_then(Value::as_u64).unwrap_or(0);
        if all {
            let text = match n {
                1 => tl!("Replaced 1 match.").to_owned(),
                n => crate::i18n::fmt_plain(tl!("Replaced {n} matches."), &[("n", &n.to_string())]),
            };
            app.set_status(text, false);
        }
    }
}

/// A small icon button in the bar.
fn tool(ui: &mut Ui, app: &NoteApp, icon: Icon, tip: &str, on: bool, enabled: bool) -> bool {
    let t = &app.tokens;
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 24.0), sense);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, tip));
    if on {
        ui.painter().rect(r, RADIUS_SMALL, t.checked, Stroke::new(1.0, t.checked_border), egui::StrokeKind::Inside);
    } else if enabled && resp.hovered() {
        ui.painter().rect_filled(r, RADIUS_SMALL, t.hover);
    }
    let style = if enabled { icons::Style::new(t.icon, t.accent) } else { icons::Style::disabled(t.icon) };
    icons::paint_styled(ui.painter(), Rect::from_center_size(r.center(), vec2(15.0, 15.0)), icon, style);
    enabled && resp.on_hover_text(tip).clicked()
}

/// A text toggle ("Aa", "ab|") in the bar.
fn toggle(ui: &mut Ui, app: &NoteApp, text: &str, tip: &str, on: bool) -> bool {
    let t = &app.tokens;
    let (r, resp) = ui.allocate_exact_size(vec2(30.0, 24.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, true, on, tip));
    if on {
        ui.painter().rect(r, RADIUS_SMALL, t.checked, Stroke::new(1.0, t.checked_border), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_filled(r, RADIUS_SMALL, t.hover);
    }
    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, regular(12.0), t.text);
    resp.on_hover_text(tip).clicked()
}

fn text_box(ui: &mut Ui, app: &NoteApp, text: &mut String, hint: &str, id: Option<egui::Id>) -> egui::Response {
    let t = &app.tokens;
    let (r, _) = ui.allocate_exact_size(vec2(220.0, 24.0), Sense::hover());
    ui.painter().rect(r, RADIUS_SMALL, t.input, Stroke::new(1.0, t.input_border), egui::StrokeKind::Inside);
    let mut edit = egui::TextEdit::singleline(text)
        .frame(egui::Frame::NONE)
        .hint_text(egui::RichText::new(hint).color(t.text_dim))
        .margin(vec2(6.0, 4.0))
        .font(regular(12.5))
        .vertical_align(egui::Align::Center)
        .desired_width(r.width() - 4.0);
    if let Some(id) = id {
        edit = edit.id(id);
    }
    ui.put(r, edit)
}

/// Draws the bar (and the replace row) at the top of the page area, and hands the matches to
/// the canvas.
pub fn bar(ui: &mut Ui, app: &mut NoteApp) {
    if app.ui.find.is_none() {
        app.canvas.find = None;
        return;
    }
    let found = matches(app);
    let t = app.tokens.clone();
    let Some(mut f) = app.ui.find.clone() else { return };
    let rows = if f.show_replace { 2.0 } else { 1.0 };
    let (full, _) = ui.allocate_exact_size(vec2(ui.available_width(), BAR_H * rows), Sense::hover());
    ui.painter().rect_filled(full, 0.0, t.panel);
    ui.painter().line_segment([full.left_bottom(), full.right_bottom()], Stroke::new(1.0, t.border));
    let mut close_now = false;
    let mut go: Option<bool> = None;
    let mut do_replace: Option<bool> = None;
    let row1 = Rect::from_min_size(full.min, vec2(full.width(), BAR_H));
    ui.scope_builder(egui::UiBuilder::new().max_rect(row1.shrink2(vec2(10.0, 5.0))).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
        let (ir, _) = ui.allocate_exact_size(vec2(18.0, 24.0), Sense::hover());
        icons::paint_styled(ui.painter(), Rect::from_center_size(ir.center(), vec2(15.0, 15.0)), Icon::Search, icons::Style::mono(t.text_dim));
        let resp = text_box(ui, app, &mut f.query, tl!("Find on page"), Some(query_id()));
        if std::mem::take(&mut f.focus) {
            resp.request_focus();
        }
        if resp.changed() {
            f.current = 0;
        }
        let (enter, shift, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.modifiers.shift, i.key_pressed(egui::Key::Escape)));
        if resp.lost_focus() && enter {
            go = Some(!shift);
            resp.request_focus();
        }
        if (resp.has_focus() || resp.lost_focus()) && esc {
            close_now = true;
        }
        let n = found.len();
        let count = match (n, f.query.is_empty()) {
            (_, true) => String::new(),
            (0, false) => tl!("No matches").to_owned(),
            (n, false) => crate::i18n::fmt_plain(tl!("{i} of {n}"), &[("i", &(f.current.min(n - 1) + 1).to_string()), ("n", &n.to_string())]),
        };
        let (cr, _) = ui.allocate_exact_size(vec2(84.0, 24.0), Sense::hover());
        let color = if n == 0 && !f.query.is_empty() { t.red } else { t.text_dim };
        ui.painter().text(pos2(cr.min.x + 4.0, cr.center().y), Align2::LEFT_CENTER, &count, regular(12.0), color);
        if tool(ui, app, Icon::ChevronUp, tl!("Previous match (Shift+Enter)"), false, n > 0) {
            go = Some(false);
        }
        if tool(ui, app, Icon::ChevronDown, tl!("Next match (Enter)"), false, n > 0) {
            go = Some(true);
        }
        ui.add_space(6.0);
        if toggle(ui, app, "Aa", tl!("Match Case"), f.match_case) {
            f.match_case = !f.match_case;
        }
        if toggle(ui, app, "\u{201c}ab\u{201d}", tl!("Whole Word"), f.whole_word) {
            f.whole_word = !f.whole_word;
        }
        ui.add_space(6.0);
        let label = if f.show_replace { tl!("Hide Replace") } else { tl!("Replace…") };
        if crate::widgets::flat_button(ui, label, false, true) {
            f.show_replace = !f.show_replace;
        }
        let close_r = Rect::from_center_size(pos2(row1.max.x - 20.0, row1.center().y), vec2(24.0, 24.0));
        if crate::panes::icon_button(ui, app, close_r, Icon::Close, tl!("Close (Esc)")) {
            close_now = true;
        }
    });
    if f.show_replace {
        let row2 = Rect::from_min_size(pos2(full.min.x, full.min.y + BAR_H), vec2(full.width(), BAR_H));
        ui.scope_builder(
            egui::UiBuilder::new().max_rect(row2.shrink2(vec2(10.0, 5.0))).layout(egui::Layout::left_to_right(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                ui.add_space(22.0);
                text_box(ui, app, &mut f.replace, tl!("Replace with"), None);
                ui.add_space(4.0);
                let any = !found.is_empty();
                if crate::widgets::flat_button(ui, tl!("Replace"), false, any) {
                    do_replace = Some(false);
                }
                if crate::widgets::flat_button(ui, tl!("Replace All"), false, any) {
                    do_replace = Some(true);
                }
            },
        );
    }
    app.ui.find = Some(f);
    if close_now {
        close(app);
        return;
    }
    if let Some(all) = do_replace {
        replace(app, all);
    }
    let found = matches(app);
    let current = app.ui.find.as_ref().map_or(0, |f| f.current);
    let reveal = go.is_some() || app.canvas.find.as_ref().is_none_or(|h| h.matches != found);
    app.canvas.find = Some(Highlights { current: (!found.is_empty()).then_some(current.min(found.len().saturating_sub(1))), matches: found, reveal });
    if let Some(fwd) = go {
        step(app, fwd);
    }
}

/// Height the bar takes when open (the page area shrinks by it).
pub fn height(app: &NoteApp) -> f32 {
    match &app.ui.find {
        Some(f) if f.show_replace => BAR_H * 2.0,
        Some(_) => BAR_H,
        None => 0.0,
    }
}
