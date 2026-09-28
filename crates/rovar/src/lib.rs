mod artboard;
mod assets;
mod auto_layout;
mod bezier;
mod component_export;
mod component_library;
mod document;
mod history;
mod i18n;
mod image_fill;
mod layer;
mod media;
mod property;
mod rotation;
mod scene_render;
mod settings;
mod shape;
mod shortcuts;
mod studio;
mod text;
mod titlebar;
mod ui_font;
mod workspace;

use gpui::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use studio::Studio;

fn window_options(cx: &gpui::App) -> WindowOptions {
    let mut options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1280.), px(800.)),
            cx,
        ))),
        window_min_size: Some(size(px(840.), px(520.))),
        app_id: Some("rovar".into()),
        ..Default::default()
    };
    titlebar::Chrome::current(cx).apply(&mut options);
    options
}

pub fn run() {
    gpui_platform::application()
        .with_assets(assets::Assets)
        .run(|cx| {
            uic::init(cx);
            if let Err(error) = titlebar::init(cx) {
                eprintln!("Could not initialize titlebar settings: {error}");
                cx.quit();
                return;
            }
            i18n::init();
            #[cfg(target_os = "macos")]
            studio::native_menu::init(cx);
            if let Err(error) = ui_font::init(cx) {
                eprintln!("Could not initialize UI font settings: {error}");
                cx.quit();
                return;
            }
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let result = cx.open_window(window_options(cx), |window, cx| {
                cx.new(|cx| {
                    Studio::new(
                        std::env::var_os("ROVAR_DATA_DIR")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| {
                                dirs::data_local_dir()
                                    .unwrap_or_else(std::env::temp_dir)
                                    .join("rovar")
                            }),
                        window,
                        cx,
                    )
                })
            });
            if let Err(error) = result {
                eprintln!("Could not open Rovar window: {error:#}");
                cx.quit();
            } else {
                cx.activate(true);
            }
        });
}
