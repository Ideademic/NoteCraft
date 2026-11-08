//! The shell's side of Review and links: the page's right-click menus (the canvas finds the
//! target, [`crate::canvas::context_menu`]), Insert Link (Ctrl+K), Word Count, opening and saving
//! attachments, the proofing language menus and File › Options › Proofing, and keeping the user
//! dictionary and the default proofing language in the settings.
//!
//! Some menu items belong to editing features that land separately (rotate, crop and alt text
//! for pictures, bring to front and send to back, paste picture, rich copy): they are looked up by
//! command id ([`EXTRA`]) and shown once one of their ids is registered.

use base64::Engine as _;
use egui::{Align, Frame, Layout, Ui};
use notecraft_doc::Guid;
use notecraft_engine::{TextPos, ids, proof, registry};
use serde_json::{Value, json};

use crate::canvas::TextCaret;
use crate::canvas::context_menu::{CanvasRequest, MenuTarget};
use crate::icons::Icon;
use crate::theme::regular;
use crate::widgets::{self, flat_button};
use crate::{NoteApp, tl};

/// Review's state in the shell.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReviewUi {
    /// Review › Spelling's pane.
    pub spelling: Option<crate::panes::spelling::SpellPane>,
    /// Insert Link (Ctrl+K).
    pub link: Option<LinkDialog>,
    /// Review › Word Count's answer, shown in a dialog.
    pub word_count: Option<Value>,
    /// An attached file waiting for "Open it?" (element, name).
    pub open_file: Option<(Guid, String)>,
    /// A link to a file on this computer waiting for "Open it?".
    pub open_link: Option<String>,
    /// A picture's alt text being edited (element, text).
    pub alt_text: Option<(Guid, String)>,
    /// craft-proof's dictionary generation the settings last copied.
    dictionary: Option<u64>,
    warmed: bool,
}

/// Insert Link's form.
#[derive(Clone, Debug, PartialEq)]
pub struct LinkDialog {
    pub outline: Guid,
    pub start: TextPos,
    pub end: TextPos,
    /// The text to show.
    pub text: String,
    pub address: String,
    /// The selection had a link already (Remove Link is offered).
    pub existing: bool,
    /// Why the address was refused.
    pub error: Option<String>,
    /// Filters the page list.
    pub filter: String,
    /// The text as it was when the dialog opened.
    original: String,
    /// The address box takes the keyboard on the first frame (from the note being edited).
    focus: bool,
}

/// Menu items of editing features that may not be in this build: the first registered id of each
/// is used, with the item's params.
pub const EXTRA: &[(&str, &[&str])] = &[
    ("rotateRight", &["picture.rotateRight", "picture.rotate", "image.rotate", "arrange.rotate", "draw.rotate"]),
    ("rotateLeft", &["picture.rotateLeft"]),
    ("flipHorizontal", &["picture.flipHorizontal", "picture.flip", "image.flip"]),
    ("flipVertical", &["picture.flipVertical"]),
    ("crop", &["picture.crop", "image.crop"]),
    ("altText", &["picture.altText", "picture.setAltText", "image.altText"]),
    ("bringToFront", &["arrange.bringToFront", "draw.bringToFront", "element.bringToFront", "order.bringToFront"]),
    ("sendToBack", &["arrange.sendToBack", "draw.sendToBack", "element.sendToBack", "order.sendToBack"]),
    ("pastePicture", &["edit.pastePicture", "insert.pastePicture", "clipboard.pastePicture"]),
    ("copy", &["edit.copy", "clipboard.copy"]),
    ("cut", &["edit.cut", "clipboard.cut"]),
    ("paste", &["edit.paste", "clipboard.paste"]),
];

/// The registered command for an [`EXTRA`] item, if any.
pub fn extra(name: &str) -> Option<&'static str> {
    EXTRA.iter().find(|(n, _)| *n == name).and_then(|(_, ids)| ids.iter().copied().find(|id| registry().get(id).is_some()))
}

// --- settings and per-frame work ---------------------------------------------------------------

/// The system's language for proofing, as a BCP 47 tag.
pub fn system_proofing_language() -> String {
    if cfg!(test) {
        return "en-US".to_owned();
    }
    let tag = craft_i18n::Region::system().numbers;
    notecraft_doc::page::clean_lang(&tag).filter(|t| !t.eq_ignore_ascii_case("und")).unwrap_or_else(|| "en-US".to_owned())
}

/// The proofing language text without one is checked in.
pub fn default_language(app: &NoteApp) -> String {
    if app.ui.prefs.proofing_language.is_empty() { system_proofing_language() } else { app.ui.prefs.proofing_language.clone() }
}

/// Settings taken at start: the user dictionary and the default proofing language.
pub fn load_prefs(session: &mut notecraft_engine::Session, prefs: &crate::Prefs) {
    proof::load_user_words(&prefs.user_words);
    let lang = if prefs.proofing_language.is_empty() { system_proofing_language() } else { prefs.proofing_language.clone() };
    let _ = session.run(ids::PROOF_SET_DEFAULT_LANGUAGE, &json!({ "lang": lang }));
}

/// Every frame: the canvas's spelling switch and requests, the default language, and the user
/// dictionary copied into the settings when it changed.
pub fn tick(app: &mut NoteApp, ctx: &egui::Context) {
    if !app.ui.review.warmed {
        app.ui.review.warmed = true;
        if proof::BACKGROUND && app.ui.prefs.check_spelling {
            proof::warm_up();
        }
    }
    app.canvas.review.spelling = app.ui.prefs.check_spelling;
    let lang = default_language(app);
    if app.session.proof.default_lang != lang {
        let _ = app.session.run(ids::PROOF_SET_DEFAULT_LANGUAGE, &json!({ "lang": lang }));
    }
    let generation = proof::dictionary_generation();
    if app.ui.review.dictionary != Some(generation) {
        app.ui.review.dictionary = Some(generation);
        let words = proof::user_words();
        if words != app.ui.prefs.user_words {
            app.ui.prefs.user_words = words;
        }
    }
    for r in std::mem::take(&mut app.canvas.review.requests) {
        match r {
            CanvasRequest::LinkDialog => open_link_dialog(app),
            CanvasRequest::OpenAttachment { element, name } => app.ui.review.open_file = Some((element, name)),
            CanvasRequest::ConfirmLink(url) => app.ui.review.open_link = Some(url),
        }
    }
    if app.ui.review.spelling.is_some() {
        crate::panes::spelling::follow_page(app);
    }
    let _ = ctx;
}

