use super::*;
use crate::scene::auto_layout::MAX_GRID_TRACKS;
use std::{cell::RefCell, collections::BTreeMap};

mod guides;

#[cfg(test)]
mod tests;

pub(in crate::editor) struct State {
    pub(in crate::editor) tracks: RefCell<BTreeMap<usize, crate::scene::auto_layout::GridTracks>>,
    show_guides: bool,
    inputs: [Entity<TextInput>; 4],
    target: Option<(String, usize)>,
    _subscriptions: Vec<Subscription>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let mut subscriptions = Vec::new();
        let inputs = std::array::from_fn(|index| {
            let input = cx.new(TextInput::new);
            subscriptions.push(cx.subscribe_in(
                &input,
                window,
                move |this, _, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::Submit(_)) {
                        this.apply_grid_number(index, cx);
                    }
                },
            ));
            subscriptions.push(
                cx.on_blur(&input.focus_handle(cx), window, move |this, _, cx| {
                    this.apply_grid_number(index, cx)
                }),
            );
            input
        });
        Self {
            tracks: RefCell::new(BTreeMap::new()),
            show_guides: true,
            inputs,
            target: None,
            _subscriptions: subscriptions,
        }
    }
}

impl Workspace {
    pub(super) fn grid_item_target(&self) -> Option<usize> {
        let id = self.layout_target()?;
        (self.is_layout_flow_item(id)
            && self
                .layer_parent(id)
                .and_then(|p| self.hierarchy.layouts.get(&p))
                .is_some_and(|l| l.axis == Axis::Grid))
        .then_some(id)
    }

    pub(super) fn refresh_grid_inputs(&mut self, cx: &mut Context<Self>) {
        self.auto_layout.grid.target = self.layout_input_target();
        let id = self.layout_target();
        let layout = id.and_then(|id| self.hierarchy.layouts.get(&id));
        let sizing = id
            .and_then(|id| self.hierarchy.sizing.get(&id))
            .copied()
            .unwrap_or_default();
        let values = [
            layout.map_or(2., |l| l.columns as f32),
            layout.and_then(|l| l.column_width).unwrap_or(100.),
            sizing.column_span as f32,
            sizing.row_span as f32,
        ];
        for (input, value) in self.auto_layout.grid.inputs.iter().zip(values) {
            input.update(cx, |input, cx| {
                input.set_value(inspector::number(value), cx)
            });
        }
    }

    fn apply_grid_number(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.auto_layout.grid.target != self.layout_input_target() {
            return;
        }
        let value = self.auto_layout.grid.inputs[index]
            .read(cx)
            .value()
            .trim()
            .parse::<f32>();
        if let Ok(value) = value {
            self.set_grid_number(index, value, cx);
        } else {
            self.refresh_grid_inputs(cx);
            cx.notify();
        }
    }

    fn set_grid_number(&mut self, index: usize, value: f32, cx: &mut Context<Self>) {
        if value.is_finite()
            && value >= 1.
            && value
                <= if index == 1 {
                    crate::scene::artboard::MAX_SIZE
                } else {
                    MAX_GRID_TRACKS as f32
                }
            && (index == 1 || value.fract() == 0.)
        {
            if index < 2
                && self
                    .layout_target()
                    .and_then(|id| self.hierarchy.layouts.get(&id))
                    .is_some_and(|l| {
                        l.axis == Axis::Grid && (index == 0 || l.column_width.is_some())
                    })
            {
                self.edit_layout(
                    |layout| {
                        if index == 0 {
                            layout.columns = value as u16
                        } else {
                            layout.column_width = Some(value)
                        }
                    },
                    cx,
                );
            } else if index >= 2
                && let Some(id) = self
                    .grid_item_target()
                    .filter(|id| self.layer_editable(*id))
            {
                let before = self.snapshot_hierarchy();
                let columns = self.hierarchy.layouts[&self.layer_parent(id).unwrap()].columns;
                let sizing = self.hierarchy.sizing.entry(id).or_default();
                let value = if index == 2 {
                    (value as u16).min(columns)
                } else {
                    value as u16
                };
                let span = if index == 2 {
                    &mut sizing.column_span
                } else {
                    &mut sizing.row_span
                };
                if *span != value {
                    *span = value;
                    self.history.borrow_mut().record(vec![before], None);
                    self.reflow_layout(cx);
                }
            }
        }
        self.refresh_grid_inputs(cx);
        cx.notify();
    }

    pub(super) fn grid_number_value(&self, index: usize) -> Option<f32> {
        let id = self.layout_target()?;
        if index < 2 {
            let layout = self
                .hierarchy
                .layouts
                .get(&id)
                .filter(|l| l.axis == Axis::Grid)?;
            if index == 0 {
                Some(layout.columns as f32)
            } else {
                layout.column_width
            }
        } else {
            self.grid_item_target()?;
            let sizing = self.hierarchy.sizing.get(&id).copied().unwrap_or_default();
            Some(if index == 2 {
                sizing.column_span
            } else {
                sizing.row_span
            } as f32)
        }
    }

