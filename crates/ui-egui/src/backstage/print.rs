//! File › Print (Ctrl+P): the printer, copies, two-sided and colour (craft-ui's printing panel),
//! what to print (the page, its section or the notebook), the paper and orientation, a range of
//! sheets, and a preview of the sheets as they will print.
//!
//! The sheets are `file.exportPdf`'s PDF. On the desktop the host's craft-print adapter sends it
//! to the spooler on a worker thread; the result lands in the status bar. In the browser the
//! PDF goes to the browser's print dialog. Save as PDF writes the same file.

use base64::Engine as _;
use craft_ui::printing::{Feedback, Labels, State};
use egui::{ColorImage, RichText, TextureHandle, TextureOptions, Ui, vec2};
use notecraft_doc::Guid;
use notecraft_engine::ids;
use notecraft_pdf::Paper;
use serde_json::{Value, json};

use crate::app::files::{self, ExportFormat, ExportScope};
use crate::services::Printing;
use crate::theme::regular;
use crate::widgets::flat_button;
use crate::{NoteApp, tl};

/// Sheets the preview shows.
const PREVIEW_SHEETS: usize = 12;
/// A preview picture's longest side, points.
const THUMB: f32 = 168.0;

/// The Print page's settings, its preview and the job being sent.
pub struct PrintPage {
    /// Printer, copies, two-sided, black and white.
    pub printer: State,
    pub scope: ExportScope,
    pub paper: Paper,
    pub landscape: bool,
    /// Print each page's title and date.
    pub header: bool,
    /// Sheets to print (`1-3, 5`); empty for all.
    pub range: String,
    preview: Option<Preview>,
    /// A job this page started is being sent: its outcome goes to the status bar.
    reporting: bool,
}

impl Default for PrintPage {
    fn default() -> Self {
        PrintPage {
            printer: State::default(),
            scope: ExportScope::Page,
            paper: Paper::Letter,
            landscape: false,
            header: true,
            range: String::new(),
            preview: None,
            reporting: false,
        }
    }
}

/// What the preview was made from.
#[derive(Clone, Debug, PartialEq)]
struct PreviewKey {
    revision: u64,
    at: (Option<Guid>, Option<Guid>, Option<Guid>),
    scope: ExportScope,
    paper: Paper,
    landscape: bool,
    header: bool,
}

struct Preview {
    key: PreviewKey,
    sheets: usize,
    pictures: Vec<TextureHandle>,
    error: Option<String>,
}

pub fn scope_name(s: ExportScope) -> &'static str {
    match s {
        ExportScope::Page => "page",
        ExportScope::Section => "section",
        ExportScope::Notebook => "notebook",
    }
}

fn paper_label(p: Paper) -> &'static str {
    match p {
        Paper::Fit => tl!("Fit to page content"),
        Paper::Letter => tl!("Letter (8.5 × 11 in)"),
        Paper::A4 => tl!("A4 (210 × 297 mm)"),
        Paper::Legal => tl!("Legal (8.5 × 14 in)"),
        Paper::A5 => tl!("A5 (148 × 210 mm)"),
    }
}

/// `file.exportPdf`'s params for the page's settings (and `range`, when printing).
pub fn params(app: &NoteApp, with_range: bool) -> Value {
    let p = &app.print;
    let mut v = json!({
        "scope": scope_name(p.scope),
        "paper": p.paper.name(),
        "landscape": p.landscape,
        "header": p.header,
        "utcOffset": app.canvas.utc_offset_minutes,
    });
    if with_range
        && !p.range.trim().is_empty()
        && let Some(m) = v.as_object_mut()
    {
        m.insert("pages".into(), json!(p.range.trim()));
    }
    v
}

/// The PDF as it will print; `None` after an error (shown in the status bar).
fn pdf(app: &mut NoteApp, with_range: bool) -> Option<Vec<u8>> {
    let out = app.run(ids::FILE_EXPORT_PDF, params(app, with_range)).ok()?;
    let b64 = out.value.get("bytesBase64").and_then(Value::as_str)?;
    base64::engine::general_purpose::STANDARD.decode(b64.as_bytes()).ok()
}

fn key(app: &NoteApp) -> PreviewKey {
    let c = app.session.current;
    let p = &app.print;
    PreviewKey {
        revision: app.session.revision,
        at: (c.notebook, c.section, c.page),
        scope: p.scope,
        paper: p.paper,
        landscape: p.landscape,
        header: p.header,
    }
}

