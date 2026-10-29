use super::*;

impl Workspace {
    pub(in crate::editor) fn sync_text_background(&mut self, cx: &mut Context<Self>) {
        if let Some(color) = self
            .selected_text()
            .and_then(|text| text.editor.read(cx).effective_style().background)
        {
            let old = self.inspector.background_picker.read(cx).value();
            if [
                old.r - color.r,
                old.g - color.g,
                old.b - color.b,
                old.a - color.a,
            ]
            .into_iter()
            .any(|delta| delta.abs() > 0.000001)
            {
                self.inspector
                    .background_picker
                    .update(cx, |picker, cx| picker.set_value(color, cx));
            }
        }
    }

    pub(in crate::editor) fn text_background_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let enabled = self.selected_text().is_some_and(|text| {
            let editor = text.editor.read(cx);
            editor.effective_style().background.is_some()
                || editor.mixed(TextProperty::BackgroundColor)
        });
        section_body()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t("text-background")),
                    )
                    .child(
                        icon_button(
                            "text-background-toggle",
                            t(if enabled {
                                "text-background-remove"
                            } else {
                                "text-background-add"
                            }),
                            if enabled {
                                GpuiIcons::LucideMinus
                            } else {
                                GpuiIcons::LucidePlus
                            },
                            false,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.inspector.paint_popovers[2]
                                    .update(cx, |state, cx| state.close(window, cx));
                                this.change_text_style(
                                    StyleChange::Background((!enabled).then(|| rgb(0xffe066))),
                                    cx,
                                );
                                this.sync_fields(cx);
                            },
                        )),
                    ),
            )
            .when(enabled, |el| {
                el.child(
                    div()
                        .flex()
                        .gap(px(4.))
                        .child(self.property_field(22, t("color-hex"), cx))
                        .child(
                            div()
                                .w(px(76.))
                                .child(self.property_field(23, t("opacity"), cx)),
                        ),
                )
            })
    }

    pub(super) fn text_background_panel(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        layers::glass_surface()
            .w(px(300.))
            .p(px(14.))
            .rounded(px(12.))
            .border_1()
            .border_color(BORDER.color())
            .shadow_lg()
            .text_color(TEXT.color())
            .text_size(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(t("text-background"))
                    .child(
                        icon_button(
                            "text-background-close",
                            t("close"),
                            GpuiIcons::LucideX,
                            false,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.inspector.paint_popovers[2]
                                .update(cx, |state, cx| state.close(window, cx));
                        })),
                    ),
            )
            .child(inspector_color_picker(&self.inspector.background_picker))
            .child(
                uic::components::color_picker::AlphaSlider::new(&self.inspector.background_picker)
                    .h(px(12.)),
            )
            .child(
                div()
                    .flex()
                    .gap(px(4.))
                    .child(self.paint_input_field(22, t("color-hex"), cx))
                    .child(
                        div()
                            .w(px(76.))
                            .child(self.paint_input_field(23, t("opacity"), cx)),
                    ),
            )
    }
}
