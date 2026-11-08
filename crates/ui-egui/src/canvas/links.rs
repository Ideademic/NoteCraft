//! Links on the canvas: hovering one shows its address and how to follow it, Ctrl+click (⌘+click
//! on macOS) follows it, and typing `]]` after `[[Page title]]` turns it into a link to that page
//! (`link.wiki`).
//!
//! Following a `notecraft:` link goes to its page (and paragraph) with `link.follow`. Other links
//! open with the system's app ([`crate::Services::open_external`]); without that hook web and
//! mail links open through egui's `open_url` (a new browser tab on the web).

use egui::{Pos2, text::CCursor, vec2};
use notecraft_doc::Guid;
use notecraft_engine::{Session, TextPos, ids};
use serde_json::{Value, json};

use super::layout::PageLayout;
use super::view::View;
use super::{CanvasState, TextCaret, run};
use crate::services::Services;

/// The link under screen point `at`: its container and address. Only the link's own glyphs count
/// (not the empty space after a line).
pub fn link_at(session: &Session, lay: &PageLayout, v: &View, at: Pos2) -> Option<(Guid, String)> {
    let outer = super::layout::outline_at(lay, v, at, 0.0)?;
    let page = session.current_page()?;
    let (frame, pos) = outer.hit_frame(v, at);
    let model = notecraft_engine::text::flow(page.outline(outer.id)?, frame)?;
    // In a table: the cell's layout.
    let o = outer.frame(frame)?;
    let p = o.paras.get(pos.para)?;
    let origin = o.galley_origin(v, pos.para)?;
    let local = at - origin;
    // The glyph under the pointer: the one the cursor is before or after.
    let glyph = |c: usize| {
        let a = p.galley.pos_from_cursor(CCursor::new(c));
        let b = p.galley.pos_from_cursor(CCursor::new(c + 1));
        a.top() <= local.y && local.y <= a.bottom() && a.left().min(b.left()) <= local.x && local.x <= a.left().max(b.left()) && c < p.chars
    };
    let ch = if glyph(pos.char) {
        pos.char
    } else if pos.char > 0 && glyph(pos.char - 1) {
        pos.char - 1
    } else {
        return None;
    };
    let para = model.para(pos.para)?;
    let mut seen = 0usize;
    for r in &para.runs {
        let n = r.text.chars().count();
        if ch < seen + n {
            return r.link.clone().map(|u| (o.id, u));
        }
        seen += n;
    }
    None
}

/// Whether Ctrl (⌘ on macOS) is held, the modifier that follows links.
fn follow_modifier(ui: &egui::Ui) -> bool {
    ui.input(|i| i.modifiers.command)
}

/// A Ctrl+click on a link: remember it to follow (after the pointer is handled). `true` when the
/// click was on a link.
pub fn press(ui: &egui::Ui, state: &mut CanvasState, session: &Session, v: &View, lay: &PageLayout, at: Pos2) -> bool {
    if !follow_modifier(ui) {
        return false;
    }
    match link_at(session, lay, v, at) {
        Some((_, url)) => {
            state.review.follow = Some(url);
            true
        }
        None => false,
    }
}

/// Follows a link the pointer asked for, and shows the hovered link's address.
pub fn frame(ui: &egui::Ui, state: &mut CanvasState, session: &mut Session, services: &Services, v: &View, lay: &PageLayout, resp: &egui::Response) {
    if let Some(url) = state.review.follow.take() {
        follow(ui.ctx(), state, session, services, &url);
    }
    let Some(at) = resp.hover_pos() else { return };
    if super::pointer::is_dragging(state) {
        return;
    }
    let Some((_, url)) = link_at(session, lay, v, at) else { return };
    if follow_modifier(ui) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let key = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
    let shown = notecraft_doc::link::InternalLink::parse(&url).and_then(|l| l.title).map_or_else(
        || url.chars().take(120).collect::<String>(),
        |t| crate::i18n::fmt_plain(crate::tl!("Page \u{201c}{title}\u{201d}"), &[("title", &t)]),
    );
    let tip = crate::i18n::fmt_plain(crate::tl!("{address}\n{key}+click to follow the link"), &[("address", &shown), ("key", key)]);
    egui::Tooltip::always_open(ui.ctx().clone(), ui.layer_id(), egui::Id::unique("nc_link_tip"), egui::PopupAnchor::Position(at + vec2(12.0, 18.0)))
        .show(|ui| ui.label(tip));
}

