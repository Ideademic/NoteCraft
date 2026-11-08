//! Browser APIs through web-sys: `localStorage`, IndexedDB, the file picker, downloads, the
//! clipboard, the colour-scheme query and the leave-page guard. No handwritten JavaScript.

use std::cell::Cell;
use std::rc::Rc;

use wasm_bindgen::JsCast as _;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::JsFuture;

use crate::persist::{IDB_KEY, IDB_NAME, IDB_STORE};

fn js(e: JsValue) -> String {
    e.as_string()
        .or_else(|| js_sys::Reflect::get(&e, &JsValue::from_str("message")).ok().and_then(|m| m.as_string()))
        .unwrap_or_else(|| format!("{e:?}"))
}

fn window() -> Result<web_sys::Window, String> {
    web_sys::window().ok_or_else(|| "no window".to_owned())
}

// --- localStorage ---------------------------------------------------------------------------

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

pub fn local_get(key: &str) -> Option<String> {
    local_storage()?.get_item(key).ok()?
}

/// Writes `value`; an error when storage is off or full (`QuotaExceededError`).
pub fn local_set(key: &str, value: &str) -> Result<(), String> {
    local_storage().ok_or("browser storage is unavailable")?.set_item(key, value).map_err(js)
}

// --- IndexedDB ------------------------------------------------------------------------------

/// A promise settled by `req`'s success (with its result) or error.
fn request_done(req: &web_sys::IdbRequest) -> JsFuture {
    let promise = js_sys::Promise::new(&mut |resolve: js_sys::Function, reject: js_sys::Function| {
        let ok_req = req.clone();
        let ok = Closure::once_into_js(move || {
            let _ = resolve.call1(&JsValue::NULL, &ok_req.result().unwrap_or(JsValue::UNDEFINED));
        });
        let err_req = req.clone();
        let err = Closure::once_into_js(move || {
            let why = err_req.error().ok().flatten().map_or_else(|| JsValue::from_str("IndexedDB request failed"), JsValue::from);
            let _ = reject.call1(&JsValue::NULL, &why);
        });
        req.set_onsuccess(Some(ok.unchecked_ref()));
        req.set_onerror(Some(err.unchecked_ref()));
    });
    JsFuture::from(promise)
}

/// A promise settled when `tx` commits (or fails / is aborted).
fn transaction_done(tx: &web_sys::IdbTransaction) -> JsFuture {
    let promise = js_sys::Promise::new(&mut |resolve: js_sys::Function, reject: js_sys::Function| {
        let ok = Closure::once_into_js(move || {
            let _ = resolve.call0(&JsValue::NULL);
        });
        let fail = reject.clone();
        let err = Closure::once_into_js(move || {
            let _ = fail.call1(&JsValue::NULL, &JsValue::from_str("IndexedDB write failed (is the disk full?)"));
        });
        let abort = Closure::once_into_js(move || {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("IndexedDB write was aborted (storage quota?)"));
        });
        tx.set_oncomplete(Some(ok.unchecked_ref()));
        tx.set_onerror(Some(err.unchecked_ref()));
        tx.set_onabort(Some(abort.unchecked_ref()));
    });
    JsFuture::from(promise)
}

async fn idb_open() -> Result<web_sys::IdbDatabase, String> {
    let factory = window()?.indexed_db().map_err(js)?.ok_or("IndexedDB is unavailable")?;
    let req = factory.open_with_u32(IDB_NAME, 1).map_err(js)?;
    let upgrade_req = req.clone();
    let on_upgrade = Closure::once_into_js(move || {
        if let Some(db) = upgrade_req.result().ok().and_then(|r| r.dyn_into::<web_sys::IdbDatabase>().ok())
            && !db.object_store_names().contains(IDB_STORE)
            && let Err(e) = db.create_object_store(IDB_STORE)
        {
            log::error!("IndexedDB: can't create the store: {}", js(e));
        }
    });
    req.set_onupgradeneeded(Some(on_upgrade.unchecked_ref()));
    let db = request_done(&req).await.map_err(js)?;
    db.dyn_into::<web_sys::IdbDatabase>().map_err(|_| "IndexedDB: not a database".to_owned())
}

fn idb_store(db: &web_sys::IdbDatabase, mode: web_sys::IdbTransactionMode) -> Result<(web_sys::IdbTransaction, web_sys::IdbObjectStore), String> {
    let tx = db.transaction_with_str_and_mode(IDB_STORE, mode).map_err(js)?;
    let store = tx.object_store(IDB_STORE).map_err(js)?;
    Ok((tx, store))
}

