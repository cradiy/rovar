use super::*;
use crate::ui::theme::Color;
use crate::{
    i18n::{self, Language},
    ui::font_picker::{FontChosen, FontPicker},
};
use gpui::SharedString;
use uic::{components::dropdown::dropdown, desktop::TitleBarMode};

pub(super) struct Panel {
    font: Option<Entity<FontPicker>>,
    language: Entity<DropdownState>,
    titlebar: Entity<DropdownState>,
    theme: Entity<DropdownState>,
    mode: TitleBarMode,
    focus: FocusHandle,
    pub error: Option<String>,
    detecting: bool,
    _font_subscription: Option<Subscription>,
}

impl Panel {
    pub fn refresh_language(&self, old_mixed: &str, cx: &mut Context<Studio>) {
        if let Some(font) = &self.font {
            font.update(cx, |font, cx| font.refresh_language(old_mixed, cx));
        }
    }
}

impl Studio {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preferences.is_some() {
            return;
        }
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.suspend(window, cx));
        }
        let font = cfg!(target_os = "linux").then(|| cx.new(|cx| FontPicker::new(window, cx)));
        if let Some(font) = &font {
            font.update(cx, |font, cx| {
                font.hide_label();
                font.set_selected(crate::ui::font::family(cx), cx);
            });
        }
        let subscription = font.as_ref().map(|font| {
            cx.subscribe(font, |this, _, event: &FontChosen, cx| {
                this.set_ui_font(event.0.clone(), cx);
            })
        });
        let configured =
            crate::settings::read(crate::settings::path().as_deref()).and_then(|config| {
                serde_json::from_value(config["titlebar"].clone()).map_err(Into::into)
            });
        let (mode, error) = match configured {
            Ok(mode) => (mode, None),
            Err(error) => (self.chrome.mode, Some(error.to_string())),
        };
        let focus = cx.focus_handle();
        self.preferences = Some(Panel {
            font,
            language: cx.new(|cx| DropdownState::new(window, cx)),
            titlebar: cx.new(|cx| DropdownState::new(window, cx)),
            theme: cx.new(|cx| DropdownState::new(window, cx)),
            mode,
            focus: focus.clone(),
            error,
            detecting: false,
            _font_subscription: subscription,
        });
        self.focus.focus(window, cx);
        window.on_next_frame(move |window, cx| focus.focus(window, cx));
        cx.notify();
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.preferences.take() {
            if let Some(font) = panel.font {
                font.update(cx, |font, cx| font.close(window, cx));
            }
            panel.language.update(cx, |menu, cx| menu.close(window, cx));
            panel.titlebar.update(cx, |menu, cx| menu.close(window, cx));
            panel.theme.update(cx, |menu, cx| menu.close(window, cx));
        }
        self.focus.focus(window, cx);
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.focus_canvas(window, cx));
        }
        cx.notify();
    }

    pub(super) fn settings_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &self.preferences else {
            return;
        };
        if let Some(font) = &panel.font
            && font.read(cx).is_open(cx)
        {
            font.update(cx, |font, cx| font.close(window, cx));
        } else if panel.language.read(cx).is_open() {
            panel.language.update(cx, |menu, cx| menu.close(window, cx));
        } else if panel.titlebar.read(cx).is_open() {
            panel.titlebar.update(cx, |menu, cx| menu.close(window, cx));
        } else if panel.theme.read(cx).is_open() {
            panel.theme.update(cx, |menu, cx| menu.close(window, cx));
        } else {
            self.close_settings(window, cx);
        }
    }

    fn set_ui_font(&mut self, family: SharedString, cx: &mut Context<Self>) {
        let result = crate::ui::font::set(family, cx);
        if let Some(panel) = &mut self.preferences {
            panel.error = result.err().map(|error| error.to_string());
            if let Some(font) = &panel.font {
                font.update(cx, |font, cx| {
                    font.set_selected(crate::ui::font::family(cx), cx)
                });
            }
        }
        cx.notify();
    }

    fn use_system_font(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.preferences else {
            return;
        };
        if panel.detecting || panel.font.is_none() {
            return;
        }
        panel.detecting = true;
        let picker = panel.font.clone();
        let detect = cx
            .background_executor()
            .spawn(async { crate::ui::font::detect() });
        cx.spawn(async move |this, cx| {
            let family = detect.await;
            let _ = this.update(cx, |this, cx| {
                if let Some(panel) = &mut this.preferences
                    && panel.font == picker
                {
                    panel.detecting = false;
                    this.set_ui_font(family.into(), cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn set_titlebar(&mut self, mode: TitleBarMode, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.preferences else {
            return;
        };
        match crate::settings::set("titlebar", serde_json::to_value(mode).unwrap()) {
            Ok(()) => {
                panel.mode = mode;
                panel.error = None;
            }
            Err(error) => panel.error = Some(error.to_string()),
        }
        panel.titlebar.update(cx, |menu, cx| menu.close(window, cx));
        cx.notify();
    }

    pub(super) fn settings_dialog(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let panel = self.preferences.as_ref().unwrap();
        let mode = crate::ui::theme::preference(cx);
        let theme = dropdown(&panel.theme)
            .w(px(230.))
            .p(px(5.))
            .rounded(px(8.))
            .bg(Color::Surface.color())
            .border_color(BORDER.color())
            .text_color(TEXT.color())
            .text_size(px(12.))
            .trigger(choice("settings-theme", mode.label()))
            .menu(
                div().flex().flex_col().children(
                    [
                        crate::ui::theme::Mode::System,
                        crate::ui::theme::Mode::Light,
                        crate::ui::theme::Mode::Dark,
                    ]
                    .into_iter()
                    .map(|choice_mode| {
                        choice_item(choice_mode.id(), choice_mode.label(), mode == choice_mode)
                            .debug_selector(move || format!("theme-{}", choice_mode.id()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let result = crate::ui::theme::set(choice_mode, window, cx);
                                if let Some(panel) = &mut this.preferences {
                                    panel.error = result.err().map(|error| error.to_string());
                                    panel.theme.update(cx, |menu, cx| menu.close(window, cx));
                                }
                                cx.notify();
                            }))
                    }),
                ),
            );
        let language = dropdown(&panel.language)
            .w(px(230.))
            .p(px(5.))
            .rounded(px(8.))
            .bg(Color::Surface.color())
            .border_color(BORDER.color())
            .text_color(TEXT.color())
            .text_size(px(12.))
            .trigger(choice("settings-language", i18n::preference().label()))
            .menu(
                div()
                    .flex()
                    .flex_col()
                    .children(Language::available().into_iter().map(|language| {
                        choice_item(
                            language.id(),
                            language.label(),
                            i18n::preference() == language,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.change_language(language, window, cx);
                                if let Some(panel) = &this.preferences {
                                    panel.language.update(cx, |menu, cx| menu.close(window, cx));
                                }
                            },
                        ))
                    })),
            );
        let chrome = panel.mode;
        let titlebar = dropdown(&panel.titlebar)
            .w(px(230.))
            .p(px(5.))
            .rounded(px(8.))
            .bg(Color::Surface.color())
            .border_color(BORDER.color())
            .text_color(TEXT.color())
            .text_size(px(12.))
            .trigger(choice("settings-titlebar", mode_label(chrome)))
            .menu(
                div().flex().flex_col().children(
                    [
                        TitleBarMode::Compact,
                        TitleBarMode::Hide,
                        TitleBarMode::System,
                    ]
                    .into_iter()
                    .map(|mode| {
                        choice_item(mode_id(mode), mode_label(mode), panel.mode == mode).on_click(
                            cx.listener(move |this, _, window, cx| {
                                this.set_titlebar(mode, window, cx)
                            }),
                        )
                    }),
                ),
            );
        let body = div()
            .id("settings-dialog")
            .debug_selector(|| "settings-dialog".into())
            .track_focus(&panel.focus)
            .w(px(520.))
            .max_w_full()
            .max_h_full()
            .overflow_y_scroll()
            .rounded(px(16.))
            .border_1()
            .border_color(Color::Text.color().opacity(0.1020))
            .bg(Color::Panel.color())
            .shadow_xl()
            .flex()
            .flex_col()
            .text_size(px(13.))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .px(px(24.))
                    .py(px(20.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .size(px(36.))
                            .rounded(px(10.))
                            .bg(Color::Accent.color().opacity(0.0941))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(ACCENT.color())
                            .child(icon(LucideIcons::Settings, 19.)),
                    )
                    .child(div().flex_1().text_size(px(18.)).child(t("settings")))
                    .child(
                        div()
                            .id("close-settings")
                            .debug_selector(|| "close-settings".into())
                            .size(px(28.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(MUTED.color())
                            .cursor_pointer()
                            .hover(|s| s.bg(Color::Text.color().opacity(0.0471)))
                            .child(icon(LucideIcons::X, 16.))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_settings(window, cx)),
                            ),
                    ),
            )
            .child(div().h(px(1.)).bg(Color::Text.color().opacity(0.0471)))
            .child(
                div()
                    .p(px(24.))
                    .flex()
                    .flex_col()
                    .gap(px(22.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(20.))
                            .child(t("language"))
                            .child(div().w(px(230.)).child(language)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(20.))
                            .child(t("settings-theme"))
                            .child(div().w(px(230.)).child(theme)),
                    )
                    .when(cfg!(target_os = "linux"), |el| {
                        el.child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(10.))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .child(t("settings-ui-font"))
                                        .when(panel.font.is_some(), |el| {
                                            el.child(
                                                div()
                                                    .id("settings-system-font")
                                                    .debug_selector(|| {
                                                        "settings-system-font".into()
                                                    })
                                                    .text_size(px(11.))
                                                    .text_color(if panel.detecting {
                                                        MUTED.color()
                                                    } else {
                                                        ACCENT.color()
                                                    })
                                                    .child(t("settings-use-system-font"))
                                                    .when(!panel.detecting, |el| {
                                                        el.cursor_pointer().on_click(cx.listener(
                                                            |this, _, _, cx| {
                                                                this.use_system_font(cx)
                                                            },
                                                        ))
                                                    }),
                                            )
                                        }),
                                )
                                .when_some(panel.font.clone(), |el, font| el.child(font))
                                .child(
                                    div()
                                        .p(px(14.))
                                        .rounded(px(8.))
                                        .bg(Color::Input.color())
                                        .border_1()
                                        .border_color(Color::Text.color().opacity(0.0314))
                                        .text_size(px(16.))
                                        .child(t("settings-font-preview")),
                                ),
                        )
                    })
                    .when(cfg!(target_os = "linux"), |el| {
                        el.child(div().h(px(1.)).bg(Color::Text.color().opacity(0.0471)))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(8.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .gap(px(20.))
                                            .child(t("settings-titlebar"))
                                            .child(div().w(px(230.)).child(titlebar)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(if chrome != self.chrome.mode {
                                                ACCENT.color()
                                            } else {
                                                MUTED.color()
                                            })
                                            .child(t(if chrome != self.chrome.mode {
                                                "settings-restart-pending"
                                            } else {
                                                "settings-titlebar-hint"
                                            })),
                                    ),
                            )
                    })
                    .when_some(panel.error.clone(), |el, error| {
                        el.child(
                            div()
                                .rounded(px(8.))
                                .p(px(12.))
                                .bg(Color::Danger.color().opacity(0.0627))
                                .text_color(Color::Danger.color())
                                .text_size(px(12.))
                                .child(i18n::message("settings-save-error", &[("error", error)])),
                        )
                    }),
            )
            .child(
                div()
                    .px(px(24.))
                    .py(px(16.))
                    .border_t_1()
                    .border_color(Color::Text.color().opacity(0.0471))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.))
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(MUTED.color())
                            .child(t("settings-auto-save")),
                    )
                    .child(
                        div()
                            .id("settings-done")
                            .debug_selector(|| "settings-done".into())
                            .px(px(20.))
                            .h(px(32.))
                            .rounded(px(7.))
                            .bg(Color::Selected.color())
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(Color::Hover.color()))
                            .child(t("close"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_settings(window, cx)),
                            ),
                    ),
            );
        gpui::deferred(
            div()
                .absolute()
                .inset_0()
                .occlude()
                .bg(Color::Overlay.color())
                .flex()
                .items_center()
                .justify_center()
                .p(px(20.))
                .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(body),
        )
        .with_priority(30)
    }
}

