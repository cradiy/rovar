use super::*;
use crate::scene::effects::{MAX_OFFSET, MAX_RADIUS, MAX_SHADOWS, Shadow};
use crate::ui::theme::Color;
use std::collections::BTreeSet;
use uic::components::popover::PopoverState;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(in crate::editor) struct Controls {
    target: (String, BTreeSet<usize>),
    rows: Vec<Row>,
    expanded: Option<usize>,
}

struct Row {
    color: gpui::Rgba,
    inputs: [Entity<TextInput>; 6],
    picker: Entity<ColorPickerState>,
    popover: Entity<PopoverState>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub(in crate::editor) fn close_shadow_menus(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for row in &self.inspector.shadows.rows {
            row.popover.update(cx, |p, cx| p.close(window, cx));
        }
    }

    pub(in crate::editor) fn shadow_number(&self, index: usize, field: usize) -> Option<f32> {
        let shadows = self.common_shadows()?;
        let shadow = shadows.get(index)?;
        match field {
            0 => Some(shadow.x),
            1 => Some(shadow.y),
            2 => Some(shadow.blur),
            3 => Some(shadow.spread),
            5 => Some(shadow.color.a * 100.),
            _ => None,
        }
    }

    fn begin_shadow_scrub(
        &mut self,
        index: usize,
        field: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some()
            || !self
                .shadow_targets()
                .iter()
                .any(|id| self.layer_editable(*id))
        {
            return;
        }
        let pending = self
            .inspector
            .shadows
            .rows
            .iter()
            .enumerate()
            .find_map(|(index, row)| {
                row.inputs
                    .iter()
                    .position(|input| input.focus_handle(cx).is_focused(window))
                    .map(|field| (index, field))
            });
        if let Some((index, field)) = pending {
            self.apply_shadow_field(index, field, cx);
        }
        self.focus.focus(window, cx);
        let Some(original) = self.shadow_number(index, field) else {
            return;
        };
        self.begin(
            GestureKind::ShadowProperty {
                index,
                field,
                original,
            },
            event.position,
            event.button,
            window,
            cx,
        );
        self.history.borrow_mut().begin_preview();
    }

    pub(in crate::editor) fn scrub_shadow_number(
        &mut self,
        index: usize,
        field: usize,
        original: f32,
        delta: f32,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        if delta.abs() < 3. && self.shadow_number(index, field) == Some(original) {
            return;
        }
        let value = number(original + (delta * if shift { 10. } else { 1. }).round());
        let Some(row) = self.inspector.shadows.rows.get(index) else {
            return;
        };
        row.inputs[field].update(cx, |input, cx| input.set_value(value, cx));
        self.apply_shadow_field(index, field, cx);
    }
    fn shadow_targets(&self) -> BTreeSet<usize> {
        self.selection_ids()
            .into_iter()
            .filter(|id| {
                !self.hierarchy.groups.contains_key(id)
                    && !self
                        .shapes
                        .iter()
                        .any(|s| s.id == *id && s.kind == ShapeKind::Video)
            })
            .collect()
    }

    fn common_shadows(&self) -> Option<Vec<Shadow>> {
        let ids = self.shadow_targets();
        let mut values = ids
            .iter()
            .map(|id| self.hierarchy.shadows.get(id).cloned().unwrap_or_default());
        let first = values.next().unwrap_or_default();
        values.all(|value| value == first).then_some(first)
    }

    pub(in crate::editor) fn shadow_padding(&self, id: usize) -> f32 {
        self.hierarchy
            .shadows
            .get(&id)
            .into_iter()
            .flatten()
            .filter(|s| s.visible())
            .map(Shadow::padding)
            .fold(0., f32::max)
            * self.view.zoom
    }

