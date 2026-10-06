use super::*;
use crate::scene::effects::{Effect, MAX_EFFECTS, MAX_OFFSET, MAX_RADIUS, Shadow, ShadowKind};
use crate::ui::theme::Color;
use std::collections::BTreeSet;
use uic::components::context_menu::{ContextMenuItem, ContextMenuTrigger};
use uic::components::popover::PopoverState;

#[cfg(test)]
mod layer_blur_tests;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(in crate::editor) struct Controls {
    target: (String, BTreeSet<usize>),
    rows: Vec<Row>,
    expanded: Option<usize>,
}

struct Row {
    kind: Kind,
    color: gpui::Rgba,
    inputs: [Entity<TextInput>; 6],
    picker: Entity<ColorPickerState>,
    popover: Entity<PopoverState>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Drop,
    Inner,
    Blur,
    Background,
}

impl Kind {
    fn of(effect: &Effect) -> Self {
        match effect {
            Effect::Shadow(s) => match s.kind {
                ShadowKind::Drop => Self::Drop,
                ShadowKind::Inner => Self::Inner,
            },
            Effect::LayerBlur { .. } => Self::Blur,
            Effect::BackgroundBlur { .. } => Self::Background,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Drop => "effect-drop-shadow",
            Self::Inner => "effect-inner-shadow",
            Self::Blur => "effect-layer-blur",
            Self::Background => "effect-background-blur",
        }
    }

    fn convert(self, effect: &Effect) -> Effect {
        let enabled = effect.enabled();
        match self {
            Self::Blur => Effect::LayerBlur {
                enabled,
                radius: 8.,
            },
            Self::Background => Effect::BackgroundBlur {
                enabled,
                radius: 8.,
            },
            Self::Drop | Self::Inner => Effect::Shadow(Shadow {
                enabled,
                kind: if self == Self::Drop {
                    ShadowKind::Drop
                } else {
                    ShadowKind::Inner
                },
                ..effect.shadow().cloned().unwrap_or_default()
            }),
        }
    }
}

fn field_values(effect: &Effect) -> [String; 6] {
    if let Some(s) = effect.shadow() {
        [
            number(s.x),
            number(s.y),
            number(s.blur),
            number(s.spread),
            hex(s.color),
            number(s.color.a * 100.),
        ]
    } else if let Effect::LayerBlur { radius, .. } | Effect::BackgroundBlur { radius, .. } = effect
    {
        [
            String::new(),
            String::new(),
            number(*radius),
            String::new(),
            String::new(),
            String::new(),
        ]
    } else {
        unreachable!()
    }
}

impl Workspace {
    pub(in crate::editor) fn close_effect_menus(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for row in &self.inspector.effects.rows {
            row.popover.update(cx, |p, cx| p.close(window, cx));
        }
    }

    pub(in crate::editor) fn effect_number(&self, index: usize, field: usize) -> Option<f32> {
        let shadows = self.common_effects()?;
        let effect = shadows.get(index)?;
        if let Effect::LayerBlur { radius, .. } | Effect::BackgroundBlur { radius, .. } = effect {
            return (field == 2).then_some(*radius);
        }
        let shadow = effect.shadow()?;
        match field {
            0 => Some(shadow.x),
            1 => Some(shadow.y),
            2 => Some(shadow.blur),
            3 => Some(shadow.spread),
            5 => Some(shadow.color.a * 100.),
            _ => None,
        }
    }