// --- links ---------------------------------------------------------------------------------------

/// Insert Link (Ctrl+K): for the selection; a caret in a link edits that link.
pub fn open_link_dialog(app: &mut NoteApp) {
    let Some(c) = app.canvas.caret.clone() else {
        app.set_status(tl!("Click in a note, or select some text, to add a link."), false);
        return;
    };
    let page = app.session.current.page;
    let (mut a, mut b) = c.range();
    let existing = app
        .session
        .run(ids::LINK_GET, &json!({"outline": c.outline, "at": a.to_json(), "page": page}))
        .ok()
        .map(|o| o.value)
        .filter(|v| !v.is_null());
    let mut address = String::new();
    if let Some(l) = &existing {
        address = l.get("url").and_then(Value::as_str).unwrap_or_default().to_owned();
        if a == b {
            a = l.get("start").and_then(TextPos::from_json).unwrap_or(a);
            b = l.get("end").and_then(TextPos::from_json).unwrap_or(b);
        }
    }
    let text = selected_text(app, c.outline, a, b);
    app.ui.review.link = Some(LinkDialog {
        outline: c.outline,
        start: a,
        end: b,
        text: text.clone(),
        address,
        existing: existing.is_some(),
        error: None,
        filter: String::new(),
        original: text,
        focus: true,
    });
}

fn selected_text(app: &NoteApp, outline: Guid, a: TextPos, b: TextPos) -> String {
    let Some(o) = app.session.current_page().and_then(|p| p.outline(outline)) else { return String::new() };
    let mut out = String::new();
    for i in a.para..=b.para.min(o.paragraphs.len().saturating_sub(1)) {
        let (x, y) = notecraft_engine::text::para_span(o, i, a, b);
        if i > a.para {
            out.push(' ');
        }
        if let Some(p) = o.paragraphs.get(i) {
            out.extend(p.text().chars().skip(x).take(y - x));
        }
    }
    out
}

/// The pages of the open notebooks whose title contains `filter`: (title, section name, id).
fn pages_matching(app: &NoteApp, filter: &str) -> Vec<(String, String, Guid)> {
    let f = filter.trim().to_lowercase();
    let mut out = Vec::new();
    for nb in &app.session.notebooks {
        for sec in nb.all_sections() {
            for p in &sec.pages {
                let title = p.display_title().to_owned();
                if f.is_empty() || title.to_lowercase().contains(&f) {
                    out.push((title, sec.name.clone(), p.id));
                    if out.len() >= 40 {
                        return out;
                    }
                }
            }
        }
    }
    out
}

fn link_dialog(ctx: &egui::Context, app: &mut NoteApp) {
    let Some(mut d) = app.ui.review.link.clone() else { return };
    let t = app.tokens.clone();
    let mut done: Option<bool> = None;
    let mut remove = false;
    let modal = egui::Modal::new(egui::Id::unique("nc_link_dialog")).frame(
        Frame::NONE.fill(t.menu).stroke(egui::Stroke::new(1.0, t.border_strong)).corner_radius(crate::theme::RADIUS_MEDIUM).inner_margin(18.0),
    );
    let resp = modal.show(ctx, |ui| {
        ui.set_width(420.0);
        ui.label(egui::RichText::new(tl!("Link")).font(regular(17.0)).color(t.text));
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tl!("Text to display")).font(regular(12.0)).color(t.text_dim));
        let r = ui.add(egui::TextEdit::singleline(&mut d.text).desired_width(f32::INFINITY));
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, crate::i18n::t(TEXT_LABEL)));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(tl!("Address")).font(regular(12.0)).color(t.text_dim));
        let r = ui.add(egui::TextEdit::singleline(&mut d.address).desired_width(f32::INFINITY).hint_text("https://"));
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::TextInput, true, crate::i18n::t(ADDRESS_LABEL)));
        if std::mem::take(&mut d.focus) {
            r.request_focus();
        }
        if r.changed() {
            d.error = None;
        }
        if let Some(e) = &d.error {
            ui.label(egui::RichText::new(e).font(regular(12.0)).color(crate::canvas::proof::line_color(t.dark)));
        }
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tl!("Or pick a page to link to")).font(regular(12.0)).color(t.text_dim));
        ui.add(egui::TextEdit::singleline(&mut d.filter).desired_width(f32::INFINITY).hint_text(tl!("Search page titles")));
        egui::ScrollArea::vertical().max_height(150.0).id_salt("nc_link_pages").show(ui, |ui| {
            for (title, section, id) in pages_matching(app, &d.filter) {
                let label = format!("{title}   \u{2014} {section}");
                if ui.selectable_label(false, egui::RichText::new(label).font(regular(12.5))).clicked()
                    && let Ok(out) = app.session.run(ids::LINK_TO_PAGE, &json!({ "id": id }))
                {
                    d.address = out.value.get("url").and_then(Value::as_str).unwrap_or_default().to_owned();
                    if d.text.trim().is_empty() {
                        d.text = title.clone();
                    }
                    d.error = None;
                }
            }
        });
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if flat_button(ui, tl!("OK"), true, !d.address.trim().is_empty()) {
                done = Some(true);
            }
            if flat_button(ui, tl!("Cancel"), false, true) {
                done = Some(false);
            }
            if d.existing && flat_button(ui, tl!("Remove Link"), false, true) {
                remove = true;
            }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && !d.address.trim().is_empty() {
            done = Some(true);
        }
    });
    if resp.should_close() && done.is_none() && !remove {
        done = Some(false);
    }
    let page = app.session.current.page;
    if remove {
        app.act(ids::LINK_REMOVE, json!({"outline": d.outline, "start": d.start.to_json(), "end": d.end.to_json(), "page": page}));
        app.ui.review.link = None;
        return;
    }
    match done {
        Some(true) => {
            let mut params = json!({"outline": d.outline, "start": d.start.to_json(), "end": d.end.to_json(), "url": d.address, "page": page});
            if (d.text != d.original || d.start == d.end)
                && !d.text.trim().is_empty()
                && let Value::Object(m) = &mut params
            {
                m.insert("text".into(), json!(d.text));
            }
            app.session.break_undo_merge();
            match app.session.run(ids::LINK_SET, &params) {
                Ok(out) => {
                    if let Some(c) = out.value.get("caret").and_then(TextPos::from_json) {
                        let mut caret = TextCaret::at(d.outline, c);
                        if let Some(old) = &app.canvas.caret {
                            caret.merge_key = old.merge_key.clone();
                        }
                        app.canvas.caret = Some(caret);
                        app.canvas.editing = Some(d.outline);
                    }
                    app.ui.review.link = None;
                }
                Err(e) => {
                    d.error = Some(e.to_string().trim_start_matches("bad parameters: ").to_owned());
                    app.ui.review.link = Some(d);
                }
            }
        }
        Some(false) => app.ui.review.link = None,
        None => app.ui.review.link = Some(d),
    }
}

