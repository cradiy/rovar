use super::*;

impl Workspace {
    pub(super) fn activate_multiple(
        &mut self,
        source: usize,
        actions: Vec<Action>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.presentation.playback.is_none() {
            return;
        }
        let mut page = self.snapshot_page_state(cx);
        let mut changed = false;
        let mut raised = self.presentation.raised.clone();
        let result = (|| -> anyhow::Result<()> {
            for action in &actions {
                match action {
                    Action::SetProperty { target, property } => {
                        property.apply(&mut page, *target)?;
                        changed = true;
                        if matches!(
                            property,
                            crate::scene::presentation::property::Property::Visible(_)
                        ) && let Some(group) = page
                            .hierarchy
                            .groups
                            .get(target)
                            .filter(|g| g.boolean.is_none())
                        {
                            raised.retain(|id| id != target);
                            if !group.layer.hidden {
                                raised.push(*target);
                            }
                        }
                    }
                    Action::ChangeVariant { target } => {
                        let root = self
                            .prototype_component(source)
                            .ok_or_else(|| anyhow::anyhow!("Missing prototype component"))?;
                        crate::scene::components::variants::switch(
                            &mut page,
                            root,
                            &target.to_string(),
                            &self.components.definitions,
                            &self.components.sets,
                        )?;
                        changed = true;
                    }
                    _ => (),
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            log::warn!("Could not apply prototype actions: {error:#}");
            return;
        }
        if changed {
            self.freeze_playback_visual(window, cx);
            self.apply_component_page(page, window, cx);
            self.presentation.raised = raised;
            self.presentation.playback.as_mut().unwrap().visual = transition::Visual::default();
            self.set_selection(BTreeSet::new(), cx);
        }
        for action in actions {
            if !matches!(
                action,
                Action::SetProperty { .. } | Action::ChangeVariant { .. }
            ) {
                self.activate_prototype(source, action, window, cx);
            }
        }
        cx.notify();
    }

    pub(in crate::editor) fn prototype_action_index(&self, id: usize, trigger: Trigger) -> usize {
        self.presentation
            .action
            .filter(|(generation, source, event, _)| {
                *generation == self.pages.generation && *source == id && *event == trigger
            })
            .map_or(0, |(_, _, _, index)| index)
    }

    pub(in crate::editor) fn prototype_action_ref(
        &self,
        id: usize,
        trigger: Trigger,
    ) -> Option<&Action> {
        let interaction = self.hierarchy.interactions.get(&id)?;
        let action = match trigger {
            Trigger::Click => &interaction.click,
            Trigger::Hover => &interaction.hover,
        };
        action
            .as_ref()?
            .steps()
            .get(self.prototype_action_index(id, trigger))
    }

    pub(in crate::editor) fn prototype_action(
        &self,
        id: usize,
        trigger: Trigger,
    ) -> Option<Action> {
        self.prototype_action_ref(id, trigger).cloned()
    }

    fn prototype_actions(&self, id: usize, trigger: Trigger) -> Vec<Action> {
        self.hierarchy
            .interactions
            .get(&id)
            .and_then(|i| i.get(trigger))
            .map_or_else(Vec::new, Action::into_steps)
    }

    fn select_action(
        &mut self,
        id: usize,
        trigger: Trigger,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_action_input(window, cx);
        self.suspend(window, cx);
        self.history.borrow_mut().break_group();
        self.presentation.action = Some((self.pages.generation, id, trigger, index));
        self.sync_action_inspector(window, cx);
        cx.notify();
    }

    pub(super) fn write_prototype_actions(
        &mut self,
        id: usize,
        trigger: Trigger,
        actions: Vec<Action>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.preview_read_only() || !self.layer_editable(id) {
            return;
        }
        self.suspend(window, cx);
        let before = self.snapshot_hierarchy();
        let interaction = self.hierarchy.interactions.entry(id).or_default();
        interaction.set(trigger, Action::from_steps(actions));
        if interaction.is_empty() {
            self.hierarchy.interactions.remove(&id);
        }
        self.history.borrow_mut().record(vec![before], None);
        cx.notify();
    }

    pub(super) fn action_cards(&self, id: usize, trigger: Trigger, cx: &mut Context<Self>) -> Div {
        let actions = self.prototype_actions(id, trigger);
        let index = self.prototype_action_index(id, trigger).min(actions.len());
        let count = actions.len().max(index + 1);
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(MUTED.color())
                            .child(t("prototype-actions")),
                    )
                    .child(
                        inspector::icon_button(
                            "prototype-add-action",
                            t("prototype-add-action"),
                            GpuiIcons::LucidePlus,
                            false,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                let count = this.prototype_actions(id, trigger).len();
                                this.select_action(id, trigger, count, window, cx);
                            },
                        )),
                    ),
            )
            .children((0..count).map(|row| {
                let action = actions.get(row);
                let label = if action.is_none() {
                    t("prototype-new-action")
                } else {
                    t(super::controls::action_label(&action.cloned()))
                };
                let detail = match action {
                    Some(Action::SetProperty { target, property }) => format!(
                        "{} · {}",
                        self.layer_name(*target, cx),
                        super::properties::label(property)
                    ),
                    Some(
                        Action::Navigate { target }
                        | Action::OpenOverlay { target, .. }
                        | Action::Media { target, .. }
                        | Action::ScrollTo { target },
                    ) => self.layer_name(*target, cx),
                    _ => String::new(),
                };
                let active = row == index;
                let mut card = div()
                    .id(("prototype-action-card", row))
                    .debug_selector(move || format!("prototype-action-card-{row}"))
                    .rounded(px(9.))
                    .border_1()
                    .border_color(if active {
                        Color::Selected.color()
                    } else {
                        BORDER.color()
                    })
                    .bg(Color::Input.color())
                    .child(
                        div()
                            .id(("prototype-action-select", row))
                            .debug_selector(move || format!("prototype-action-select-{row}"))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .px(px(10.))
                            .h(px(52.))
                            .cursor_pointer()
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(MUTED.color())
                                    .child(format!("{:02}", row + 1)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(12.))
                                    .text_color(TEXT.color())
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.))
                                    .child(label)
                                    .when(!detail.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .text_size(px(11.))
                                                .text_color(MUTED.color())
                                                .truncate()
                                                .child(detail),
                                        )
                                    }),
                            )
                            .when(action.is_some(), |el| {
                                el.child(
                                    inspector::icon_button(
                                        format!("prototype-action-up-{row}"),
                                        t("prototype-move-up"),
                                        GpuiIcons::LucideArrowUp,
                                        false,
                                    )
                                    .when(row == 0, |el| el.opacity(0.3))
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            cx.stop_propagation();
                                            if row > 0 {
                                                this.select_action(id, trigger, row, window, cx);
                                                let mut actions =
                                                    this.prototype_actions(id, trigger);
                                                actions.swap(row, row - 1);
                                                this.write_prototype_actions(
                                                    id, trigger, actions, window, cx,
                                                );
                                                this.select_action(
                                                    id,
                                                    trigger,
                                                    row - 1,
                                                    window,
                                                    cx,
                                                );
                                            }
                                        },
                                    )),
                                )
                                .child(
                                    inspector::icon_button(
                                        format!("prototype-action-remove-{row}"),
                                        t("prototype-remove-action"),
                                        GpuiIcons::LucideX,
                                        false,
                                    )
                                    .on_click(cx.listener(
                                        move |this, _, window, cx| {
                                            cx.stop_propagation();
                                            this.select_action(id, trigger, row, window, cx);
                                            let mut actions = this.prototype_actions(id, trigger);
                                            actions.remove(row);
                                            let next = row.min(actions.len().saturating_sub(1));
                                            this.write_prototype_actions(
                                                id, trigger, actions, window, cx,
                                            );
                                            this.select_action(id, trigger, next, window, cx);
                                        },
                                    )),
                                )
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_action(id, trigger, row, window, cx);
                            })),
                    );
                if active {
                    card = card.child(
                        div()
                            .px(px(10.))
                            .pb(px(12.))
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(self.action_editor(id, trigger, action.cloned(), cx)),
                    );
                }
                card
            }))
    }
}