fn choice(id: &'static str, label: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(34.))
        .px(px(10.))
        .rounded(px(7.))
        .bg(Color::Input.color())
        .border_1()
        .border_color(Color::Text.color().opacity(0.0627))
        .flex()
        .items_center()
        .gap(px(10.))
        .cursor_pointer()
        .hover(|s| s.bg(Color::Hover.color()))
        .child(div().flex_1().child(label))
        .child(icon(LucideIcons::ChevronDown, 12.).text_color(MUTED.color()))
}

fn choice_item(id: &'static str, label: &'static str, selected: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || format!("settings-option-{id}"))
        .h(px(32.))
        .px(px(8.))
        .rounded(px(5.))
        .flex()
        .items_center()
        .gap(px(8.))
        .cursor_pointer()
        .hover(|s| s.bg(Color::Selected.color()))
        .child(
            div()
                .w(px(16.))
                .when(selected, |el| el.child(icon(LucideIcons::Check, 14.))),
        )
        .child(label)
}

fn mode_id(mode: TitleBarMode) -> &'static str {
    match mode {
        TitleBarMode::Compact => "settings-titlebar-inline",
        TitleBarMode::Hide => "settings-titlebar-hidden",
        TitleBarMode::System => "settings-titlebar-system",
    }
}

fn mode_label(mode: TitleBarMode) -> &'static str {
    t(mode_id(mode))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{TestAppContext, VisualTestContext, size};

    fn draw(visual: &mut VisualTestContext) {
        visual.cx.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear());
        visual.cx.run_until_parked();
    }

    fn click(visual: &mut VisualTestContext, selector: &'static str) {
        draw(visual);
        let position = visual
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("Missing {selector}"))
            .center();
        visual.simulate_click(position, Default::default());
        draw(visual);
    }

    #[gpui::test]
    fn settings_menu_modal_and_nested_popups_keep_keyboard_focus(cx: &mut TestAppContext) {
        cx.update(uic::init);
        let directory = rovar_storage::tempfile::tempdir().unwrap();
        let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
            Studio::new(directory.path().into(), window, cx)
        });
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        click(&mut visual, "app-menu");
        click(&mut visual, "settings");
        assert!(visual.debug_bounds("settings-dialog").is_some());
        visual.simulate_keystrokes("ctrl-n");
        draw(&mut visual);
        window
            .update(&mut visual.cx, |studio, _, _| {
                assert!(studio.tabs.is_empty())
            })
            .unwrap();

        click(&mut visual, "settings-language");
        assert!(visual.debug_bounds("settings-option-en-US").is_some());
        visual.simulate_keystrokes("escape");
        draw(&mut visual);
        assert!(visual.debug_bounds("settings-option-en-US").is_none());
        assert!(visual.debug_bounds("settings-dialog").is_some());

        click(&mut visual, "settings-theme");
        assert!(visual.debug_bounds("theme-system").is_some());
        assert!(visual.debug_bounds("theme-light").is_some());
        assert!(visual.debug_bounds("theme-dark").is_some());
        visual.simulate_keystrokes("escape");
        draw(&mut visual);
        assert!(visual.debug_bounds("theme-light").is_none());
        assert!(visual.debug_bounds("settings-dialog").is_some());

        if cfg!(target_os = "linux") {
            click(&mut visual, "settings-titlebar");
            assert!(
                visual
                    .debug_bounds("settings-option-settings-titlebar-hidden")
                    .is_some()
            );
            visual.simulate_keystrokes("escape");
            draw(&mut visual);
            assert!(
                visual
                    .debug_bounds("settings-option-settings-titlebar-hidden")
                    .is_none()
            );
            click(&mut visual, "font-picker");
            click(&mut visual, "font-search");
            visual.simulate_input("Sans");
            draw(&mut visual);
            assert!(visual.debug_bounds("settings-dialog").is_some());
            visual.simulate_keystrokes("escape");
            draw(&mut visual);
            assert!(visual.debug_bounds("font-search").is_none());
            assert!(visual.debug_bounds("settings-dialog").is_some());
        } else {
            assert!(visual.debug_bounds("settings-titlebar").is_none());
            assert!(visual.debug_bounds("font-picker").is_none());
            assert!(visual.debug_bounds("settings-system-font").is_none());
        }

        visual.simulate_keystrokes("escape");
        draw(&mut visual);
        assert!(visual.debug_bounds("settings-dialog").is_none());
        visual.simulate_keystrokes("ctrl-,");
        draw(&mut visual);
        assert!(visual.debug_bounds("settings-dialog").is_some());
        click(&mut visual, "settings-done");
        assert!(visual.debug_bounds("settings-dialog").is_none());
    }
}
