use super::*;
use uic::components::dropdown::{DropdownPlacement, dropdown};

impl Workspace {
    pub(super) fn set_start(
        &mut self,
        start: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview_read_only()
            || self.hierarchy.start == start
            || start.is_some_and(|id| !self.boards.iter().any(|b| b.id == id))
        {
            return;
        }
        self.suspend(window, cx);
        let before = self.snapshot_hierarchy();
        self.hierarchy.start = start;
        self.history.borrow_mut().record(vec![before], None);
        cx.notify();
    }

    pub(super) fn set_click(
        &mut self,
        id: usize,
        action: Option<Action>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_prototype_action(id, Trigger::Click, action, window, cx);
    }

    pub(in crate::editor) fn set_prototype_action(
        &mut self,
        id: usize,
        trigger: Trigger,
        action: Option<Action>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview_read_only()
            || !self.layer_editable(id)
            || self
                .hierarchy
                .interactions
                .get(&id)
                .and_then(|i| i.get(trigger))
                == action
            || action.is_some_and(|a| {
                a.remap(|target| self.boards.iter().any(|b| b.id == target).then_some(target))
                    .is_none()
            })
            || matches!(action, Some(Action::ChangeVariant { target }) if !self.prototype_variants(id).iter().any(|(id, _)| *id == target))
        {
            return;
        }
        self.suspend(window, cx);
        let before = self.snapshot_hierarchy();
        let interaction = self.hierarchy.interactions.entry(id).or_default();
        interaction.set(trigger, action);
        if interaction.is_empty() {
            self.hierarchy.interactions.remove(&id);
        }
        self.history.borrow_mut().record(vec![before], None);
        cx.notify();
    }

    pub(in crate::editor) fn presentation_controls(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let ids = self.selection_ids();
        let selected = (ids.len() == 1).then(|| *ids.first().unwrap());
        let mut section = div()
            .id("prototype-controls")
            .debug_selector(|| "prototype-controls".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .child(
                inspector::inspector_section(t("prototype-start"))
                    .child(self.presentation_menu(None, cx)),
            );
        if let Some(id) = selected {
            section =
                section.child(
                    inspector::inspector_section(t("prototype-interaction"))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(MUTED.color())
                                .truncate()
                                .child(self.layer_name(id, cx)),
                        )
                        .child(div().flex().gap(px(4.)).children(
                            [Trigger::Click, Trigger::Hover].map(|trigger| {
                                let key = if trigger == Trigger::Click {
                                    "prototype-click"
                                } else {
                                    "prototype-hover"
                                };
                                div()
                                    .id(key)
                                    .debug_selector(move || key.into())
                                    .flex_1()
                                    .h(px(28.))
                                    .rounded(px(5.))
                                    .text_size(px(12.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .bg(if self.presentation.trigger == trigger {
                                        Color::Selected.color()
                                    } else {
                                        Color::Input.color()
                                    })
                                    .child(t(key))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.cancel_gesture(window, cx);
                                        this.presentation.trigger = trigger;
                                        cx.notify();
                                    }))
                            }),
                        ))
                        .child(self.presentation_menu(Some(id), cx)),
                );
        } else {
            section = section.child(
                div()
                    .p(px(14.))
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .child(t("prototype-select")),
            );
        }
        section
    }

    fn presentation_menu(
        &self,
        source: Option<usize>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let index = usize::from(source.is_some());
        let trigger = self.presentation.trigger;
        let action = source.and_then(|id| {
            self.hierarchy
                .interactions
                .get(&id)
                .and_then(|i| i.get(trigger))
        });
        let target = if source.is_none() {
            self.hierarchy.start
        } else if let Some(Action::Navigate { target }) = action {
            Some(target)
        } else {
            None
        };
        let empty = t(if source.is_none() {
            "prototype-auto"
        } else {
            "prototype-none"
        });
        let label = if let Some(Action::ChangeVariant { target }) = action {
            let name = self
                .components
                .sets
                .values()
                .find_map(|set| set.variants.get(&target.to_string()))
                .cloned()
                .unwrap_or_else(|| t("prototype-missing-variant").into());
            crate::i18n::message("prototype-change-variant", &[("name", name)])
        } else if action == Some(Action::Back) {
            t("prototype-back").to_owned()
        } else {
            target
                .and_then(|id| self.boards.iter().find(|b| b.id == id))
                .map_or_else(|| empty.to_owned(), |b| b.name.clone())
        };
        let mut options = vec![(0usize, empty.to_owned(), None)];
        if source.is_some() {
            options.push((1, t("prototype-back").to_owned(), Some(Action::Back)));
        }
        options.extend(
            self.boards
                .iter()
                .filter(|b| !b.layer.hidden)
                .enumerate()
                .map(|(i, b)| {
                    (
                        i + 2,
                        b.name.clone(),
                        Some(Action::Navigate { target: b.id }),
                    )
                }),
        );
        if let Some(source) = source {
            let start = options.len();
            options.extend(self.prototype_variants(source).into_iter().enumerate().map(
                |(i, (target, name))| {
                    (
                        start + i,
                        crate::i18n::message("prototype-change-variant", &[("name", name)]),
                        Some(Action::ChangeVariant { target }),
                    )
                },
            ));
        }
        dropdown(&self.presentation.menus[index])
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
                    .id(if source.is_some() {
                        "prototype-click-menu"
                    } else {
                        "prototype-start-menu"
                    })
                    .debug_selector(move || {
                        if source.is_some() {
                            "prototype-click-menu".into()
                        } else {
                            "prototype-start-menu".into()
                        }
                    })
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
                    .id(("prototype-options", index))
                    .max_h(px(300.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                    .children(options.into_iter().map(|(key, label, option)| {
                        let current = if source.is_some() {
                            option == action
                        } else {
                            option == target.map(|target| Action::Navigate { target })
                        };
                        div()
                            .id(("prototype-option", key))
                            .debug_selector(move || format!("prototype-option-{index}-{key}"))
                            .h(px(28.))
                            .flex_shrink_0()
                            .px(px(8.))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .when(current, |el| {
                                el.bg(Color::Selected.color()).text_color(ACCENT.color())
                            })
                            .hover(|s| s.bg(Color::Hover.color()))
                            .child(
                                div()
                                    .w(px(14.))
                                    .children(current.then(|| icon(LucideIcons::Check, 14.))),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(label))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.presentation.menus[index]
                                    .update(cx, |menu, cx| menu.close(window, cx));
                                if let Some(id) = source {
                                    if trigger == Trigger::Click {
                                        this.set_click(id, option, window, cx);
                                    } else {
                                        this.set_prototype_action(id, trigger, option, window, cx);
                                    }
                                } else {
                                    this.set_start(
                                        option.and_then(|a| {
                                            if let Action::Navigate { target } = a {
                                                Some(target)
                                            } else {
                                                None
                                            }
                                        }),
                                        window,
                                        cx,
                                    );
                                }
                            }))
                    })),
            )
    }
}