    fn begin_effect_scrub(
        &mut self,
        index: usize,
        field: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some()
            || !self
                .effect_targets()
                .iter()
                .any(|id| self.layer_editable(*id))
        {
            return;
        }
        let pending = self
            .inspector
            .effects
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
            self.apply_effect_field(index, field, cx);
        }
        self.focus.focus(window, cx);
        let Some(original) = self.effect_number(index, field) else {
            return;
        };
        self.begin(
            GestureKind::EffectProperty {
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

    pub(in crate::editor) fn scrub_effect_number(
        &mut self,
        index: usize,
        field: usize,
        original: f32,
        delta: f32,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        if delta.abs() < 3. && self.effect_number(index, field) == Some(original) {
            return;
        }
        let value = number(original + (delta * if shift { 10. } else { 1. }).round());
        let Some(row) = self.inspector.effects.rows.get(index) else {
            return;
        };
        row.inputs[field].update(cx, |input, cx| input.set_value(value, cx));
        self.apply_effect_field(index, field, cx);
    }
    fn effect_targets(&self) -> BTreeSet<usize> {
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

    fn common_effects(&self) -> Option<Vec<Effect>> {
        let ids = self.effect_targets();
        let mut values = ids
            .iter()
            .map(|id| self.hierarchy.effects.get(id).cloned().unwrap_or_default());
        let first = values.next().unwrap_or_default();
        values.all(|value| value == first).then_some(first)
    }

    pub(in crate::editor) fn effect_padding(&self, id: usize) -> f32 {
        self.hierarchy.effects.get(&id).map_or(0., |effects| {
            crate::scene::effects::padding(effects) * self.view.zoom
        })
    }

    // Primitive-to-path edits cannot retain a backdrop whose clipping geometry
    // is unsupported. Record removal alongside the shape edit for one-step undo.
    pub(in crate::editor) fn remove_unsupported_background_blur(
        &mut self,
        id: usize,
    ) -> Option<Change> {
        use crate::scene::effects::backdrop;
        if !self
            .shapes
            .iter()
            .any(|s| s.id == id && !backdrop::supports_shape(s))
            || !self.hierarchy.effects.get(&id).is_some_and(|effects| {
                effects
                    .iter()
                    .any(|e| matches!(e, Effect::BackgroundBlur { .. }))
            })
        {
            return None;
        }
        let before = self.snapshot_hierarchy();
        let effects = self.hierarchy.effects.get_mut(&id).unwrap();
        effects.retain(|e| !matches!(e, Effect::BackgroundBlur { .. }));
        if effects.is_empty() {
            self.hierarchy.effects.remove(&id);
        }
        Some(before)
    }

    fn edit_effects(
        &mut self,
        group: Option<Group>,
        edit: impl Fn(&mut Vec<Effect>),
        cx: &mut Context<Self>,
    ) {
        let before = self.snapshot_hierarchy();
        for id in self.effect_targets() {
            if !self.layer_editable(id) {
                continue;
            }
            let shadows = self.hierarchy.effects.entry(id).or_default();
            edit(shadows);
            if shadows.is_empty() {
                self.hierarchy.effects.remove(&id);
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

    fn apply_effect_field(&mut self, index: usize, field: usize, cx: &mut Context<Self>) {
        if self.inspector.effects.target != (self.pages.active.clone(), self.effect_targets()) {
            return;
        }
        let Some(row) = self.inspector.effects.rows.get(index) else {
            return;
        };
        let value = row.inputs[field].read(cx).value().to_string();
        let Some(mut effect) = self.common_effects().and_then(|s| s.get(index).cloned()) else {
            return;
        };
        if row.kind != Kind::of(&effect) {
            return;
        }
        if let Effect::LayerBlur { radius, .. } | Effect::BackgroundBlur { radius, .. } =
            &mut effect
        {
            if field == 2
                && let Ok(value) = value.trim().parse::<f32>()
                && value.is_finite()
            {
                *radius = value.clamp(0., MAX_RADIUS);
            }
        } else if let Some(shadow) = effect.shadow_mut() {
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
        }
        self.edit_effects(
            None,
            |shadows| {
                if let Some(s) = shadows.get_mut(index) {
                    *s = effect.clone();
                }
            },
            cx,
        );
        self.refresh_effect_fields(cx);
    }

    fn refresh_effect_fields(&mut self, cx: &mut Context<Self>) {
        let shadows = self.common_effects().unwrap_or_default();
        for (row, effect) in self.inspector.effects.rows.iter_mut().zip(shadows) {
            if let Some(shadow) = effect.shadow()
                && row.color != shadow.color
            {
                row.color = shadow.color;
                row.picker
                    .update(cx, |picker, cx| picker.set_value(shadow.color, cx));
            }
            for (input, value) in row.inputs.iter().zip(field_values(&effect)) {
                input.update(cx, |input, cx| {
                    if input.value().as_ref() != value {
                        input.set_value(value, cx);
                    }
                });
            }
        }
    }

    pub(in crate::editor) fn sync_effect_controls(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = (self.pages.active.clone(), self.effect_targets());
        let shadows = self.common_effects().unwrap_or_default();
        let changed = self.inspector.effects.target != target
            || self.inspector.effects.rows.len() != shadows.len()
            || self
                .inspector
                .effects
                .rows
                .iter()
                .zip(&shadows)
                .any(|(row, effect)| row.kind != Kind::of(effect));
        if changed {
            for row in &self.inspector.effects.rows {
                row.popover.update(cx, |p, cx| p.close(window, cx));
            }
            let expanded = (self.inspector.effects.target == target)
                .then_some(self.inspector.effects.expanded)
                .flatten();
            self.inspector.effects = Controls {
                target,
                expanded,
                ..Default::default()
            };
            for (index, effect) in shadows.iter().enumerate() {
                let kind = Kind::of(effect);
                let color = effect.shadow().map_or(Shadow::default().color, |s| s.color);
                let mut subscriptions = Vec::new();
                let inputs = std::array::from_fn(|field| {
                    let input = cx.new(TextInput::new);
                    subscriptions.push(cx.subscribe(
                        &input,
                        move |this, _, event: &InputEvent, cx| {
                            if matches!(event, InputEvent::Submit(_)) {
                                this.apply_effect_field(index, field, cx);
                            }
                        },
                    ));
                    subscriptions.push(cx.on_blur(
                        &input.focus_handle(cx),
                        window,
                        move |this, _, cx| this.apply_effect_field(index, field, cx),
                    ));
                    input
                });
                let picker = cx.new(|cx| ColorPickerState::new(color, cx));
                let picker_target = self.inspector.effects.target.clone();
                subscriptions.push(cx.subscribe(
                    &picker,
                    move |this, _, event: &ColorPickerEvent, cx| {
                        let (ColorPickerEvent::Preview(color) | ColorPickerEvent::Commit(color)) =
                            *event;
                        if picker_target != (this.pages.active.clone(), this.effect_targets()) {
                            return;
                        }
                        let ids = this.effect_targets().into_iter().collect();
                        this.edit_effects(
                            Some(Group::Effect(ids, index, 4)),
                            |shadows| {
                                if let Some(effect) = shadows.get_mut(index)
                                    && Kind::of(effect) == kind
                                    && let Some(s) = effect.shadow_mut()
                                {
                                    s.color = color;
                                }
                            },
                            cx,
                        );
                        if matches!(event, ColorPickerEvent::Commit(_)) {
                            this.history.borrow_mut().break_group();
                        }
                        this.refresh_effect_fields(cx);
                    },
                ));
                self.inspector.effects.rows.push(Row {
                    kind,
                    color,
                    inputs,
                    picker,
                    popover: cx.new(|cx| PopoverState::new(window, cx)),
                    _subscriptions: subscriptions,
                });
            }
        }
        let editing = self.inspector.effects.rows.iter().any(|row| {
            row.inputs
                .iter()
                .any(|input| input.focus_handle(cx).is_focused(window))
        });
        if changed || !editing {
            self.refresh_effect_fields(cx);
        }
    }

    pub(in crate::editor) fn effect_controls(&self, cx: &mut Context<Self>) -> Div {
        if self.effect_targets().is_empty() {
            return div();
        }
        let shadows = self.common_effects();
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
                        shadows.as_ref().is_some_and(|s| s.len() < MAX_EFFECTS),
                        |el| {
                            el.on_click(cx.listener(|this, _, _, cx| {
                                let index = this.common_effects().unwrap_or_default().len();
                                this.edit_effects(
                                    None,
                                    |s| {
                                        if s.len() < MAX_EFFECTS {
                                            s.push(Effect::default());
                                        }
                                    },
                                    cx,
                                );
                                this.inspector.effects.expanded = Some(index);
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
                    .map(|(index, effect)| self.effect_row(index, effect, cx)),
            )
    }

    fn toggle_effect_details(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let pending = self
            .inspector
            .effects
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
            self.apply_effect_field(index, field, cx);
        }
        self.close_effect_menus(window, cx);
        self.focus.focus(window, cx);
        self.inspector.effects.expanded =
            (self.inspector.effects.expanded != Some(index)).then_some(index);
        cx.notify();
    }

    fn effect_row(&self, index: usize, effect: &Effect, cx: &mut Context<Self>) -> Div {
        let expanded = self.inspector.effects.expanded == Some(index);
        let id = self.inspector.effects.rows[index].inputs[0].entity_id();
        let details = self.effect_details(index, effect, cx);
        div()
            .flex()
            .flex_col()
            .child(self.effect_header(index, effect, expanded, cx))
            .child(
                gpui_effects::animated_collapse(("effect-details", id), expanded, move || details)
                    .duration(crate::ui::DISCLOSURE_DURATION),
            )
    }

    fn effect_header(
        &self,
        index: usize,
        effect: &Effect,
        expanded: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .id(format!("shadow-edit-{index}"))
                    .debug_selector(move || format!("shadow-edit-{index}"))
                    .w(px(26.))
                    .h(px(30.))
                    .px(px(6.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(|s| s.bg(BORDER.color()))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(crate::ui::disclosure_icon(
                        (
                            "effect-chevron",
                            self.inspector.effects.rows[index].inputs[0].entity_id(),
                        ),
                        expanded,
                        14.,
                        MUTED.color(),
                    ))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.toggle_effect_details(index, window, cx)
                    })),
            )
            .child(self.effect_kind_control(index, effect, cx))
            .child(
                button(
                    format!("shadow-toggle-{index}"),
                    if effect.enabled() {
                        LucideIcons::Eye
                    } else {
                        LucideIcons::EyeOff
                    },
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.edit_effects(
                        None,
                        |s| {
                            if let Some(s) = s.get_mut(index) {
                                s.toggle();
                            }
                        },
                        cx,
                    )
                })),
            )
            .child(
                button(format!("shadow-remove-{index}"), LucideIcons::Minus).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.edit_effects(
                            None,
                            |s| {
                                if index < s.len() {
                                    s.remove(index);
                                }
                            },
                            cx,
                        )
                    },
                )),
            )
    }

    fn effect_details(
        &self,
        index: usize,
        effect: &Effect,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let row = &self.inspector.effects.rows[index];
        let picker = row.picker.clone();
        div()
            .id("effect-fields")
            .debug_selector(move || format!("effect-details-{index}"))
            .pt(px(8.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .when(
                matches!(
                    effect,
                    Effect::LayerBlur { .. } | Effect::BackgroundBlur { .. }
                ),
                |el| el.child(self.shadow_input(index, 2, t("effect-blur"), cx)),
            )
            .when_some(effect.shadow(), |el, shadow| {
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
                                        .debug_selector(move || format!("shadow-color-{index}"))
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

    fn effect_kind_control(
        &self,
        index: usize,
        effect: &Effect,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let weak = cx.entity().downgrade();
        let target = (self.pages.active.clone(), self.effect_targets());
        let current = Kind::of(effect);
        let enabled = target.1.iter().any(|id| self.layer_editable(*id));
        let blur_supported = target
            .1
            .iter()
            .all(|id| !self.boards.iter().any(|b| b.id == *id));
        let background_supported = target.1.iter().all(|id| {
            self.boards.iter().any(|b| b.id == *id)
                || self
                    .shapes
                    .iter()
                    .any(|s| s.id == *id && crate::scene::effects::backdrop::supports_shape(s))
        });
        let trigger = div()
            .id(format!("shadow-kind-{index}"))
            .debug_selector(move || format!("shadow-kind-{index}"))
            .h(px(30.))
            .px(px(6.))
            .rounded(px(5.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(BORDER.color()))
            .text_size(px(12.))
            .child(t(current.label()))
            .child(icon(LucideIcons::ChevronDown, 12.).text_color(MUTED.color()));
        div().flex_1().min_w_0().child(
            ContextMenuTrigger::new(trigger, move |_, _| {
                let mut menu = super::super::context_menu::menu(180., "shadow-kind-menu");
                for kind in [Kind::Drop, Kind::Inner, Kind::Blur, Kind::Background] {
                    let label = kind.label();
                    if (kind == Kind::Blur && !blur_supported)
                        || (kind == Kind::Background && !background_supported)
                    {
                        continue;
                    }
                    let weak = weak.clone();
                    let target = target.clone();
                    menu = menu.item(
                        ContextMenuItem::action_with(
                            move |_, _| {
                                div()
                                    .debug_selector(move || label.into())
                                    .h(px(28.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .child(div().w(px(14.)).when(kind == current, |el| {
                                        el.child(icon(LucideIcons::Check, 13.))
                                    }))
                                    .child(t(label))
                            },
                            move |_, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    if target != (this.pages.active.clone(), this.effect_targets())
                                        || this
                                            .common_effects()
                                            .and_then(|s| s.get(index).map(Kind::of))
                                            != Some(current)
                                    {
                                        return;
                                    }
                                    if current == kind {
                                        return;
                                    }
                                    this.edit_effects(
                                        None,
                                        |shadows| {
                                            if let Some(shadow) = shadows.get_mut(index) {
                                                *shadow = kind.convert(shadow);
                                            }
                                        },
                                        cx,
                                    );
                                });
                            },
                        )
                        .disabled(!enabled),
                    );
                }
                menu
            })
            .id(format!("shadow-kind-trigger-{index}")),
        )
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
                                this.begin_effect_scrub(index, field, event, window, cx)
                            }),
                        )
                    }),
            )
            .child(
                inspector_input(&self.inspector.effects.rows[index].inputs[field])
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
