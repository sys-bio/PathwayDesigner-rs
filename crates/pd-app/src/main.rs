// Don't open a console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod canvas;
mod fonts;
mod renderer;
mod samples;

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

        let result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| Ok(Box::new(PathwayApp::new(cc)))),
            )
            .await;

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
