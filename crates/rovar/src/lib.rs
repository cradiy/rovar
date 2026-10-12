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
    #[cfg(not(target_family = "wasm"))]
    app::start();
}
