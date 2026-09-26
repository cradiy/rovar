use super::*;
use gpui::{WindowControlArea, rgba};

impl Studio {
    pub(super) fn window_controls(
        &self,
        window: &Window,
        _cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let windows = self.chrome.windows;
        let maximized = window.is_maximized();
        div()
            .id("window-controls")
            .debug_selector(|| "window-controls".into())
            .absolute()
            .right_0()
            .top_0()
            .h(px(tabs::BAR_HEIGHT))
            .w(px(self.chrome.controls_width()))
            .flex()
            .occlude()
            .when(!windows, |el| el.items_center().gap(px(6.)).px(px(10.)))
            .children(
                [
                    (
                        "window-minimize",
                        WindowControlArea::Min,
                        LucideIcons::Minus,
                    ),
                    (
                        "window-maximize",
                        WindowControlArea::Max,
                        if maximized {
                            LucideIcons::Copy
                        } else {
                            LucideIcons::Square
                        },
                    ),
                    ("window-close", WindowControlArea::Close, LucideIcons::X),
                ]
                .map(|(id, area, glyph)| {
                    let button = div()
                        .id(id)
                        .debug_selector(move || id.into())
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(MUTED))
                        .when(windows, |el| {
                            el.w(px(46.))
                                .h_full()
                                .window_control_area(area)
                                .hover(move |style| {
                                    style
                                        .bg(if area == WindowControlArea::Close {
                                            rgba(0xc42b1cff)
                                        } else {
                                            rgba(0xffffff18)
                                        })
                                        .text_color(rgb(0xffffff))
                                })
                        })
                        .when(!windows, |el| {
                            el.size(px(24.)).rounded_full().bg(rgba(0xffffff0e)).hover(
                                move |style| {
                                    style
                                        .bg(if area == WindowControlArea::Close {
                                            rgba(0xc42b1ccc)
                                        } else {
                                            rgba(0xffffff22)
                                        })
                                        .text_color(rgb(TEXT))
                                },
                            )
                        });
                    #[cfg(not(target_os = "windows"))]
                    let button = button.on_click(_cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        match area {
                            WindowControlArea::Min => window.minimize_window(),
                            WindowControlArea::Max => window.zoom_window(),
                            WindowControlArea::Close => this.begin_close(window, cx),
                            WindowControlArea::Drag => unreachable!(),
                        }
                    }));
                    button.child(icon(glyph, if windows { 14. } else { 12. }))
                }),
            )
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext, size};
    use uic::desktop::TitleBarMode;

    fn draw(visual: &mut VisualTestContext) {
        visual.cx.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear());
        visual.cx.run_until_parked();
    }

    #[gpui::test]
    fn linux_modes_reserve_controls_and_close_waits_for_storage(cx: &mut TestAppContext) {
        cx.update(uic::init);
        let root = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            cx.set_global(crate::titlebar::Chrome {
                mode: TitleBarMode::Compact,
                macos: false,
                windows: false,
            })
        });
        let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
            Studio::new(root.path().into(), window, cx)
        });
        handle
            .update(cx, |studio, window, cx| studio.new_document(window, cx))
            .unwrap();
        let mut visual = VisualTestContext::from_window(handle.into(), cx);
        draw(&mut visual);
        let path = handle
            .update(&mut visual.cx, |studio, _, _| {
                studio.tabs[0].file.path.clone()
            })
            .unwrap();
        for mode in [
            TitleBarMode::Hide,
            TitleBarMode::System,
            TitleBarMode::Compact,
        ] {
            handle
                .update(&mut visual.cx, |studio, _, cx| {
                    studio.chrome.mode = mode;
                    cx.notify();
                })
                .unwrap();
            draw(&mut visual);
            assert_eq!(
                visual.debug_bounds("window-controls").is_some(),
                mode == TitleBarMode::Compact
            );
        }
        let controls = visual.debug_bounds("window-controls").unwrap();
        let language = visual.debug_bounds("language-menu").unwrap();
        let tabs = visual.debug_bounds("document-tabs").unwrap();
        assert!(language.right() < controls.left());
        assert!(tabs.right() + px(70.) < language.left());
        let new_tab = visual.debug_bounds("new-tab").unwrap().center();
        visual.simulate_click(new_tab, Default::default());
        draw(&mut visual);
        handle
            .update(&mut visual.cx, |studio, _, cx| {
                assert_eq!(studio.tabs.len(), 2);
                studio.library.update(cx, |library, _| library.busy = true);
            })
            .unwrap();
        let close = visual.debug_bounds("window-close").unwrap().center();
        visual.simulate_click(close, Default::default());
        draw(&mut visual);
        handle
            .update(&mut visual.cx, |studio, _, cx| {
                assert!(studio.closing && studio.awaiting_library);
                studio.library.update(cx, |library, cx| {
                    library.busy = false;
                    cx.notify();
                });
            })
            .unwrap();
        visual.cx.run_until_parked();
        assert!(handle.update(&mut visual.cx, |_, _, _| ()).is_err());
        assert!(crate::document::load(&path).is_ok());
    }
}
