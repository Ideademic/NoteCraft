//! Named window states for screenshots (the `ui_shot` example) and tests: each sets up the UI
//! the way a person would have it at that moment.

use serde_json::json;

use super::NoteApp;
use crate::backstage::BackstagePage;
use crate::ribbon::RibbonTab;

/// Every named state.
pub const SHOTS: [&str; 13] = [
    "main",
    "tab-insert",
    "tab-draw",
    "tab-view",
    "backstage-info",
    "backstage-new",
    "backstage-open",
    "backstage-export",
    "backstage-print",
    "backstage-options",
    "backstage-about",
    "panes-collapsed",
    "busy-notebook",
];

/// The ribbon's layouts and display options (`ribbon-display-menu` also needs the options arrow
/// clicked; the `ui_shot` example does that).
pub const RIBBON_SHOTS: [&str; 8] = [
    "simplified-home",
    "simplified-insert",
    "simplified-draw",
    "simplified-view",
    "ribbon-display-menu",
    "ribbon-full-screen",
    "ribbon-full-screen-revealed",
    "ribbon-tabs-only",
];

/// The History tab and its views.
pub const HISTORY_SHOTS: [&str; 5] = ["tab-history", "simplified-history", "history-recycle-bin", "history-versions", "history-recent-edits"];

/// Real fonts and paragraph formatting: a page in several fonts, alignments, list styles and
/// tags, and the font box open over it (classic and simplified; the `ui_shot` example opens it).
pub const FONT_SHOTS: [&str; 3] = ["formats-page", "font-box", "simplified-font-box"];

/// The editing features: find and replace on a page, the Tags Summary and Templates panes, a
/// picture being cropped, Insert Space being dragged, the Draw tab's Arrange and Rotate, and the
/// Format Painter armed (classic and simplified).
pub const EDITING_SHOTS: [&str; 8] = [
    "editing-find",
    "editing-tags",
    "editing-templates",
    "editing-crop",
    "editing-insert-space",
    "editing-draw",
    "editing-painter",
    "editing-simplified-draw",
];