/// Accessible names of the link dialog's boxes (tests find them by these).
pub const TEXT_LABEL: &str = "Link text";
pub const ADDRESS_LABEL: &str = "Link address";

// --- word count, attachments --------------------------------------------------------------------

/// Review › Word Count: the selection's, else the page's.
pub fn word_count(app: &mut NoteApp) {
    let params = crate::canvas::selection_params(&app.canvas).unwrap_or_else(|| json!({}));
    if let Ok(out) = app.run(ids::REVIEW_WORD_COUNT, params) {
        app.ui.review.word_count = Some(out.value);
    }
}

fn word_count_dialog(ctx: &egui::Context, app: &mut NoteApp) {
    let Some(v) = app.ui.review.word_count.clone() else { return };
    let t = app.tokens.clone();
    let n = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0).to_string();
    let selection = v.get("scope").and_then(Value::as_str) == Some("selection");
    let mut close = false;
    let modal = egui::Modal::new(egui::Id::unique("nc_word_count")).frame(
        Frame::NONE.fill(t.menu).stroke(egui::Stroke::new(1.0, t.border_strong)).corner_radius(crate::theme::RADIUS_MEDIUM).inner_margin(18.0),
    );
    let resp = modal.show(ctx, |ui| {
        ui.set_width(300.0);
        ui.label(egui::RichText::new(tl!("Word Count")).font(regular(17.0)).color(t.text));
        ui.label(egui::RichText::new(if selection { tl!("The selected text") } else { tl!("This page") }).font(regular(12.0)).color(t.text_dim));
        ui.add_space(8.0);
        egui::Grid::new("nc_word_count_grid").num_columns(2).spacing([24.0, 6.0]).show(ui, |ui| {
            for (label, key) in [
                (tl!("Words"), "words"),
                (tl!("Characters (no spaces)"), "charactersNoSpaces"),
                (tl!("Characters (with spaces)"), "characters"),
                (tl!("Paragraphs"), "paragraphs"),
            ] {
                ui.label(egui::RichText::new(label).font(regular(13.0)).color(t.text));
                ui.label(egui::RichText::new(n(key)).font(regular(13.0)).color(t.text));
                ui.end_row();
            }
        });
        ui.add_space(12.0);
        if flat_button(ui, tl!("Close"), true, true) || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            close = true;
        }
    });
    if close || resp.should_close() {
        app.ui.review.word_count = None;
    }
}

/// Right-click › Save As (an attached file, a picture, a printout's PDF).
pub fn save_attachment(app: &mut NoteApp, element: Guid) {
    let name = app
        .session
        .current_page()
        .and_then(|p| p.element(element))
        .map(|e| match &e.kind {
            notecraft_doc::ElementKind::EmbeddedFile { name, .. } => name.clone(),
            notecraft_doc::ElementKind::Printout(p) => p.file_name.clone(),
            notecraft_doc::ElementKind::Image(i) => format!("{}.{}", tl!("Picture"), i.format.extension()),
            _ => tl!("Attachment").to_owned(),
        })
        .unwrap_or_default();
    let page = app.session.current.page;
    crate::app::files::save_as(app, name, ids::FILE_EXPORT_ATTACHMENT, json!({ "element": element, "page": page }));
}

/// Opens an attached file with the system's app (the desktop), or downloads it (the web).
pub fn open_attachment(app: &mut NoteApp, element: Guid) {
    let page = app.session.current.page;
    let Ok(out) = app.run(ids::FILE_EXPORT_ATTACHMENT, json!({ "element": element, "page": page })) else { return };
    let name = out.value.get("fileName").and_then(Value::as_str).unwrap_or("Attachment").to_owned();
    let Some(bytes) = out.value.get("bytesBase64").and_then(Value::as_str).and_then(|b| base64::engine::general_purpose::STANDARD.decode(b).ok())
    else {
        return;
    };
    let result = match (&app.services.open_file, &app.services.download) {
        (Some(open), _) => open(&name, &bytes),
        (None, Some(download)) => download(&name, &bytes),
        (None, None) => Err(tl!("This version can't open files here.").to_owned()),
    };
    if let Err(e) = result {
        app.set_status(e, true);
    }
}

