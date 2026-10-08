use super::*;
use uic::components::dropdown::{DropdownPlacement, dropdown};

impl Workspace {
    fn set_hover_exit(
        &mut self,
        id: usize,
        exit: HoverExit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview_read_only()
            || !self.layer_editable(id)
            || !self
                .hierarchy
                .interactions
                .get(&id)
                .is_some_and(|i| i.hover.is_some() && i.hover_exit != exit)
            || matches!(exit, HoverExit::ChangeVariant { target } if !self.prototype_variants(id).iter().any(|(id, _)| *id == target))
        {
            return;
        }
        self.suspend(window, cx);
        let before = self.snapshot_hierarchy();
        self.hierarchy.interactions.get_mut(&id).unwrap().hover_exit = exit;
        self.history.borrow_mut().record(vec![before], None);
        cx.notify();
    }

    pub(super) fn hover_exit_controls(
        &self,
        id: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let current = self.hierarchy.interactions[&id].hover_exit;
        let mut options = vec![
            (HoverExit::Restore, t("prototype-hover-restore").to_owned()),
            (HoverExit::Keep, t("prototype-hover-keep").to_owned()),
        ];
        options.extend(
            self.prototype_variants(id)
                .into_iter()
                .map(|(target, name)| {
                    (
                        HoverExit::ChangeVariant { target },
                        crate::i18n::message("prototype-change-variant", &[("name", name)]),
                    )
                }),
        );
        let label = options
            .iter()
            .find(|(value, _)| *value == current)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| t("prototype-missing-variant").into());
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .pt(px(6.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .child(t("prototype-hover-exit")),
            )
            .child(
                dropdown(&self.presentation.menus[2])
                    .placement(DropdownPlacement::BottomStart)
                    .w(px(230.))
                    .p(px(5.))
                    .rounded(px(10.))
                    .bg(Color::Input.color())
                    .border_1()
                    .border_color(BORDER.color())
                    .shadow_lg()
                    .text_size(px(12.))
                    .text_color(TEXT.color())
                    .trigger(
                        div()
                            .id("prototype-hover-exit-menu")
                            .debug_selector(|| "prototype-hover-exit-menu".into())
                            .h(px(32.))
                            .w_full()
                            .min_w_0()
                            .px(px(8.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(BORDER.color())
                            .bg(Color::Input.color())
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .cursor_pointer()
                            .text_size(px(12.))
                            .text_color(TEXT.color())
                            .hover(|s| s.bg(Color::Hover.color()))
                            .child(div().flex_1().min_w_0().truncate().child(label))
                            .child(icon(LucideIcons::ChevronDown, 12.)),
                    )
                    .menu(
                        div()
                            .id("prototype-hover-exit-options")
                            .max_h(px(300.))
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                            .children(options.into_iter().enumerate().map(
                                |(key, (value, label))| {
                                    div()
                                        .id(("prototype-exit-option", key))
                                        .debug_selector(move || {
                                            format!("prototype-exit-option-{key}")
                                        })
                                        .h(px(28.))
                                        .flex_shrink_0()
                                        .px(px(8.))
                                        .rounded(px(5.))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .cursor_pointer()
                                        .when(current == value, |el| {
                                            el.bg(Color::Selected.color())
                                                .text_color(ACCENT.color())
                                        })
                                        .hover(|s| s.bg(Color::Hover.color()))
                                        .child(
                                            div().w(px(14.)).children(
                                                (current == value)
                                                    .then(|| icon(LucideIcons::Check, 14.)),
                                            ),
                                        )
                                        .child(div().flex_1().min_w_0().truncate().child(label))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.presentation.menus[2]
                                                .update(cx, |menu, cx| menu.close(window, cx));
                                            this.set_hover_exit(id, value, window, cx);
                                        }))
                                },
                            )),
                    ),
            )
    }
}
