use super::*;
use uic::components::dropdown::{DropdownPlacement, dropdown};

impl Workspace {
    pub(in crate::editor::presentation) fn set_prototype_behavior(
        &mut self,
        id: usize,
        scroll: ScrollAxis,
        fixed: bool,
        pass_through: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview_read_only()
            || !self.layer_editable(id)
            || (!scroll.is_none()
                && !self.boards.iter().any(|b| b.id == id)
                && !self.hierarchy.layouts.contains_key(&id))
        {
            return;
        }
        let old = self
            .hierarchy
            .interactions
            .get(&id)
            .cloned()
            .unwrap_or_default();
        if (old.scroll, old.fixed, old.pass_through) == (scroll, fixed, pass_through) {
            return;
        }
        self.suspend(window, cx);
        let before = self.snapshot_hierarchy();
        let settings = self.hierarchy.interactions.entry(id).or_default();
        settings.scroll = scroll;
        settings.fixed = fixed;
        settings.pass_through = pass_through;
        if settings.is_empty() {
            self.hierarchy.interactions.remove(&id);
        }
        self.history.borrow_mut().record(vec![before], None);
        cx.notify();
    }

    pub(in crate::editor::presentation) fn prototype_scroll_targets(
        &self,
        cx: &gpui::App,
    ) -> Vec<(usize, String)> {
        self.canvas_layer_order()
            .into_iter()
            .filter(|id| {
                self.ancestors(*id).iter().any(|parent| {
                    self.hierarchy
                        .interactions
                        .get(parent)
                        .is_some_and(|i| !i.scroll.is_none())
                })
            })
            .map(|id| (id, self.layer_name(id, cx)))
            .collect()
    }

    pub(in crate::editor::presentation) fn prototype_scroll_controls(
        &self,
        id: usize,
        cx: &mut Context<Self>,
    ) -> Div {
        let settings = self
            .hierarchy
            .interactions
            .get(&id)
            .cloned()
            .unwrap_or_default();
        let scroll = settings.scroll;
        let fixed = settings.fixed;
        let pass_through = settings.pass_through;
        let axes = [
            ("prototype-scroll-none", ScrollAxis::None),
            ("prototype-scroll-vertical", ScrollAxis::Vertical),
            ("prototype-scroll-horizontal", ScrollAxis::Horizontal),
            ("prototype-scroll-both", ScrollAxis::Both),
        ];
        super::super::authoring::section()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(t("prototype-behavior")),
            )
            .when(
                self.boards.iter().any(|b| b.id == id) || self.hierarchy.layouts.contains_key(&id),
                |el| {
                    el.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                super::super::authoring::caption(t("prototype-scroll")).w(px(76.)),
                            )
                            .child(
                                div().flex_1().min_w_0().child(
                                    dropdown(&self.presentation.menus[6])
                                        .placement(DropdownPlacement::BottomEnd)
                                        .w(px(176.))
                                        .p(px(5.))
                                        .bg(Color::Input.color())
                                        .text_color(TEXT.color())
                                        .border_color(BORDER.color())
                                        .rounded(px(8.))
                                        .trigger(
                                            div()
                                                .id("prototype-scroll-menu")
                                                .debug_selector(|| "prototype-scroll-menu".into())
                                                .h(px(34.))
                                                .px(px(10.))
                                                .rounded(px(6.))
                                                .bg(Color::Input.color())
                                                .flex()
                                                .items_center()
                                                .gap(px(8.))
                                                .text_size(px(12.))
                                                .child(
                                                    div().flex_1().child(t(axes
                                                        .iter()
                                                        .find(|(_, axis)| *axis == scroll)
                                                        .unwrap()
                                                        .0)),
                                                )
                                                .child(icon(GpuiIcons::LucideChevronDown, 12.)),
                                        )
                                        .menu(div().flex().flex_col().children(axes.map(
                                            |(key, axis)| {
                                                div()
                                                    .id(key)
                                                    .debug_selector(move || key.into())
                                                    .flex_shrink_0()
                                                    .min_w_0()
                                                    .h(px(28.))
                                                    .rounded(px(5.))
                                                    .text_size(px(11.))
                                                    .flex()
                                                    .items_center()
                                                    .px(px(8.))
                                                    .gap(px(8.))
                                                    .cursor_pointer()
                                                    .bg(if scroll == axis {
                                                        Color::Selected.color()
                                                    } else {
                                                        Color::Transparent.color()
                                                    })
                                                    .child(
                                                        icon(GpuiIcons::LucideCheck, 12.).opacity(
                                                            if scroll == axis { 1. } else { 0. },
                                                        ),
                                                    )
                                                    .child(t(key))
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.presentation.menus[6]
                                                                .update(cx, |state, cx| {
                                                                    state.close(window, cx)
                                                                });
                                                            this.set_prototype_behavior(
                                                                id,
                                                                axis,
                                                                fixed,
                                                                pass_through,
                                                                window,
                                                                cx,
                                                            );
                                                        },
                                                    ))
                                            },
                                        ))),
                                ),
                            ),
                    )
                },
            )
            .children(
                [
                    ("prototype-fixed", fixed),
                    ("prototype-pass-through", pass_through),
                ]
                .map(|(key, enabled)| {
                    super::super::authoring::toggle(key, t(key), enabled).on_click(cx.listener(
                        move |this, _, window, cx| {
                            this.set_prototype_behavior(
                                id,
                                scroll,
                                if key == "prototype-fixed" {
                                    !fixed
                                } else {
                                    fixed
                                },
                                if key == "prototype-pass-through" {
                                    !pass_through
                                } else {
                                    pass_through
                                },
                                window,
                                cx,
                            );
                        },
                    ))
                }),
            )
    }
}
