// Don't open a console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod antimony_panel;
mod app;
mod autosave;
mod canvas;
mod document;
mod export;
mod files;
mod fonts;
mod inspector;
#[cfg(target_os = "macos")]
mod macos_menu;
mod prefs;
#[cfg(not(target_arch = "wasm32"))]
mod recent;
mod renderer;
mod samples;
mod tree;
#[cfg(target_arch = "wasm32")]
mod web;

use app::PathwayApp;

/// Desktop entry point (SPEC-Rust §15.1): a native window. A file named on
/// the command line (as when a file is opened with the app) is opened.
#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    install_crash_dialog();
    let file = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_inner_size([1200.0, 800.0])
        .with_title("PathwayDesigner");
    if let Ok(icon) =
        eframe::icon_data::from_png_bytes(include_bytes!("../../../assets/icon/icon-256.png"))
    {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "PathwayDesigner",
        options,
        Box::new(move |cc| {
            let mut app = PathwayApp::new(cc);
            if let Some(path) = file {
                app.open_path(&path);
            }
            Ok(Box::new(app))
        }),
    )
}

/// If the app crashes, says so before it closes (SPEC-Rust §14.4). The
/// autosave, written to disk every 10 seconds while there are unsaved
/// changes, is offered back at the next start.
#[cfg(not(target_arch = "wasm32"))]
fn install_crash_dialog() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default(info);
        let _ = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("PathwayDesigner stopped")
            .set_description(format!(
                "PathwayDesigner hit an internal error and has to close.\n\n{info}\n\n\
                 Unsaved changes up to the last autosave (at most 10 seconds before \
                 this) will be offered back when you start it again."
            ))
            .set_buttons(rfd::MessageButtons::Ok)
            .show();
    }));
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
