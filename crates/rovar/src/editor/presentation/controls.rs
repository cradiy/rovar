use super::*;
use uic::components::dropdown::{DropdownPlacement, dropdown};

fn action_label(action: Option<Action>) -> &'static str {
    match action {
        None => "prototype-none",
        Some(Action::Navigate { .. }) => "prototype-navigate",
        Some(Action::OpenOverlay { .. }) => "prototype-open-overlay",
        Some(Action::CloseOverlay) => "prototype-close-overlay",
        Some(Action::Back) => "prototype-back",
        Some(Action::ChangeVariant { .. }) => "prototype-variant-action",
    }
}

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
                    .child(self.presentation_menu(None, false, cx)),
            );
        if let Some(id) = selected {
            let trigger = self.presentation.trigger;
            let action = self
                .hierarchy
                .interactions
                .get(&id)
                .and_then(|i| i.get(trigger));
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
                                    .gap(px(6.))
                                    .cursor_pointer()
                                    .bg(if self.presentation.trigger == trigger {
                                        Color::Selected.color()
                                    } else {
                                        Color::Input.color()
                                    })
                                    .child(t(key))
                                    .when(
                                        self.hierarchy
                                            .interactions
                                            .get(&id)
                                            .and_then(|i| i.get(trigger))
                                            .is_some(),
                                        |el| {
                                            el.child(
                                                div()
                                                    .size(px(5.))
                                                    .rounded_full()
                                                    .bg(ACCENT.color()),
                                            )
                                        },
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.cancel_gesture(window, cx);
                                        this.presentation.trigger = trigger;
                                        cx.notify();
                                    }))
                            }),
                        ))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(MUTED.color())
                                .child(t("prototype-action")),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .child(div().flex_1().min_w_0().child(self.presentation_menu(
                                    Some(id),
                                    false,
                                    cx,
                                )))
                                .when(action.is_some(), |row| {
                                    row.child(
                                        inspector::icon_button(
                                            "prototype-disconnect",
                                            t("prototype-disconnect"),
                                            LucideIcons::Unlink,
                                            false,
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                this.set_prototype_action(
                                                    id, trigger, None, window, cx,
                                                );
                                            }),
                                        ),
                                    )
                                }),
                        )
                        .when(
                            matches!(
                                action,
                                Some(
                                    Action::Navigate { .. }
                                        | Action::OpenOverlay { .. }
                                        | Action::ChangeVariant { .. }
                                )
                            ),
                            |el| {
                                el.child(div().text_size(px(12.)).text_color(MUTED.color()).child(
                                    t(if matches!(action, Some(Action::ChangeVariant { .. })) {
                                        "prototype-variant-target"
                                    } else {
                                        "prototype-frame-target"
                                    }),
                                ))
                                .child(self.presentation_menu(Some(id), true, cx))
                            },
                        )
                        .child(self.overlay_controls(id, trigger, cx))
                        .when(action.is_some(), |el| {
                            el.child(self.transition_controls(id, trigger, cx))
                        })
                        .when(trigger == Trigger::Hover && action.is_some(), |el| {
                            el.child(self.hover_exit_controls(id, cx))
                        }),
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
        target_menu: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let index = if target_menu {
            4
        } else {
            usize::from(source.is_some())
        };
        let trigger = self.presentation.trigger;
        let action = source.and_then(|id| {
            self.hierarchy
                .interactions
                .get(&id)
                .and_then(|i| i.get(trigger))
        });
        let target = if source.is_none() {
            self.hierarchy.start
        } else if let Some(Action::Navigate { target } | Action::OpenOverlay { target, .. }) =
            action
        {
            Some(target)
        } else {
            None
        };
        let empty = t(if source.is_none() {
            "prototype-auto"
        } else {
            "prototype-none"
        });
        let label = if source.is_some() && !target_menu {
            t(action_label(action)).to_owned()
        } else if let Some(Action::ChangeVariant { target }) = action {
            self.components
                .sets
                .values()
                .find_map(|set| set.variants.get(&target.to_string()))
                .cloned()
                .unwrap_or_else(|| t("prototype-missing-variant").into())
        } else {
            target
                .and_then(|id| self.boards.iter().find(|b| b.id == id))
                .map_or_else(|| empty.to_owned(), |b| b.name.clone())
        };
        let mut options = Vec::new();
        if source.is_none() {
            options.push((0usize, empty.to_owned(), None));
        }
        if source.is_none()
            || (target_menu && !matches!(action, Some(Action::ChangeVariant { .. })))
        {
            options.extend(
                self.boards
                    .iter()
                    .filter(|b| !b.layer.hidden)
                    .enumerate()
                    .map(|(i, b)| {
                        (
                            i + 2,
                            b.name.clone(),
                            Some(match action {
                                Some(Action::OpenOverlay { settings, .. }) => Action::OpenOverlay {
                                    target: b.id,
                                    settings,
                                },
                                _ => Action::Navigate { target: b.id },
                            }),
                        )
                    }),
            );
        }
        if let Some(source) = source {
            let variants = self.prototype_variants(source);
            if target_menu && matches!(action, Some(Action::ChangeVariant { .. })) {
                options.extend(
                    variants.into_iter().enumerate().map(|(i, (target, name))| {
                        (i, name, Some(Action::ChangeVariant { target }))
                    }),
                );
            } else if !target_menu {
                options.push((0, t("prototype-none").to_owned(), None));
                options.push((1, t("prototype-back").to_owned(), Some(Action::Back)));
                let board = target
                    .filter(|id| self.boards.iter().any(|b| b.id == *id && !b.layer.hidden))
                    .or_else(|| {
                        self.boards
                            .iter()
                            .find(|b| {
                                !b.layer.hidden
                                    && b.id != source
                                    && !self.ancestors(source).contains(&b.id)
                            })
                            .map(|b| b.id)
                    })
                    .or_else(|| self.boards.iter().find(|b| !b.layer.hidden).map(|b| b.id));
                if let Some(target) = board {
                    options.push((
                        2,
                        t("prototype-navigate").to_owned(),
                        Some(Action::Navigate { target }),
                    ));
                    let settings = match action {
                        Some(Action::OpenOverlay { settings, .. }) => settings,
                        _ => Default::default(),
                    };
                    options.push((
                        3,
                        t("prototype-open-overlay").to_owned(),
                        Some(Action::OpenOverlay { target, settings }),
                    ));
                }
                options.push((
                    4,
                    t("prototype-close-overlay").to_owned(),
                    Some(Action::CloseOverlay),
                ));
                let variant = match action {
                    Some(Action::ChangeVariant { target }) => Some(target),
                    _ => variants.first().map(|(id, _)| *id),
                };
                if let Some(target) = variant {
                    options.push((
                        5,
                        t("prototype-variant-action").to_owned(),
                        Some(Action::ChangeVariant { target }),
                    ));
                }
            }
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
                    .id(if target_menu {
                        "prototype-target-menu"
                    } else if source.is_some() {
                        "prototype-click-menu"
                    } else {
                        "prototype-start-menu"
                    })
                    .debug_selector(move || {
                        if target_menu {
                            "prototype-target-menu".into()
                        } else if source.is_some() {
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
                        let current = if source.is_some() && !target_menu {
                            action_label(option) == action_label(action)
                        } else if source.is_some() {
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