/// Follows `url`: a `notecraft:` link goes to its page (and paragraph); other links open outside.
pub fn follow(ctx: &egui::Context, state: &mut CanvasState, session: &mut Session, services: &Services, url: &str) {
    let Some(out) = run(state, session, ids::LINK_FOLLOW, json!({ "url": url })) else { return };
    if out.value.get("internal").and_then(Value::as_bool) == Some(true) {
        if let Some(outline) = out.value.get("outline").and_then(Value::as_str).and_then(Guid::parse) {
            let para = out.value.get("para").and_then(Value::as_u64).and_then(|p| usize::try_from(p).ok()).unwrap_or(0);
            state.review.goto = Some((outline, para));
        }
        return;
    }
    let Some(external) = out.value.get("external").and_then(Value::as_str) else { return };
    let web = matches!(notecraft_doc::link::scheme(external).as_deref(), Some("http" | "https" | "mailto"));
    if !web && services.open_external.is_some() {
        // A file on this computer (it may be a program): the shell asks first.
        state.review.requests.push(super::context_menu::CanvasRequest::ConfirmLink(external.to_owned()));
        return;
    }
    open_external(ctx, services, external, &mut state.messages);
}

/// Opens a web, mail or file link outside NoteCraft (no questions asked).
pub fn open_external(ctx: &egui::Context, services: &Services, external: &str, messages: &mut Vec<String>) {
    let web = matches!(notecraft_doc::link::scheme(external).as_deref(), Some("http" | "https" | "mailto"));
    let result = match &services.open_external {
        Some(open) => open(external),
        None => {
            if web {
                ctx.open_url(egui::OpenUrl::new_tab(external));
                Ok(())
            } else {
                Err(crate::tl!("NoteCraft can't open this kind of link here.").to_owned())
            }
        }
    };
    if let Err(e) = result
        && messages.len() < 8
    {
        messages.push(e);
    }
}

/// After following a link to a paragraph: once its page is the canvas's, the caret goes to the
/// paragraph's start and scrolls into view.
pub fn apply_goto(state: &mut CanvasState, session: &Session) {
    let Some((outline, para)) = state.review.goto else { return };
    if state.page != session.current.page {
        return;
    }
    state.review.goto = None;
    let Some(o) = session.current_page().and_then(|p| p.outline(outline)) else { return };
    let pos = notecraft_engine::text::clamp_ref(o, TextPos::new(para, 0));
    state.caret = Some(TextCaret::at(outline, pos));
    state.editing = Some(outline);
    state.selection.elements.clear();
    state.reveal_caret = true;
}

/// After typing `typed`: `[[name]]` just before the caret becomes a link to page "name".
pub fn after_insert(state: &mut CanvasState, session: &mut Session, typed: &str) {
    if !typed.ends_with(']') {
        return;
    }
    let Some(c) = state.caret.clone() else { return };
    let page = state.page;
    let Ok(out) =
        session.run(ids::LINK_WIKI, &json!({"outline": c.outline, "cell": c.cell.map(|x| x.to_json()), "at": c.head.to_json(), "page": page}))
    else {
        return;
    };
    if let Some(pos) = out.value.get("caret").and_then(TextPos::from_json)
        && let Some(cc) = state.caret.as_mut()
    {
        cc.head = pos;
        cc.anchor = pos;
        session.break_undo_merge();
    }
}
