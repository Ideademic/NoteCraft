//! Typing in note containers with the engine's selection model (`{para, char}` positions).
//!
//! While a container (or a waiting one, see [`super::PendingContainer`]) has the caret, the
//! canvas holds egui's keyboard focus (so the window's own shortcuts stay out of the way) and
//! turns keys into commands: text → `text.insert` (with the pending style), Backspace/Delete →
//! `text.delete`, Enter → `text.splitParagraph`, Tab/Shift+Tab → `format.indent`/`outdent`, the
//! registry's `format.*` shortcuts (Ctrl+B, Ctrl+1…), Ctrl+Z/Y → `edit.undo`/`redo`. Typing
//! passes the container's undo merge key, so a burst of typing is one undo step. Arrows,
//! Home/End (Ctrl: whole container), Shift to select, Ctrl+A, and copy/cut/paste as plain text.
//!
//! A container left empty is deleted when the caret leaves it, as in OneNote.
//!
//! In a table the caret is in one cell's paragraphs (`TextCaret::cell`, the commands' `cell`):
//! Tab and Shift+Tab move between cells and the arrows cross them ([`super::table`]).

use egui::text::CCursor;
use egui::{Event, Key, Modifiers, Ui};
use notecraft_doc::{Color, Guid};
use notecraft_engine::text::{Flow, Frame};
use notecraft_engine::{CmdError, CmdOutput, Session, TextPos, ids, text};
use serde_json::{Value, json};

use super::layout::{OutlineLayout, PageLayout};
use super::view::{self, View};
use super::{CanvasState, CurrentFormat, PendingStyle, TextCaret, run};
use crate::services::Services;

/// Ends editing: the caret leaves its container (deleted if left empty), unless it is in `keep`.
pub fn leave(state: &mut CanvasState, session: &mut Session, keep: Option<Guid>) {
    state.pending = None;
    let Some(c) = state.caret.clone() else {
        state.editing = None;
        return;
    };
    if Some(c.outline) == keep {
        return;
    }
    state.caret = None;
    state.editing = None;
    state.pending_style = PendingStyle::default();
    let page = state.page;
    let empty = page
        .and_then(|p| session.notebooks.iter().find_map(|n| n.find_page(p)))
        .and_then(|(_, p)| p.outline(c.outline))
        .is_some_and(|o| o.is_empty() && o.paragraphs().all(|p| p.tag.is_none() && p.list.is_none()));
    if empty {
        run(state, session, ids::OUTLINE_DELETE, json!({"id": c.outline, "page": page, "mergeKey": c.merge_key}));
    }
    session.break_undo_merge();
}

/// Drops a caret whose container is gone (undo, another window) and clamps it to the text.
pub fn keep_caret_valid(state: &mut CanvasState, session: &Session) {
    let Some(c) = state.caret.as_mut() else { return };
    match session.current_page().and_then(|p| p.outline(c.outline)) {
        Some(o) => super::table::normalize_caret(c, o),
        None => {
            state.caret = None;
            state.editing = None;
        }
    }
}

/// Scrolls the caret into view after the keyboard moved it.
pub fn caret_visible(state: &mut CanvasState, v: &View, lay: &PageLayout, sheet: egui::Rect) {
    if !std::mem::take(&mut state.reveal_caret) {
        return;
    }
    let Some(c) = &state.caret else { return };
    if let Some(r) = lay.outline(c.outline).and_then(|o| o.frame_caret_rect(v, c.frame(), c.head)) {
        view::reveal(state, sheet, r);
    }
}

/// The flow the caret is in.
pub(crate) fn flow<'a>(session: &'a Session, c: &TextCaret) -> Option<&'a dyn Flow> {
    text::flow(session.current_page()?.outline(c.outline)?, c.frame())
}