    pub(super) fn commit_grid_input(&mut self, window: &Window, cx: &mut Context<Self>) {
        if let Some(index) = self
            .auto_layout
            .grid
            .inputs
            .iter()
            .position(|input| input.focus_handle(cx).is_focused(window))
        {
            self.apply_grid_number(index, cx);
        }
    }

    pub(super) fn scrub_grid_number(
        &mut self,
        index: usize,
        original: f32,
        delta: f32,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        let step = if index == 1 { 1. } else { 8. };
        let max = if index == 1 {
            crate::scene::artboard::MAX_SIZE
        } else {
            MAX_GRID_TRACKS as f32
        };
        let value =
            (original + (delta / step * if shift { 10. } else { 1. }).round()).clamp(1., max);
        if self.grid_number_value(index) != Some(value) {
            self.set_grid_number(index, value, cx);
        }
    }

    fn grid_number(&self, index: usize, label: &'static str, cx: &mut Context<Self>) -> Div {
        div()
            .debug_selector(move || format!("grid-number-{index}"))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .id(("grid-drag", index))
                    .debug_selector(move || format!("grid-drag-{index}"))
                    .flex_1()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .text_color(rgb(MUTED))
                    .child(t(label))
                    .when(
                        self.layout_target()
                            .is_some_and(|id| self.layer_editable(id)),
                        |el| {
                            el.cursor(gpui::CursorStyle::ResizeLeftRight)
                                .hover(|s| s.text_color(rgb(TEXT)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, event, window, cx| {
                                        this.begin_layout_scrub(index + 6, event, window, cx)
                                    }),
                                )
                        },
                    ),
            )
            .child(
                Input::new(&self.auto_layout.grid.inputs[index])
                    .w(px(70.))
                    .h(px(26.))
                    .px(px(6.))
                    .py_0()
                    .rounded(px(5.))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT))
                    .bg(rgb(0x282b33))
                    .border_color(rgb(BORDER))
                    .appearance(InputAppearance {
                        focus_border: rgb(ACCENT).into(),
                        caret: rgb(ACCENT).into(),
                        selection: gpui::rgba(0xb4a2ee44).into(),
                        ..Default::default()
                    }),
            )
    }

    pub(super) fn grid_container_controls(
        &self,
        layout: &Container,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(self.grid_number(0, "layout-columns", cx))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(
                        div()
                            .flex_1()
                            .text_color(rgb(MUTED))
                            .child(t("layout-column-width")),
                    )
                    .children(
                        [(false, "layout-equal-columns"), (true, "layout-fixed")].map(
                            |(fixed, label)| {
                                let active = layout.column_width.is_some() == fixed;
                                div()
                                    .id(label)
                                    .debug_selector(move || label.into())
                                    .h(px(26.))
                                    .px(px(8.))
                                    .rounded(px(5.))
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .bg(rgb(if active { 0x383044 } else { 0x282b33 }))
                                    .text_color(rgb(if active { ACCENT } else { TEXT }))
                                    .child(t(label))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        let width = this
                                            .layout_target()
                                            .and_then(|id| this.world_rect(id))
                                            .map_or(100., |r| r.width);
                                        this.edit_layout(
                                            |layout| {
                                                if layout.column_width.is_some() == fixed {
                                                    return;
                                                }
                                                layout.column_width = if fixed {
                                                    Some(
                                                        ((width
                                                            - layout.padding[1]
                                                            - layout.padding[3]
                                                            - layout.gap
                                                                * (layout.columns - 1) as f32)
                                                            / layout.columns as f32)
                                                            .clamp(
                                                                1.,
                                                                crate::scene::artboard::MAX_SIZE,
                                                            ),
                                                    )
                                                } else {
                                                    None
                                                };
                                            },
                                            cx,
                                        );
                                    }))
                            },
                        ),
                    ),
            )
            .when(layout.column_width.is_some(), |el| {
                el.child(self.grid_number(1, "layout-column-width", cx))
            })
            .child(
                div()
                    .id("grid-guides-toggle")
                    .debug_selector(|| "grid-guides-toggle".into())
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(26.))
                    .cursor_pointer()
                    .text_color(rgb(if self.auto_layout.grid.show_guides {
                        ACCENT
                    } else {
                        MUTED
                    }))
                    .child(icon(
                        if self.auto_layout.grid.show_guides {
                            LucideIcons::Eye
                        } else {
                            LucideIcons::EyeOff
                        },
                        14.,
                    ))
                    .child(t("layout-grid-guides"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.auto_layout.grid.show_guides = !this.auto_layout.grid.show_guides;
                        cx.notify();
                    })),
            )
    }

    pub(super) fn grid_item_controls(&self, cx: &mut Context<Self>) -> Div {
        inspector::inspector_section(t("layout-grid-cell"))
            .text_size(px(12.))
            .font_weight(FontWeight::NORMAL)
            .child(self.grid_number(2, "layout-column-span", cx))
            .child(self.grid_number(3, "layout-row-span", cx))
    }
}