/// Makes the preview again when what it shows changed.
fn refresh_preview(ctx: &egui::Context, app: &mut NoteApp) {
    let k = key(app);
    if app.print.preview.as_ref().is_some_and(|p| p.key == k) {
        return;
    }
    let preview = match app.session.run(ids::FILE_EXPORT_PDF, &params(app, false)) {
        Ok(out) => {
            let bytes = out
                .value
                .get("bytesBase64")
                .and_then(Value::as_str)
                .and_then(|s| base64::engine::general_purpose::STANDARD.decode(s.as_bytes()).ok());
            let sheets = out.value.get("sheets").and_then(Value::as_u64).map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX));
            let pictures = bytes
                .map(|b| notecraft_pdf::thumbnails(&b, (THUMB * 2.0) as u32, PREVIEW_SHEETS))
                .unwrap_or_default()
                .into_iter()
                .enumerate()
                .filter_map(|(i, t)| {
                    let size = [usize::try_from(t.width).ok()?, usize::try_from(t.height).ok()?];
                    let img = ColorImage::from_rgba_unmultiplied(size, &t.rgba);
                    Some(ctx.load_texture(format!("nc-print-preview-{i}"), img, TextureOptions::LINEAR))
                })
                .collect();
            Preview { key: k, sheets, pictures, error: None }
        }
        Err(e) => Preview { key: k, sheets: 0, pictures: Vec::new(), error: Some(e.to_string()) },
    };
    app.print.preview = Some(preview);
}

/// Print: makes the PDF and hands it to the printer or the browser.
pub fn print(app: &mut NoteApp) {
    if app.print.printer.busy() {
        app.set_status(tl!("A print job is still being sent."), true);
        return;
    }
    if matches!(app.services.print, Some(Printing::Spooler { .. })) && !app.print.printer.ready() {
        app.set_status(tl!("Choose a printer first."), true);
        return;
    }
    let Some(bytes) = pdf(app, true) else { return };
    let outcome: Result<&str, String> = match &app.services.print {
        Some(Printing::Spooler { submit, .. }) => {
            let job = submit(bytes, app.print.printer.settings.clone());
            match app.print.printer.begin(job) {
                Ok(()) => {
                    app.print.reporting = true;
                    Ok(tl!("Sending to the printer…"))
                }
                Err(_) => Err(tl!("A print job is still being sent.").to_owned()),
            }
        }
        Some(Printing::Browser(open)) => open(&bytes).map(|()| tl!("Opened the print dialog.")),
        None => Err(tl!("Printing isn't available here. Use Save as PDF instead.").to_owned()),
    };
    match outcome {
        Ok(msg) => app.set_status(msg, false),
        Err(e) => app.set_status(e, true),
    }
}

/// Save as PDF with the Print page's paper and range, through File › Export's save path.
pub fn save_pdf(app: &mut NoteApp) {
    let name = files::export_name(app, app.print.scope, ExportFormat::Pdf);
    let params = params(app, true);
    files::export_with(app, name, ids::FILE_EXPORT_PDF, params);
}

/// Every frame: reports a finished print job in the status bar.
pub fn poll(app: &mut NoteApp, ctx: &egui::Context) {
    if !app.print.reporting {
        return;
    }
    app.print.printer.poll(ctx);
    if app.print.printer.busy() {
        return;
    }
    app.print.reporting = false;
    match app.print.printer.message.clone() {
        Some(Feedback::Submitted(receipt)) => {
            let text = if receipt.trim().is_empty() {
                tl!("Sent to the printer.").to_owned()
            } else {
                crate::i18n::fmt_plain(tl!("Sent to the printer: {receipt}"), &[("receipt", receipt.trim())])
            };
            app.set_status(text, false);
        }
        Some(Feedback::Failed(e)) => app.set_status(crate::i18n::fmt_plain(tl!("Printing failed: {error}"), &[("error", &e)]), true),
        Some(Feedback::WorkerStopped) => app.set_status(tl!("Printing stopped unexpectedly."), true),
        _ => {}
    }
}

fn feedback_text(f: &Feedback) -> String {
    match f {
        Feedback::FindingPrinters => tl!("Looking for printers…").to_owned(),
        Feedback::NoPrinters => tl!("No printers were found. Use Save as PDF instead.").to_owned(),
        Feedback::DiscoveryFailed => tl!("The printers couldn't be listed.").to_owned(),
        Feedback::Submitting => tl!("Sending to the printer…").to_owned(),
        Feedback::Submitted(r) => crate::i18n::fmt_plain(tl!("Sent to the printer: {receipt}"), &[("receipt", r.trim())]),
        Feedback::Failed(e) => crate::i18n::fmt_plain(tl!("Printing failed: {error}"), &[("error", e)]),
        Feedback::WorkerStopped => tl!("Printing stopped unexpectedly.").to_owned(),
    }
}

