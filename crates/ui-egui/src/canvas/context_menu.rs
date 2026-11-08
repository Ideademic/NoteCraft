//! What a right-click on the page is on. The canvas works out the target (text, with the
//! misspelling or link under the pointer; a picture, attached file, printout, kept content or
//! ink; a note container's grip bar; empty paper) and puts it in [`CanvasReview::menu`]; the shell
//! draws the menu ([`crate::review::context_menu`]), where everything the app can do (dialogs,
//! saving files, the ribbon's menus) is at hand.
//!
//! A right-click in text outside the selection moves the caret there first, as in OneNote; on a
//! picture or ink it selects that item.

use egui::Pos2;
use notecraft_doc::{ElementKind, Guid, Point};
use notecraft_engine::{Session, TextPos, ids};
use serde_json::{Value, json};

use super::layout::PageLayout;
use super::view::View;
use super::{CanvasState, TextCaret, edit, images, ink, run, text};

/// The canvas's part of the Review features, between frames.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasReview {
    /// Red wavy lines under misspelled words (the shell copies File › Options' "Check spelling as
    /// you type" here).
    pub spelling: bool,
    /// What the context menu is for (set on right-click).
    pub menu: Option<MenuTarget>,
    /// The context menu should open this frame (at the pointer).
    pub open_menu: bool,
    /// What only the shell can do (dialogs, saving files); the shell drains it every frame.
    pub requests: Vec<CanvasRequest>,
    /// Input to hand the canvas's keyboard handling next frame (the menu's Cut, Copy and Paste).
    pub inject: Vec<egui::Event>,
    /// A link Ctrl+clicked this frame, to follow.
    pub(crate) follow: Option<String>,
    /// After following a link to a paragraph: the caret goes there once its page shows.
    pub(crate) goto: Option<(Guid, usize)>,
    /// The misspellings marked on screen last frame: (container, paragraph, start, end).
    pub marks: Vec<(Guid, usize, usize, usize)>,
}

impl Default for CanvasReview {
    fn default() -> Self {
        CanvasReview {
            spelling: true,
            menu: None,
            open_menu: false,
            requests: Vec::new(),
            inject: Vec::new(),
            follow: None,
            goto: None,
            marks: Vec::new(),
        }
    }
}

/// Something the canvas asks the shell to do.
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasRequest {
    /// Insert › Link (Ctrl+K) for the caret's selection.
    LinkDialog,
    /// Open an attached file (double-click on it).
    OpenAttachment { element: Guid, name: String },
    /// Open a link to a file on this computer, once the user says so.
    ConfirmLink(String),
}

/// What was right-clicked.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuTarget {
    /// Text in a note container, the caret (and selection) set; the misspelling there
    /// (`proof.suggestions`' answer) and the link there, if any.
    Text {
        outline: Guid,
        at: TextPos,
        issue: Option<Value>,
        link: Option<String>,
    },
    /// A note container's grip bar.
    Container(Guid),
    Picture(Guid),
    Attachment {
        element: Guid,
        name: String,
    },
    Printout(Guid),
    /// Content NoteCraft keeps but can't show (an equation, a recording…).
    Kept {
        element: Guid,
        label: String,
    },
    /// Selected ink strokes.
    Ink,
    /// Empty paper, at this page point.
    Page(Point),
}

/// Works out the target of a right-click at `at` and prepares the selection for it.
pub fn on_secondary_click(state: &mut CanvasState, session: &mut Session, v: &View, lay: &PageLayout, at: Pos2) {
    let target = target_at(state, session, v, lay, at);
    state.review.menu = Some(target);
    state.review.open_menu = true;
}

fn target_at(state: &mut CanvasState, session: &mut Session, v: &View, lay: &PageLayout, at: Pos2) -> MenuTarget {
    let page_pt = v.to_page(at);
    // Note containers, top-most first: the grip bar, then the text.
    for o in lay.outlines.iter().rev() {
        let (frame, grip, _) = text::frame_rects(o, v);
        if grip.contains(at) {
            edit::leave(state, session, Some(o.id));
            state.selection.elements = vec![o.id];
            return MenuTarget::Container(o.id);
        }
        if !frame.contains(at) {
            continue;
        }
        let pos = o.hit(v, at);
        let inside = state.caret.as_ref().is_some_and(|c| {
            let (a, b) = c.range();
            c.outline == o.id && !c.is_collapsed() && a <= pos && pos <= b
        });
        if !inside {
            if state.caret.as_ref().is_none_or(|c| c.outline != o.id) {
                edit::leave(state, session, Some(o.id));
            }
            state.pending = None;
            state.selection.elements.clear();
            let key = state.caret.as_ref().map(|c| c.merge_key.clone());
            let mut c = TextCaret::at(o.id, pos);
            if let Some(k) = key {
                c.merge_key = k;
            }
            state.caret = Some(c);
            state.editing = Some(o.id);
        }
        let page = state.page;
        let issue = state
            .review
            .spelling
            .then(|| session.run(ids::PROOF_SUGGESTIONS, &json!({"outline": o.id, "at": pos.to_json(), "page": page})).ok())
            .flatten()
            .map(|out| out.value)
            .filter(|v| !v.is_null());
        let link = super::links::link_at(session, lay, v, at).map(|(_, url)| url);
        return MenuTarget::Text { outline: o.id, at: pos, issue, link };
    }
    // Pictures, files, printouts and kept content.
    let hit = session.current_page().and_then(|page| {
        page.elements.iter().rev().find_map(|el| {
            let r = images::item_rect(state, page, el)?;
            if !v.rect(r).contains(at) {
                return None;
            }
            let target = match &el.kind {
                ElementKind::Image(_) => MenuTarget::Picture(el.id),
                ElementKind::EmbeddedFile { name, .. } => MenuTarget::Attachment { element: el.id, name: name.clone() },
                ElementKind::Printout(_) => MenuTarget::Printout(el.id),
                ElementKind::Opaque { jcid, data } => MenuTarget::Kept { element: el.id, label: images::opaque_label(el.id, *jcid, data).to_owned() },
                ElementKind::Outline(_) => return None,
            };
            Some((el.id, target))
        })
    });
    if let Some((id, target)) = hit {
        edit::leave(state, session, None);
        state.pending = None;
        super::pointer::clear_ink_selection(state, session);
        state.selection.elements = vec![id];
        return target;
    }
    // Ink: the stroke under the pointer joins the selection unless it is in it already.
    if let Some(stroke) = ink::stroke_at(session, v, at) {
        edit::leave(state, session, None);
        state.selection.elements.clear();
        if !session.ink_selection.contains(&stroke)
            && let Some(out) = run(state, session, ids::DRAW_LASSO_SELECT, json!({"ids": [stroke]}))
        {
            state.selection.strokes = super::pointer::ids_of(&out.value);
        }
        return MenuTarget::Ink;
    }
    edit::leave(state, session, None);
    state.selection.elements.clear();
    super::pointer::clear_ink_selection(state, session);
    MenuTarget::Page(page_pt)
}

/// A double-click on an attached file asks the shell to open it.
pub fn on_double_click(state: &mut CanvasState, session: &Session, v: &View, at: Option<Pos2>) {
    let Some(at) = at else { return };
    let Some(id) = images::element_at(state, session, v, at) else { return };
    if let Some(ElementKind::EmbeddedFile { name, .. }) = session.current_page().and_then(|p| p.element(id)).map(|e| &e.kind) {
        state.review.requests.push(CanvasRequest::OpenAttachment { element: id, name: name.clone() });
    }
}
