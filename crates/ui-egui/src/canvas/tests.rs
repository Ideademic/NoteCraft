//! Canvas tests driven through egui_kittest: typing, Enter, Ctrl+B, the pens, the eraser, the
//! lasso, To Do boxes, container drags and pictures. `canvas_screenshots` renders sample pages
//! (light and dark) with wgpu into `$NOTECRAFT_CANVAS_SHOTS` when that is set.

use egui::{Event, Key, Modifiers, PointerButton, Pos2, pos2, vec2};
use egui_kittest::Harness;
use notecraft_doc::{ElementKind, InkTool, Tag};
use notecraft_engine::{Session, ids};
use serde_json::{Value, json};

use super::*;
use crate::services::Services;
use crate::theme::Tokens;

struct T {
    canvas: CanvasState,
    session: Session,
    tokens: Tokens,
    services: Services,
}

fn blank() -> T {
    let mut session = Session::new_default();
    session.run(ids::PAGE_NEW, &json!({"title": "Test page"})).unwrap();
    T { canvas: CanvasState::default(), session, tokens: Tokens::light(), services: Services::default() }
}

fn harness(t: T) -> Harness<'static, T> {
    Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, t: &mut T| {
            t.tokens.apply(ui.ctx());
            show(ui, &mut t.canvas, &mut t.session, &t.tokens, &t.services);
        },
        t,
    )
}

/// Screen position of page point (x, y) (no scroll).
fn at(h: &Harness<'_, T>, x: f32, y: f32) -> Pos2 {
    let rect = h.state().canvas.last_rect.unwrap();
    let sheet = page::sheet_rect(rect);
    let k = h.state().canvas.scale();
    pos2(sheet.min.x + x * k - h.state().canvas.scroll.x, sheet.min.y + y * k - h.state().canvas.scroll.y)
}

fn press(h: &mut Harness<'_, T>, p: Pos2, down: bool, mods: Modifiers) {
    h.event(Event::PointerMoved(p));
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: down, modifiers: mods });
    h.step();
}

fn click(h: &mut Harness<'_, T>, p: Pos2) {
    h.event(Event::PointerMoved(p));
    h.step();
    press(h, p, true, Modifiers::NONE);
    press(h, p, false, Modifiers::NONE);
    h.run_steps(2);
}

fn drag(h: &mut Harness<'_, T>, pts: &[Pos2]) {
    let Some(first) = pts.first().copied() else { return };
    h.event(Event::PointerMoved(first));
    h.step();
    press(h, first, true, Modifiers::NONE);
    for p in pts.iter().skip(1) {
        h.event(Event::PointerMoved(*p));
        h.step();
    }
    let last = pts.last().copied().unwrap_or(first);
    press(h, last, false, Modifiers::NONE);
    h.run_steps(2);
}

fn type_text(h: &mut Harness<'_, T>, s: &str) {
    h.event(Event::Text(s.into()));
    h.run_steps(2);
}