fn label(ui: &mut Ui, text: &str) -> egui::Response {
    let t = crate::Tokens::get(ui.ctx());
    ui.label(RichText::new(text).font(regular(12.5)).color(t.text_dim))
}

/// The page.
pub fn show(ui: &mut Ui, app: &mut NoteApp) {
    let t = app.tokens.clone();
    super::heading(ui, tl!("Print"));
    if app.session.current_page().is_none() {
        super::dim(ui, tl!("Open a page to print it."));
        return;
    }
    refresh_preview(ui.ctx(), app);
    let width = ui.available_width();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(280.0f32.min(width * 0.45));
            ui.horizontal(|ui| {
                let busy = app.print.printer.busy();
                if flat_button(ui, tl!("Print"), true, !busy) {
                    print(app);
                }
                if flat_button(ui, tl!("Save as PDF…"), false, true) {
                    save_pdf(app);
                }
            });
            ui.add_space(6.0);
            settings(ui, app);
        });
        ui.add_space(24.0);
        ui.vertical(|ui| preview(ui, app, &t));
    });
}

fn settings(ui: &mut Ui, app: &mut NoteApp) {
    match &app.services.print {
        Some(Printing::Spooler { discover, .. }) => {
            super::subheading(ui, tl!("Printer"));
            let labels = Labels {
                printer: tl!("Printer"),
                select_printer: tl!("Select a printer"),
                refresh: tl!("Refresh"),
                copies: tl!("Copies"),
                printer_default: tl!("Printer default"),
                duplex: tl!("Two-sided"),
                one_sided: tl!("One-sided"),
                long_edge: tl!("Flip on long edge"),
                short_edge: tl!("Flip on short edge"),
                grayscale: tl!("Black and white"),
            };
            app.print.printer.show(ui, ui.scope_id().with("nc_print_printer"), discover, &labels);
            if let Some(m) = &app.print.printer.message {
                super::dim(ui, &feedback_text(m));
            }
        }
        Some(Printing::Browser(_)) => {
            super::dim(ui, tl!("Your browser's print dialog chooses the printer and the number of copies."));
        }
        None => super::dim(ui, tl!("Printing isn't available here. Use Save as PDF instead.")),
    }
    super::subheading(ui, tl!("Settings"));
    label(ui, tl!("Print"));
    for (s, text) in
        [(ExportScope::Page, tl!("This page")), (ExportScope::Section, tl!("This section")), (ExportScope::Notebook, tl!("This notebook"))]
    {
        ui.radio_value(&mut app.print.scope, s, text);
    }
    let paper_id = label(ui, tl!("Paper")).id;
    craft_ui::ComboBox::from_id_salt("nc_print_paper")
        .width(240.0)
        .selected_text(paper_label(app.print.paper))
        .show_ui(ui, |ui| {
            for p in Paper::ALL {
                ui.selectable_value(&mut app.print.paper, p, paper_label(p));
            }
        })
        .response
        .labelled_by(paper_id);
    ui.add_enabled_ui(app.print.paper != Paper::Fit, |ui| {
        ui.horizontal(|ui| {
            ui.radio_value(&mut app.print.landscape, false, tl!("Portrait"));
            ui.radio_value(&mut app.print.landscape, true, tl!("Landscape"));
        });
    });
    let range_id = label(ui, tl!("Pages")).id;
    let range =
        ui.add(egui::TextEdit::singleline(&mut app.print.range).hint_text(tl!("All, or e.g. 1-3, 5")).desired_width(240.0)).labelled_by(range_id);
    let bad = !app.print.range.trim().is_empty() && notecraft_pdf::parse_ranges(&app.print.range).is_none();
    if bad {
        let t = crate::Tokens::get(ui.ctx());
        ui.label(RichText::new(tl!("Type sheet numbers and ranges, like 1-3, 5.")).font(regular(12.0)).color(t.red));
    }
    let _ = range;
    ui.checkbox(&mut app.print.header, tl!("Page titles and dates"));
}