fn open_file_prompt(ctx: &egui::Context, app: &mut NoteApp) {
    let Some((element, name)) = app.ui.review.open_file.clone() else { return };
    let t = app.tokens.clone();
    let mut answer = None;
    let modal = egui::Modal::new(egui::Id::unique("nc_open_attachment")).frame(
        Frame::NONE.fill(t.menu).stroke(egui::Stroke::new(1.0, t.border_strong)).corner_radius(crate::theme::RADIUS_MEDIUM).inner_margin(20.0),
    );
    let resp = modal.show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(
            egui::RichText::new(crate::i18n::fmt_plain(tl!("Open \u{201c}{name}\u{201d}?"), &[("name", &name)])).font(regular(17.0)).color(t.text),
        );
        ui.add_space(6.0);
        let body = if app.services.open_file.is_some() {
            tl!("It opens in the app your computer uses for it. Files from people or places you don't trust can harm your computer.")
        } else {
            tl!("Your browser downloads it. Files from people or places you don't trust can harm your computer.")
        };
        ui.label(egui::RichText::new(body).font(regular(13.0)).color(t.text_dim));
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if flat_button(ui, tl!("Open"), true, true) {
                answer = Some(true);
            }
            if flat_button(ui, tl!("Cancel"), false, true) {
                answer = Some(false);
            }
        });
    });
    if resp.should_close() && answer.is_none() {
        answer = Some(false);
    }
    if let Some(yes) = answer {
        app.ui.review.open_file = None;
        if yes {
            open_attachment(app, element);
        }
    }
}

fn open_link_prompt(ctx: &egui::Context, app: &mut NoteApp) {
    let Some(url) = app.ui.review.open_link.clone() else { return };
    let t = app.tokens.clone();
    let mut answer = None;
    let modal = egui::Modal::new(egui::Id::unique("nc_open_link")).frame(
        Frame::NONE.fill(t.menu).stroke(egui::Stroke::new(1.0, t.border_strong)).corner_radius(crate::theme::RADIUS_MEDIUM).inner_margin(20.0),
    );
    let shown: String = notecraft_doc::link::decode(url.trim_start_matches("file://")).chars().take(200).collect();
    let resp = modal.show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(egui::RichText::new(tl!("Open this file?")).font(regular(17.0)).color(t.text));
        ui.add_space(6.0);
        ui.label(egui::RichText::new(shown).font(regular(12.5)).color(t.text));
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(tl!("The link opens a file on this computer. Files from people or places you don't trust can harm it."))
                .font(regular(13.0))
                .color(t.text_dim),
        );
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            if flat_button(ui, tl!("Open"), true, true) {
                answer = Some(true);
            }
            if flat_button(ui, tl!("Cancel"), false, true) {
                answer = Some(false);
            }
        });
    });
    if resp.should_close() && answer.is_none() {
        answer = Some(false);
    }
    if let Some(yes) = answer {
        app.ui.review.open_link = None;
        if yes {
            let mut messages = Vec::new();
            crate::canvas::links::open_external(ctx, &app.services, &url, &mut messages);
            if let Some(m) = messages.pop() {
                app.set_status(m, true);
            }
        }
    }
}

fn alt_text_prompt(ctx: &egui::Context, app: &mut NoteApp) {
    let Some((element, mut text)) = app.ui.review.alt_text.clone() else { return };
    let Some(cmd) = extra("altText") else {
        app.ui.review.alt_text = None;
        return;
    };
    let t = app.tokens.clone();
    let mut answer = None;
    let modal = egui::Modal::new(egui::Id::unique("nc_alt_text")).frame(
        Frame::NONE.fill(t.menu).stroke(egui::Stroke::new(1.0, t.border_strong)).corner_radius(crate::theme::RADIUS_MEDIUM).inner_margin(18.0),
    );
    let resp = modal.show(ctx, |ui| {
        ui.set_width(380.0);
        ui.label(egui::RichText::new(tl!("Alt Text")).font(regular(17.0)).color(t.text));
        ui.label(egui::RichText::new(tl!("Describe the picture for people who can't see it.")).font(regular(12.0)).color(t.text_dim));
        let r = ui.add(egui::TextEdit::multiline(&mut text).desired_rows(3).desired_width(f32::INFINITY));
        if !r.has_focus() {
            r.request_focus();
        }
        ui.horizontal(|ui| {
            if flat_button(ui, tl!("OK"), true, true) {
                answer = Some(true);
            }
            if flat_button(ui, tl!("Cancel"), false, true) {
                answer = Some(false);
            }
        });
    });
    if resp.should_close() && answer.is_none() {
        answer = Some(false);
    }
    match answer {
        Some(yes) => {
            app.ui.review.alt_text = None;
            if yes {
                let page = app.session.current.page;
                app.act(cmd, json!({"id": element, "element": element, "page": page, "alt": text, "text": text}));
            }
        }
        None => app.ui.review.alt_text = Some((element, text)),
    }
}

/// The dialogs and the page's context menu, after the frame's panels.
pub fn dialogs(ctx: &egui::Context, app: &mut NoteApp) {
    context_menu(ctx, app);
    link_dialog(ctx, app);
    word_count_dialog(ctx, app);
    open_file_prompt(ctx, app);
    open_link_prompt(ctx, app);
    alt_text_prompt(ctx, app);
}

// --- the page's context menu ---------------------------------------------------------------

/// Width of the page's context menu.
const MENU_W: f32 = 250.0;