/// The selected text, paragraphs separated by newlines (a table's text as tab-separated rows).
fn selected_text(state: &CanvasState, session: &Session) -> Option<String> {
    if let Some(t) = super::table::selected_cells_text(state, session) {
        return Some(t);
    }
    let c = state.caret.as_ref().filter(|c| !c.is_collapsed())?;
    let o = flow(session, c)?;
    let (a, b) = c.range();
    let mut out = String::new();
    for i in a.para..=b.para.min(o.count().saturating_sub(1)) {
        let t: Vec<char> = match (o.para(i), c.cell) {
            (Some(p), _) => p.text().chars().collect(),
            (None, None) => session
                .current_page()
                .and_then(|p| p.outline(c.outline))
                .and_then(|ol| ol.table(i))
                .map(|t| t.text().chars().collect())
                .unwrap_or_default(),
            (None, Some(_)) => Vec::new(),
        };
        let s = if i == a.para { a.char.min(t.len()) } else { 0 };
        let e = if i == b.para { b.char.min(t.len()) } else { t.len() };
        if i > a.para {
            out.push('\n');
        }
        out.extend(t.get(s..e.max(s)).unwrap_or_default());
    }
    Some(out)
}

/// The caret's range as command params (in its cell, and the selected cells).
pub(crate) fn range_params(state: &CanvasState, c: &TextCaret) -> Value {
    let (a, b) = c.range();
    let mut v = json!({"outline": c.outline, "start": a.to_json(), "end": b.to_json(), "page": state.page});
    if let Value::Object(m) = &mut v {
        if let Some(cell) = c.cell {
            m.insert("cell".into(), cell.to_json());
        }
        if let Some(cells) = c.cells {
            m.insert("cells".into(), cells.to_json());
        }
    }
    v
}

/// Creates the waiting container (typing started) and moves the caret into it.
fn ensure_outline(state: &mut CanvasState, session: &mut Session) -> Option<TextCaret> {
    if let Some(c) = &state.caret {
        return Some(c.clone());
    }
    let p = state.pending.take()?;
    let out = run(state, session, ids::OUTLINE_NEW, json!({"x": p.at.x, "y": p.at.y, "page": state.page, "mergeKey": p.merge_key}))?;
    let id = out.value.get("id").and_then(Value::as_str).and_then(Guid::parse)?;
    let c = TextCaret { merge_key: p.merge_key, ..TextCaret::at(id, TextPos::default()) };
    state.caret = Some(c.clone());
    state.editing = Some(id);
    Some(c)
}

/// The new caret a text command returned.
pub(crate) fn caret_from(out: &CmdOutput) -> Option<TextPos> {
    out.value.get("caret").and_then(TextPos::from_json)
}

pub(crate) fn set_head(state: &mut CanvasState, pos: TextPos, extend: bool) {
    if let Some(c) = state.caret.as_mut() {
        c.head = pos;
        if !extend {
            c.anchor = pos;
        }
    }
    state.reveal_caret = true;
}

/// Types `t` at the caret (over the selection).
pub fn insert(state: &mut CanvasState, session: &mut Session, t: &str) {
    let t: String = t.replace("\r\n", "\n").replace('\r', "\n").chars().filter(|c| *c == '\n' || *c == '\t' || !c.is_control()).collect();
    if t.is_empty() {
        return;
    }
    let Some(mut c) = ensure_outline(state, session) else { return };
    if c.cells.take().is_some()
        && let Some(cc) = state.caret.as_mut()
    {
        // Typing over selected cells types in the caret's cell.
        cc.cells = None;
    }
    let mut params = range_params(state, &c);
    if let Value::Object(m) = &mut params {
        m.insert("text".into(), Value::String(t.clone()));
        m.insert("mergeKey".into(), Value::String(c.merge_key.clone()));
        if !state.pending_style.is_empty() {
            m.insert("style".into(), state.pending_style.to_json());
        }
    }
    if let Some(out) = run(state, session, ids::TEXT_INSERT, params) {
        state.pending_style = PendingStyle::default();
        if let Some(pos) = caret_from(&out) {
            set_head(state, pos, false);
        }
        super::links::after_insert(state, session, &t);
    }
}

/// Deletes the selection, or a character/word back (`backward`) or forward.
fn delete(state: &mut CanvasState, session: &mut Session, backward: bool, word: bool) {
    let Some(c) = state.caret.clone() else { return };
    if super::table::delete_cells(state, session) || super::table::delete_at_edge(state, session, backward) {
        return;
    }
    let mut params = if c.is_collapsed() {
        json!({"outline": c.outline, "at": c.head.to_json(), "backward": backward, "unit": if word { "word" } else { "char" }, "page": state.page})
    } else {
        range_params(state, &c)
    };
    if let Value::Object(m) = &mut params {
        m.insert("mergeKey".into(), Value::String(c.merge_key.clone()));
        if let Some(cell) = c.cell {
            m.insert("cell".into(), cell.to_json());
        }
    }
    if let Some(out) = run(state, session, ids::TEXT_DELETE, params)
        && let Some(pos) = caret_from(&out)
    {
        set_head(state, pos, false);
    }
}