/// A page of notes with tags, a few repeated words and a picture, for the editing shots.
fn editing_page(app: &mut NoteApp) -> (Option<notecraft_doc::Guid>, Option<notecraft_doc::Guid>) {
    use notecraft_engine::ids;
    let id = |v: Option<serde_json::Value>| v.and_then(|v| v.get("id").and_then(|x| x.as_str()).and_then(notecraft_doc::Guid::parse));
    let _ = app.run(ids::PAGE_NEW, json!({ "title": "Garden plans", "blank": true }));
    let outline = id(app.run(ids::OUTLINE_NEW, json!({ "x": 36, "y": 108 })).ok().map(|o| o.value));
    let paras = json!([
        {"runs": [{"text": "Spring garden"}], "style": "heading1"},
        {"runs": [{"text": "Plant tomatoes along the south fence; tomatoes like the sun."}]},
        {"runs": [{"text": "Order tomato seeds", "bold": true}], "tag": {"type": "toDo", "checked": false}},
        {"runs": [{"text": "Fix the water butt"}], "tag": {"type": "toDo", "checked": true}},
        {"runs": [{"text": "Try a herb spiral by the door"}], "tag": {"type": "idea"}},
        {"runs": [{"text": "Which tomatoes grow best in pots?"}], "tag": {"type": "question"}},
        {"runs": [{"text": "Compost: "}, {"text": "turn it every two weeks", "color": "#c0392b", "italic": true}]}
    ]);
    if let Some(o) = outline {
        let _ = app.run(ids::OUTLINE_RESIZE, json!({ "id": o, "width": 400 }));
        let _ = app.run(ids::TEXT_SET_PARAGRAPHS, json!({ "id": o, "paragraphs": paras }));
    }
    let mut rgba = Vec::with_capacity(240 * 160 * 4);
    for y in 0..160u32 {
        for x in 0..240u32 {
            let sky = y < 90;
            let (r, g, b) = if sky { (120 + (y / 2) as u8, 180 + (y / 3) as u8, 235) } else { (60 + (x / 8) as u8, 140 + ((x + y) % 40) as u8, 60) };
            let sun = (x as i32 - 180).pow(2) + (y as i32 - 40).pow(2) < 400;
            let (r, g, b) = if sun { (250, 210, 70) } else { (r, g, b) };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    let picture = crate::app::files::png_from_rgba(240, 160, &rgba).and_then(|png| {
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png);
        id(app.run(ids::EDIT_PASTE_PICTURE, json!({ "bytesBase64": b64, "x": 480, "y": 120 })).ok().map(|o| o.value))
    });
    if let Some(p) = picture {
        let _ = app.run(ids::OUTLINE_RESIZE, json!({ "id": p, "width": 240 }));
    }
    (outline, picture)
}

/// The states of [`EDITING_SHOTS`]; `false` for other names.
fn prepare_editing(app: &mut NoteApp, shot: &str) -> bool {
    use crate::canvas::{TextCaret, Tool};
    if !EDITING_SHOTS.contains(&shot) {
        return false;
    }
    let (outline, picture) = editing_page(app);
    // The canvas is on this page already (a new page would reset its selection and crop).
    app.canvas.page = app.session.current.page;
    match shot {
        "editing-find" => {
            let mut f = crate::dialogs::find::FindBar::new("tomato");
            f.replace = "pepper".into();
            f.show_replace = true;
            f.current = 1;
            f.focus = false;
            app.ui.find = Some(f);
        }
        "editing-tags" => {
            app.ui.ribbon_tab = RibbonTab::Home;
            crate::panes::tags::toggle(app);
            if let Some(mut p) = app.ui.tags_pane.clone() {
                p.scope = "page".into();
                crate::panes::tags::refresh(app, &mut p);
                app.ui.tags_pane = Some(p);
            }
        }
        "editing-templates" => {
            app.ui.ribbon_tab = RibbonTab::Insert;
            crate::panes::templates::toggle(app);
        }
        "editing-crop" => {
            app.ui.ribbon_tab = RibbonTab::Draw;
            if let Some(p) = picture {
                let _ = app.run(notecraft_engine::ids::PICTURE_ROTATE, json!({ "element": p, "degrees": 90 }));
                let _ = crate::canvas::crop::begin(&mut app.canvas, &mut app.session, p);
                if let Some(c) = app.canvas.crop.as_mut() {
                    c.rect = notecraft_doc::Rect::new(c.full.x + c.full.w * 0.1, c.full.y + c.full.h * 0.15, c.full.w * 0.75, c.full.h * 0.6);
                }
            }
        }
        "editing-insert-space" => {
            app.ui.ribbon_tab = RibbonTab::Insert;
            app.canvas.set_tool(Tool::InsertSpace);
            app.canvas.gesture = crate::canvas::pointer::Gesture::Space { from: 200.0, to: 260.0 };
        }
        "editing-draw" | "editing-simplified-draw" => {
            app.ui.ribbon_tab = RibbonTab::Draw;
            app.canvas.set_tool(Tool::Select);
            if let Some(p) = picture {
                app.canvas.selection.elements = vec![p];
            }
            if shot == "editing-simplified-draw" {
                app.ui.ribbon_layout = crate::ribbon::display::RibbonLayout::Simplified;
            }
        }
        "editing-painter" => {
            app.ui.ribbon_tab = RibbonTab::Home;
            if let Some(o) = outline {
                app.canvas.caret = Some(TextCaret::at(o, notecraft_engine::TextPos::new(2, 3)));
                app.canvas.editing = Some(o);
                let _ = crate::canvas::format_painter::pick(&mut app.canvas, &mut app.session, true);
            }
        }
        _ => {}
    }
    true
}

/// The states of [`FONT_SHOTS`]; `false` for other names.
fn prepare_fonts(app: &mut NoteApp, shot: &str) -> bool {
    use notecraft_doc::{Align, ListKind, ListStyle, ParaStyle, Paragraph, Run, Tag};
    use notecraft_engine::ids;
    if !FONT_SHOTS.contains(&shot) {
        return false;
    }
    let run = |text: &str, font: Option<&str>, bold: bool, italic: bool| Run {
        text: text.to_owned(),
        font: font.map(str::to_owned),
        bold,
        italic,
        ..Run::default()
    };
    let p = |runs: Vec<Run>| Paragraph { runs, ..Paragraph::default() };
    let mut paras = vec![
        Paragraph { style: ParaStyle::Heading1, ..p(vec![run("Fonts, alignment and tags", None, false, false)]) },
        p(vec![
            run("Calibri ", None, false, false),
            run("bold ", None, true, false),
            run("italic ", None, false, true),
            run("bold italic", None, true, true),
        ]),
        p(vec![run("Cambria for headings and long reading", Some("Cambria"), false, false)]),
        p(vec![
            run("Arial, ", Some("Arial"), false, false),
            run("Times New Roman, ", Some("Times New Roman"), false, false),
            run("Georgia", Some("Georgia"), true, false),
        ]),
        p(vec![run("let total = notes.len(); // Courier New", Some("Courier New"), false, false)]),
        Paragraph { align: Align::Center, ..p(vec![run("A centred line", None, false, false)]) },
        Paragraph { align: Align::Right, ..p(vec![run("Right-aligned, like a signature", None, false, true)]) },
        Paragraph {
            align: Align::Justify,
            ..p(vec![run(
                "Justified text fills the container from edge to edge on every line but the last, the way a printed column does.",
                None,
                false,
                false,
            )])
        },
    ];
    for (st, text) in [(ListStyle::Arrow, "Arrow bullets"), (ListStyle::Square, "Square bullets"), (ListStyle::Dash, "Dash bullets")] {
        paras.push(Paragraph { list: Some(ListKind::Bullet), list_style: Some(st), ..p(vec![run(text, None, false, false)]) });
    }
    for text in ["Upper-case roman", "and on"] {
        paras.push(Paragraph { list: Some(ListKind::Numbered), list_style: Some(ListStyle::UpperRoman), ..p(vec![run(text, None, false, false)]) });
    }
    paras.push(Paragraph {
        list: Some(ListKind::Numbered),
        list_style: Some(ListStyle::LowerAlpha),
        indent: 1,
        ..p(vec![run("a nested letter", None, false, false)])
    });
    for (t, text) in [
        (Tag::ToDo { checked: true }, "Book the room"),
        (Tag::Important, "Deadline is Friday"),
        (Tag::Question, "Who takes notes?"),
        (Tag::Idea, "Try a shared notebook"),
        (Tag::Critical, "Back up first"),
        (Tag::Contact, "Ana Silva"),
        (Tag::Phone, "+351 21 000 0000"),
        (Tag::Address, "Rua Augusta 1, Lisboa"),
        (Tag::RememberForLater, "Remember for later"),
        (Tag::Definition, "Definition: a tag is a mark on a note"),
    ] {
        paras.push(Paragraph { tag: Some(t), ..p(vec![run(text, None, false, false)]) });
    }
    let page = app.run(ids::PAGE_NEW, json!({ "title": "Formats" })).ok().and_then(|o| o.value.get("id").and_then(|v| v.as_str()).map(str::to_owned));
    let outline =
        app.run(ids::OUTLINE_NEW, json!({ "x": 36, "y": 108 })).ok().and_then(|o| o.value.get("id").and_then(|v| v.as_str()).map(str::to_owned));
    if let Some(id) = outline.as_deref() {
        let _ = app.run(ids::OUTLINE_RESIZE, json!({ "id": id, "width": 440 }));
        let _ = app.run(ids::TEXT_SET_PARAGRAPHS, json!({ "id": id, "paragraphs": serde_json::to_value(&paras).unwrap_or_default() }));
        if let (Some(g), Some(pg)) = (notecraft_doc::Guid::parse(id), page.as_deref().and_then(notecraft_doc::Guid::parse)) {
            app.canvas.page = Some(pg);
            app.canvas.editing = Some(g);
            // The caret in "Cambria for headings": the font box shows Cambria.
            app.canvas.caret = Some(crate::canvas::TextCaret::at(g, notecraft_engine::TextPos::new(2, 4)));
        }
    }
    for f in ["Cambria", "Courier New", "Georgia"] {
        app.ui.prefs.remember_font(f);
    }
    if shot == "simplified-font-box" {
        app.ui.ribbon_layout = crate::ribbon::display::RibbonLayout::Simplified;
    }
    true
}

/// The History states of [`HISTORY_SHOTS`]; `false` for other names.
fn prepare_history(app: &mut NoteApp, shot: &str) -> bool {
    use notecraft_engine::ids;
    let run = |app: &mut NoteApp, id: &str, p: serde_json::Value| app.run(id, p).ok().map(|o| o.value);
    let id_of = |v: Option<serde_json::Value>| v.and_then(|v| v.get("id").and_then(serde_json::Value::as_str).map(str::to_owned));
    match shot {
        "tab-history" => app.ui.ribbon_tab = RibbonTab::History,
        "simplified-history" => {
            app.ui.ribbon_layout = crate::ribbon::display::RibbonLayout::Simplified;
            app.ui.ribbon_tab = RibbonTab::History;
        }
        "history-recycle-bin" => {
            app.ui.ribbon_tab = RibbonTab::History;
            for (title, text) in
                [("Trip ideas", "Lisbon in May\nPorto by train"), ("Old agenda", "Budget review\nHiring plan"), ("Scratch", "phone: call back")]
            {
                let page = id_of(run(app, ids::PAGE_NEW, json!({ "title": title })));
                run(app, ids::OUTLINE_NEW, json!({ "x": 36, "y": 108, "text": text }));
                run(app, ids::PAGE_DELETE, json!({ "id": page }));
            }
            let sec = id_of(run(app, ids::SECTION_NEW, json!({ "name": "Archive 2025" })));
            run(app, ids::PAGE_RENAME, json!({ "title": "Year in review" }));
            run(app, ids::SECTION_DELETE, json!({ "id": sec }));
            crate::panes::history::open_bin(app);
            if let Some(v) = app.ui.bin_view.as_mut() {
                // The oldest deletion: a page with text.
                let first =
                    app.session.current_notebook().and_then(|n| n.recycle_bin.pages.iter().find(|d| d.page.title == "Trip ideas")).map(|d| d.page.id);
                v.page = first.or(v.page);
            }
        }
        "history-versions" => {
            app.ui.ribbon_tab = RibbonTab::History;
            let page = app.session.current.page;
            // Versions kept over a morning of edits (made here directly: the session keeps at most
            // one every five minutes, by the clock).
            if let Some(p) = page.and_then(|id| app.session.page_mut(id)) {
                let base = p.modified;
                let mut older = p.clone();
                for (i, title) in ["Welcome (first notes)", "Welcome to NoteCraft (draft)", "Welcome to NoteCraft"].into_iter().enumerate() {
                    older.title = title.to_owned();
                    if i == 0 {
                        older.ink.clear();
                    }
                    older.modified = base - chrono::Duration::minutes(90 - 30 * i as i64);
                    p.versions.push(notecraft_doc::PageVersion::of(&older));
                }
            }
            app.session.revision += 1;
            run(app, ids::HISTORY_SHOW_VERSIONS, json!({ "value": true }));
            let v = page.and_then(|id| app.session.current_page().filter(|p| p.id == id)).and_then(|p| p.versions.first()).map(|v| v.id);
            run(app, ids::HISTORY_VIEW_VERSION, json!({ "page": page, "version": v }));
        }
        "history-recent-edits" => {
            app.ui.ribbon_tab = RibbonTab::History;
            for title in ["Shopping list", "Book notes"] {
                run(app, ids::PAGE_NEW, json!({ "title": title }));
            }
            // A page changed elsewhere since it was last seen: unread, so bold.
            let unread = id_of(run(app, ids::PAGE_NEW, json!({ "title": "Edited on my phone" })));
            if let Some(g) = unread.as_deref().and_then(notecraft_doc::Guid::parse) {
                app.session.read_state.mark_unread(g);
            }
            if let Some(first) = app.session.current_section().and_then(|s| s.pages.first()).map(|p| p.id) {
                run(app, ids::NAV_PAGE, json!({ "id": first }));
            }
            app.ui.recent_edits = Some("7d".into());
        }
        _ => return false,
    }
    true
}

/// Options' language picker (open; the `ui_shot` example clicks it) and the Check for Updates
/// window answered "up to date" without the network.
pub const PLATFORM_SHOTS: [&str; 2] = ["options-language-menu", "update-up-to-date"];

/// The states of [`PLATFORM_SHOTS`]; `false` for other names.
fn prepare_platform(app: &mut NoteApp, shot: &str) -> bool {
    match shot {
        "options-language-menu" => crate::backstage::open(app, BackstagePage::Options),
        "update-up-to-date" => {
            crate::backstage::open(app, BackstagePage::About);
            #[cfg(not(target_arch = "wasm32"))]
            {
                app.updates = crate::updates::Updates::default().with_source(std::sync::Arc::new(|| Ok(None)));
            }
            crate::updates::check(app);
        }
        _ => return false,
    }
    true
}

/// Review and links: the Review tab (classic and simplified) over a page with misspellings,
/// links, an attachment and kept content; the Spelling pane; the right-click menu on a misspelled
/// word, on the paper and on the attachment (the `ui_shot` example right-clicks); Insert Link.
pub const REVIEW_SHOTS: [&str; 7] =
    ["tab-review", "simplified-review", "review-spelling-pane", "review-word-menu", "review-page-menu", "review-file-menu", "review-link-dialog"];

/// Where the `ui_shot` example right-clicks in a [`REVIEW_SHOTS`] state: (note container,
/// paragraph, character) for text, else a page point.
pub fn review_click(app: &NoteApp, shot: &str) -> Option<ReviewClick> {
    let page = app.session.current_page()?;
    let first = page.elements.iter().find(|e| matches!(e.kind, notecraft_doc::ElementKind::Outline(_)))?.id;
    match shot {
        "review-word-menu" => Some(ReviewClick::Text(first, 0, 33)),
        "review-page-menu" => Some(ReviewClick::Page(520.0, 470.0)),
        "review-file-menu" => Some(ReviewClick::Page(330.0, 342.0)),
        _ => None,
    }
}

/// See [`review_click`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReviewClick {
    Text(notecraft_doc::Guid, usize, usize),
    Page(f32, f32),
}

