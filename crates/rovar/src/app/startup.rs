use super::Studio;
#[cfg(target_family = "wasm")]
use crate::web;
use crate::{i18n, platform, ui::assets, ui::font, ui::titlebar};
use gpui::{AppContext, Bounds, WindowBounds, WindowOptions, px, size};

pub(super) fn window_options(cx: &gpui::App) -> WindowOptions {
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

pub(crate) fn start() {
    #[cfg(not(target_family = "wasm"))]
    let application = gpui_platform::application();
    #[cfg(target_family = "wasm")]
    let application =
        gpui::Application::with_platform(std::rc::Rc::new(gpui_web::WebPlatform::new(false)));
    application.with_assets(assets::Assets).run(|cx| {
        uic::init(cx);
        if let Err(error) = crate::ui::theme::init(cx) {
            eprintln!("Could not initialize theme settings: {error}");
            cx.quit();
            return;
        }
        #[cfg(target_family = "wasm")]
        web::load_fonts(cx);
        if let Err(error) = titlebar::init(cx) {
            eprintln!("Could not initialize titlebar settings: {error}");
            cx.quit();
            return;
        }
        i18n::init();
        #[cfg(target_os = "macos")]
        super::native_menu::init(cx);
        if let Err(error) = font::init(cx) {
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
            cx.new(|cx| Studio::new(platform::workspace_directory(), window, cx))
        });
        if let Err(error) = result {
            eprintln!("Could not open Rovar window: {error:#}");
            cx.quit();
        } else {
            cx.activate(true);
        }
    });
}