pub(crate) fn split(state: &mut CanvasState, session: &mut Session) {
    let Some(c) = ensure_outline(state, session) else { return };
    let mut params = range_params(state, &c);
    if let Value::Object(m) = &mut params {
        m.insert("mergeKey".into(), Value::String(c.merge_key.clone()));
    }
    if let Some(out) = run(state, session, ids::TEXT_SPLIT_PARAGRAPH, params)
        && let Some(pos) = caret_from(&out)
    {
        set_head(state, pos, false);
    }
}

/// Character formats that become the pending style on a collapsed caret.
const CHAR_FORMATS: [&str; 8] = [
    ids::FORMAT_BOLD,
    ids::FORMAT_ITALIC,
    ids::FORMAT_UNDERLINE,
    ids::FORMAT_STRIKE,
    ids::FORMAT_HIGHLIGHT,
    ids::FORMAT_COLOR,
    ids::FORMAT_SIZE,
    ids::FORMAT_FONT,
];

fn parse_color(v: Option<&Value>) -> Option<Option<Color>> {
    match v {
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Color::parse_hex(s).map(Some),
        _ => None,
    }
}

/// Changes the pending style for character format `id` (a collapsed caret outside a word).
fn toggle_pending(state: &mut CanvasState, session: &Session, id: &str, extra: &Value) {
    let cur = current_format(state, session).unwrap_or_default();
    let flip = |v: Option<bool>| Some(!v.unwrap_or(false));
    let set = extra.get("value").and_then(Value::as_bool);
    let p = &mut state.pending_style;
    match id {
        ids::FORMAT_BOLD => p.bold = set.map(Some).unwrap_or_else(|| flip(cur.bold)),
        ids::FORMAT_ITALIC => p.italic = set.map(Some).unwrap_or_else(|| flip(cur.italic)),
        ids::FORMAT_UNDERLINE => p.underline = set.map(Some).unwrap_or_else(|| flip(cur.underline)),
        ids::FORMAT_STRIKE => p.strike = set.map(Some).unwrap_or_else(|| flip(cur.strike)),
        ids::FORMAT_HIGHLIGHT => {
            let c = parse_color(extra.get("color")).unwrap_or(Some(Color::HIGHLIGHT_YELLOW));
            p.highlight = Some(if extra.get("color").is_none() && cur.highlight == Some(c) { None } else { c });
        }
        ids::FORMAT_COLOR => p.color = parse_color(extra.get("color")).or(p.color),
        ids::FORMAT_SIZE => {
            p.size = match extra.get("size") {
                Some(Value::Null) => Some(None),
                Some(v) => v.as_f64().map(|s| Some((s as f32).clamp(1.0, 400.0))).or(p.size),
                None => p.size,
            }
        }
        ids::FORMAT_FONT => {
            p.font = match extra.get("font") {
                Some(Value::Null) => Some(None),
                Some(Value::String(s)) => Some(Some(s.chars().take(64).collect())),
                _ => p.font.clone(),
            }
        }
        _ => {}
    }
}