/// The states of [`REVIEW_SHOTS`]; `false` for other names.
fn prepare_review(app: &mut NoteApp, shot: &str) -> bool {
    use notecraft_engine::ids;
    if !REVIEW_SHOTS.contains(&shot) {
        return false;
    }
    let _ = app.run(ids::PAGE_NEW, json!({"title": "Trip planning"}));
    let page = app.session.current.page;
    let text = "Things to pack for the weekend: sunscren, a map and the camerra.\nAsk Sam about the [[Packing list]] and the train times.\nTickets are on the booking site; directions are in the attachment below.\nTomorrow we leave early.";
    let o = app
        .run(ids::OUTLINE_NEW, json!({"x": 36, "y": 108, "text": text}))
        .ok()
        .and_then(|o| o.value["id"].as_str().and_then(notecraft_doc::Guid::parse));
    if let Some(o) = o {
        // `[[Packing list]]` typed: the wiki link.
        let _ = app.run(ids::LINK_WIKI, json!({"outline": o, "at": {"para": 1, "char": 34}}));
        let _ = app.run(
            ids::LINK_SET,
            json!({"outline": o, "start": {"para": 2, "char": 19}, "end": {"para": 2, "char": 31}, "url": "https://example.com/tickets"}),
        );
        let _ = app.run(ids::PROOF_SET_LANGUAGE, json!({"outline": o, "start": {"para": 3, "char": 0}, "lang": "en-GB"}));
    }
    let _ = app.run(ids::INSERT_FILE, json!({"bytesBase64": "SGVsbG8=", "name": "Directions.pdf", "x": 300, "y": 330}));
    if let Some(p) = app.session.current_page_mut() {
        // Content NoteCraft keeps but can't show (an equation, say), with no geometry: its
        // placeholder sits below the rest.
        p.elements
            .push(notecraft_doc::PageElement::new(notecraft_doc::ElementKind::Opaque { jcid: 0x0006_0099, data: std::sync::Arc::new(Vec::new()) }));
    }
    let _ = page;
    match shot {
        "tab-review" | "review-word-menu" | "review-page-menu" | "review-file-menu" => app.ui.ribbon_tab = RibbonTab::Review,
        "simplified-review" => {
            app.ui.ribbon_tab = RibbonTab::Review;
            app.ui.ribbon_layout = crate::ribbon::display::RibbonLayout::Simplified;
        }
        "review-spelling-pane" => {
            app.ui.ribbon_tab = RibbonTab::Review;
            crate::panes::spelling::open(app);
        }
        "review-link-dialog" => {
            app.ui.ribbon_tab = RibbonTab::Review;
            if let Some(o) = o {
                app.canvas.caret = Some(crate::canvas::TextCaret {
                    anchor: notecraft_engine::TextPos::new(3, 0),
                    head: notecraft_engine::TextPos::new(3, 8),
                    ..crate::canvas::TextCaret::at(o, notecraft_engine::TextPos::new(3, 8))
                });
                app.canvas.editing = Some(o);
                crate::review::open_link_dialog(app);
            }
        }
        _ => {}
    }
    true
}

