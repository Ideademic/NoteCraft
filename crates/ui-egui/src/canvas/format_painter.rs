//! Home › Format Painter: picks up the text formatting at the caret (`format.painterPick`: font,
//! size, colours, highlight, bold/italic/underline/strikethrough, and the paragraph's style,
//! alignment and list), then paints it onto the next text selected or word clicked
//! (`format.painterApply`). A click arms it for one use; a double-click keeps it on until Esc
//! (or the button is clicked again). Tags and links aren't copied, as in OneNote.

use notecraft_engine::{Session, ids};
use serde_json::Value;

use super::{CanvasState, run, selection_params};

/// Formatting waiting to be painted.
#[derive(Clone, Debug, PartialEq)]
pub struct Armed {
    /// `format.painterPick`'s output.
    pub format: Value,
    /// Stays on after painting (double-click) until Esc.
    pub sticky: bool,
}

/// Picks up the formatting at the caret and arms the painter. `false` when no text is being
/// edited (nothing to pick up).
pub fn pick(state: &mut CanvasState, session: &mut Session, sticky: bool) -> bool {
    let Some(params) = selection_params(state) else { return false };
    match session.run(ids::FORMAT_PAINTER_PICK, &params) {
        Ok(out) => {
            state.painter = Some(Armed { format: out.value, sticky });
            true
        }
        Err(e) => {
            state.messages.push(e.to_string());
            false
        }
    }
}

/// Paints onto the caret's selection (or the word at a collapsed caret); a one-use painter
/// disarms. Called when a click or drag in a container ends.
pub fn apply(state: &mut CanvasState, session: &mut Session) {
    let Some(armed) = state.painter.clone() else { return };
    let Some(mut params) = selection_params(state) else { return };
    if let Value::Object(m) = &mut params {
        m.insert("format".into(), armed.format.clone());
    }
    session.break_undo_merge();
    run(state, session, ids::FORMAT_PAINTER_APPLY, params);
    session.break_undo_merge();
    if !armed.sticky {
        state.painter = None;
    }
}

/// Turns the painter off (Esc, a click on empty paper with a one-use painter).
pub fn cancel(state: &mut CanvasState) {
    state.painter = None;
}