fn outlines(h: &Harness<'_, T>) -> Vec<notecraft_doc::Outline> {
    h.state()
        .session
        .current_page()
        .unwrap()
        .elements
        .iter()
        .filter_map(|e| match &e.kind {
            ElementKind::Outline(o) => Some(o.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn typing_on_empty_paper_creates_a_container() {
    let mut h = harness(blank());
    h.run_steps(2);
    let p = at(&h, 120.0, 220.0);
    click(&mut h, p);
    assert!(h.state().canvas.pending.is_some(), "the caret waits on the paper");
    assert!(outlines(&h).is_empty(), "no container until something is typed");
    type_text(&mut h, "Hello");
    type_text(&mut h, " world");
    let os = outlines(&h);
    assert_eq!(os.len(), 1);
    assert_eq!(os[0].text(), "Hello world");
    assert!((os[0].pos.x - 120.0).abs() < 1.0 && (os[0].pos.y - 220.0).abs() < 1.0, "{:?}", os[0].pos);
    assert_eq!(h.state().canvas.editing, Some(h.state().canvas.caret.as_ref().unwrap().outline));
    // The whole burst (creation and typing) is one undo step.
    h.state_mut().session.run(ids::EDIT_UNDO, &json!({})).unwrap();
    h.run_steps(2);
    assert!(outlines(&h).is_empty());
}

#[test]
fn clicking_away_from_an_empty_container_removes_it() {
    let mut h = harness(blank());
    h.run_steps(2);
    let p = at(&h, 100.0, 200.0);
    click(&mut h, p);
    type_text(&mut h, "x");
    h.key_press(Key::Backspace);
    h.run_steps(2);
    assert_eq!(outlines(&h).len(), 1);
    let q = at(&h, 400.0, 500.0);
    click(&mut h, q);
    assert!(outlines(&h).is_empty(), "an empty container goes when the caret leaves");
}

#[test]
fn enter_splits_and_backspace_joins() {
    let mut h = harness(blank());
    h.run_steps(2);
    let p = at(&h, 100.0, 200.0);
    click(&mut h, p);
    type_text(&mut h, "ab");
    h.key_press(Key::ArrowLeft);
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(2);
    let os = outlines(&h);
    assert_eq!(os[0].paragraphs.len(), 2);
    assert_eq!(os[0].text(), "a\nb");
    assert_eq!(h.state().canvas.caret.as_ref().unwrap().head, TextPos::new(1, 0));
    h.key_press(Key::Backspace);
    h.run_steps(2);
    assert_eq!(outlines(&h)[0].text(), "ab");
    h.key_press(Key::Tab);
    h.run_steps(2);
    assert_eq!(outlines(&h)[0].paragraphs[0].indent, 1, "Tab indents");
}

#[test]
fn ctrl_b_bolds_the_selection_and_sets_pending_style() {
    let mut h = harness(blank());
    h.run_steps(2);
    let p = at(&h, 100.0, 200.0);
    click(&mut h, p);
    type_text(&mut h, "hello world");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(2);
    let o = &outlines(&h)[0];
    assert!(o.paragraphs[0].runs.iter().all(|r| r.bold), "{:?}", o.paragraphs[0].runs);
    assert_eq!(current_format(&h.state().canvas, &h.state().session).unwrap().bold, Some(true));
    // A collapsed caret at the end: Ctrl+I is pending, then applies to what is typed.
    h.key_press(Key::End);
    h.run_steps(1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::I);
    h.run_steps(1);
    assert_eq!(h.state().canvas.pending_style.italic, Some(true));
    type_text(&mut h, "!");
    let o = &outlines(&h)[0];
    let last = o.paragraphs[0].runs.last().unwrap();
    assert!(last.italic && last.bold && last.text == "!", "{last:?}");
    let sel = selection_params(&h.state().canvas).unwrap();
    assert_eq!(sel["start"], json!({"para": 0, "char": 12}));
}

fn add_outline(t: &mut T, x: f32, y: f32, paragraphs: Value) -> notecraft_doc::Guid {
    let out = t.session.run(ids::OUTLINE_NEW, &json!({"x": x, "y": y})).unwrap();
    let id = out.value["id"].as_str().and_then(notecraft_doc::Guid::parse).unwrap();
    t.session.run(ids::TEXT_SET_PARAGRAPHS, &json!({"id": id, "paragraphs": paragraphs})).unwrap();
    id
}

#[test]
fn clicking_a_to_do_box_ticks_it() {
    let mut t = blank();
    let id = add_outline(&mut t, 80.0, 160.0, json!([{"runs": [{"text": "Buy milk"}], "tag": {"type": "toDo", "checked": false}}]));
    let mut h = harness(t);
    h.run_steps(2);
    let v = view::View::new(&h.state().canvas, page::sheet_rect(h.state().canvas.last_rect.unwrap()));
    let lay = layout::page_layout(&h.ctx, &h.state().canvas, &h.state().session, &h.state().tokens, &v);
    let b = layout::todo_box(lay.outline(id).unwrap(), &v, 0).unwrap();
    click(&mut h, b.center());
    let o = h.state().session.current_page().unwrap().outline(id).unwrap().clone();
    assert_eq!(o.paragraphs[0].tag, Some(Tag::ToDo { checked: true }));
    click(&mut h, b.center());
    let o = h.state().session.current_page().unwrap().outline(id).unwrap().clone();
    assert_eq!(o.paragraphs[0].tag, Some(Tag::ToDo { checked: false }));
}

#[test]
fn the_grip_moves_a_container() {
    let mut t = blank();
    let id = add_outline(&mut t, 80.0, 160.0, json!([{"runs": [{"text": "Move me"}]}]));
    let mut h = harness(t);
    h.run_steps(2);
    // Hover first so the frame shows, then drag the grip bar.
    let inside = at(&h, 90.0, 165.0);
    h.event(Event::PointerMoved(inside));
    h.run_steps(2);
    let grip = at(&h, 80.0 + 144.0, 160.0) - vec2(0.0, text::FRAME_PAD_Y + text::GRIP_H / 2.0);
    let to = grip + vec2(60.0, 40.0);
    drag(&mut h, &[grip, grip + vec2(20.0, 10.0), to]);
    let o = h.state().session.current_page().unwrap().outline(id).unwrap().clone();
    let k = h.state().canvas.scale();
    assert!((o.pos.x - (80.0 + 60.0 / k)).abs() < 0.6 && (o.pos.y - (160.0 + 40.0 / k)).abs() < 0.6, "{:?}", o.pos);
}

#[test]
fn pen_draws_eraser_erases() {
    let mut t = blank();
    t.canvas.set_tool(Tool::Pen);
    let mut h = harness(t);
    h.run_steps(2);
    let pts: Vec<Pos2> = (0..12).map(|i| at(&h, 100.0 + i as f32 * 10.0, 300.0 + (i as f32).sin() * 5.0)).collect();
    drag(&mut h, &pts);
    let ink = h.state().session.current_page().unwrap().ink.clone();
    assert_eq!(ink.len(), 1);
    assert_eq!(ink[0].tool, InkTool::Pen);
    assert!(ink[0].points.len() >= 10);
    assert!(outlines(&h).is_empty(), "drawing doesn't make containers");
    h.state_mut().canvas.set_tool(Tool::Eraser);
    h.run_steps(1);
    let pts = [at(&h, 150.0, 250.0), at(&h, 150.0, 300.0), at(&h, 150.0, 350.0)];
    drag(&mut h, &pts);
    assert!(h.state().session.current_page().unwrap().ink.is_empty());
}

#[test]
fn lasso_selects_and_delete_deletes() {
    let mut t = blank();
    t.canvas.set_tool(Tool::Highlighter);
    let mut h = harness(t);
    h.run_steps(2);
    let pts = [at(&h, 200.0, 300.0), at(&h, 240.0, 302.0), at(&h, 280.0, 300.0)];
    drag(&mut h, &pts);
    assert_eq!(h.state().session.current_page().unwrap().ink[0].tool, InkTool::Highlighter);
    h.state_mut().canvas.set_tool(Tool::Lasso);
    h.run_steps(1);
    let loop_pts = [at(&h, 180.0, 280.0), at(&h, 300.0, 280.0), at(&h, 300.0, 320.0), at(&h, 180.0, 320.0), at(&h, 181.0, 281.0)];
    drag(&mut h, &loop_pts);
    assert_eq!(h.state().session.ink_selection.len(), 1);
    assert_eq!(h.state().canvas.selection.strokes.len(), 1);
    // Drag the selection to move it.
    let pts = [at(&h, 240.0, 300.0), at(&h, 250.0, 310.0), at(&h, 260.0, 320.0)];
    drag(&mut h, &pts);
    let s = h.state().session.current_page().unwrap().ink[0].clone();
    assert!((s.points[0].x - 220.0).abs() < 1.0, "{:?}", s.points[0]);
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert!(h.state().session.current_page().unwrap().ink.is_empty());
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        let sun = ((fx - 0.7).powi(2) + (fy - 0.3).powi(2)).sqrt() < 0.12;
        let hill = fy > 0.65 + 0.12 * (fx * 6.0).sin();
        if sun {
            image::Rgba([0xFF, 0xC8, 0x3D, 255])
        } else if hill {
            image::Rgba([(0x3A as f32 + 40.0 * fx) as u8, 0x9A, 0x5B, 255])
        } else {
            image::Rgba([(0x7F as f32 + 60.0 * fy) as u8, (0xB8 as f32 + 40.0 * fy) as u8, 0xF0, 255])
        }
    });
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

#[test]
fn pictures_select_resize_and_delete() {
    let mut t = blank();
    t.canvas.set_tool(Tool::Select);
    let b64 = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(png(120, 80))
    };
    let out = t.session.run(ids::INSERT_IMAGE, &json!({"bytesBase64": b64, "name": "pic.png", "x": 100.0, "y": 150.0})).unwrap();
    let id = out.value["id"].as_str().and_then(notecraft_doc::Guid::parse).unwrap();
    let mut h = harness(t);
    h.run_steps(2);
    let p = at(&h, 120.0, 170.0);
    click(&mut h, p);
    assert_eq!(h.state().canvas.selection.elements, vec![id]);
    let size = |h: &Harness<'_, T>| match &h.state().session.current_page().unwrap().element(id).unwrap().kind {
        ElementKind::Image(i) => i.size,
        _ => panic!(),
    };
    let s0 = size(&h);
    let corner = at(&h, 100.0 + s0.w, 150.0 + s0.h);
    drag(&mut h, &[corner, corner + vec2(20.0, 5.0), corner + vec2(40.0, 10.0)]);
    let s1 = size(&h);
    assert!(s1.w > s0.w + 10.0, "{s0:?} → {s1:?}");
    assert!((s1.w / s1.h - s0.w / s0.h).abs() < 0.01, "the shape is kept");
    h.key_press(Key::Delete);
    h.run_steps(2);
    assert!(h.state().session.current_page().unwrap().element(id).is_none());
}

#[test]
fn the_title_renames_the_page() {
    let mut h = harness(blank());
    h.run_steps(2);
    let p = at(&h, 40.0, 44.0);
    click(&mut h, p);
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    type_text(&mut h, "Groceries");
    assert_eq!(h.state().session.current_page().unwrap().title, "Groceries");
    assert!(outlines(&h).is_empty(), "a click on the title doesn't start a note");
}

#[test]
fn ctrl_wheel_zooms() {
    let mut h = harness(blank());
    h.run_steps(2);
    let p = at(&h, 200.0, 200.0);
    h.event(Event::PointerMoved(p));
    h.event(Event::Zoom(1.5));
    h.run_steps(2);
    assert!((h.state().canvas.zoom - 1.5).abs() < 1e-3, "{}", h.state().canvas.zoom);
}

// --- Screenshots -----------------------------------------------------------------------------

/// A page like a real one: title, college rule lines, two containers with headings, bullets,
/// numbering and To Do tags, a highlighter and pen strokes over the text, and a picture.
fn sample(dark: bool) -> T {
    let mut t = blank();
    t.tokens = Tokens::for_dark(dark);
    let s = &mut t.session;
    s.run(ids::PAGE_RENAME, &json!({"title": "Project kickoff"})).unwrap();
    s.run(ids::VIEW_RULE_LINES, &json!({"rule": "college"})).unwrap();
    add_outline(
        &mut t,
        48.0,
        112.0,
        json!([
            {"runs": [{"text": "Agenda"}], "style": "heading1"},
            {"runs": [{"text": "Welcome and introductions"}], "list": "bullet"},
            {"runs": [{"text": "Goals for the "}, {"text": "first quarter", "bold": true}], "list": "bullet"},
            {"runs": [{"text": "Ship the "}, {"text": "beta", "italic": true}, {"text": " by March"}], "list": "bullet", "indent": 1},
            {"runs": [{"text": "Hire two designers"}], "list": "bullet", "indent": 1},
            {"runs": [{"text": "Budget review"}], "list": "bullet"},
            {"runs": [{"text": "Action items"}], "style": "heading2"},
            {"runs": [{"text": "Send the meeting notes"}], "tag": {"type": "toDo", "checked": true}},
            {"runs": [{"text": "Book the venue for "}, {"text": "Friday", "underline": true}], "tag": {"type": "toDo", "checked": false}},
            {"runs": [{"text": "Draft the press release"}], "tag": {"type": "toDo", "checked": false}},
            {"runs": [{"text": "Remember: the demo is "}, {"text": "on the 14th", "highlight": "#ffff00"}], "tag": {"type": "important"}},
        ]),
    );
    add_outline(
        &mut t,
        430.0,
        112.0,
        json!([
            {"runs": [{"text": "Timeline"}], "style": "heading2"},
            {"runs": [{"text": "Research and interviews"}], "list": "numbered"},
            {"runs": [{"text": "Prototype"}], "list": "numbered"},
            {"runs": [{"text": "Paper sketches"}], "list": "numbered", "indent": 1},
            {"runs": [{"text": "Clickable mock-up"}], "list": "numbered", "indent": 1},
            {"runs": [{"text": "Usability testing"}], "list": "numbered"},
            {"runs": [{"text": ""}]},
            {"runs": [{"text": "\u{201c}Make it simple, but significant.\u{201d}"}], "style": "quote"},
            {"runs": [{"text": "let launch = plan.ship();"}], "style": "code"},
            {"runs": [{"text": "Who owns the budget?", "color": "#c42b1c"}], "tag": {"type": "question"}},
        ]),
    );
    let b64 = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(png(320, 200))
    };
    t.session.run(ids::INSERT_IMAGE, &json!({"bytesBase64": b64, "name": "hills.png", "x": 440.0, "y": 360.0})).unwrap();
    // Highlighter over "Budget review", a pen loop around "first quarter", a pen underline
    // under "Usability testing" and a pencil arrow at the To Do list.
    let hl: Vec<[f32; 3]> = (0..16).map(|i| [68.0 + i as f32 * 5.0, 194.5 + (i as f32 * 0.4).sin() * 0.8, 0.5]).collect();
    t.session.run(ids::DRAW_STROKE, &json!({"points": hl, "tool": "highlighter", "color": "#ffff00", "width": 13.0})).unwrap();
    let circle: Vec<[f32; 3]> = (0..64)
        .map(|i| {
            let a = i as f32 / 58.0 * std::f32::consts::TAU + 2.6;
            [168.0 + 38.0 * a.cos(), 152.0 + 10.5 * a.sin(), 0.35 + 0.6 * (a / 2.0).sin().abs()]
        })
        .collect();
    t.session.run(ids::DRAW_STROKE, &json!({"points": circle, "tool": "pen", "color": "#c42b1c", "width": 2.0})).unwrap();
    let under: Vec<[f32; 3]> =
        (0..30).map(|i| [452.0 + i as f32 * 2.8, 202.0 + (i as f32 * 0.35).sin() * 1.0, 0.3 + (i as f32 / 30.0) * 0.7]).collect();
    t.session.run(ids::DRAW_STROKE, &json!({"points": under, "tool": "pen", "color": "#1f5fbf", "width": 2.0})).unwrap();
    let arrow: Vec<[f32; 3]> = (0..24).map(|i| [300.0 - i as f32 * 4.0, 252.0 - (i as f32 * 0.13).sin() * 8.0, 0.5]).collect();
    t.session.run(ids::DRAW_STROKE, &json!({"points": arrow, "tool": "pencil", "width": 1.2})).unwrap();
    let head = [[214.0, 241.0, 0.5], [207.0, 246.0, 0.5], [215.0, 250.0, 0.5]];
    t.session.run(ids::DRAW_STROKE, &json!({"points": head, "tool": "pencil", "width": 1.2})).unwrap();
    t
}

