//! Review and links in the whole window: misspellings get wavy lines and the right-click menu
//! fixes them, the Spelling pane (F7) steps through a page, Ctrl+K puts a link on text and
//! Ctrl+click follows it, `[[Page]]` links jump, and the page's other right-click menus work.

use std::sync::{Arc, Mutex};

use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use notecraft_doc::{ElementKind, Guid};
use notecraft_engine::{TextPos, ids};
use serde_json::{Value, json};

use super::*;

fn harness(app: NoteApp) -> Harness<'static, NoteApp> {
    Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut NoteApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    )
}

/// The window on a new page with one note container holding `text`.
fn app_with(text: &str) -> (NoteApp, Guid) {
    let mut s = Session::new_default();
    s.run(ids::PAGE_NEW, &json!({"title": "Review test"})).unwrap();
    let o = s.run(ids::OUTLINE_NEW, &json!({"x": 72, "y": 140, "text": text})).unwrap().value["id"].as_str().and_then(Guid::parse).unwrap();
    (NoteApp::new(s, Services::default()), o)
}

/// The screen point of character `ch` of paragraph `para` (just right of the caret there).
fn spot(h: &Harness<'_, NoteApp>, outline: Guid, para: usize, ch: usize) -> Pos2 {
    let st = h.state();
    let r = crate::canvas::caret_screen_rect(&h.ctx, &st.canvas, &st.session, &st.tokens, outline, TextPos::new(para, ch)).unwrap();
    egui::pos2(r.left() + 2.0, r.center().y)
}

fn click_at(h: &mut Harness<'_, NoteApp>, p: Pos2, button: PointerButton, mods: Modifiers) {
    h.event(Event::PointerMoved(p));
    h.step();
    if mods != Modifiers::NONE {
        h.event(Event::ModifiersChanged(mods));
    }
    h.event(Event::PointerButton { pos: p, button, pressed: true, modifiers: mods });
    h.step();
    h.event(Event::PointerButton { pos: p, button, pressed: false, modifiers: mods });
    if mods != Modifiers::NONE {
        h.event(Event::ModifiersChanged(Modifiers::NONE));
    }
    h.run_steps(3);
}

