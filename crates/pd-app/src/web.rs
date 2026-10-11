//! Browser-only pieces (SPEC-Rust §10.1, §10.2, §14.4): the File System
//! Access API, downloads, the page title, the unsaved-changes warning when a
//! tab is closed, and the page shown if the app crashes.
//!
//! The File System Access API is newer than `web-sys`'s stable bindings, so it
//! is reached through `js_sys::Reflect`; browsers without it fall back to
//! downloads.

use std::cell::Cell;

use js_sys::{Array, Function, Object, Promise, Reflect};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use wasm_bindgen_futures::JsFuture;

fn window() -> web_sys::Window {
    web_sys::window().expect("running in a browser")
}

fn get(target: &JsValue, key: &str) -> Result<JsValue, String> {
    Reflect::get(target, &JsValue::from_str(key)).map_err(describe)
}

fn describe(e: JsValue) -> String {
    e.as_string()
        .or_else(|| get(&e, "message").ok().and_then(|m| m.as_string()))
        .unwrap_or_else(|| "unknown error".to_owned())
}

/// Calls `target.method(args…)` and awaits the promise it returns.
async fn call_async(target: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, String> {
    let f: Function = get(target, method)?.dyn_into().map_err(describe)?;
    let promise = f
        .apply(target, &args.iter().cloned().collect::<Array>())
        .map_err(describe)?;
    JsFuture::from(Promise::from(promise))
        .await
        .map_err(describe)
}

/// A file picked with the File System Access API, kept so Save can write to
/// it again.
#[derive(Clone, Debug)]
pub struct FileHandle(JsValue);

impl FileHandle {
    pub fn name(&self) -> String {
        get(&self.0, "name")
            .ok()
            .and_then(|n| n.as_string())
            .unwrap_or_default()
    }
}

/// True where the browser can save back to a file (Chromium-based browsers).
pub fn has_file_system_access() -> bool {
    get(&window(), "showSaveFilePicker").is_ok_and(|f| f.is_function())
}

/// The `types` option of the file pickers: one entry per filter.
fn picker_types(filters: &[(&str, &[&str])]) -> Result<Array, String> {
    let types = Array::new();
    for (label, exts) in filters {
        let list: Array = exts
            .iter()
            .map(|e| JsValue::from_str(&format!(".{e}")))
            .collect();
        let mime = mime_type(exts[0]);
        let accept = Object::new();
        Reflect::set(&accept, &mime.into(), &list).map_err(describe)?;
        let kind = Object::new();
        Reflect::set(&kind, &"description".into(), &(*label).into()).map_err(describe)?;
        Reflect::set(&kind, &"accept".into(), &accept).map_err(describe)?;
        types.push(&kind);
    }
    Ok(types)
}

/// Asks for a file to open, keeping its handle so Save can write back to it
/// later. `None` if the user cancelled.
pub async fn pick_open_file(
    filters: &[(&str, &[&str])],
) -> Result<Option<(FileHandle, String)>, String> {
    let options = Object::new();
    Reflect::set(&options, &"types".into(), &picker_types(filters)?.into()).map_err(describe)?;
    let handles = match call_async(&window(), "showOpenFilePicker", &[options.into()]).await {
        Ok(h) => h,
        Err(e) if e.contains("abort") || e.contains("cancel") => return Ok(None),
        Err(e) => return Err(e),
    };
    let handle = Array::from(&handles).get(0);
    let file = call_async(&handle, "getFile", &[]).await?;
    let text = call_async(&file, "text", &[]).await?;
    Ok(Some((
        FileHandle(handle),
        text.as_string().unwrap_or_default(),
    )))
}

/// Asks where to save. `None` if the user cancelled.
pub async fn pick_save_file(
    name: &str,
    label: &str,
    exts: &[&str],
) -> Result<Option<FileHandle>, String> {
    let options = Object::new();
    Reflect::set(&options, &"suggestedName".into(), &name.into()).map_err(describe)?;
    Reflect::set(
        &options,
        &"types".into(),
        &picker_types(&[(label, exts)])?.into(),
    )
    .map_err(describe)?;
    match call_async(&window(), "showSaveFilePicker", &[options.into()]).await {
        Ok(handle) => Ok(Some(FileHandle(handle))),
        // The user closed the picker.
        Err(e) if e.contains("abort") || e.contains("cancel") => Ok(None),
        Err(e) => Err(e),
    }
}

/// The media type of a file with extension `ext`.
fn mime_type(ext: &str) -> &'static str {
    match ext {
        "json" => "application/json",
        "png" => "image/png",
        "pdf" => "application/pdf",
        "svg" => "image/svg+xml",
        "xml" | "sbml" => "application/xml",
        _ => "text/plain",
    }
}

