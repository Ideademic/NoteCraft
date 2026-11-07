//! NoteCraft in the browser.
//!
//! Runs the same [`notecraft_ui_egui::NoteApp`] as the desktop app through eframe's web runner
//! (wgpu: WebGPU where available, WebGL2 otherwise). Build with `trunk build --release` from this
//! directory (output in `dist/web`).
//!
//! Differences from the desktop app (phase 2): files open through the browser's picker and
//! arrive in `Services::inbox`; exports download; the scratch notebook is kept in browser
//! storage (`notecraft_engine::io::MemoryStorage`'s `on_write`).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

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