/// Steps until the spelling marks are in (the check runs on a background thread).
fn wait_for_marks(h: &mut Harness<'_, NoteApp>) {
    for _ in 0..400 {
        h.step();
        if !h.state().canvas.review.marks.is_empty() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("no spelling marks appeared");
}

fn text_of(app: &NoteApp, o: Guid) -> String {
    app.session.current_page().unwrap().outline(o).unwrap().text()
}

#[test]
fn a_misspelled_word_is_underlined_and_the_menu_fixes_it() {
    let (app, o) = app_with("I like ovr cats");
    let mut h = harness(app);
    h.run_steps(3);
    wait_for_marks(&mut h);
    assert_eq!(h.state().canvas.review.marks, vec![(o, 0, 7, 10)]);
    let p = spot(&h, o, 0, 8);
    click_at(&mut h, p, PointerButton::Secondary, Modifiers::NONE);
    let _ = h.get_by_label("Possible spelling mistake");
    let _ = h.get_by_label("Add to Dictionary");
    h.get_by_label("over").click();
    h.run_steps(4);
    assert_eq!(text_of(h.state(), o), "I like over cats");
    // The new word is right: no marks.
    for _ in 0..40 {
        h.step();
    }
    assert!(h.state().canvas.review.marks.is_empty(), "{:?}", h.state().canvas.review.marks);
    // Check Spelling as You Type off: no marks at all.
    h.state_mut().session.run(ids::TEXT_INSERT, &json!({"outline": o, "text": " zqxwvt"})).unwrap();
    h.state_mut().ui.prefs.check_spelling = false;
    h.run_steps(30);
    assert!(h.state().canvas.review.marks.is_empty());
}

#[test]
fn the_spelling_pane_steps_through_the_page() {
    let (app, o) = app_with("the brwn fox and the brwn dog jmps");
    let mut h = harness(app);
    h.run_steps(3);
    h.key_press(egui::Key::F7);
    h.run_steps(4);
    assert!(h.state().ui.review.spelling.is_some(), "F7 opens the pane");
    let _ = h.get_by_label("Not in Dictionary");
    // The word is selected on the page.
    let c = h.state().canvas.caret.clone().unwrap();
    assert_eq!((c.outline, c.range()), (o, (TextPos::new(0, 4), TextPos::new(0, 8))));
    let first = h.state().ui.review.spelling.as_ref().and_then(|p| p.issue.clone()).unwrap();
    assert_eq!(first["word"], "brwn");
    assert!(first["suggestions"].as_array().unwrap().iter().any(|s| s == "brown"), "{first}");
    // Pick "brown" and Change All.
    let pick = first["suggestions"].as_array().unwrap().iter().position(|s| s == "brown").unwrap();
    h.state_mut().ui.review.spelling.as_mut().unwrap().choice = pick;
    h.get_by_label("Change All").click();
    h.run_steps(4);
    assert_eq!(text_of(h.state(), o), "the brown fox and the brown dog jmps");
    let next = h.state().ui.review.spelling.as_ref().and_then(|p| p.issue.clone()).unwrap();
    assert_eq!(next["word"], "jmps");
    h.get_by_label("Ignore").click();
    h.run_steps(4);
    let _ = h.get_by_label("The spelling check is complete.");
    assert_eq!(text_of(h.state(), o), "the brown fox and the brown dog jmps", "Ignore changes nothing");
    h.key_press(egui::Key::F7);
    h.run_steps(2);
    assert!(h.state().ui.review.spelling.is_none(), "F7 closes it again");
}

fn opened() -> (Services, Arc<Mutex<Vec<String>>>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = log.clone();
    let services = Services {
        open_external: Some(Box::new(move |u: &str| {
            l.lock().unwrap().push(u.to_owned());
            Ok(())
        })),
        ..Services::default()
    };
    (services, log)
}

#[test]
fn ctrl_k_links_text_and_ctrl_click_opens_it() {
    let (mut app, o) = app_with("read the docs today");
    let (services, log) = opened();
    app.services = services;
    let mut h = harness(app);
    h.run_steps(3);
    // Select "the docs".
    let (a, b) = (spot(&h, o, 0, 5), spot(&h, o, 0, 13));
    click_at(&mut h, a, PointerButton::Primary, Modifiers::NONE);
    h.state_mut().canvas.caret.as_mut().unwrap().anchor = TextPos::new(0, 5);
    h.state_mut().canvas.caret.as_mut().unwrap().head = TextPos::new(0, 13);
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::K);
    h.run_steps(3);
    let d = h.state().ui.review.link.clone().expect("Ctrl+K opens the link dialog");
    assert_eq!(d.text, "the docs");
    h.get_by_label(crate::review::ADDRESS_LABEL).type_text("example.com/docs");
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert!(h.state().ui.review.link.is_none(), "{:?}", h.state().ui.review.link);
    let runs = h.state().session.current_page().unwrap().outline(o).unwrap().paragraphs[0].runs.clone();
    let linked: Vec<(&str, Option<&str>)> = runs.iter().map(|r| (r.text.as_str(), r.link.as_deref())).collect();
    assert!(linked.contains(&("the docs", Some("https://example.com/docs"))), "{linked:?}");
    assert_eq!(text_of(h.state(), o), "read the docs today", "typing in the dialog stays out of the note");
    // A plain click edits; Ctrl+click follows.
    let mid = spot(&h, o, 0, 8);
    let _ = b;
    click_at(&mut h, mid, PointerButton::Primary, Modifiers::NONE);
    assert!(log.lock().unwrap().is_empty());
    click_at(&mut h, mid, PointerButton::Primary, Modifiers::COMMAND);
    assert_eq!(*log.lock().unwrap(), vec!["https://example.com/docs".to_owned()]);
    // A bad address is refused in the dialog.
    h.state_mut().canvas.caret.as_mut().unwrap().anchor = TextPos::new(0, 0);
    h.state_mut().canvas.caret.as_mut().unwrap().head = TextPos::new(0, 4);
    h.key_press_modifiers(Modifiers::COMMAND, egui::Key::K);
    h.run_steps(3);
    h.get_by_label(crate::review::ADDRESS_LABEL).type_text("javascript:alert(1)");
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert!(h.state().ui.review.link.as_ref().is_some_and(|d| d.error.is_some()), "the dialog says why");
}