/// A spelling suggestion in the menu: the word in bold, where the other items have their text.
fn suggestion_item(ui: &mut Ui, word: &str) -> bool {
    let t = crate::theme::Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(160.0), 24.0), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, word));
    if resp.hovered() {
        ui.painter().rect_filled(r, egui::CornerRadius::same(crate::theme::RADIUS_SMALL), t.hover);
    }
    crate::theme::paint_strong(ui.painter(), egui::pos2(r.min.x + 30.0, r.center().y), egui::Align2::LEFT_CENTER, word, regular(12.5), t.text);
    resp.clicked()
}

fn menu_id() -> egui::Id {
    egui::Id::unique("nc_canvas_context_menu")
}

/// The page's right-click menu, for the target the canvas found.
pub fn context_menu(ctx: &egui::Context, app: &mut NoteApp) {
    let open_now = std::mem::take(&mut app.canvas.review.open_menu);
    if app.canvas.review.menu.is_none() {
        return;
    }
    let popup = egui::Popup::new(menu_id(), ctx.clone(), egui::PopupAnchor::PointerFixed, egui::LayerId::background())
        .kind(egui::PopupKind::Menu)
        .layout(Layout::top_down_justified(Align::Min))
        .style(egui::containers::menu::menu_style)
        .width(MENU_W)
        .open_memory(open_now.then_some(egui::SetOpenCommand::Bool(true)));
    let shown = popup.show(|ui| {
        ui.set_width(MENU_W);
        if let Some(target) = app.canvas.review.menu.clone() {
            menu(ui, app, &target);
        }
    });
    if shown.is_none() && !open_now {
        app.canvas.review.menu = None;
    }
}

/// Whether the page's context menu is open.
pub fn menu_open(ctx: &egui::Context) -> bool {
    egui::Popup::is_id_open(ctx, menu_id())
}

fn item(ui: &mut Ui, icon: Option<Icon>, text: &str, enabled: bool) -> bool {
    widgets::menu_item(ui, icon, text, enabled)
}

/// Feeds `ev` to the canvas's keyboard handling next frame (Cut, Copy, Paste).
fn inject(app: &mut NoteApp, ev: egui::Event) {
    app.canvas.review.inject.push(ev);
}

fn paste(app: &mut NoteApp) {
    if let Some(id) = extra("paste") {
        app.act(id, json!({}));
        return;
    }
    // The host reads the clipboard (on the web it arrives as a paste a moment later).
    if let Some(text) = app.services.clipboard_text.as_ref().and_then(|f| f()) {
        inject(app, egui::Event::Paste(text));
    }
}

fn menu(ui: &mut Ui, app: &mut NoteApp, target: &MenuTarget) {
    match target {
        MenuTarget::Text { outline, at, issue, link } => text_menu(ui, app, *outline, *at, issue.as_ref(), link.as_deref()),
        MenuTarget::Container(id) => {
            if let Some(cmd) = extra("copy")
                && item(ui, Some(Icon::Copy), tl!("Copy"), true)
            {
                app.act(cmd, json!({"ids": [id]}));
            }
            if item(ui, Some(Icon::Link), tl!("Copy Link to Paragraph"), true) {
                copy_paragraph_link(ui.ctx(), app, *id, 0);
            }
            ui.separator();
            if item(ui, Some(Icon::Delete), tl!("Delete Note Container"), true) {
                app.act(ids::OUTLINE_DELETE, json!({"id": id}));
                app.canvas.selection.elements.clear();
            }
        }
        MenuTarget::Picture(id) => {
            if item(ui, Some(Icon::Export), tl!("Save As…"), true) {
                save_attachment(app, *id);
            }
            if let Some(cmd) = extra("copy")
                && item(ui, Some(Icon::Copy), tl!("Copy"), true)
            {
                app.act(cmd, json!({"ids": [id]}));
            }
            picture_extras(ui, app, *id);
            order_extras(ui, app, json!({"id": id, "ids": [id]}));
            ui.separator();
            if item(ui, Some(Icon::Delete), tl!("Delete"), true) {
                app.act(ids::OUTLINE_DELETE, json!({"id": id}));
                app.canvas.selection.elements.clear();
            }
        }
        MenuTarget::Attachment { element, name } => {
            widgets::menu_heading(ui, name);
            if item(ui, Some(Icon::Open), tl!("Open"), true) {
                app.ui.review.open_file = Some((*element, name.clone()));
            }
            if item(ui, Some(Icon::Export), tl!("Save As…"), true) {
                save_attachment(app, *element);
            }
            ui.separator();
            if item(ui, Some(Icon::Delete), tl!("Delete"), true) {
                app.act(ids::OUTLINE_DELETE, json!({"id": element}));
                app.canvas.selection.elements.clear();
            }
        }
        MenuTarget::Printout(id) => {
            if item(ui, Some(Icon::Export), tl!("Save Original As…"), true) {
                save_attachment(app, *id);
            }
            order_extras(ui, app, json!({"id": id, "ids": [id]}));
            ui.separator();
            if item(ui, Some(Icon::Delete), tl!("Delete"), true) {
                app.act(ids::OUTLINE_DELETE, json!({"id": id}));
                app.canvas.selection.elements.clear();
            }
        }
        MenuTarget::Kept { element, label } => {
            widgets::menu_heading(ui, label);
            widgets::menu_heading(ui, tl!("NoteCraft keeps it as it is when saving."));
            ui.separator();
            if item(ui, Some(Icon::Delete), tl!("Delete"), true) {
                app.act(ids::OUTLINE_DELETE, json!({"id": element}));
                app.canvas.selection.elements.clear();
            }
        }
        MenuTarget::Ink => ink_menu(ui, app),
        MenuTarget::Page(at) => page_menu(ui, app, *at),
    }
}

