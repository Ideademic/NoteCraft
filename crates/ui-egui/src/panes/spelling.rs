//! Review › Spelling (F7): a pane on the right that steps through the misspelled words on the
//! page. The word is selected on the page; Change and Change All put in the chosen suggestion,
//! Ignore skips this one, Ignore All every one until NoteCraft closes, Add to Dictionary keeps the
//! word for good.

use egui::{Align2, Rect, Sense, Stroke, Ui, pos2, vec2};
use notecraft_doc::Guid;
use notecraft_engine::{TextPos, ids};
use serde_json::{Value, json};

use crate::theme::{paint_strong, regular};
use crate::widgets::flat_button;
use crate::{NoteApp, icons::Icon, tl};

/// Width of the pane.
pub const PANE_W: f32 = 250.0;

/// The pane's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpellPane {
    /// The page checked.
    pub page: Option<Guid>,
    /// The misspelling shown (`proof.check`'s issue with `suggestions`), `None` when done.
    pub issue: Option<Value>,
    /// Which issue of the page's list it is (the next one after a fix or Ignore is the same
    /// index of the new list).
    pub index: usize,
    /// The suggestion picked.
    pub choice: usize,
    /// The page's revision the issue belongs to.
    revision: u64,
}

/// F7: opens the pane, or closes it.
pub fn toggle(app: &mut NoteApp) {
    if app.ui.review.spelling.is_some() {
        app.ui.review.spelling = None;
    } else {
        open(app);
    }
}

/// Opens the pane at the first misspelling on the page.
pub fn open(app: &mut NoteApp) {
    app.ui.backstage = false;
    let mut pane = SpellPane { page: app.session.current.page, ..SpellPane::default() };
    find(app, &mut pane, true);
    app.ui.review.spelling = Some(pane);
}

/// Looks up the misspelling at `pane.index` (or the last one) and selects it on the page.
fn find(app: &mut NoteApp, pane: &mut SpellPane, select: bool) {
    pane.page = app.session.current.page;
    pane.revision = app.session.revision;
    pane.choice = 0;
    let list =
        app.session.run(ids::PROOF_CHECK, &json!({})).ok().and_then(|o| o.value.get("issues").and_then(Value::as_array).cloned()).unwrap_or_default();
    if list.is_empty() {
        pane.issue = None;
        return;
    }
    pane.index = pane.index.min(list.len() - 1);
    let Some(issue) = list.get(pane.index).cloned() else { return };
    let (Some(outline), Some(para), Some(start), Some(end)) =
        (issue.get("outline").and_then(Value::as_str).and_then(Guid::parse), num(&issue, "para"), num(&issue, "start"), num(&issue, "end"))
    else {
        pane.issue = None;
        return;
    };
    let full = app.session.run(ids::PROOF_SUGGESTIONS, &json!({"outline": outline, "at": {"para": para, "char": start}})).ok().map(|o| o.value);
    pane.issue = full.filter(|v| !v.is_null()).or(Some(issue));
    // Select the word on the page, scrolled into view.
    if !select {
        return;
    }
    let c = crate::canvas::TextCaret {
        anchor: TextPos::new(para, start),
        head: TextPos::new(para, end),
        ..crate::canvas::TextCaret::at(outline, TextPos::new(para, end))
    };
    app.canvas.caret = Some(c);
    app.canvas.editing = Some(outline);
    app.canvas.selection.elements.clear();
    app.canvas.reveal_caret = true;
}

fn num(v: &Value, k: &str) -> Option<usize> {
    v.get(k).and_then(Value::as_u64).and_then(|n| usize::try_from(n).ok())
}

fn suggestions(issue: &Value) -> Vec<String> {
    issue.get("suggestions").and_then(Value::as_array).map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect()).unwrap_or_default()
}

/// Keeps the pane on the current page: another page is checked from its start.
pub fn follow_page(app: &mut NoteApp) {
    let Some(mut pane) = app.ui.review.spelling.take() else { return };
    if pane.page != app.session.current.page {
        pane.index = 0;
        find(app, &mut pane, false);
    } else if pane.revision != app.session.revision {
        // Edited meanwhile (by typing, undo…): look again at the same place.
        find(app, &mut pane, false);
    }
    app.ui.review.spelling = Some(pane);
}