    fn edit_shadows(
        &mut self,
        group: Option<Group>,
        edit: impl Fn(&mut Vec<Shadow>),
        cx: &mut Context<Self>,
    ) {
        let before = self.snapshot_hierarchy();
        for id in self.shadow_targets() {
            if !self.layer_editable(id) {
                continue;
            }
            let shadows = self.hierarchy.shadows.entry(id).or_default();
            edit(shadows);
            if shadows.is_empty() {
                self.hierarchy.shadows.remove(&id);
            }
        }
        if let Change::Hierarchy { value, .. } = &before
            && *value != self.hierarchy
        {
            if group.is_none() {
                self.history.borrow_mut().break_group();
            }
            self.history.borrow_mut().record(vec![before], group);
            cx.notify();
        }
    }

    fn apply_shadow_field(&mut self, index: usize, field: usize, cx: &mut Context<Self>) {
        if self.inspector.shadows.target != (self.pages.active.clone(), self.shadow_targets()) {
            return;
        }
        let Some(row) = self.inspector.shadows.rows.get(index) else {
            return;
        };
        let value = row.inputs[field].read(cx).value().to_string();
        let Some(mut shadow) = self.common_shadows().and_then(|s| s.get(index).cloned()) else {
            return;
        };
        if field == 4 {
            let hex = value.trim().trim_start_matches('#');
            if hex.len() == 6
                && let Ok(value) = u32::from_str_radix(hex, 16)
            {
                let color = rgb(value);
                shadow.color.r = color.r;
                shadow.color.g = color.g;
                shadow.color.b = color.b;
            }
        } else if let Ok(value) = value.trim().parse::<f32>()
            && value.is_finite()
        {
            match field {
                0 => shadow.x = value.clamp(-MAX_OFFSET, MAX_OFFSET),
                1 => shadow.y = value.clamp(-MAX_OFFSET, MAX_OFFSET),
                2 => shadow.blur = value.clamp(0., MAX_RADIUS),
                3 => shadow.spread = value.clamp(-MAX_RADIUS, MAX_RADIUS),
                5 => shadow.color.a = (value / 100.).clamp(0., 1.),
                _ => {}
            }
        }
        self.edit_shadows(
            None,
            |shadows| {
                if let Some(s) = shadows.get_mut(index) {
                    *s = shadow.clone();
                }
            },
            cx,
        );
        self.refresh_shadow_fields(cx);
    }

    fn refresh_shadow_fields(&mut self, cx: &mut Context<Self>) {
        let shadows = self.common_shadows().unwrap_or_default();
        for (row, shadow) in self.inspector.shadows.rows.iter_mut().zip(shadows) {
            if row.color != shadow.color {
                row.color = shadow.color;
                row.picker
                    .update(cx, |picker, cx| picker.set_value(shadow.color, cx));
            }
            for (input, value) in row.inputs.iter().zip([
                number(shadow.x),
                number(shadow.y),
                number(shadow.blur),
                number(shadow.spread),
                hex(shadow.color),
                number(shadow.color.a * 100.),
            ]) {
                input.update(cx, |input, cx| {
                    if input.value().as_ref() != value {
                        input.set_value(value, cx);
                    }
                });
            }
        }
    }