fn picture_extras(ui: &mut Ui, app: &mut NoteApp, id: Guid) {
    let p = json!({"id": id, "ids": [id], "element": id, "page": app.session.current.page});
    let rotations: Vec<(&str, &str, Value)> = [
        ("rotateRight", tl!("Rotate Right 90°"), json!({"degrees": 90, "direction": "right"})),
        ("rotateLeft", tl!("Rotate Left 90°"), json!({"degrees": -90, "direction": "left"})),
        ("flipHorizontal", tl!("Flip Horizontal"), json!({"axis": "horizontal"})),
        ("flipVertical", tl!("Flip Vertical"), json!({"axis": "vertical"})),
    ]
    .into_iter()
    .filter(|(n, _, _)| extra(n).is_some())
    .collect();
    if !rotations.is_empty() {
        ui.menu_button(tl!("Rotate"), |ui| {
            for (name, label, extra_params) in &rotations {
                if let Some(cmd) = extra(name)
                    && item(ui, Some(Icon::Rotate), label, true)
                {
                    app.act(cmd, merge(p.clone(), extra_params.clone()));
                }
            }
        });
    }
    if let Some(cmd) = extra("crop")
        && item(ui, None, tl!("Crop"), true)
    {
        app.act(cmd, p.clone());
    }
    if extra("altText").is_some() && item(ui, None, tl!("Alt Text…"), true) {
        let current = app
            .session
            .current_page()
            .and_then(|pg| pg.element(id))
            .and_then(|e| match &e.kind {
                notecraft_doc::ElementKind::Image(i) => Some(i.alt.clone()),
                _ => None,
            })
            .unwrap_or_default();
        app.ui.review.alt_text = Some((id, current));
    }
}

fn order_extras(ui: &mut Ui, app: &mut NoteApp, p: Value) {
    let front = extra("bringToFront");
    let back = extra("sendToBack");
    if front.is_none() && back.is_none() {
        return;
    }
    ui.menu_button(tl!("Order"), |ui| {
        if let Some(cmd) = front
            && item(ui, None, tl!("Bring to Front"), true)
        {
            app.act(cmd, p.clone());
        }
        if let Some(cmd) = back
            && item(ui, None, tl!("Send to Back"), true)
        {
            app.act(cmd, p.clone());
        }
    });
}

fn merge(mut a: Value, b: Value) -> Value {
    if let (Value::Object(m), Value::Object(e)) = (&mut a, b) {
        m.extend(e);
    }
    a
}

fn copy_paragraph_link(ctx: &egui::Context, app: &mut NoteApp, outline: Guid, para: usize) {
    let page = app.session.current.page;
    if let Ok(out) = app.run(ids::LINK_TO_PARAGRAPH, json!({"outline": outline, "para": para, "page": page}))
        && let Some(url) = out.value.get("url").and_then(Value::as_str)
    {
        ctx.copy_text(url.to_owned());
        app.set_status(tl!("Link copied."), false);
    }
}

fn format(app: &mut NoteApp, id: &str, extra: Value) {
    if let Some(Err(e)) = crate::canvas::format_command(&mut app.canvas, &mut app.session, id, extra) {
        app.set_status(e.to_string(), true);
    }
}

fn text_menu(ui: &mut Ui, app: &mut NoteApp, outline: Guid, at: TextPos, issue: Option<&Value>, link: Option<&str>) {
    let page = app.session.current.page;
    if let Some(issue) = issue {
        let word = issue.get("word").and_then(Value::as_str).unwrap_or_default().to_owned();
        widgets::menu_heading(ui, tl!("Possible spelling mistake"));
        let list: Vec<String> = issue
            .get("suggestions")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|s| s.as_str().map(str::to_owned)).collect())
            .unwrap_or_default();
        if list.is_empty() {
            ui.label(egui::RichText::new(tl!("(no suggestions)")).italics().font(regular(12.5)));
        }
        for s in list {
            if suggestion_item(ui, &s) {
                let mut p = json!({"outline": outline, "para": issue.get("para"), "start": issue.get("start"), "end": issue.get("end"), "word": word, "text": s, "page": page});
                if let Value::Object(m) = &mut p {
                    m.retain(|_, v| !v.is_null());
                }
                app.session.break_undo_merge();
                if let Ok(out) = app.run(ids::PROOF_APPLY, p)
                    && let Some(c) = out.value.get("caret").and_then(TextPos::from_json)
                    && let Some(caret) = app.canvas.caret.as_mut()
                {
                    caret.anchor = c;
                    caret.head = c;
                }
                ui.close();
            }
        }
        if item(ui, None, tl!("Ignore"), true) {
            app.act(ids::PROOF_IGNORE, json!({"outline": outline, "para": issue.get("para"), "start": issue.get("start"), "page": page}));
        }
        if item(ui, None, tl!("Ignore All"), true) {
            app.act(ids::PROOF_IGNORE_ALL, json!({"word": word}));
        }
        if item(ui, Some(Icon::Spelling), tl!("Add to Dictionary"), true) {
            app.act(ids::PROOF_ADD_TO_DICTIONARY, json!({"word": word}));
        }
        ui.separator();
    }
    let selected = app.canvas.caret.as_ref().is_some_and(|c| !c.is_collapsed());
    if item(ui, Some(Icon::Cut), tl!("Cut"), selected) {
        match extra("cut") {
            Some(cmd) => app.act(cmd, crate::widgets::selection_params(app, json!({}))),
            None => inject(app, egui::Event::Cut),
        }
    }
    if item(ui, Some(Icon::Copy), tl!("Copy"), selected) {
        match extra("copy") {
            Some(cmd) => app.act(cmd, crate::widgets::selection_params(app, json!({}))),
            None => inject(app, egui::Event::Copy),
        }
    }
    if item(ui, Some(Icon::Paste), tl!("Paste"), true) {
        paste(app);
    }
    if app.services.clipboard_text.is_some()
        && item(ui, Some(Icon::Paste), tl!("Paste as Plain Text"), true)
        && let Some(text) = app.services.clipboard_text.as_ref().and_then(|f| f())
    {
        inject(app, egui::Event::Paste(text));
    }
    ui.separator();
    if item(ui, Some(Icon::Link), if link.is_some() { tl!("Edit Link…") } else { tl!("Link…") }, true) {
        open_link_dialog(app);
    }
    if let Some(url) = link {
        let url = url.to_owned();
        if item(ui, None, tl!("Open Link"), true) {
            crate::canvas::links::follow(ui.ctx(), &mut app.canvas, &mut app.session, &app.services, &url);
        }
        if item(ui, Some(Icon::Copy), tl!("Copy Link"), true) {
            ui.ctx().copy_text(url);
        }
        if item(ui, None, tl!("Remove Link"), true) {
            app.act(ids::LINK_REMOVE, json!({"outline": outline, "at": at.to_json(), "page": page}));
        }
    }
    if item(ui, None, tl!("Copy Link to Paragraph"), true) {
        copy_paragraph_link(ui.ctx(), app, outline, at.para);
    }
    ui.separator();
    ui.menu_button(tl!("Tag"), |ui| crate::ribbon::paragraph::tag_menu(ui, app));
    ui.menu_button(tl!("Paragraph"), |ui| {
        for a in notecraft_doc::Align::ALL {
            let (icon, label) = crate::ribbon::paragraph::align_item(a);
            if item(ui, Some(icon), label, true) {
                format(app, ids::FORMAT_ALIGN, json!({"align": a.name()}));
            }
        }
        ui.separator();
        if item(ui, Some(Icon::Bullets), tl!("Bullets"), true) {
            format(app, ids::FORMAT_BULLETS, json!({}));
        }
        if item(ui, Some(Icon::Numbering), tl!("Numbering"), true) {
            format(app, ids::FORMAT_NUMBERING, json!({}));
        }
        if item(ui, Some(Icon::Indent), tl!("Increase Indent"), true) {
            format(app, ids::FORMAT_INDENT, json!({}));
        }
        if item(ui, Some(Icon::Outdent), tl!("Decrease Indent"), true) {
            format(app, ids::FORMAT_OUTDENT, json!({}));
        }
    });
    ui.menu_button(tl!("Set Language"), |ui| language_items(ui, app));
    ui.separator();
    if item(ui, Some(Icon::Delete), tl!("Delete"), selected)
        && let Some(c) = app.canvas.caret.clone()
    {
        let (a, b) = c.range();
        if let Ok(out) = app.run(ids::TEXT_DELETE, json!({"outline": c.outline, "start": a.to_json(), "end": b.to_json(), "page": page}))
            && let Some(p) = out.value.get("caret").and_then(TextPos::from_json)
            && let Some(caret) = app.canvas.caret.as_mut()
        {
            caret.anchor = p;
            caret.head = p;
        }
    }
}