#[test]
fn typing_a_wiki_link_makes_a_page_and_it_jumps_there() {
    let (app, o) = app_with("");
    let mut h = harness(app);
    h.run_steps(3);
    let host = h.state().session.current.page.unwrap();
    let p = spot(&h, o, 0, 0);
    click_at(&mut h, p, PointerButton::Primary, Modifiers::NONE);
    for t in ["Plan: ", "[[Trip ideas]", "]"] {
        h.event(Event::Text(t.into()));
        h.run_steps(2);
    }
    assert_eq!(text_of(h.state(), o), "Plan: Trip ideas");
    let target =
        h.state().session.current_section().unwrap().pages.iter().find(|p| p.title == "Trip ideas").map(|p| p.id).expect("the page was made");
    assert_eq!(h.state().session.current.page, Some(host), "typing stays on the page");
    let at = spot(&h, o, 0, 9);
    click_at(&mut h, at, PointerButton::Primary, Modifiers::COMMAND);
    h.run_steps(3);
    assert_eq!(h.state().session.current.page, Some(target), "Ctrl+click jumps to the page");
}

#[test]
fn copy_link_to_paragraph_goes_back_to_it() {
    let (app, o) = app_with("first\nsecond\nthird");
    let mut h = harness(app);
    h.run_steps(3);
    let p = spot(&h, o, 2, 2);
    click_at(&mut h, p, PointerButton::Secondary, Modifiers::NONE);
    h.get_by_label("Copy Link to Paragraph").click();
    h.step();
    let url = h.output().platform_output.commands.iter().find_map(|c| match c {
        egui::OutputCommand::CopyText(t) => Some(t.clone()),
        _ => None,
    });
    let url = url.expect("the link is copied");
    assert!(url.starts_with("notecraft:"));
    let here = h.state().session.current.page;
    h.state_mut().act(ids::PAGE_NEW, json!({"title": "Away"}));
    h.run_steps(2);
    let st = h.state_mut();
    crate::canvas::links::follow(&egui::Context::default(), &mut st.canvas, &mut st.session, &st.services, &url);
    h.run_steps(3);
    assert_eq!(h.state().session.current.page, here);
    let c = h.state().canvas.caret.clone().unwrap();
    assert_eq!((c.outline, c.head), (o, TextPos::new(2, 0)), "the caret is at the paragraph");
}

