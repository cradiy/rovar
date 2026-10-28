use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Section {
    General,
    Appearance,
    #[cfg(any(unix, windows))]
    Ai,
    About,
}

impl Section {
    fn key(self) -> &'static str {
        match self {
            Self::General => "settings-general",
            Self::Appearance => "settings-appearance",
            #[cfg(any(unix, windows))]
            Self::Ai => "settings-skill-title",
            Self::About => "settings-about",
        }
    }
    pub fn label(self) -> &'static str {
        t(self.key())
    }
    pub fn description(self) -> &'static str {
        t(match self {
            Self::General => "settings-general-description",
            Self::Appearance => "settings-appearance-description",
            #[cfg(any(unix, windows))]
            Self::Ai => "settings-ai-description",
            Self::About => "settings-about-description",
        })
    }
    fn glyph(self) -> GpuiIcons {
        match self {
            Self::General => GpuiIcons::LucideSettings2,
            Self::Appearance => GpuiIcons::LucidePalette,
            #[cfg(any(unix, windows))]
            Self::Ai => GpuiIcons::LucideSparkles,
            Self::About => GpuiIcons::LucideInfo,
        }
    }
}

pub(super) fn card() -> gpui::Div {
    div()
        .p(px(22.))
        .rounded(px(14.))
        .bg(Color::Panel.color())
        .border_1()
        .border_color(Color::Text.color().opacity(0.06))
}

impl Studio {
    fn select_settings_section(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(panel) = &mut self.preferences else {
            return;
        };
        if let Some(font) = &panel.font {
            font.update(cx, |font, cx| font.close(window, cx));
        }
        panel.language.update(cx, |menu, cx| menu.close(window, cx));
        panel.titlebar.update(cx, |menu, cx| menu.close(window, cx));
        panel.theme.update(cx, |menu, cx| menu.close(window, cx));
        panel.section = section;
        panel.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn settings_shell(
        &self,
        content: gpui::Div,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let panel = self.preferences.as_ref().unwrap();
        let compact = window.viewport_size().width < px(900.);
        div()
            .id("settings-page")
            .debug_selector(|| "settings-page".into())
            .size_full()
            .flex()
            .flex_col()
            .bg(Color::Workspace.color())
            .track_focus(&panel.focus)
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .relative()
                    .h(px(tabs::BAR_HEIGHT))
                    .flex_shrink_0()
                    .child(crate::ui::titlebar::drag_region().absolute().inset_0())
                    .child(
                        div()
                            .absolute()
                            .left(px(16. + self.chrome.left_inset()))
                            .h_full()
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .id("settings-back")
                                    .debug_selector(|| "settings-back".into())
                                    .occlude()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .h(px(32.))
                                    .px(px(10.))
                                    .rounded(px(8.))
                                    .text_size(px(13.))
                                    .text_color(MUTED.color())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(Color::Hover.color()).text_color(TEXT.color()))
                                    .child(icon(GpuiIcons::LucideArrowLeft, 16.))
                                    .child(t("settings-back"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_settings(window, cx)
                                    })),
                            ),
                    )
                    .when(self.chrome.controls_width() > 0., |el| {
                        el.child(self.window_controls(window, cx))
                    }),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(
                        div()
                            .w(px(if compact { 176. } else { 224. }))
                            .flex_shrink_0()
                            .border_r_1()
                            .border_color(Color::Text.color().opacity(0.05))
                            .px(px(16.))
                            .py(px(24.))
                            .flex()
                            .flex_col()
                            .gap(px(26.))
                            .child(
                                div()
                                    .px(px(12.))
                                    .text_size(px(20.))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(t("settings")),
                            )
                            .child(
                                div().flex().flex_col().gap(px(4.)).children(
                                    [
                                        Section::General,
                                        Section::Appearance,
                                        #[cfg(any(unix, windows))]
                                        Section::Ai,
                                        Section::About,
                                    ]
                                    .into_iter()
                                    .map(|section| {
                                        let selected = panel.section == section;
                                        div()
                                            .id(section.key())
                                            .debug_selector(move || section.key().into())
                                            .h(px(40.))
                                            .px(px(12.))
                                            .flex()
                                            .items_center()
                                            .gap(px(10.))
                                            .rounded(px(9.))
                                            .text_size(px(13.))
                                            .cursor_pointer()
                                            .text_color(if selected {
                                                TEXT.color()
                                            } else {
                                                MUTED.color()
                                            })
                                            .when(selected, |el| el.bg(Color::Selected.color()))
                                            .when(!selected, |el| {
                                                el.hover(|s| {
                                                    s.bg(Color::Hover.color())
                                                        .text_color(TEXT.color())
                                                })
                                            })
                                            .child(icon(section.glyph(), 16.))
                                            .child(section.label())
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.select_settings_section(section, window, cx)
                                            }))
                                    }),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .id(("settings-content", panel.section as usize))
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_y_scroll()
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_center()
                                    .px(px(if compact { 28. } else { 56. }))
                                    .py(px(48.))
                                    .child(content),
                            ),
                    ),
            )
    }
}