/// The pane.
pub fn pane(ui: &mut Ui, app: &mut NoteApp) {
    let Some(mut pane) = app.ui.review.spelling.clone() else { return };
    let t = app.tokens.clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, t.panel);
    ui.painter().line_segment([full.left_top(), full.left_bottom()], Stroke::new(1.0, t.border));
    let header = Rect::from_min_size(full.min, vec2(full.width(), 36.0));
    paint_strong(ui.painter(), pos2(header.min.x + 12.0, header.center().y), Align2::LEFT_CENTER, tl!("Spelling"), regular(14.0), t.text);
    let close = Rect::from_center_size(pos2(header.max.x - 18.0, header.center().y), vec2(24.0, 24.0));
    if super::icon_button(ui, app, close, Icon::Close, tl!("Close")) {
        app.ui.review.spelling = None;
        return;
    }
    let body = Rect::from_min_max(pos2(full.min.x + 12.0, header.max.y + 4.0), pos2(full.max.x - 12.0, full.max.y - 8.0));
    let mut act: Option<Act> = None;
    ui.scope_builder(egui::UiBuilder::new().max_rect(body).layout(egui::Layout::top_down(egui::Align::Min)), |ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        let Some(issue) = pane.issue.clone() else {
            ui.label(egui::RichText::new(tl!("The spelling check is complete.")).font(regular(13.0)).color(t.text));
            ui.add_space(6.0);
            if flat_button(ui, tl!("Check Again"), false, true) {
                act = Some(Act::Again);
            }
            return;
        };
        let word = issue.get("word").and_then(Value::as_str).unwrap_or_default().to_owned();
        ui.label(egui::RichText::new(tl!("Not in Dictionary")).font(regular(11.5)).color(t.text_dim));
        ui.label(egui::RichText::new(&word).font(regular(18.0)).color(crate::canvas::proof::line_color(t.dark)));
        let lang = issue.get("lang").and_then(Value::as_str).unwrap_or_default();
        ui.label(egui::RichText::new(notecraft_engine::proof::language_name(lang)).font(regular(11.5)).color(t.text_dim));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tl!("Suggestions")).font(regular(11.5)).color(t.text_dim));
        let list = suggestions(&issue);
        if list.is_empty() {
            ui.label(egui::RichText::new(tl!("(no suggestions)")).italics().font(regular(12.5)).color(t.text_dim));
        }
        for (i, s) in list.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
            resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, i == pane.choice, s));
            if i == pane.choice {
                ui.painter().rect_filled(r, 2.0, t.page_list_selected);
            } else if resp.hovered() {
                ui.painter().rect_filled(r, 2.0, t.hover);
            }
            ui.painter().text(pos2(r.min.x + 8.0, r.center().y), Align2::LEFT_CENTER, s, regular(13.0), t.text);
            if resp.clicked() {
                pane.choice = i;
            }
            if resp.double_clicked() {
                act = Some(Act::Change);
            }
        }
        ui.add_space(8.0);
        let some = !list.is_empty();
        ui.horizontal(|ui| {
            if flat_button(ui, tl!("Change"), true, some) {
                act = Some(Act::Change);
            }
            if flat_button(ui, tl!("Change All"), false, some) {
                act = Some(Act::ChangeAll);
            }
        });
        ui.horizontal(|ui| {
            if flat_button(ui, tl!("Ignore"), false, true) {
                act = Some(Act::Ignore);
            }
            if flat_button(ui, tl!("Ignore All"), false, true) {
                act = Some(Act::IgnoreAll);
            }
        });
        if flat_button(ui, tl!("Add to Dictionary"), false, true) {
            act = Some(Act::Add);
        }
    });
    app.ui.review.spelling = Some(pane.clone());
    if let Some(a) = act {
        run(app, &mut pane, a);
        app.ui.review.spelling = Some(pane);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Act {
    Change,
    ChangeAll,
    Ignore,
    IgnoreAll,
    Add,
    Again,
}

fn run(app: &mut NoteApp, pane: &mut SpellPane, a: Act) {
    let Some(issue) = pane.issue.clone() else {
        pane.index = 0;
        find(app, pane, true);
        return;
    };
    let word = issue.get("word").and_then(Value::as_str).unwrap_or_default().to_owned();
    let pick = suggestions(&issue).get(pane.choice).cloned();
    let at = json!({"outline": issue.get("outline"), "para": issue.get("para"), "start": issue.get("start"), "end": issue.get("end"), "word": word});
    match a {
        Act::Change => {
            if let (Some(text), Value::Object(mut m)) = (pick, at) {
                m.insert("text".into(), json!(text));
                app.act(ids::PROOF_APPLY, Value::Object(m));
            }
        }
        Act::ChangeAll => {
            if let Some(text) = pick {
                app.act(ids::PROOF_CHANGE_ALL, json!({"word": word, "text": text}));
            }
        }
        Act::Ignore => app.act(ids::PROOF_IGNORE, at),
        Act::IgnoreAll => app.act(ids::PROOF_IGNORE_ALL, json!({"word": word})),
        Act::Add => app.act(ids::PROOF_ADD_TO_DICTIONARY, json!({"word": word})),
        Act::Again => pane.index = 0,
    }
    find(app, pane, true);
}
