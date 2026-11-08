//! NoteCraft in the browser.
//!
//! Runs the same [`notecraft_ui_egui::NoteApp`] as the desktop app through eframe's web runner
//! (wgpu: WebGPU where available, WebGL2 otherwise). Build with `trunk build --release` from this
//! directory (output in `dist/web`).
//!
//! Differences from the desktop app:
//! - Open, Import and Insert use the browser's file picker; files can also be dropped on the
//!   page. `.one` sections picked or dropped together open as one notebook ([`open`]).
//! - Exports download.
//! - The notebooks are kept in the browser between visits (`localStorage`, IndexedDB when they
//!   outgrow it), and so are the preferences and where the user was ([`persist`], [`prefs`]).
//!   Leaving the page with changes not yet kept asks first.
//!
//! URL flags: `?webgl` forces WebGL2; `?fresh` starts with a new notebook, leaving the stored one
//! until the next save replaces it.
//!
//! Hosted (`?host=parent` in a same-origin `<iframe>`, or `data-host="page"` on the canvas), a
//! page sends the notebook to edit and stores what NoteCraft saves, each notebook as one
//! `.onepkg` ([`host`]).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(any(target_arch = "wasm32", test))]
mod host;
#[cfg(any(target_arch = "wasm32", test))]
mod open;
#[cfg(any(target_arch = "wasm32", test))]
mod persist;
#[cfg(any(target_arch = "wasm32", test))]
mod prefs;

#[cfg(target_arch = "wasm32")]
mod browser;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    use std::io::Write as _;
    let _ = writeln!(std::io::stderr(), "notecraft-web only runs in the browser: build it with `trunk build --release` in apps/notecraft-web");
}