#[test]
fn canvas_screenshots() {
    let Ok(dir) = std::env::var("NOTECRAFT_CANVAS_SHOTS") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    for dark in [false, true] {
        let mut t = sample(dark);
        // Hover the first container and put the caret in the second, as while working.
        t.canvas.zoom = 1.0;
        let mut h = Harness::builder().with_size(vec2(1100.0, 760.0)).with_pixels_per_point(2.0).wgpu().build_ui_state(
            |ui, t: &mut T| {
                t.tokens.apply(ui.ctx());
                show(ui, &mut t.canvas, &mut t.session, &t.tokens, &t.services);
            },
            t,
        );
        h.set_render_cursor(false);
        h.run_steps(3);
        let p = at(&h, 600.0, 240.0);
        click(&mut h, p);
        h.event(Event::PointerMoved(at(&h, 260.0, 150.0)));
        h.run_steps(4);
        let img = h.render().unwrap();
        img.save(dir.join(format!("canvas-{}.png", if dark { "dark" } else { "light" }))).unwrap();
    }
    // The page inside the whole window.
    for dark in [false, true] {
        let t = sample(dark);
        let mut app = crate::NoteApp::new(t.session, Services::default());
        app.ui.prefs.appearance = if dark { crate::Appearance::Dark } else { crate::Appearance::Light };
        let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).with_pixels_per_point(2.0).wgpu().build_ui_state(
            |ui, app: &mut crate::NoteApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
        h.run_steps(6);
        let img = h.render().unwrap();
        img.save(dir.join(format!("canvas-window-{}.png", if dark { "dark" } else { "light" }))).unwrap();
    }
}

#[test]
fn typing_continues_after_a_ribbon_button() {
    use egui_kittest::kittest::Queryable;
    let mut session = Session::new_default();
    session.run(ids::PAGE_NEW, &json!({"title": "Ribbon"})).unwrap();
    let app = crate::NoteApp::new(session, Services::default());
    let mut h = Harness::builder().with_size(vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut crate::NoteApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    let rect = h.state().canvas.last_rect.unwrap();
    let p = rect.min + vec2(200.0, 260.0);
    h.event(Event::PointerMoved(p));
    h.step();
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.step();
    h.event(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    h.event(Event::Text("plain ".into()));
    h.run_steps(2);
    h.get_by_label("Bold").click();
    h.run_steps(2);
    h.event(Event::Text("bold".into()));
    h.run_steps(2);
    let page = h.state().session.current_page().unwrap();
    let o = page.elements.iter().find_map(|e| match &e.kind {
        ElementKind::Outline(o) => Some(o.clone()),
        _ => None,
    });
    let o = o.expect("a container");
    assert_eq!(o.text(), "plain bold", "typing goes on after the ribbon click");
}
