// Don't open a console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod antimony_panel;
mod app;
mod autosave;
mod canvas;
mod document;
mod files;
mod fonts;
mod inspector;
mod prefs;
mod renderer;
mod samples;
mod tree;
#[cfg(target_arch = "wasm32")]
mod web;

use app::PathwayApp;

/// Desktop entry point: open a native window. The desktop app proper comes in
/// milestone D1; until then this keeps the native build compiling and runnable.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_title("PathwayDesigner"),
        ..Default::default()
    };
    eframe::run_native(
        "PathwayDesigner",
        options,
        Box::new(|cc| Ok(Box::new(PathwayApp::new(cc)))),
    )
}

/// Web entry point: attach the app to the <canvas> in index.html.
#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    wasm_bindgen_futures::spawn_local(async {
        let document = eframe::web_sys::window()
            .expect("no window")
            .document()
            .expect("no document");
        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("no element with id 'the_canvas_id'")
            .dyn_into::<eframe::web_sys::HtmlCanvasElement>()
            .expect("'the_canvas_id' is not a <canvas>");

        let runner = eframe::WebRunner::new();
        let result = runner
            .clone()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(PathwayApp::new(cc)))),
            )
            .await;

        web::install_unload_warning();
        watch_for_crash(runner);

        // Replace the "Loading…" message with the app, or with an error.
        if let Some(loading) = document.get_element_by_id("loading_text") {
            match result {
                Ok(()) => loading.remove(),
                Err(e) => {
                    loading.set_inner_html(&format!("PathwayDesigner failed to start: {e:?}"))
                }
            }
        }
    });
}

/// If the app panics, replaces the page with a message and a way to get the
/// autosaved diagram back (SPEC-Rust §14.4).
#[cfg(target_arch = "wasm32")]
fn watch_for_crash(runner: eframe::WebRunner) {
    use eframe::wasm_bindgen::{JsCast as _, closure::Closure};
    let check = Closure::<dyn FnMut()>::new(move || {
        if let Some(summary) = runner.panic_summary() {
            web::show_crash_page(&summary.message());
            runner.destroy();
        }
    });
    if let Some(w) = eframe::web_sys::window() {
        let _ = w.set_interval_with_callback_and_timeout_and_arguments_0(
            check.as_ref().unchecked_ref(),
            1000,
        );
    }
    check.forget();
}