/// See [`super::format_command`].
pub fn format(state: &mut CanvasState, session: &mut Session, id: &str, extra: Value) -> Option<Result<CmdOutput, CmdError>> {
    let is_char = CHAR_FORMATS.contains(&id);
    if state.caret.is_none() && state.pending.is_some() {
        if is_char {
            toggle_pending(state, session, id, &extra);
            return Some(Ok(CmdOutput::unchanged()));
        }
        ensure_outline(state, session)?;
    }
    let c = state.caret.clone()?;
    let mut params = range_params(state, &c);
    if let (Value::Object(m), Value::Object(e)) = (&mut params, &extra) {
        for (k, v) in e {
            m.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
    session.break_undo_merge();
    let r = session.run(id, &params);
    match &r {
        Ok(out) if out.value.get("collapsed").and_then(Value::as_bool) == Some(true) && is_char => toggle_pending(state, session, id, &extra),
        Ok(out) => {
            if let Some(pos) = caret_from(out) {
                set_head(state, pos, false);
            }
        }
        Err(e) => state.messages.push(e.to_string()),
    }
    Some(r)
}

/// Common value of `it`, `None` when mixed (or empty).
fn common<T: PartialEq>(mut it: impl Iterator<Item = T>) -> Option<T> {
    let first = it.next()?;
    for x in it {
        if x != first {
            return None;
        }
    }
    Some(first)
}

/// See [`super::current_format`].
pub fn current_format(state: &CanvasState, session: &Session) -> Option<CurrentFormat> {
    let p = &state.pending_style;
    let mut f = if let Some(c) = &state.caret {
        let (runs, paras) = super::table::selection_formats(c, session)?;
        CurrentFormat {
            bold: common(runs.iter().map(|r| r.bold)),
            italic: common(runs.iter().map(|r| r.italic)),
            underline: common(runs.iter().map(|r| r.underline)),
            strike: common(runs.iter().map(|r| r.strike)),
            highlight: common(runs.iter().map(|r| r.highlight)),
            color: common(runs.iter().map(|r| r.color)),
            font: common(runs.iter().map(|r| r.font.clone())),
            size: common(runs.iter().map(|r| r.size)),
            style: common(paras.iter().map(|pg| pg.style)),
            list: common(paras.iter().map(|pg| pg.list)),
            list_style: common(paras.iter().map(|p| p.marker_style())),
            align: common(paras.iter().map(|pg| pg.align)),
            tag: common(paras.iter().map(|pg| pg.tag)),
        }
    } else if state.pending.is_some() {
        CurrentFormat {
            bold: Some(false),
            italic: Some(false),
            underline: Some(false),
            strike: Some(false),
            highlight: Some(None),
            color: Some(None),
            font: Some(None),
            size: Some(None),
            style: Some(notecraft_doc::ParaStyle::Normal),
            list: Some(None),
            list_style: Some(None),
            align: Some(notecraft_doc::Align::Left),
            tag: Some(None),
        }
    } else {
        return None;
    };
    let collapsed = state.caret.as_ref().is_none_or(TextCaret::is_collapsed);
    if collapsed {
        f.bold = p.bold.or(f.bold);
        f.italic = p.italic.or(f.italic);
        f.underline = p.underline.or(f.underline);
        f.strike = p.strike.or(f.strike);
        f.highlight = p.highlight.or(f.highlight);
        f.color = p.color.or(f.color);
        f.font = p.font.clone().or(f.font);
        f.size = p.size.or(f.size);
    }
    Some(f)
}

// --- Caret movement ----------------------------------------------------------------------

/// Where the caret's flow is: the session, the container and the cell.
#[derive(Clone, Copy)]
struct At<'a> {
    session: &'a Session,
    outline: Guid,
    frame: Frame,
}

impl At<'_> {
    fn of<'a>(session: &'a Session, c: &TextCaret) -> At<'a> {
        At { session, outline: c.outline, frame: c.frame() }
    }

    fn flow(&self) -> Option<&dyn Flow> {
        text::flow(self.session.current_page()?.outline(self.outline)?, self.frame)
    }
}

fn para_len(at: At<'_>, para: usize) -> usize {
    at.flow().map_or(0, |f| text::len_at(f, para))
}

fn para_count(at: At<'_>) -> usize {
    at.flow().map_or(0, |f| f.count())
}

fn para_chars(at: At<'_>, para: usize) -> Vec<char> {
    at.flow().and_then(|f| f.para(para)).map(|p| p.text().chars().collect()).unwrap_or_default()
}

fn step_char(at: At<'_>, pos: TextPos, forward: bool) -> TextPos {
    if forward {
        if pos.char < para_len(at, pos.para) {
            TextPos::new(pos.para, pos.char + 1)
        } else if pos.para + 1 < para_count(at) {
            TextPos::new(pos.para + 1, 0)
        } else {
            pos
        }
    } else if pos.char > 0 {
        TextPos::new(pos.para, pos.char - 1)
    } else if pos.para > 0 {
        TextPos::new(pos.para - 1, para_len(at, pos.para - 1))
    } else {
        pos
    }
}