/// The blob kept in IndexedDB, if any.
pub async fn idb_get() -> Result<Option<Vec<u8>>, String> {
    let db = idb_open().await?;
    let (_tx, store) = idb_store(&db, web_sys::IdbTransactionMode::Readonly)?;
    let req = store.get(&JsValue::from_str(IDB_KEY)).map_err(js)?;
    let v = request_done(&req).await.map_err(js)?;
    db.close();
    if v.is_undefined() || v.is_null() {
        return Ok(None);
    }
    Ok(Some(js_sys::Uint8Array::new(&v).to_vec()))
}

/// Stores `bytes` in IndexedDB, unless `still_current` says a newer write has started by the
/// time the database is open. `Ok(false)`: skipped.
pub async fn idb_put(bytes: Vec<u8>, still_current: impl Fn() -> bool) -> Result<bool, String> {
    let db = idb_open().await?;
    if !still_current() {
        db.close();
        return Ok(false);
    }
    let (tx, store) = idb_store(&db, web_sys::IdbTransactionMode::Readwrite)?;
    let data = js_sys::Uint8Array::from(bytes.as_slice());
    drop(bytes);
    store.put_with_key(&data, &JsValue::from_str(IDB_KEY)).map_err(js)?;
    let done = transaction_done(&tx).await.map_err(js);
    db.close();
    done.map(|_| true)
}

/// Removes the IndexedDB copy (the blob moved back to `localStorage`).
pub async fn idb_delete(still_current: impl Fn() -> bool) -> Result<(), String> {
    let db = idb_open().await?;
    if still_current() {
        let (tx, store) = idb_store(&db, web_sys::IdbTransactionMode::Readwrite)?;
        store.delete(&JsValue::from_str(IDB_KEY)).map_err(js)?;
        transaction_done(&tx).await.map_err(js)?;
    }
    db.close();
    Ok(())
}

// --- files in -------------------------------------------------------------------------------

/// Opens the browser's file picker (`accept`: `.one,.onepkg`; empty for any file). The chosen
/// files go to `on_pick`; nothing happens when the user cancels. Must run while the browser
/// counts a click as recent (eframe runs the app inside `pointerup`, so a button can call it).
pub fn pick_files(accept: &str, multiple: bool, on_pick: impl FnOnce(Vec<web_sys::File>) + 'static) -> Result<(), String> {
    let document = window()?.document().ok_or("no document")?;
    let input: web_sys::HtmlInputElement = document.create_element("input").map_err(js)?.dyn_into().map_err(|_| "not an input".to_owned())?;
    input.set_type("file");
    input.set_multiple(multiple);
    if !accept.is_empty() {
        input.set_accept(accept);
    }
    let picked = input.clone();
    let on_change = Closure::once_into_js(move || {
        let mut files = Vec::new();
        if let Some(list) = picked.files() {
            for i in 0..list.length().min(crate::open::MAX_FILES as u32) {
                if let Some(f) = list.get(i) {
                    files.push(f);
                }
            }
        }
        if !files.is_empty() {
            on_pick(files);
        }
    });
    input.set_onchange(Some(on_change.unchecked_ref()));
    input.click();
    Ok(())
}

/// A picked or dropped file's bytes, refusing ones over `max` before reading them.
pub async fn read_file(file: &web_sys::File, max: usize) -> Result<Vec<u8>, String> {
    if file.size() > max as f64 {
        return Err(format!("{} is larger than {} MB", file.name(), max >> 20));
    }
    let buf = JsFuture::from(file.array_buffer()).await.map_err(js)?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

// --- files out ------------------------------------------------------------------------------

/// The MIME type of a download, by extension.
pub fn mime_for(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).as_deref() {
        Some("onepkg") => "application/onenote",
        Some("one") => "application/onenote",
        Some("html" | "htm") => "text/html",
        Some("md") => "text/markdown",
        Some("txt") => "text/plain",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// Opens a web or mail link in a new tab. Only those: a note's `javascript:` or `file:` link
/// never runs in the page.
pub fn open_link(url: &str) -> Result<(), String> {
    if !matches!(notecraft_doc::link::scheme(url).as_deref(), Some("http" | "https" | "mailto")) {
        return Err("The browser opens web and e-mail links only.".to_owned());
    }
    window()?.open_with_url_and_target_and_features(url, "_blank", "noopener,noreferrer").map_err(js)?;
    Ok(())
}

/// Hands `bytes` to the browser as a download named `name`.
pub fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    let window = window()?;
    let document = window.document().ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime_for(name));
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).map_err(js)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js)?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a").map_err(js)?.dyn_into().map_err(|_| "not a link".to_owned())?;
    a.set_href(&url);
    a.set_download(name);
    a.style().set_property("display", "none").map_err(js)?;
    let body = document.body().ok_or("no body")?;
    body.append_child(&a).map_err(js)?;
    a.click();
    a.remove();
    // Revoke once the download has its own reference.
    let revoke = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    window.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 30_000).map_err(js)?;
    Ok(())
}

