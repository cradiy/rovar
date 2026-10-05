use super::*;

pub(in crate::editor) fn action(
    id: &'static str,
    label: &'static str,
    glyph: LucideIcons,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(32.))
        .min_w_0()
        .px(px(8.))
        .rounded(px(6.))
        .border_1()
        .border_color(BORDER.color())
        .bg(Color::Input.color())
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .cursor_pointer()
        .text_size(px(12.))
        .text_color(TEXT.color())
        .hover(|s| s.bg(Color::Hover.color()))
        .child(icon(glyph, 16.).flex_shrink_0())
        .child(div().min_w_0().truncate().child(label))
}

impl Workspace {
    pub(in crate::editor) fn composition_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let boolean = self.can_boolean();
        let mask = self.mask_controls(cx);
        if !boolean && mask.is_none() {
            return None;
        }
        Some(
            div()
                .debug_selector(|| "composition-controls".into())
                .px(px(14.))
                .py(px(10.))
                .border_b_1()
                .border_color(BORDER.color())
                .flex_shrink_0()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .when(boolean, |el| {
                            el.child(div().flex_1().min_w_0().child(self.boolean_menu(false, cx)))
                        })
                        .children(mask)
                        .when(self.selected_boolean().is_some() && boolean, |el| {
                            el.child(
                                action(
                                    "boolean-release",
                                    t("boolean-release"),
                                    LucideIcons::Ungroup,
                                )
                                .flex_shrink_0()
                                .on_click(cx.listener(
                                    |this, _, window, cx| {
                                        if this.selected_boolean().is_some() && this.can_boolean() {
                                            this.suspend(window, cx);
                                            this.ungroup_selection(cx);
                                        }
                                    },
                                )),
                            )
                        }),
                )
                .when(boolean && self.boolean_result_empty(), |el| {
                    el.child(
                        div()
                            .debug_selector(|| "boolean-empty-result".into())
                            .text_size(px(12.))
                            .text_color(MUTED.color())
                            .child(t("boolean-empty-result")),
                    )
                })
                .into_any_element(),
        )
    }
}