    pub(in crate::editor) fn sync_shadow_controls(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = (self.pages.active.clone(), self.shadow_targets());
        let shadows = self.common_shadows().unwrap_or_default();
        let changed = self.inspector.shadows.target != target
            || self.inspector.shadows.rows.len() != shadows.len();
        if changed {
            for row in &self.inspector.shadows.rows {
                row.popover.update(cx, |p, cx| p.close(window, cx));
            }
            let expanded = (self.inspector.shadows.target == target)
                .then_some(self.inspector.shadows.expanded)
                .flatten();
            self.inspector.shadows = Controls {
                target,
                expanded,
                ..Default::default()
            };
            for (index, shadow) in shadows.iter().enumerate() {
                let mut subscriptions = Vec::new();
                let inputs = std::array::from_fn(|field| {
                    let input = cx.new(TextInput::new);
                    subscriptions.push(cx.subscribe(
                        &input,
                        move |this, _, event: &InputEvent, cx| {
                            if matches!(event, InputEvent::Submit(_)) {
                                this.apply_shadow_field(index, field, cx);
                            }
                        },
                    ));
                    subscriptions.push(cx.on_blur(
                        &input.focus_handle(cx),
                        window,
                        move |this, _, cx| this.apply_shadow_field(index, field, cx),
                    ));
                    input
                });
                let picker = cx.new(|cx| ColorPickerState::new(shadow.color, cx));
                subscriptions.push(cx.subscribe(
                    &picker,
                    move |this, _, event: &ColorPickerEvent, cx| {
                        let (ColorPickerEvent::Preview(color) | ColorPickerEvent::Commit(color)) =
                            *event;
                        let ids = this.shadow_targets().into_iter().collect();
                        this.edit_shadows(
                            Some(Group::Shadow(ids, index, 4)),
                            |shadows| {
                                if let Some(s) = shadows.get_mut(index) {
                                    s.color = color;
                                }
                            },
                            cx,
                        );
                        if matches!(event, ColorPickerEvent::Commit(_)) {
                            this.history.borrow_mut().break_group();
                        }
                        this.refresh_shadow_fields(cx);
                    },
                ));
                self.inspector.shadows.rows.push(Row {
                    color: shadow.color,
                    inputs,
                    picker,
                    popover: cx.new(|cx| PopoverState::new(window, cx)),
                    _subscriptions: subscriptions,
                });
            }
        }
        let editing = self.inspector.shadows.rows.iter().any(|row| {
            row.inputs
                .iter()
                .any(|input| input.focus_handle(cx).is_focused(window))
        });
        if changed || !editing {
            self.refresh_shadow_fields(cx);
        }
    }

