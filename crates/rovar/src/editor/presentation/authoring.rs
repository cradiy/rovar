use super::*;
use uic::components::popover::{Popover, PopoverPlacement};

pub(super) fn section() -> Div {
    div()
        .mx(px(16.))
        .py(px(18.))
        .flex_shrink_0()
        .border_t_1()
        .border_color(BORDER.color())
        .flex()
        .flex_col()
        .gap(px(16.))
}

pub(super) fn caption(label: &'static str) -> Div {
    div()
        .text_size(px(11.))
        .text_color(MUTED.color())
        .child(label)
}

pub(super) fn toggle(key: &'static str, label: &'static str, enabled: bool) -> gpui::Stateful<Div> {
    div()
        .id(key)
        .debug_selector(move || key.into())
        .h(px(34.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(12.))
        .text_size(px(12.))
        .cursor_pointer()
        .child(div().flex_1().min_w_0().child(label))
        .child(
            div()
                .w(px(30.))
                .h(px(18.))
                .p(px(2.))
                .flex_shrink_0()
                .rounded_full()
                .bg(if enabled {
                    ACCENT.color()
                } else {
                    Color::Hover.color()
                })
                .flex()
                .items_center()
                .when(enabled, |el| el.justify_end())
                .child(div().size(px(14.)).rounded_full().bg(if enabled {
                    Color::Panel.color()
                } else {
                    MUTED.color()
                })),
        )
}

pub(super) fn glyph(action: Option<&Action>) -> GpuiIcons {
    match action {
        Some(Action::SetProperty { .. }) => GpuiIcons::LucideSlidersHorizontal,
        Some(Action::ChangeVariant { .. }) => GpuiIcons::LucideDiamond,
        Some(Action::Navigate { .. }) => GpuiIcons::LucideArrowRight,
        Some(Action::Back) => GpuiIcons::LucideArrowLeft,
        Some(Action::OpenOverlay { .. }) => GpuiIcons::LucideCopy,
        Some(Action::CloseOverlay) => GpuiIcons::LucideX,
        Some(Action::ScrollTo { .. }) => GpuiIcons::LucideArrowDownToLine,
        Some(Action::Media { .. }) => GpuiIcons::LucidePlay,
        _ => GpuiIcons::LucidePlus,
    }
}

impl Workspace {
    pub(in crate::editor) fn prototype_editor_focus(&self, cx: &gpui::App) -> Option<FocusHandle> {
        self.presentation
            .editor
            .read(cx)
            .is_open()
            .then(|| self.presentation.editor.focus_handle(cx))
    }

    pub(in crate::editor) fn sync_prototype_authoring(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.presentation.editor.read(cx).is_open()
            && (!self.presentation.tab
                || self
                    .presentation
                    .action
                    .is_none_or(|(generation, source, trigger, _)| {
                        generation != self.pages.generation
                            || self.selection_ids() != BTreeSet::from([source])
                            || trigger != self.presentation.trigger
                    }))
        {
            self.close_action_editor(window, cx);
        }
        if !self.presentation.tab || self.selection_ids().len() != 1 {
            self.presentation
                .transition_editor
                .update(cx, |state, cx| state.close(window, cx));
        }
    }

    pub(super) fn close_action_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_action_input(window, cx);
        for menu in &self.presentation.menus {
            menu.update(cx, |state, cx| state.close(window, cx));
        }
        for popover in &self.inspector.paint_popovers {
            popover.update(cx, |state, cx| state.close(window, cx));
        }
        self.presentation
            .editor
            .update(cx, |state, cx| state.close(window, cx));
        cx.notify();
    }

    pub(super) fn action_popover(
        &self,
        id: usize,
        trigger: Trigger,
        height: Pixels,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let index = self.prototype_action_index(id, trigger);
        let action = self.prototype_action(id, trigger);
        let count = self.prototype_actions(id, trigger).len();
        div()
            .id("prototype-action-editor")
            .debug_selector(|| "prototype-action-editor".into())
            .w(px(304.))
            .max_h(height.min(px(620.)))
            .flex()
            .flex_col()
            .child(
                div()
                    .p(px(12.))
                    .border_b_1()
                    .border_color(BORDER.color())
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icon(glyph(action.as_ref()), 16.).text_color(ACCENT.color()))
                    .child(div().flex_1().min_w_0().child(self.presentation_menu(
                        Some(id),
                        controls::Menu::Action,
                        cx,
                    )))
                    .child(
                        inspector::icon_button(
                            "prototype-editor-close",
                            t("close"),
                            GpuiIcons::LucideX,
                            false,
                        )
                        .size(px(24.))
                        .on_click(
                            cx.listener(|this, _, window, cx| this.close_action_editor(window, cx)),
                        ),
                    ),
            )
            .child(
                div()
                    .id("prototype-editor-fields")
                    .max_h((height - px(150.)).max(px(80.)).min(px(490.)))
                    .overflow_y_scroll()
                    .p(px(16.))
                    .child(self.action_editor(id, trigger, action, cx)),
            )
            .child(
                div()
                    .px(px(12.))
                    .py(px(8.))
                    .border_t_1()
                    .border_color(BORDER.color())
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .id("prototype-remove-action")
                            .debug_selector(|| "prototype-remove-action".into())
                            .h(px(30.))
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(11.))
                            .text_color(Color::Danger.color())
                            .cursor_pointer()
                            .child(icon(GpuiIcons::LucideTrash2, 14.))
                            .child(t("prototype-remove-action"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.remove_prototype_action(id, trigger, index, window, cx)
                            })),
                    )
                    .children(
                        [
                            (true, GpuiIcons::LucideArrowUp, "prototype-move-up"),
                            (false, GpuiIcons::LucideArrowDown, "prototype-move-down"),
                        ]
                        .map(|(up, glyph, label)| {
                            let enabled = if up { index > 0 } else { index + 1 < count };
                            inspector::icon_button(
                                format!(
                                    "prototype-action-{}-{index}",
                                    if up { "up" } else { "down" }
                                ),
                                t(label),
                                glyph,
                                false,
                            )
                            .size(px(28.))
                            .opacity(if enabled { 1. } else { 0.3 })
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    if enabled {
                                        this.move_prototype_action(
                                            id, trigger, index, up, window, cx,
                                        );
                                    }
                                },
                            ))
                        }),
                    ),
            )
    }

    pub(super) fn action_row_popover(
        &self,
        row: gpui::Stateful<Div>,
        id: usize,
        trigger: Trigger,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let weak = cx.entity().downgrade();
        Popover::new(&self.presentation.editor)
            .placement(PopoverPlacement::LeftStart)
            .gap(px(28.))
            .priority(80)
            .close_on_outside(false)
            .p_0()
            .rounded(px(12.))
            .border_1()
            .border_color(BORDER.color())
            .bg(Color::Surface.color())
            .text_color(TEXT.color())
            .shadow_lg()
            .trigger(row)
            .content(move |window, cx| {
                div().children(
                    weak.update(cx, |this, cx| {
                        this.action_popover(
                            id,
                            trigger,
                            window.viewport_size().height - px(32.),
                            cx,
                        )
                    })
                    .ok(),
                )
            })
            .into_any_element()
    }

    pub(super) fn transition_summary(
        &self,
        id: usize,
        trigger: Trigger,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let current = self.prototype_transition(id, trigger);
        let label = t(match current {
            Transition::Instant => "prototype-instant",
            Transition::Dissolve { .. } => "prototype-dissolve",
            Transition::Blur { .. } => "prototype-blur",
            Transition::Wipe { .. } => "prototype-wipe",
        });
        let summary = current
            .duration_ms()
            .map_or_else(|| label.to_owned(), |ms| format!("{label} · {ms} ms"));
        let weak = cx.entity().downgrade();
        Popover::new(&self.presentation.transition_editor)
            .close_on_outside(false)
            .placement(PopoverPlacement::LeftStart)
            .gap(px(28.))
            .priority(80)
            .p(px(16.))
            .w(px(304.))
            .bg(Color::Surface.color())
            .text_color(TEXT.color())
            .border_color(BORDER.color())
            .rounded(px(12.))
            .trigger(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(caption(t("prototype-transition")).w(px(76.)))
                    .child(
                        div()
                            .id("prototype-transition-menu")
                            .debug_selector(|| "prototype-transition-menu".into())
                            .flex_1()
                            .min_w_0()
                            .h(px(34.))
                            .px(px(10.))
                            .rounded(px(6.))
                            .bg(Color::Input.color())
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(11.))
                            .child(div().flex_1().min_w_0().truncate().child(summary))
                            .child(icon(GpuiIcons::LucideChevronDown, 12.)),
                    ),
            )
            .content(move |_, cx| {
                div().children(
                    weak.update(cx, |this, cx| this.transition_controls(id, trigger, cx))
                        .ok(),
                )
            })
    }
}
