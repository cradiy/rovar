#[cfg(any(unix, windows))]
mod agent;
mod app;
mod asset;
mod document;
mod editor;
mod i18n;
mod platform;
mod remote;
mod render;
mod scene;
mod settings;
mod svg;
mod ui;
#[cfg(target_family = "wasm")]
mod web;

pub fn run() {
    #[cfg(target_family = "wasm")]
    web::start();
    #[cfg(any(unix, windows))]
    agent::run();
    #[cfg(all(not(target_family = "wasm"), not(any(unix, windows))))]
    app::start();
}