    pub(in crate::editor) fn shadow_controls(&self, cx: &mut Context<Self>) -> Div {
        if self.shadow_targets().is_empty() {
            return div();
        }
        let shadows = self.common_shadows();
        div()
            .flex_shrink_0()
            .px(px(14.))
            .py(px(10.))
            .border_b_1()
            .border_color(BORDER.color())
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t("effects")),
                    )
                    .child(button("shadow-add", LucideIcons::Plus).when(
                        shadows.as_ref().is_some_and(|s| s.len() < MAX_SHADOWS),
                        |el| {
                            el.on_click(cx.listener(|this, _, _, cx| {
                                let index = this.common_shadows().unwrap_or_default().len();
                                this.edit_shadows(
                                    None,
                                    |s| {
                                        if s.len() < MAX_SHADOWS {
                                            s.push(Shadow::default());
                                        }
                                    },
                                    cx,
                                );
                                this.inspector.shadows.expanded = Some(index);
                            }))
                        },
                    )),
            )
            .when(shadows.is_none(), |el| {
                el.child(
                    div()
                        .text_size(px(12.))
                        .text_color(MUTED.color())
                        .child(t("effects-mixed")),
                )
            })
            .children(
                shadows
                    .unwrap_or_default()
                    .iter()
                    .enumerate()
                    .map(|(index, shadow)| self.shadow_row(index, shadow, cx)),
            )
    }

    fn shadow_row(&self, index: usize, shadow: &Shadow, cx: &mut Context<Self>) -> Div {
        let row = &self.inspector.shadows.rows[index];
        let picker = row.picker.clone();
        let expanded = self.inspector.shadows.expanded == Some(index);
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .id(format!("shadow-edit-{index}"))
                            .flex_1()
                            .min_w_0()
                            .h(px(30.))
                            .px(px(6.))
                            .rounded(px(5.))
                            .cursor_pointer()
                            .hover(|s| s.bg(BORDER.color()))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                icon(
                                    if expanded {
                                        LucideIcons::ChevronDown
                                    } else {
                                        LucideIcons::ChevronRight
                                    },
                                    14.,
                                )
                                .text_color(MUTED.color()),
                            )
                            .child(div().text_size(px(12.)).child(t("effect-drop-shadow")))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.inspector.shadows.expanded = (!expanded).then_some(index);
                                cx.notify();
                            })),
                    )
                    .child(
                        button(
                            format!("shadow-toggle-{index}"),
                            if shadow.enabled {
                                LucideIcons::Eye
                            } else {
                                LucideIcons::EyeOff
                            },
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_shadows(
                                None,
                                |s| {
                                    if let Some(s) = s.get_mut(index) {
                                        s.enabled = !s.enabled;
                                    }
                                },
                                cx,
                            )
                        })),
                    )
                    .child(
                        button(format!("shadow-remove-{index}"), LucideIcons::Minus).on_click(
                            cx.listener(move |this, _, _, cx| {
                                this.edit_shadows(
                                    None,
                                    |s| {
                                        if index < s.len() {
                                            s.remove(index);
                                        }
                                    },
                                    cx,
                                )
                            }),
                        ),
                    ),
            )
            .when(expanded, |el| {
                el.child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child(self.shadow_input(index, 0, "X", cx))
                        .child(self.shadow_input(index, 1, "Y", cx)),
                )
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child(self.shadow_input(index, 2, t("effect-blur"), cx))
                        .child(self.shadow_input(index, 3, t("effect-spread"), cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            Popover::new(&row.popover)
                                .placement(PopoverPlacement::LeftStart)
                                .gap(px(20.))
                                .p_0()
                                .border_0()
                                .bg(Color::Transparent.color())
                                .trigger(
                                    div()
                                        .id(format!("shadow-color-{index}"))
                                        .size(px(28.))
                                        .rounded(px(5.))
                                        .border_1()
                                        .border_color(BORDER.color())
                                        .bg(shadow.color)
                                        .cursor_pointer(),
                                )
                                .content(move |_, _| {
                                    layers::glass_surface()
                                        .w(px(256.))
                                        .p(px(14.))
                                        .rounded(px(12.))
                                        .border_1()
                                        .border_color(BORDER.color())
                                        .shadow_lg()
                                        .text_color(TEXT.color())
                                        .child(inspector_color_picker(&picker))
                                }),
                        )
                        .child(self.shadow_input(index, 4, "#", cx))
                        .child(self.shadow_input(index, 5, "%", cx)),
                )
            })
    }

    fn shadow_input(
        &self,
        index: usize,
        field: usize,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .debug_selector(move || format!("shadow-input-{index}-{field}"))
            .flex_1()
            .min_w_0()
            .h(px(30.))
            .px(px(8.))
            .rounded(px(5.))
            .bg(Color::Input.color())
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .id(format!("shadow-drag-{index}-{field}"))
                    .debug_selector(move || format!("shadow-drag-{index}-{field}"))
                    .h_full()
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .text_color(MUTED.color())
                    .child(label)
                    .when(field != 4, |el| {
                        el.cursor(gpui::CursorStyle::ResizeLeftRight).on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event, window, cx| {
                                this.begin_shadow_scrub(index, field, event, window, cx)
                            }),
                        )
                    }),
            )
            .child(
                inspector_input(&self.inspector.shadows.rows[index].inputs[field])
                    .flex_1()
                    .min_w_0()
                    .h(px(28.))
                    .px_0()
                    .py_0()
                    .border_color(Color::Transparent.color())
                    .bg(Color::Transparent.color())
                    .rounded(px(4.)),
            )
    }
}

fn button(id: impl Into<gpui::SharedString>, glyph: LucideIcons) -> gpui::Stateful<Div> {
    let id = id.into();
    div()
        .id(id.clone())
        .debug_selector(move || id.to_string())
        .size(px(28.))
        .rounded(px(5.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(BORDER.color()))
        .child(icon(glyph, 15.).text_color(MUTED.color()))
}