#[test]
fn page_menus_change_the_page_and_its_items() {
    let (mut app, _) = app_with("note");
    let downloads = Arc::new(Mutex::new(Vec::<(String, usize)>::new()));
    let opened_files = Arc::new(Mutex::new(Vec::<String>::new()));
    let (d, f) = (downloads.clone(), opened_files.clone());
    app.services.download = Some(Box::new(move |n: &str, b: &[u8]| {
        d.lock().unwrap().push((n.to_owned(), b.len()));
        Ok(())
    }));
    app.services.open_file = Some(Box::new(move |n: &str, _b: &[u8]| {
        f.lock().unwrap().push(n.to_owned());
        Ok(())
    }));
    let b64 = |b: &[u8]| base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b);
    let file = app.run(ids::INSERT_FILE, json!({"bytesBase64": b64(b"hello"), "name": "notes.txt", "x": 400, "y": 300})).unwrap().value["id"]
        .as_str()
        .and_then(Guid::parse)
        .unwrap();
    app.run(ids::DRAW_STROKE, json!({"points": [[500, 380], [560, 400], [620, 380]], "width": 3})).unwrap();
    let mut h = harness(app);
    h.run_steps(3);
    let to_screen = |h: &Harness<'_, NoteApp>, x: f32, y: f32| {
        let o = h.state().canvas.last_origin.unwrap();
        let k = h.state().canvas.scale();
        egui::pos2(o.x + x * k, o.y + y * k)
    };
    // Empty paper: Rule Lines ▸ College Ruled.
    let paper = to_screen(&h, 300.0, 420.0);
    click_at(&mut h, paper, PointerButton::Secondary, Modifiers::NONE);
    h.get_by_label("Rule Lines").hover();
    h.run_steps(3);
    h.get_by_label("College Ruled").click();
    h.run_steps(3);
    assert_eq!(h.state().session.current_page().unwrap().background.rule, notecraft_doc::RuleLines::College);
    // The attached file: Save As downloads it, Open asks and opens it.
    let at_file = to_screen(&h, 420.0, 318.0);
    click_at(&mut h, at_file, PointerButton::Secondary, Modifiers::NONE);
    h.get_by_label("Save As…").click();
    h.run_steps(3);
    assert_eq!(*downloads.lock().unwrap(), vec![("notes.txt".to_owned(), 5)]);
    click_at(&mut h, at_file, PointerButton::Secondary, Modifiers::NONE);
    h.get_by_label("Open").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.review.open_file.as_ref().map(|(id, _)| *id), Some(file), "it asks first");
    h.get_by_label("Open").click();
    h.run_steps(3);
    assert_eq!(*opened_files.lock().unwrap(), vec!["notes.txt".to_owned()]);
    // Ink: Thickness, then Delete.
    let ink = to_screen(&h, 560.0, 400.0);
    click_at(&mut h, ink, PointerButton::Secondary, Modifiers::NONE);
    let _ = h.get_by_label("Recolor");
    h.get_by_label("Delete").click();
    h.run_steps(3);
    assert!(h.state().session.current_page().unwrap().ink.is_empty());
}

#[test]
fn kept_content_shows_a_placeholder_that_can_be_deleted() {
    let (mut app, _) = app_with("x");
    let page = app.session.current_page_mut().unwrap();
    page.elements.push(notecraft_doc::PageElement::new(ElementKind::Opaque { jcid: 0x0006_0099, data: std::sync::Arc::new(b"junk".to_vec()) }));
    let id = page.elements.last().unwrap().id;
    let mut h = harness(app);
    h.run_steps(3);
    let r = {
        let st = h.state();
        crate::canvas::images::item_rect(&st.canvas, st.session.current_page().unwrap(), st.session.current_page().unwrap().element(id).unwrap())
            .unwrap()
    };
    let o = h.state().canvas.last_origin.unwrap();
    let k = h.state().canvas.scale();
    let p = egui::pos2(o.x + (r.x + 20.0) * k, o.y + (r.y + r.h / 2.0) * k);
    click_at(&mut h, p, PointerButton::Secondary, Modifiers::NONE);
    let _ = h.get_by_label("Content NoteCraft can't show yet");
    h.get_by_label("Delete").click();
    h.run_steps(3);
    assert!(h.state().session.current_page().unwrap().element(id).is_none());
    let _: Option<Value> = None;
}

#[test]
fn the_review_tab_counts_words_and_sets_the_language() {
    let (app, o) = app_with("one two three");
    let mut h = harness(app);
    h.run_steps(3);
    h.get_by_label("Review").click();
    h.run_steps(3);
    h.get_by_label("Word Count").click();
    h.run_steps(3);
    assert_eq!(h.state().ui.review.word_count.as_ref().map(|v| v["words"].as_u64()), Some(Some(5)), "two title words and three");
    h.get_by_label("Close").click();
    h.run_steps(2);
    assert!(h.state().ui.review.word_count.is_none());
    // With the caret in the text, Language ▸ English (United Kingdom) marks its paragraph.
    let p = spot(&h, o, 0, 1);
    click_at(&mut h, p, PointerButton::Primary, Modifiers::NONE);
    h.get_by_label("Language").click();
    h.run_steps(3);
    h.get_by_label("English (United Kingdom)").click();
    h.run_steps(3);
    assert_eq!(h.state().session.current_page().unwrap().outline(o).unwrap().paragraphs[0].lang.as_deref(), Some("en-GB"));
}