fn ink_menu(ui: &mut Ui, app: &mut NoteApp) {
    ui.menu_button(tl!("Recolor"), |ui| {
        if let Some(c) = crate::widgets::color::grid(ui, &crate::widgets::color::INK, 4, None) {
            app.act(ids::DRAW_RECOLOR, json!({"color": crate::widgets::color::hex(c)}));
            ui.close();
        }
    });
    ui.menu_button(tl!("Thickness"), |ui| {
        for (label, w) in [(tl!("Extra Fine"), 0.5), (tl!("Fine"), 1.0), (tl!("Medium"), 2.0), (tl!("Thick"), 3.5), (tl!("Extra Thick"), 6.0)] {
            if item(ui, Some(Icon::ColorThickness), label, true) {
                app.act(ids::DRAW_RECOLOR, json!({"width": w}));
            }
        }
    });
    let ink = app.session.ink_selection.clone();
    if let Some(cmd) = extra("rotateRight")
        && item(ui, Some(Icon::Rotate), tl!("Rotate Right 90°"), true)
    {
        app.act(cmd, json!({"ids": ink, "degrees": 90, "direction": "right"}));
    }
    order_extras(ui, app, json!({"ids": ink}));
    ui.separator();
    if item(ui, Some(Icon::Delete), tl!("Delete"), !app.session.ink_selection.is_empty()) {
        app.act(ids::DRAW_DELETE_SELECTION, json!({}));
        app.canvas.selection.strokes.clear();
    }
}

fn page_menu(ui: &mut Ui, app: &mut NoteApp, at: notecraft_doc::Point) {
    if item(ui, Some(Icon::Paste), tl!("Paste"), true) {
        // A waiting container there takes the pasted text (it is created by typing).
        app.canvas.pending = Some(crate::canvas::PendingContainer { at, merge_key: format!("new:{}", Guid::new()) });
        paste(app);
    }
    let picture_cmd = extra("pastePicture");
    if (picture_cmd.is_some() || app.services.clipboard_image.is_some()) && item(ui, Some(Icon::Picture), tl!("Paste Picture"), true) {
        match picture_cmd {
            Some(cmd) => app.act(cmd, json!({"x": at.x, "y": at.y})),
            None => paste_picture(app),
        }
    }
    ui.separator();
    if item(ui, Some(Icon::Type), tl!("New Note Container Here"), true)
        && let Ok(out) = app.run(ids::OUTLINE_NEW, json!({"x": at.x, "y": at.y}))
        && let Some(id) = out.value.get("id").and_then(Value::as_str).and_then(Guid::parse)
    {
        app.canvas.caret = Some(TextCaret::at(id, TextPos::default()));
        app.canvas.editing = Some(id);
    }
    ui.separator();
    ui.menu_button(tl!("Page Color"), |ui| crate::ribbon::view::page_color_menu(ui, app));
    ui.menu_button(tl!("Rule Lines"), |ui| crate::ribbon::view::rule_lines_menu(ui, app));
    if item(ui, Some(Icon::Link), tl!("Copy Link to Page"), app.session.current.page.is_some())
        && let Ok(out) = app.run(ids::LINK_TO_PAGE, json!({}))
        && let Some(url) = out.value.get("url").and_then(Value::as_str)
    {
        ui.ctx().copy_text(url.to_owned());
        app.set_status(tl!("Link copied."), false);
    }
}

