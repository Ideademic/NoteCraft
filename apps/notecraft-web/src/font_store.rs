//! Fonts the user added in the browser (the font box's Add Fonts), kept in IndexedDB so they are
//! there again after a reload (`localStorage` holds about 5 MB, less than one CJK font). A
//! database of its own, so the notebooks' store and its versions are left alone. Every failure is
//! logged and skipped: a font that can't be kept still works for the session.
//!
//! Follows WordCraft's `apps/wordcraft-web/src/web/font_store.rs` (storytold/wordcraft, MIT OR
//! Apache-2.0).

use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use web_sys::{IdbDatabase, IdbRequest, IdbTransactionMode};

const DB_NAME: &str = "notecraft-fonts";
const STORE: &str = "fonts";
/// Most stored fonts read back at start.
const MAX_STORED: u32 = 200;

fn js(e: JsValue) -> String {
    format!("{e:?}")
}

/// Waits for an IndexedDB request; its result.
async fn done(req: &IdbRequest) -> Result<JsValue, String> {
    let promise = js_sys::Promise::new(&mut |resolve: js_sys::Function, reject: js_sys::Function| {
        let ok_req = req.clone();
        let ok = Closure::once_into_js(move || {
            let _ = resolve.call1(&JsValue::NULL, &ok_req.result().unwrap_or(JsValue::UNDEFINED));
        });
        let err = Closure::once_into_js(move || {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("IndexedDB request failed"));
        });
        req.set_onsuccess(Some(ok.unchecked_ref()));
        req.set_onerror(Some(err.unchecked_ref()));
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.map_err(js)
}

/// The database, with its font store made on first use.
async fn open() -> Result<IdbDatabase, String> {
    let factory = web_sys::window().ok_or("no window")?.indexed_db().map_err(js)?.ok_or("IndexedDB is unavailable")?;
    let req = factory.open_with_u32(DB_NAME, 1).map_err(js)?;
    let upgrade_req = req.clone();
    let upgrade = Closure::<dyn FnMut()>::new(move || {
        if let Ok(db) = upgrade_req.result().and_then(|r| r.dyn_into::<IdbDatabase>())
            && !db.object_store_names().contains(STORE)
            && let Err(e) = db.create_object_store(STORE)
        {
            log::error!("couldn't make the font store: {e:?}");
        }
    });
    req.set_onupgradeneeded(Some(upgrade.as_ref().unchecked_ref()));
    let db = done(&req).await;
    req.set_onupgradeneeded(None);
    drop(upgrade);
    db?.dyn_into::<IdbDatabase>().map_err(js)
}

async fn store_async(name: String, bytes: Vec<u8>) -> Result<(), String> {
    let db = open().await?;
    let store = db.transaction_with_str_and_mode(STORE, IdbTransactionMode::Readwrite).map_err(js)?.object_store(STORE).map_err(js)?;
    let req = store.put_with_key(&js_sys::Uint8Array::from(bytes.as_slice()), &JsValue::from_str(&name)).map_err(js)?;
    done(&req).await.map(|_| ())
}

/// The key a font file is kept under: its name without folders, at most 128 characters.
fn key(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let clean: String = base.chars().filter(|c| !c.is_control()).take(128).collect();
    if clean.trim().is_empty() { "font".to_owned() } else { clean }
}

/// Keeps `bytes` (a font file as the user gave it) under its file name.
pub fn store(name: &str, bytes: &[u8]) {
    let (name, bytes) = (key(name), bytes.to_vec());
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = store_async(name.clone(), bytes).await {
            log::error!("couldn't keep the font {name} for the next visit: {e}");
        }
    });
}

/// Adds every stored font (each checked like a new one; one that fails is logged and skipped),
/// then repaints: the canvas lays out again when the fonts change.
pub fn load_all(ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        let r: Result<(), String> = async {
            let db = open().await?;
            let store = db.transaction_with_str(STORE).map_err(js)?.object_store(STORE).map_err(js)?;
            let keys = done(&store.get_all_keys().map_err(js)?).await?;
            let values = done(&store.get_all().map_err(js)?).await?;
            let (keys, values) = (js_sys::Array::from(&keys), js_sys::Array::from(&values));
            for i in 0..keys.length().min(values.length()).min(MAX_STORED) {
                let name = keys.get(i).as_string().unwrap_or_default();
                let Ok(bytes) = values.get(i).dyn_into::<js_sys::Uint8Array>() else { continue };
                if bytes.length() as usize > notecraft_ui_egui::fonts::MAX_FONT_FILE_BYTES {
                    log::warn!("stored font {name} is too large; skipped");
                    continue;
                }
                if let Err(e) = notecraft_ui_egui::fonts::add_font_file(&bytes.to_vec()) {
                    log::warn!("stored font {name} skipped: {e}");
                }
            }
            Ok(())
        }
        .await;
        if let Err(e) = r {
            log::warn!("no stored fonts: {e}");
        }
        ctx.request_repaint();
    });
}
