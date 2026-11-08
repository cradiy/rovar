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

    pub(super) fn prototype_actions(&self, id: usize, trigger: Trigger) -> Vec<Action> {
        self.hierarchy
            .interactions
            .get(&id)
            .and_then(|i| i.get(trigger))
            .map_or_else(Vec::new, Action::into_steps)
    }

    pub(super) fn select_action(
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
        self.presentation
            .editor
            .update(cx, |state, cx| state.open(window, cx));
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

    pub(super) fn remove_prototype_action(
        &mut self,
        id: usize,
        trigger: Trigger,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_action_input(window, cx);
        let mut actions = self.prototype_actions(id, trigger);
        if index >= actions.len() {
            return;
        }
        actions.remove(index);
        let empty = actions.is_empty();
        let next = index.min(actions.len().saturating_sub(1));
        self.write_prototype_actions(id, trigger, actions, window, cx);
        if empty {
            self.close_action_editor(window, cx);
        } else {
            self.select_action(id, trigger, next, window, cx);
        }
    }

    pub(super) fn move_prototype_action(
        &mut self,
        id: usize,
        trigger: Trigger,
        index: usize,
        up: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.commit_action_input(window, cx);
        let mut actions = self.prototype_actions(id, trigger);
        let next = if up {
            index.checked_sub(1)
        } else {
            index.checked_add(1)
        };
        let Some(next) = next.filter(|next| *next < actions.len() && index < actions.len()) else {
            return;
        };
        actions.swap(index, next);
        self.write_prototype_actions(id, trigger, actions, window, cx);
        self.select_action(id, trigger, next, window, cx);
    }

    pub(super) fn action_cards(&self, id: usize, trigger: Trigger, cx: &mut Context<Self>) -> Div {
        let actions = self.prototype_actions(id, trigger);
        let index = self.prototype_action_index(id, trigger);
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t("prototype-actions"))
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(MUTED.color())
                                    .child(actions.len().to_string()),
                            ),
                    )
                    .when(!actions.is_empty(), |el| {
                        el.child(self.presentation_menu(Some(id), controls::Menu::AddAction, cx))
                    }),
            )
            .children(actions.iter().enumerate().map(|(row, action)| {
                let active = row == index && self.presentation.editor.read(cx).is_open();
                let detail = self.action_summary(action, cx);
                let content = div()
                    .id(("prototype-action-select", row))
                    .debug_selector(move || format!("prototype-action-select-{row}"))
                    .h(px(56.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .border_l_2()
                    .border_color(if active {
                        ACCENT.color()
                    } else {
                        Color::Transparent.color()
                    })
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .hover(|s| s.bg(Color::Hover.color()))
                    .when(active, |el| el.bg(Color::Selected.color()))
                    .child(
                        icon(authoring::glyph(Some(action)), 16.).text_color(if active {
                            ACCENT.color()
                        } else {
                            MUTED.color()
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .truncate()
                                    .child(t(controls::action_label(&Some(action.clone())))),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(MUTED.color())
                                    .truncate()
                                    .child(detail),
                            ),
                    )
                    .child(icon(GpuiIcons::LucideEllipsis, 14.).text_color(MUTED.color()));
                let content = content.on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.select_action(id, trigger, row, window, cx);
                }));
                let row_element = if row == index {
                    self.action_row_popover(content, id, trigger, cx)
                } else {
                    content.into_any_element()
                };
                div()
                    .border_b_1()
                    .border_color(if active {
                        Color::Transparent.color()
                    } else {
                        BORDER.color()
                    })
                    .child(row_element)
            }))
            .when(actions.is_empty(), |el| {
                el.child(self.presentation_menu(Some(id), controls::Menu::AddAction, cx))
            })
    }

    fn action_summary(&self, action: &Action, cx: &gpui::App) -> String {
        match action {
            Action::SetProperty { target, property } => {
                use crate::scene::presentation::property::{Boolean, Property};
                let value = match property {
                    Property::Visible(value) => t(match value {
                        Boolean::Set(true) => "prototype-show",
                        Boolean::Set(false) => "prototype-hide",
                        Boolean::Toggle => "prototype-toggle",
                    })
                    .to_owned(),
                    Property::Opacity(value) => format!(
                        "{} {}%",
                        properties::label(property),
                        inspector::number(value * 100.)
                    ),
                    _ => properties::label(property).to_owned(),
                };
                format!("{} · {value}", self.layer_name(*target, cx))
            }
            Action::OpenOverlay { target, settings } => format!(
                "{} · {}",
                self.layer_name(*target, cx),
                t(match settings.position {
                    crate::scene::presentation::OverlayPosition::Center =>
                        "prototype-overlay-center",
                    crate::scene::presentation::OverlayPosition::Bottom =>
                        "prototype-overlay-bottom",
                    crate::scene::presentation::OverlayPosition::Right => "prototype-overlay-right",
                })
            ),
            Action::Navigate { target } | Action::ScrollTo { target } => {
                self.layer_name(*target, cx)
            }
            Action::Media { target, command } => format!(
                "{} · {}",
                self.layer_name(*target, cx),
                t(media::label(*command))
            ),
            Action::ChangeVariant { target } => self
                .components
                .sets
                .values()
                .find_map(|set| set.variants.get(&target.to_string()))
                .cloned()
                .unwrap_or_else(|| t("prototype-missing-variant").into()),
            _ => String::new(),
        }
    }
}