fn paste_picture(app: &mut NoteApp) {
    let Some(img) = app.services.clipboard_image.as_ref().and_then(|f| f()) else {
        app.set_status(tl!("There's no picture on the clipboard."), false);
        return;
    };
    match crate::app::files::png_from_rgba(img.width, img.height, &img.rgba) {
        Some(png) => crate::app::files::insert_bytes(app, crate::services::OpenPurpose::Picture, tl!("Pasted picture.png"), &png),
        None => app.set_status(tl!("The picture on the clipboard couldn't be read."), true),
    }
}

// --- languages ----------------------------------------------------------------------------------

/// Set Language: the proofing language for the selection (or the paragraph at the caret, or the
/// whole page), then Don't Check, then the default.
pub fn language_items(ui: &mut Ui, app: &mut NoteApp) {
    let target = app.canvas.caret.as_ref().map(|c| {
        let (a, b) = c.range();
        json!({"outline": c.outline, "start": a.to_json(), "end": b.to_json()})
    });
    widgets::menu_heading(ui, if target.is_some() { tl!("Mark the selected text as") } else { tl!("Mark the whole page as") });
    let current = current_language(app);
    for (tag, _) in proof::LANGUAGES.iter().filter(|(t, _)| *t != "zxx") {
        let name = proof::language_name(tag);
        let shown = if proof::has_dictionary(tag) { name } else { crate::i18n::fmt_plain(tl!("{language} (no dictionary)"), &[("language", &name)]) };
        if widgets::check_item(ui, current.as_deref().is_some_and(|c| c.eq_ignore_ascii_case(tag)), &shown) {
            set_language(app, target.clone(), json!(tag));
        }
    }
    ui.separator();
    if widgets::check_item(ui, current.as_deref() == Some("zxx"), tl!("Do Not Check Spelling")) {
        set_language(app, target.clone(), json!("zxx"));
    }
    if item(ui, None, tl!("Use the Default Language"), true) {
        set_language(app, target, Value::Null);
    }
    ui.separator();
    widgets::menu_heading(ui, &crate::i18n::fmt_plain(tl!("Default: {language}"), &[("language", &proof::language_name(&default_language(app)))]));
}

fn set_language(app: &mut NoteApp, target: Option<Value>, lang: Value) {
    let mut p = target.unwrap_or_else(|| json!({}));
    if let Value::Object(m) = &mut p {
        m.insert("lang".into(), lang);
        m.insert("page".into(), json!(app.session.current.page));
    }
    app.act(ids::PROOF_SET_LANGUAGE, p);
}

/// The proofing language at the caret (the paragraph's own), `None` for the default or mixed.
fn current_language(app: &NoteApp) -> Option<String> {
    let c = app.canvas.caret.as_ref()?;
    let o = app.session.current_page()?.outline(c.outline)?;
    o.paragraphs.get(c.head.para)?.lang.clone()
}

/// The accessible name of Options › Proofing's language picker.
pub const PROOFING_LANGUAGE_LABEL: &str = "Proofing language";

/// File › Options › Proofing.
pub fn options_section(ui: &mut Ui, app: &mut NoteApp) {
    crate::backstage::subheading(ui, tl!("Proofing"));
    ui.checkbox(&mut app.ui.prefs.check_spelling, tl!("Check spelling as you type"));
    let t = app.tokens.clone();
    let system = system_proofing_language();
    let auto = app.ui.prefs.proofing_language.is_empty();
    let shown = if auto {
        crate::i18n::fmt_plain(tl!("The system's language ({language})"), &[("language", &proof::language_name(&system))])
    } else {
        proof::language_name(&app.ui.prefs.proofing_language)
    };
    let mut pick: Option<String> = None;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Proofing language:")).font(regular(12.5)).color(t.text_dim));
        let r = craft_ui::ComboBox::from_id_salt("nc_proofing_language").width(260.0).selected_text(shown).show_ui(ui, |ui| {
            if ui
                .selectable_label(
                    auto,
                    crate::i18n::fmt_plain(tl!("The system's language ({language})"), &[("language", &proof::language_name(&system))]),
                )
                .clicked()
            {
                pick = Some(String::new());
            }
            for (tag, _) in proof::LANGUAGES.iter().filter(|(t, _)| *t != "zxx") {
                if ui.selectable_label(app.ui.prefs.proofing_language.eq_ignore_ascii_case(tag), proof::language_name(tag)).clicked() {
                    pick = Some((*tag).to_owned());
                }
            }
        });
        r.response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, crate::i18n::t(PROOFING_LANGUAGE_LABEL)));
    });
    if let Some(p) = pick {
        app.ui.prefs.proofing_language = p;
    }
    crate::backstage::dim(
        ui,
        tl!("English is always checked. Other languages are checked when a Hunspell dictionary for them is installed on this computer."),
    );
    let words = app.ui.prefs.user_words.clone();
    if !words.is_empty() {
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(crate::i18n::fmt_plain(tl!("Words added to the dictionary: {n}"), &[("n", &words.len().to_string())]))
                .font(regular(12.5))
                .color(t.text_dim),
        );
        egui::ScrollArea::vertical().max_height(120.0).id_salt("nc_user_words").show(ui, |ui| {
            for w in words.iter().take(500) {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(w).font(regular(12.5)).color(t.text));
                    if ui.small_button(tl!("Remove")).clicked() {
                        app.act(ids::PROOF_REMOVE_FROM_DICTIONARY, json!({"word": w}));
                    }
                });
            }
        });
    }
}