/// The ribbon states of [`RIBBON_SHOTS`]; `false` for other names.
fn prepare_ribbon(app: &mut NoteApp, shot: &str) -> bool {
    use crate::ribbon::display::{RibbonDisplay, RibbonLayout};
    let simplified = |app: &mut NoteApp, tab| {
        app.ui.ribbon_layout = RibbonLayout::Simplified;
        app.ui.ribbon_tab = tab;
    };
    match shot {
        "simplified-home" => simplified(app, RibbonTab::Home),
        "simplified-insert" => simplified(app, RibbonTab::Insert),
        "simplified-draw" => simplified(app, RibbonTab::Draw),
        "simplified-view" => simplified(app, RibbonTab::View),
        "ribbon-display-menu" => simplified(app, RibbonTab::Home),
        "ribbon-full-screen" => app.ui.ribbon_display = RibbonDisplay::FullScreen,
        "ribbon-full-screen-revealed" => {
            simplified(app, RibbonTab::Home);
            app.ui.ribbon_display = RibbonDisplay::FullScreen;
            app.ui.ribbon_peek = true;
        }
        "ribbon-tabs-only" => app.ui.ribbon_display = RibbonDisplay::TabsOnly,
        _ => return false,
    }
    true
}

/// Puts `app` in the state named `shot` (unknown names leave it as it is).
pub fn prepare(app: &mut NoteApp, shot: &str) {
    if prepare_ribbon(app, shot)
        || prepare_platform(app, shot)
        || prepare_history(app, shot)
        || prepare_fonts(app, shot)
        || prepare_editing(app, shot)
        || prepare_review(app, shot)
    {
        return;
    }
    match shot {
        "tab-insert" => app.ui.ribbon_tab = RibbonTab::Insert,
        "tab-draw" => app.ui.ribbon_tab = RibbonTab::Draw,
        "tab-view" => app.ui.ribbon_tab = RibbonTab::View,
        "backstage-info" => crate::backstage::open(app, BackstagePage::Info),
        "backstage-new" => crate::backstage::open(app, BackstagePage::New),
        "backstage-open" => {
            for (name, path, color) in [
                ("Recipes", "/Users/me/Documents/Recipes.onepkg", "#f28b8b"),
                ("Work Notes", "/Users/me/Documents/NoteCraft Notebooks/Work Notes", "#9cd87e"),
            ] {
                app.session.recent.add(path.into(), name);
                app.ui.prefs.remember(name, path, Some(color.into()));
            }
            crate::backstage::open(app, BackstagePage::Open);
        }
        "backstage-export" => crate::backstage::open(app, BackstagePage::Export),
        "backstage-print" => crate::backstage::open(app, BackstagePage::Print),
        "backstage-options" => crate::backstage::open(app, BackstagePage::Options),
        "backstage-about" => crate::backstage::open(app, BackstagePage::About),
        "panes-collapsed" => {
            app.ui.notebook_pane_open = false;
            app.ui.page_list_open = false;
        }
        "busy-notebook" => busy(app),
        _ => {}
    }
}