fn step_word(at: At<'_>, pos: TextPos, forward: bool) -> TextPos {
    let chars = para_chars(at, pos.para);
    let word = |c: Option<&char>| c.is_some_and(|c| c.is_alphanumeric() || *c == '_');
    let mut i = pos.char.min(chars.len());
    if forward {
        if i >= chars.len() {
            return step_char(at, pos, true);
        }
        while i < chars.len() && word(chars.get(i)) {
            i += 1;
        }
        while i < chars.len() && !word(chars.get(i)) {
            i += 1;
        }
    } else {
        if i == 0 {
            return step_char(at, pos, false);
        }
        while i > 0 && !word(chars.get(i - 1)) {
            i -= 1;
        }
        while i > 0 && word(chars.get(i - 1)) {
            i -= 1;
        }
    }
    TextPos::new(pos.para, i)
}

/// The galley row holding character `ch`: (row index, its first character).
fn row_of(g: &egui::Galley, ch: usize) -> (usize, usize) {
    let mut base = 0;
    let last = g.rows.len().saturating_sub(1);
    for (i, r) in g.rows.iter().enumerate() {
        let n = r.char_count_including_newline().0;
        if ch < base + n || i == last {
            return (i, base);
        }
        base += n;
    }
    (0, 0)
}

/// The caret one line up or down at `goal_x` (pixels from the container's left).
pub(crate) fn vertical(o: &OutlineLayout, pos: TextPos, goal_x: f32, down: bool) -> TextPos {
    let Some(p) = o.paras.get(pos.para) else { return pos };
    let (row, _) = row_of(&p.galley, pos.char);
    let target = if down {
        if row + 1 < p.galley.rows.len() {
            Some((pos.para, row + 1))
        } else if pos.para + 1 < o.paras.len() {
            Some((pos.para + 1, 0))
        } else {
            None
        }
    } else if row > 0 {
        Some((pos.para, row - 1))
    } else if pos.para > 0 {
        o.paras.get(pos.para - 1).map(|q| (pos.para - 1, q.galley.rows.len().saturating_sub(1)))
    } else {
        None
    };
    let Some((para, row)) = target else {
        return if down { TextPos::new(pos.para, p.chars) } else { TextPos::new(pos.para, 0) };
    };
    let Some(q) = o.paras.get(para) else { return pos };
    let mut base = 0;
    for (i, r) in q.galley.rows.iter().enumerate() {
        if i == row {
            let col = r.char_at(goal_x - q.galley_x() - r.pos.x).0;
            return TextPos::new(para, (base + col).min(q.chars));
        }
        base += r.char_count_including_newline().0;
    }
    TextPos::new(para, q.chars)
}

pub(crate) fn caret_x(o: &OutlineLayout, pos: TextPos) -> f32 {
    o.paras.get(pos.para).map_or(0.0, |p| p.galley_x() + p.galley.pos_from_cursor(CCursor::new(pos.char)).left())
}

fn home_end(o: &OutlineLayout, pos: TextPos, end: bool) -> TextPos {
    let Some(p) = o.paras.get(pos.para) else { return pos };
    let (row, base) = row_of(&p.galley, pos.char);
    let n = p.galley.rows.get(row).map_or(0, |r| r.char_count_excluding_newline().0);
    TextPos::new(pos.para, if end { (base + n).min(p.chars) } else { base })
}

fn mods_cmd(m: &Modifiers) -> bool {
    m.command || m.ctrl
}

/// The tag shortcuts (Mod+2 … Mod+9 put a tag on, Mod+0 takes them off; Mod+1, To Do, is the
/// To Do command's own): `format.tag` with its params.
fn tag_shortcut(key: Key, m: Modifiers) -> Option<Value> {
    use notecraft_engine::cmd::format::{TAG_KEYS, TAG_REMOVE_KEY};
    let matches = |k: &str| {
        crate::app::keys::parse_shortcut(k).is_some_and(|sc| {
            sc.logical_key == key && sc.modifiers.command == m.command && sc.modifiers.alt == m.alt && sc.modifiers.shift == m.shift
        })
    };
    if matches(TAG_REMOVE_KEY) {
        return Some(json!({ "tag": null }));
    }
    TAG_KEYS.iter().filter(|(_, t)| !matches!(t, notecraft_doc::Tag::ToDo { .. })).find(|(k, _)| matches(k)).map(|(_, t)| json!({ "tag": t.name() }))
}

