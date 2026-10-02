#[cfg(all(not(target_os = "windows"), not(target_family = "wasm")))]
use gpui::MouseButton;
use gpui::{
    App, Div, Global, TitlebarOptions, WindowControlArea, WindowOptions, div, point, prelude::*, px,
};
use uic::desktop::TitleBarMode;

#[derive(Clone, Copy)]
pub(crate) struct Chrome {
    pub mode: TitleBarMode,
    pub macos: bool,
    pub windows: bool,
}
impl Global for Chrome {}

impl Chrome {
    fn for_platform(mode: TitleBarMode, macos: bool, windows: bool) -> Self {
        Self {
            mode: if macos || windows {
                TitleBarMode::Compact
            } else {
                mode
            },
            macos,
            windows,
        }
    }

    pub fn current(cx: &App) -> Self {
        cx.try_global::<Self>().copied().unwrap_or_else(|| {
            Self::for_platform(
                TitleBarMode::default(),
                cfg!(target_os = "macos"),
                cfg!(target_os = "windows"),
            )
        })
    }

    pub fn left_inset(self) -> f32 {
        if self.macos { 80. } else { 0. }
    }

    pub fn controls_width(self) -> f32 {
        if cfg!(target_family = "wasm") || self.macos || self.mode != TitleBarMode::Compact {
            0.
        } else if self.windows {
            138.
        } else {
            104.
        }
    }

    pub fn apply(self, options: &mut WindowOptions) {
        options.app_owns_titlebar_drag = self.macos;
        options.titlebar = Some(TitlebarOptions {
            title: Some("Rovar".into()),
            ..Default::default()
        });
        self.mode.apply_to_window_options(options);
        if self.macos {
            options.titlebar.as_mut().unwrap().traffic_light_position =
                Some(point(px(12.), px(16.)));
        } else if self.mode != TitleBarMode::System {
            options.titlebar = None;
        }
    }
}

pub(crate) fn init(cx: &mut App) -> std::io::Result<()> {
    let path = crate::settings::path()
        .ok_or_else(|| std::io::Error::other("Could not locate settings directory"))?;
    let mode = crate::settings::titlebar(&path, TitleBarMode::default)?;
    cx.set_global(Chrome::for_platform(
        mode,
        cfg!(target_os = "macos"),
        cfg!(target_os = "windows"),
    ));
    Ok(())
}

pub(crate) fn drag_region() -> Div {
    let region = div().window_control_area(WindowControlArea::Drag);
    #[cfg(all(not(target_os = "windows"), not(target_family = "wasm")))]
    let region = region.on_mouse_down(MouseButton::Left, |event, window, cx| {
        if event.click_count == 2 {
            window.zoom_window();
        } else {
            window.start_window_move();
        }
        cx.stop_propagation();
    });
    #[cfg(target_os = "linux")]
    let region = region.on_mouse_down(MouseButton::Right, |event, window, cx| {
        window.show_window_menu(event.position);
        cx.stop_propagation();
    });
    region
}

#[cfg(target_os = "linux")]
pub(crate) fn resize_edges() -> impl IntoIterator<Item = gpui::AnyElement> {
    use gpui::{CursorStyle, ResizeEdge};
    [
        (ResizeEdge::Top, CursorStyle::ResizeUpDown),
        (ResizeEdge::Bottom, CursorStyle::ResizeUpDown),
        (ResizeEdge::Left, CursorStyle::ResizeLeftRight),
        (ResizeEdge::Right, CursorStyle::ResizeLeftRight),
        (ResizeEdge::TopLeft, CursorStyle::ResizeUpLeftDownRight),
        (ResizeEdge::TopRight, CursorStyle::ResizeUpRightDownLeft),
        (ResizeEdge::BottomLeft, CursorStyle::ResizeUpRightDownLeft),
        (ResizeEdge::BottomRight, CursorStyle::ResizeUpLeftDownRight),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (edge, cursor))| {
        div()
            .id(("window-resize", index))
            .absolute()
            .occlude()
            .cursor(cursor)
            .map(|el| match edge {
                ResizeEdge::Top => el.top_0().left(px(6.)).right(px(6.)).h(px(3.)),
                ResizeEdge::Bottom => el.bottom_0().left(px(6.)).right(px(6.)).h(px(3.)),
                ResizeEdge::Left => el.left_0().top(px(6.)).bottom(px(6.)).w(px(3.)),
                ResizeEdge::Right => el.right_0().top(px(6.)).bottom(px(6.)).w(px(3.)),
                ResizeEdge::TopLeft => el.top_0().left_0().size(px(6.)),
                ResizeEdge::TopRight => el.top_0().right_0().size(px(6.)),
                ResizeEdge::BottomLeft => el.bottom_0().left_0().size(px(6.)),
                ResizeEdge::BottomRight => el.bottom_0().right_0().size(px(6.)),
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.start_window_resize(edge);
                cx.stop_propagation();
            })
            .into_any_element()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_modes_configure_native_decorations_and_control_space() {
        for mode in [
            TitleBarMode::Compact,
            TitleBarMode::Hide,
            TitleBarMode::System,
        ] {
            let linux = Chrome::for_platform(mode, false, false);
            let mut options = WindowOptions::default();
            linux.apply(&mut options);
            assert_eq!(
                options.window_decorations,
                Some(if mode == TitleBarMode::System {
                    gpui::WindowDecorations::Server
                } else {
                    gpui::WindowDecorations::Client
                })
            );
            assert_eq!(options.titlebar.is_some(), mode == TitleBarMode::System);
            assert_eq!(linux.controls_width() > 0., mode == TitleBarMode::Compact);
            let mac = Chrome::for_platform(mode, true, false);
            mac.apply(&mut options);
            assert!(options.titlebar.as_ref().unwrap().appears_transparent);
            assert!(
                options
                    .titlebar
                    .as_ref()
                    .unwrap()
                    .traffic_light_position
                    .is_some()
            );
            assert!(options.app_owns_titlebar_drag);
            assert!(mac.left_inset() > 0.);
            assert_eq!(mac.controls_width(), 0.);
            let windows = Chrome::for_platform(mode, false, true);
            windows.apply(&mut options);
            assert!(options.titlebar.is_none());
            assert_eq!(windows.controls_width(), 138.);
            assert_eq!(windows.mode, TitleBarMode::Compact);
        }
    }
}
