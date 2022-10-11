#![warn(clippy::all, rust_2018_idioms)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release



// When compiling natively:
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    // Log to stdout (if you run with `RUST_LOG=debug`).
    tracing_subscriber::fmt::fmt().with_level(true).with_max_level(tracing_subscriber::filter::LevelFilter::DEBUG).init();
    let mut native_options = eframe::NativeOptions::default();
    native_options.drag_and_drop_support = true;
    native_options.resizable = true;
    eframe::run_native(
        "senile ant simulator",
        native_options,
        Box::new(|cc| Box::new(eframe_frontend::AppState::new(cc))),
    );
}

// when compiling to web using trunk.
#[cfg(target_arch = "wasm32")]
fn main() {
    // Make sure panics are logged using `console.error`.
    console_error_panic_hook::set_once();

    // Redirect tracing to console.log and friends:
    tracing_wasm::set_as_global_default();

    console_log::init_with_level(log::Level::Trace).expect("Cannot Log :(");

    let web_options = eframe::WebOptions::default();
    eframe::start_web(
        "the_canvas_id", // hardcode it
        web_options,
        Box::new(|cc| Box::new(eframe_frontend::AppState::new(cc))),
    )
    .expect("failed to start eframe");
}