/// `contents` as a Blob of the type a file called `name` holds.
fn blob(name: &str, contents: &[u8]) -> Result<web_sys::Blob, String> {
    let parts = Array::of1(&js_sys::Uint8Array::from(contents));
    let options = web_sys::BlobPropertyBag::new();
    let ext = name.rsplit_once('.').map_or("", |(_, e)| e);
    options.set_type(mime_type(&ext.to_ascii_lowercase()));
    web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(describe)
}

/// Writes `contents` to a picked file, replacing what was there.
pub async fn write_file(handle: &FileHandle, contents: &[u8]) -> Result<(), String> {
    let writable = call_async(&handle.0, "createWritable", &[]).await?;
    let data = js_sys::Uint8Array::from(contents);
    call_async(&writable, "write", &[data.into()]).await?;
    call_async(&writable, "close", &[]).await?;
    Ok(())
}

/// Offers `contents` as a download called `name`.
pub fn download(name: &str, contents: &[u8]) -> Result<(), String> {
    let blob = blob(name, contents)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(describe)?;
    let document = window().document().ok_or("no document")?;
    let a: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(describe)?
        .dyn_into()
        .map_err(|_| "not an anchor".to_owned())?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}

/// Opens a PDF in a new tab, where the browser's viewer can print it
/// (SPEC-Rust §9). The tab keeps the PDF's address, so it isn't revoked.
pub fn open_pdf(pdf: &[u8]) -> Result<(), String> {
    let blob = blob("print.pdf", pdf)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(describe)?;
    match window().open_with_url_and_target(&url, "_blank") {
        Ok(Some(_)) => Ok(()),
        _ => Err(
            "The browser blocked the new tab for printing. Allow pop-ups for this \
                  page, or use Export ▸ PDF… and print the file."
                .to_owned(),
        ),
    }
}

pub fn set_title(title: &str) {
    if let Some(d) = window().document() {
        d.set_title(title);
    }
}

thread_local! {
    static MODIFIED: Cell<bool> = const { Cell::new(false) };
}

/// Tells the unsaved-changes warning whether there is anything to lose.
pub fn set_modified(modified: bool) {
    MODIFIED.with(|m| m.set(modified));
}

/// Makes the browser ask before closing or reloading the tab while there are
/// unsaved changes (S §13.1). Call once.
pub fn install_unload_warning() {
    let handler = Closure::<dyn Fn(web_sys::Event)>::new(|event: web_sys::Event| {
        if MODIFIED.with(Cell::get) {
            event.prevent_default();
            // Older browsers need returnValue set as well.
            let _ = Reflect::set(&event, &"returnValue".into(), &"".into());
        }
    });
    let _ =
        window().add_event_listener_with_callback("beforeunload", handler.as_ref().unchecked_ref());
    handler.forget();
}

/// Replaces the page with a message that the app stopped, and a button to
/// download the last autosave (SPEC-Rust §14.4).
pub fn show_crash_page(message: &str) {
    let Some(document) = window().document() else {
        return;
    };
    let saved = window()
        .local_storage()
        .ok()
        .flatten()
        .and_then(|s| s.get_item(crate::autosave::KEY).ok().flatten());
    let escape = |s: &str| {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    };
    let recovery = if saved.is_some() {
        "<p>Your last autosaved diagram can be downloaded and opened again.</p>\
         <button id='pd-recover'>Download the autosaved diagram</button>"
    } else {
        "<p>There was no autosaved diagram to recover.</p>"
    };
    let html = format!(
        "<div style='font:15px sans-serif;max-width:40em;margin:3em auto;padding:0 16px;color:#1a1a1a'>\
         <h2>PathwayDesigner stopped</h2>\
         <p>Something went wrong and the app can't continue. Reload the page to start again.</p>\
         {recovery}\
         <details style='margin-top:1em'><summary>Details</summary><pre style='white-space:pre-wrap'>{}</pre></details>\
         </div>",
        escape(message)
    );
    if let Some(body) = document.body() {
        body.set_inner_html(&html);
        body.style().set_property("overflow", "auto").ok();
    }
    if let (Some(text), Some(button)) = (saved, document.get_element_by_id("pd-recover")) {
        let json = crate::autosave::document_of(&text).unwrap_or(text);
        let click = Closure::<dyn Fn()>::new(move || {
            let _ = download("recovered.json", json.as_bytes());
        });
        let _ = button.add_event_listener_with_callback("click", click.as_ref().unchecked_ref());
        click.forget();
    }
}