/// A notebook with a few sections, a section group, pages and subpages.
fn busy(app: &mut NoteApp) {
    let run = |app: &mut NoteApp, id: &str, p: serde_json::Value| {
        let _ = app.run(id, p);
    };
    use notecraft_engine::ids;
    let first = app.session.current.section;
    for name in ["Meetings", "Research", "Travel"] {
        run(app, ids::SECTION_NEW, json!({ "name": name }));
    }
    run(app, ids::SECTION_GROUP_NEW, json!({ "name": "Archive" }));
    run(app, ids::NOTEBOOK_NEW, json!({ "name": "Recipes" }));
    if let Some(nb) = app.session.notebooks.first().map(|n| n.id) {
        run(app, ids::NAV_NOTEBOOK, json!({ "id": nb }));
    }
    if let Some(s) = first {
        run(app, ids::NAV_SECTION, json!({ "id": s }));
    }
    for title in ["Project kickoff", "Agenda", "Action items", "Reading list"] {
        run(app, ids::PAGE_NEW, json!({ "title": title }));
    }
    run(app, ids::PAGE_INDENT, json!({}));
    if let Some(p) = app.session.current_section().and_then(|s| s.pages.first()).map(|p| p.id) {
        run(app, ids::NAV_PAGE, json!({ "id": p }));
    }
    app.ui.search = String::new();
}