/// The registry command a key press is the shortcut of (formatting and Insert Date/Time).
fn shortcut_command(key: Key, m: Modifiers) -> Option<&'static str> {
    for spec in notecraft_engine::registry().all() {
        if !(spec.id.starts_with("format.") || spec.id == ids::INSERT_DATE_TIME) {
            continue;
        }
        let Some(keys) = spec.shortcut else { continue };
        for k in keys.split(" / ") {
            let Some(sc) = crate::app::keys::parse_shortcut(k) else { continue };
            if sc.logical_key == key && sc.modifiers.command == m.command && sc.modifiers.alt == m.alt && sc.modifiers.shift == m.shift {
                return Some(spec.id);
            }
        }
    }
    None
}

/// Keyboard and text input for the caret, and Delete/Esc for selected items.
pub fn keyboard(ui: &mut Ui, state: &mut CanvasState, session: &mut Session, services: &Services, v: &View, lay: &PageLayout, id: egui::Id) {
    let _ = services;
    let editing = state.caret.is_some() || state.pending.is_some();
    let focused_other = ui.memory(|m| m.focused().is_some_and(|f| f != id));
    if editing && !focused_other {
        ui.memory_mut(|m| {
            m.request_focus(id);
            m.set_focus_lock_filter(id, egui::EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: true });
        });
    }
    let has_focus = ui.memory(|m| m.has_focus(id));
    if editing && has_focus {
        type_keys(ui, state, session, lay, v, id);
        return;
    }
    if ui.memory(|m| m.focused().is_some()) {
        return;
    }
    // Nothing has the keyboard: Delete removes what's selected on the page, Esc lets it go.
    let (del, esc) = ui.input(|i| (i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace), i.key_pressed(Key::Escape)));
    if del {
        if !session.ink_selection.is_empty() {
            run(state, session, ids::DRAW_DELETE_SELECTION, json!({}));
            state.selection.strokes.clear();
        }
        for el in std::mem::take(&mut state.selection.elements) {
            run(state, session, ids::OUTLINE_DELETE, json!({"id": el, "page": state.page}));
        }
    }
    if esc {
        state.selection.elements.clear();
        super::pointer::clear_ink_selection(state, session);
    }
}

fn type_keys(ui: &mut Ui, state: &mut CanvasState, session: &mut Session, lay: &PageLayout, v: &View, id: egui::Id) {
    let events = ui.input(|i| i.events.clone());
    for ev in events {
        match ev {
            Event::Text(t) => {
                let cmd = ui.input(|i| i.modifiers.command && !i.modifiers.alt);
                if !cmd {
                    insert(state, session, &t);
                }
            }
            Event::Ime(egui::ImeEvent::Commit(t)) => insert(state, session, &t),
            Event::Paste(t) => insert(state, session, &t),
            Event::Copy => {
                if let Some(t) = selected_text(state, session) {
                    ui.ctx().copy_text(t);
                }
            }
            Event::Cut => {
                if let Some(t) = selected_text(state, session) {
                    ui.ctx().copy_text(t);
                    delete(state, session, true, false);
                }
            }
            Event::Key { key, pressed: true, modifiers, .. } => key_press(ui, state, session, lay, v, key, modifiers),
            _ => {}
        }
        if state.caret.is_none() && state.pending.is_none() {
            break;
        }
    }
    // Where the IME's candidate window goes.
    if let Some(c) = &state.caret
        && let Some(o) = lay.outline(c.outline)
        && let Some(cr) = o.frame_caret_rect(v, c.frame(), c.head)
    {
        let generation = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            c.outline.hash(&mut h);
            h.finish()
        };
        let ime = egui::output::IMEOutput::new(id, generation, egui::IMEPurpose::Normal, o.rect(v), cr);
        ui.ctx().output_mut(|out| out.ime = Some(ime));
    }
}