// --- clipboard ------------------------------------------------------------------------------

fn clipboard() -> Option<web_sys::Clipboard> {
    let nav = web_sys::window()?.navigator();
    // Missing outside secure contexts (plain http on another host).
    js_sys::Reflect::has(&nav, &JsValue::from_str("clipboard")).ok().filter(|has| *has)?;
    Some(nav.clipboard())
}

/// Reads the clipboard's text (the browser may ask the user first); `None` on refusal.
pub async fn clipboard_text() -> Option<String> {
    let v = JsFuture::from(clipboard()?.read_text()).await.ok()?;
    v.as_string().filter(|t| !t.is_empty())
}

/// The clipboard's first picture as file bytes, unless it also holds text (text is pasted
/// instead, as on the desktop).
pub async fn clipboard_picture() -> Option<Vec<u8>> {
    let items = JsFuture::from(clipboard()?.read()).await.ok()?;
    let items = js_sys::Array::from(&items);
    for item in items.iter() {
        let item: web_sys::ClipboardItem = item.dyn_into().ok()?;
        let types: Vec<String> = item.types().iter().filter_map(|t| t.as_string()).collect();
        if types.iter().any(|t| t == "text/plain") {
            return None;
        }
        if let Some(ty) = types.iter().find(|t| t.starts_with("image/")) {
            let blob: web_sys::Blob = JsFuture::from(item.get_type(ty)).await.ok()?.dyn_into().ok()?;
            if blob.size() > crate::open::MAX_FILE_BYTES as f64 {
                return None;
            }
            let buf = JsFuture::from(blob.array_buffer()).await.ok()?;
            return Some(js_sys::Uint8Array::new(&buf).to_vec());
        }
    }
    None
}

/// Puts text on the clipboard; `false` when the browser has no clipboard here.
pub fn set_clipboard_text(text: &str) -> bool {
    match clipboard() {
        Some(c) => {
            let promise = c.write_text(text);
            wasm_bindgen_futures::spawn_local(async move {
                if let Err(e) = JsFuture::from(promise).await {
                    log::warn!("copy refused: {}", js(e));
                }
            });
            true
        }
        None => false,
    }
}

/// Pictures pasted with Ctrl+V while the clipboard holds no text (a screenshot, Copy Image):
/// eframe's paste handler only reads text, so their bytes go to `on_picture`.
pub fn listen_for_pasted_pictures(on_picture: impl Fn(String, Vec<u8>) + 'static) -> Result<(), String> {
    let document = window()?.document().ok_or("no document")?;
    let on_picture = Rc::new(on_picture);
    let on_paste = Closure::<dyn FnMut(web_sys::ClipboardEvent)>::new(move |e: web_sys::ClipboardEvent| {
        let Some(data) = e.clipboard_data() else { return };
        if !data.get_data("text").unwrap_or_default().is_empty() {
            return;
        }
        let Some(files) = data.files() else { return };
        for i in 0..files.length().min(20) {
            let Some(file) = files.get(i) else { continue };
            if !file.type_().starts_with("image/") {
                continue;
            }
            let on_picture = on_picture.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match read_file(&file, crate::open::MAX_FILE_BYTES).await {
                    Ok(bytes) => on_picture("Pasted picture.png".to_owned(), bytes),
                    Err(e) => log::warn!("pasted picture: {e}"),
                }
            });
        }
    });
    document.add_event_listener_with_callback("paste", on_paste.as_ref().unchecked_ref()).map_err(js)?;
    // The listener lives as long as the page.
    on_paste.forget();
    Ok(())
}

// --- the page -------------------------------------------------------------------------------

/// Whether the system prefers dark colours (`prefers-color-scheme`); the query follows changes.
pub fn prefers_dark() -> Box<dyn Fn() -> Option<bool>> {
    let query = web_sys::window().and_then(|w| w.match_media("(prefers-color-scheme: dark)").ok().flatten());
    Box::new(move || query.as_ref().map(web_sys::MediaQueryList::matches))
}

/// Closing or reloading the tab while `unsaved` is set asks first (the browser's own question).
pub fn guard_unload(unsaved: Rc<Cell<bool>>) -> Result<(), String> {
    let on_unload = Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(move |e: web_sys::BeforeUnloadEvent| {
        if unsaved.get() {
            e.prevent_default();
            // Older browsers ask only when `returnValue` is set.
            e.set_return_value("unsaved changes");
        }
    });
    window()?.add_event_listener_with_callback("beforeunload", on_unload.as_ref().unchecked_ref()).map_err(js)?;
    on_unload.forget();
    Ok(())
}

/// The page's query string (`?fresh`).
pub fn query() -> String {
    web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default()
}