fn preview(ui: &mut Ui, app: &mut NoteApp, t: &crate::Tokens) {
    super::subheading(ui, tl!("Preview"));
    let Some(p) = &app.print.preview else { return };
    if let Some(e) = &p.error {
        ui.label(RichText::new(e).font(regular(12.5)).color(t.red));
        return;
    }
    let text = if p.sheets == 1 { tl!("1 sheet").to_owned() } else { crate::i18n::fmt_plain(tl!("{n} sheets"), &[("n", &p.sheets.to_string())]) };
    super::dim(ui, &text);
    let more = p.sheets.saturating_sub(p.pictures.len());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(14.0, 14.0);
        for (i, tex) in p.pictures.iter().enumerate() {
            let [w, h] = tex.size();
            let s = THUMB / (w.max(h).max(1) as f32);
            let size = vec2(w as f32 * s, h as f32 * s);
            ui.vertical(|ui| {
                let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                ui.painter().rect_filled(r.translate(vec2(2.0, 2.0)), 0.0, t.border);
                ui.painter().image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                ui.painter().rect_stroke(r, 0.0, egui::Stroke::new(1.0, t.border_strong), egui::StrokeKind::Outside);
                ui.label(RichText::new((i + 1).to_string()).font(regular(11.5)).color(t.text_dim));
            });
        }
    });
    if more > 0 {
        super::dim(ui, &crate::i18n::fmt_plain(tl!("and {n} more"), &[("n", &more.to_string())]));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn printing_sends_the_pdf_to_the_host() {
        let got: Arc<Mutex<Vec<Vec<u8>>>> = Arc::default();
        let sink = got.clone();
        let services = crate::Services {
            print: Some(Printing::Browser(Box::new(move |b: &[u8]| {
                sink.lock().unwrap().push(b.to_vec());
                Ok(())
            }))),
            ..crate::Services::default()
        };
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), services);
        app.print.range = "1".into();
        print(&mut app);
        let sent = got.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert!(sent[0].starts_with(b"%PDF-"));
        assert_eq!(notecraft_pdf::thumbnails::sheet_count(&sent[0]), 1);
        assert!(!app.ui.status_error, "{:?}", app.ui.status);
    }

    #[test]
    fn spooler_jobs_report_in_the_status_bar() {
        let discover: craft_ui::printing::Discover = Box::new(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = tx.send(vec![craft_ui::printing::Printer { name: "Office".into(), label: String::new(), default: true }]);
            rx
        });
        let submit: craft_ui::printing::Submit = Box::new(|bytes, settings| {
            let (tx, rx) = std::sync::mpsc::channel();
            let _ = tx.send(if bytes.starts_with(b"%PDF-") && settings.printer == "Office" {
                Ok("request id is Office-7".into())
            } else {
                Err("bad".into())
            });
            rx
        });
        let services = crate::Services { print: Some(Printing::Spooler { discover, submit }), ..crate::Services::default() };
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), services);
        print(&mut app);
        assert!(app.ui.status_error, "no printer chosen yet");
        let ctx = egui::Context::default();
        if let Some(Printing::Spooler { discover, .. }) = &app.services.print {
            app.print.printer.refresh(discover);
        }
        app.print.printer.poll(&ctx);
        assert!(app.print.printer.ready());
        print(&mut app);
        poll(&mut app, &ctx);
        assert_eq!(app.ui.status.as_deref(), Some("Sent to the printer: request id is Office-7"));
        assert!(!app.print.printer.busy());
    }

    #[test]
    fn ctrl_p_opens_the_print_page_with_a_preview() {
        use egui_kittest::kittest::Queryable;
        let app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1440.0, 900.0)).build_ui_state(
            |ui, app: &mut NoteApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            app,
        );
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::P);
        h.run_steps(3);
        assert!(h.state().ui.backstage && h.state().ui.backstage_page == crate::backstage::BackstagePage::Print);
        let _ = h.get_by_label("Save as PDF…");
        let p = h.state().print.preview.as_ref().expect("a preview");
        assert_eq!(p.sheets, 1);
        assert_eq!(p.pictures.len(), 1);
        assert!(p.error.is_none());
    }

    #[test]
    fn params_follow_the_settings() {
        let mut app = NoteApp::new(notecraft_engine::Session::new_default(), crate::Services::default());
        app.print.scope = ExportScope::Section;
        app.print.paper = Paper::A4;
        app.print.landscape = true;
        app.print.range = " 2-3 ".into();
        let p = params(&app, true);
        assert_eq!(p["scope"], "section");
        assert_eq!(p["paper"], "a4");
        assert_eq!(p["pages"], "2-3");
        assert!(params(&app, false).get("pages").is_none());
        print(&mut app);
        assert!(app.ui.status_error, "no way to print: says so");
    }
}