fn key_press(ui: &Ui, state: &mut CanvasState, session: &mut Session, lay: &PageLayout, v: &View, key: Key, m: Modifiers) {
    if super::table::key(state, session, lay, v, key, m) {
        return;
    }
    if let Some(params) = tag_shortcut(key, m) {
        let _ = format(state, session, ids::FORMAT_TAG, params);
        return;
    }
    if let Some(cmd) = shortcut_command(key, m) {
        if cmd == ids::INSERT_DATE_TIME {
            if let Some(c) = ensure_outline(state, session)
                && let Some(out) = run(
                    state,
                    session,
                    cmd,
                    json!({"outline": c.outline, "cell": c.cell.map(|x| x.to_json()), "at": c.head.to_json(), "page": state.page, "utcOffsetMinutes": state.utc_offset_minutes}),
                )
                && let Some(pos) = caret_from(&out)
            {
                set_head(state, pos, false);
            }
        } else {
            let _ = format(state, session, cmd, json!({}));
        }
        return;
    }
    let cmd = mods_cmd(&m);
    match key {
        Key::Z if cmd => {
            let id = if m.shift { ids::EDIT_REDO } else { ids::EDIT_UNDO };
            run(state, session, id, json!({}));
            return;
        }
        Key::Y if cmd => {
            run(state, session, ids::EDIT_REDO, json!({}));
            return;
        }
        Key::A if cmd => {
            if let Some(cc) = state.caret.clone() {
                let at = At::of(session, &cc);
                let last = para_count(at).saturating_sub(1);
                let all = (TextPos::new(0, 0), TextPos::new(last, para_len(at, last)));
                if let Some(c) = state.caret.as_mut() {
                    if cc.cell.is_some() && cc.range() == all {
                        // Again in a cell: the whole table.
                        super::table::select_whole_table(c, session);
                    } else {
                        c.anchor = all.0;
                        c.head = all.1;
                    }
                }
            }
            return;
        }
        Key::Escape => {
            let id = state.caret.as_ref().map(|c| c.outline);
            leave(state, session, None);
            if let Some(id) = id
                && session.current_page().and_then(|p| p.element(id)).is_some()
            {
                state.selection.elements = vec![id];
            }
            ui.memory_mut(|mem| mem.surrender_focus(ui.scope_id().with("nc_canvas")));
            return;
        }
        Key::Enter => {
            split(state, session);
            return;
        }
        Key::Tab => {
            if state.caret.is_some() && !super::table::tab_makes_table(state, session, m) {
                let id = if m.shift { ids::FORMAT_OUTDENT } else { ids::FORMAT_INDENT };
                let _ = format(state, session, id, json!({}));
            }
            return;
        }
        Key::Backspace => {
            delete(state, session, true, m.alt || cmd);
            return;
        }
        Key::Delete => {
            delete(state, session, false, m.alt || cmd);
            return;
        }
        _ => {}
    }
    let Some(c) = state.caret.clone() else { return };
    let word = m.alt || cmd;
    let extend = m.shift;
    let o = lay.outline(c.outline).and_then(|o| o.frame(c.frame()));
    let (a, b) = c.range();
    let at = At::of(session, &c);
    let new = match key {
        Key::ArrowLeft if !extend && !c.is_collapsed() => a,
        Key::ArrowRight if !extend && !c.is_collapsed() => b,
        Key::ArrowLeft => {
            if word {
                step_word(at, c.head, false)
            } else {
                step_char(at, c.head, false)
            }
        }
        Key::ArrowRight => {
            if word {
                step_word(at, c.head, true)
            } else {
                step_char(at, c.head, true)
            }
        }
        Key::ArrowUp | Key::ArrowDown => {
            let Some(o) = o else { return };
            let goal = c.goal_x.unwrap_or_else(|| caret_x(o, c.head));
            let pos = vertical(o, c.head, goal, key == Key::ArrowDown);
            set_head(state, pos, extend);
            if let Some(cc) = state.caret.as_mut() {
                cc.goal_x = Some(goal);
                cc.cells = None;
            }
            if !extend {
                super::table::enter_table(state, session, key == Key::ArrowDown);
            }
            return;
        }
        Key::Home if cmd => TextPos::new(0, 0),
        Key::End if cmd => {
            let last = para_count(at).saturating_sub(1);
            TextPos::new(last, para_len(at, last))
        }
        Key::Home => o.map_or(TextPos::new(c.head.para, 0), |o| home_end(o, c.head, false)),
        Key::End => o.map_or(c.head, |o| home_end(o, c.head, true)),
        _ => return,
    };
    session.break_undo_merge();
    state.pending_style = PendingStyle::default();
    set_head(state, new, extend);
    if let Some(cc) = state.caret.as_mut() {
        cc.goal_x = None;
        cc.cells = None;
    }
    // Onto a table's block: into its first or last cell.
    if !extend {
        super::table::enter_table(state, session, matches!(key, Key::ArrowRight | Key::ArrowDown | Key::End));
    }
}
