use super::*;
use crate::scene::history::PaintTarget;
use crate::scene::paint_layers::Fill;
use std::borrow::Cow;

impl Workspace {
    fn paint_target(&self) -> Option<PaintTarget> {
        let mut stops = self.inspector.paint_stops;
        stops[self.inspector.stroke_editing as usize] = self.inspector.active_stop;
        Some(PaintTarget {
            id: self.selected_shape.or(self.selected)?,
            layers: [self.paint_layer_index(false), self.paint_layer_index(true)],
            stops,
            stroke: self.inspector.stroke_editing,
        })
    }

    fn begin_paint_stack_edit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<PaintTarget> {
        if self.gesture.is_some() || self.preview_read_only() {
            return None;
        }
        let target = self
            .paint_target()
            .filter(|target| self.layer_editable(target.id))?;
        if self.image_crop.is_some() {
            self.finish_image_crop(false, window, cx);
        }
        self.history.borrow_mut().begin_preview();
        Some(target)
    }

    fn finish_paint_stack_edit(&mut self, before: PaintTarget) {
        let mut changes = self.history.borrow_mut().end_preview(false);
        if changes.is_empty() {
            return;
        }
        changes.push(Change::PaintTarget {
            before,
            after: self.paint_target().unwrap(),
        });
        self.history.borrow_mut().record(changes, None);
    }

    pub(in crate::editor) fn paint_layer_index(&self, stroke: bool) -> usize {
        if self.inspector.paint_object != self.selected_shape.or(self.selected)
            || self.inspector.paint_page != self.pages.active
        {
            return 0;
        }
        let count = if let Some(s) = self.selected_shape() {
            if stroke {
                s.stroke_layers.len()
            } else {
                s.fill_layers.len()
            }
        } else {
            self.selected_board().map_or(0, |b| b.fill_layers.len())
        };
        self.inspector.paint_layers[stroke as usize].min(count)
    }

    pub(in crate::editor) fn paint_shape(&self) -> Option<Cow<'_, Shape>> {
        let [fill, stroke] = [self.paint_layer_index(false), self.paint_layer_index(true)];
        self.paint_shape_at(fill, stroke)
    }
    pub(in crate::editor) fn paint_shape_at(
        &self,
        fill: usize,
        stroke: usize,
    ) -> Option<Cow<'_, Shape>> {
        let shape = self.selected_shape()?;
        if fill > shape.fill_layers.len() || stroke > shape.stroke_layers.len() {
            return None;
        }
        if fill == 0 && stroke == 0 {
            return Some(Cow::Borrowed(shape));
        }
        let mut value = shape.clone();
        if fill > 0 {
            shape.fill_layers[fill - 1].apply_shape(&mut value);
        }
        if stroke > 0 {
            value.stroke = shape.stroke_layers[stroke - 1].clone();
        }
        Some(Cow::Owned(value))
    }

    pub(in crate::editor) fn paint_board(&self) -> Option<Cow<'_, Artboard>> {
        let index = self.paint_layer_index(false);
        self.paint_board_at(index)
    }
    pub(in crate::editor) fn paint_board_at(&self, index: usize) -> Option<Cow<'_, Artboard>> {
        let board = self.selected_board()?;
        if index > board.fill_layers.len() {
            return None;
        }
        if index == 0 {
            return Some(Cow::Borrowed(board));
        }
        let mut value = board.clone();
        board.fill_layers[index - 1].apply_board(&mut value);
        Some(Cow::Owned(value))
    }

    pub(in crate::editor) fn edit_shape_paint<R>(
        &mut self,
        edit: impl FnOnce(&mut Shape) -> R,
    ) -> Option<R> {
        let [fill, stroke] = [self.paint_layer_index(false), self.paint_layer_index(true)];
        self.edit_shape_paint_at(fill, stroke, edit)
    }

    pub(super) fn edit_shape_paint_at<R>(
        &mut self,
        fill: usize,
        stroke: usize,
        edit: impl FnOnce(&mut Shape) -> R,
    ) -> Option<R> {
        self.edit_shape(|shape| {
            let base = Fill::from_shape(shape);
            let base_stroke = shape.stroke.clone();
            if fill > 0 {
                shape.fill_layers[fill - 1].clone().apply_shape(shape);
            }
            if stroke > 0 {
                shape.stroke = shape.stroke_layers[stroke - 1].clone();
            }
            let before_fill = Fill::from_shape(shape);
            let before_stroke = shape.stroke.clone();
            let result = edit(shape);
            if shape.color != before_fill.color
                || shape.fill_mode != before_fill.fill_mode
                || shape.gradient != before_fill.gradient
            {
                shape.color_style = None;
            }
            if shape.stroke.color != before_stroke.color
                || shape.stroke.fill_mode != before_stroke.fill_mode
                || shape.stroke.gradient != before_stroke.gradient
            {
                shape.stroke.color_style = None;
            }
            if fill > 0 {
                shape.fill_layers[fill - 1] = Fill::from_shape(shape);
                base.apply_shape(shape);
            }
            if stroke > 0 {
                shape.stroke_layers[stroke - 1] = shape.stroke.clone();
                shape.stroke = base_stroke;
            }
            result
        })
    }

    pub(in crate::editor) fn edit_board_paint<R>(
        &mut self,
        group: Option<Group>,
        edit: impl FnOnce(&mut Artboard) -> R,
    ) -> Option<R> {
        let index = self.paint_layer_index(false);
        self.edit_board_paint_at(index, group, edit)
    }

    pub(super) fn edit_board_paint_at<R>(
        &mut self,
        index: usize,
        group: Option<Group>,
        edit: impl FnOnce(&mut Artboard) -> R,
    ) -> Option<R> {
        self.edit_board(group, |board| {
            let base = Fill::from_board(board);
            if index > 0 {
                board.fill_layers[index - 1].clone().apply_board(board);
            }
            let before = Fill::from_board(board);
            let result = edit(board);
            if board.color != before.color
                || board.fill_mode != before.fill_mode
                || board.gradient != before.gradient
            {
                board.color_style = None;
            }
            if index > 0 {
                board.fill_layers[index - 1] = Fill::from_board(board);
                base.apply_board(board);
            }
            result
        })
    }

    pub(in crate::editor) fn select_paint_layer(
        &mut self,
        stroke: bool,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changing_layer = self.paint_layer_index(stroke) != index;
        if self.image_crop.is_some() {
            self.finish_image_crop(false, window, cx);
        }
        self.media.image_request += 1;
        for popover in &self.inspector.paint_popovers {
            popover.update(cx, |p, cx| {
                p.dismiss(
                    uic::components::popover::PopoverDismissReason::Programmatic,
                    false,
                    window,
                    cx,
                )
            });
        }
        self.history.borrow_mut().break_group();
        if self.inspector.paint_object != self.selected_shape.or(self.selected) {
            self.inspector.paint_layers = [0; 2];
        }
        self.inspector.paint_object = self.selected_shape.or(self.selected);
        self.inspector.paint_page = self.pages.active.clone();
        self.inspector.paint_layers[stroke as usize] = index;
        self.inspector.paint_stops[self.inspector.stroke_editing as usize] =
            self.inspector.active_stop;
        self.inspector.stroke_editing = stroke;
        self.inspector.active_stop = if changing_layer {
            0
        } else {
            self.inspector.paint_stops[stroke as usize]
        };
        self.inspector.paint_stops[stroke as usize] = self.inspector.active_stop;
        self.sync_fields(cx);
        cx.notify();
    }

    fn add_paint_layer(&mut self, stroke: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(before) = self.begin_paint_stack_edit(window, cx) else {
            return;
        };
        let index = if self.selected_shape.is_some() {
            self.edit_shape(|s| {
                if stroke {
                    if !s.stroke.enabled && s.stroke_layers.is_empty() {
                        s.stroke.enabled = true;
                        0
                    } else {
                        s.stroke_layers.push(crate::scene::shape::Stroke {
                            enabled: true,
                            ..Default::default()
                        });
                        s.stroke_layers.len()
                    }
                } else if !s.fill_enabled && s.fill_layers.is_empty() {
                    s.fill_enabled = true;
                    0
                } else {
                    let mut fill =
                        Fill::from_shape(&Shape::new(0, None, ShapeKind::Rectangle, s.rect));
                    fill.opacity = 0.5;
                    s.fill_layers.push(fill);
                    s.fill_layers.len()
                }
            })
        } else {
            self.edit_board(None, |b| {
                if !b.fill_enabled && b.fill_layers.is_empty() {
                    b.fill_enabled = true;
                    0
                } else {
                    let mut fill =
                        Fill::from_shape(&Shape::new(0, None, ShapeKind::Rectangle, b.rect));
                    fill.opacity = 0.5;
                    b.fill_layers.push(fill);
                    b.fill_layers.len()
                }
            })
        };
        if let Some(index) = index {
            self.select_paint_layer(stroke, index, window, cx);
        }
        self.finish_paint_stack_edit(before);
    }

    fn remove_paint_layer(
        &mut self,
        stroke: bool,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(before) = self.begin_paint_stack_edit(window, cx) else {
            return;
        };
        let active = self.paint_layer_index(stroke);
        if self.selected_shape.is_some() {
            self.edit_shape(|s| {
                if stroke {
                    if index > 0 {
                        s.stroke_layers.remove(index - 1);
                    } else if s.stroke_layers.is_empty() {
                        s.stroke = Default::default();
                    } else {
                        s.stroke = s.stroke_layers.remove(0);
                    }
                } else if index > 0 {
                    s.fill_layers.remove(index - 1);
                } else if s.fill_layers.is_empty() {
                    Fill::from_shape(&Shape::new(0, None, ShapeKind::Rectangle, s.rect))
                        .apply_shape(s);
                    s.fill_enabled = false;
                } else {
                    s.fill_layers.remove(0).apply_shape(s);
                }
            });
        } else {
            self.edit_board(None, |b| {
                if index > 0 {
                    b.fill_layers.remove(index - 1);
                } else if b.fill_layers.is_empty() {
                    let mut fill =
                        Fill::from_shape(&Shape::new(0, None, ShapeKind::Rectangle, b.rect));
                    fill.color = rgb(0xffffff);
                    fill.apply_board(b);
                    b.fill_enabled = false;
                } else {
                    b.fill_layers.remove(0).apply_board(b);
                }
            });
        }
        let next = if active == index {
            self.inspector.paint_stops[stroke as usize] = 0;
            if self.inspector.stroke_editing == stroke {
                self.inspector.active_stop = 0;
            }
            index.saturating_sub(1)
        } else {
            active - usize::from(index < active)
        };
        self.inspector.paint_layers[stroke as usize] = next;
        self.select_paint_layer(stroke, next, window, cx);
        self.finish_paint_stack_edit(before);
    }

    fn toggle_paint_layer(
        &mut self,
        stroke: bool,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.select_paint_layer(stroke, index, window, cx);
        if self.selected_shape.is_some() {
            self.edit_shape_paint(|s| {
                if stroke {
                    s.stroke.hidden = !s.stroke.hidden;
                } else {
                    s.fill_hidden = !s.fill_hidden;
                }
            });
        } else {
            self.edit_board_paint(None, |b| b.fill_hidden = !b.fill_hidden);
        }
        self.history.borrow_mut().break_group();
        cx.notify();
    }

    fn move_paint_layer(
        &mut self,
        stroke: bool,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(before) = self.begin_paint_stack_edit(window, cx) else {
            return;
        };
        let from = self.paint_layer_index(stroke);
        let lower = from.min(to);
        let upper = from.max(to);
        if self.selected_shape.is_some() {
            self.edit_shape(|s| {
                if stroke {
                    if lower == 0 {
                        std::mem::swap(&mut s.stroke, &mut s.stroke_layers[upper - 1]);
                    } else {
                        s.stroke_layers.swap(lower - 1, upper - 1);
                    }
                } else if lower == 0 {
                    let fill = Fill::from_shape(s);
                    s.fill_layers[upper - 1].clone().apply_shape(s);
                    s.fill_layers[upper - 1] = fill;
                } else {
                    s.fill_layers.swap(lower - 1, upper - 1);
                }
            });
        } else {
            self.edit_board(None, |b| {
                if lower == 0 {
                    let fill = Fill::from_board(b);
                    b.fill_layers[upper - 1].clone().apply_board(b);
                    b.fill_layers[upper - 1] = fill;
                } else {
                    b.fill_layers.swap(lower - 1, upper - 1);
                }
            });
        }
        // Only the stack index changed; keep editing the same paint and stop.
        self.inspector.paint_layers[stroke as usize] = to;
        self.select_paint_layer(stroke, to, window, cx);
        self.finish_paint_stack_edit(before);
    }

    pub(in crate::editor) fn paint_layer_section(
        &self,
        stroke: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let (enabled, count) = if let Some(s) = self.selected_shape() {
            if stroke {
                (s.stroke.enabled, s.stroke_layers.len())
            } else {
                (s.fill_enabled, s.fill_layers.len())
            }
        } else if let Some(b) = self.selected_board() {
            (b.fill_enabled, b.fill_layers.len())
        } else {
            return div();
        };
        let active = self.paint_layer_index(stroke);
        section_body()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .when(enabled || count > 0, |el| {
                        el.child(self.color_style_control(stroke, cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .debug_selector(move || {
                                if stroke {
                                    "shape-stroke".into()
                                } else {
                                    "shape-fill".into()
                                }
                            })
                            .child(t(if stroke { "stroke" } else { "fill" })),
                    )
                    .when(count > 0, |el| {
                        el.children([true, false].into_iter().map(|up| {
                            let available = if up {
                                active < count
                            } else {
                                active > usize::from(!enabled)
                            };
                            icon_button(
                                format!(
                                    "move-{}-{}",
                                    if stroke { "stroke" } else { "fill" },
                                    if up { "up" } else { "down" }
                                ),
                                t(if up { "bring-forward" } else { "send-backward" }),
                                if up {
                                    GpuiIcons::LucideChevronUp
                                } else {
                                    GpuiIcons::LucideChevronDown
                                },
                                false,
                            )
                            .size(px(24.))
                            .opacity(if available { 1. } else { 0.3 })
                            .when(available, |el| {
                                el.on_click(cx.listener(move |this, _, window, cx| {
                                    this.move_paint_layer(
                                        stroke,
                                        if up { active + 1 } else { active - 1 },
                                        window,
                                        cx,
                                    );
                                }))
                            })
                        }))
                    })
                    .child(
                        icon_button(
                            if stroke {
                                "add-stroke-layer"
                            } else {
                                "add-fill-layer"
                            },
                            t("paint-enable"),
                            GpuiIcons::LucidePlus,
                            false,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| this.add_paint_layer(stroke, window, cx),
                        )),
                    ),
            )
            .children(
                (0..=count)
                    .rev()
                    .filter(|i| *i != 0 || enabled)
                    .map(|index| {
                        let hidden = if let Some(s) = self.paint_shape_at(
                            if stroke { 0 } else { index },
                            if stroke { index } else { 0 },
                        ) {
                            if stroke {
                                s.stroke.hidden
                            } else {
                                s.fill_hidden
                            }
                        } else {
                            self.paint_board_at(index).is_some_and(|b| b.fill_hidden)
                        };
                        let remove = div()
                            .id((
                                if stroke {
                                    "remove-stroke-layer"
                                } else {
                                    "remove-fill-layer"
                                },
                                index,
                            ))
                            .debug_selector(move || {
                                format!(
                                    "remove-{}-layer-{index}",
                                    if stroke { "stroke" } else { "fill" }
                                )
                            })
                            .size(px(26.))
                            .flex_shrink_0()
                            .rounded(px(7.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(Color::Selected.color()))
                            .child(icon(GpuiIcons::LucideMinus, 14.))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.remove_paint_layer(stroke, index, window, cx)
                            }))
                            .into_any_element();
                        let remove = div()
                            .flex()
                            .items_center()
                            .flex_shrink_0()
                            .child(
                                icon_button(
                                    format!(
                                        "toggle-{}-layer-{index}",
                                        if stroke { "stroke" } else { "fill" }
                                    ),
                                    t(if hidden { "show-layer" } else { "hide-layer" }),
                                    if hidden {
                                        GpuiIcons::LucideEyeOff
                                    } else {
                                        GpuiIcons::LucideEye
                                    },
                                    false,
                                )
                                .size(px(24.))
                                .on_click(cx.listener(
                                    move |this, _, window, cx| {
                                        this.toggle_paint_layer(stroke, index, window, cx);
                                    },
                                )),
                            )
                            .child(remove)
                            .into_any_element();
                        div()
                            .id((if stroke { "stroke-layer" } else { "fill-layer" }, index))
                            .debug_selector(move || {
                                format!("{}-layer-{index}", if stroke { "stroke" } else { "fill" })
                            })
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    if this.paint_layer_index(stroke) != index
                                        || this.inspector.stroke_editing != stroke
                                    {
                                        this.select_paint_layer(stroke, index, window, cx);
                                    }
                                }),
                            )
                            .child(if self.selected_shape.is_some() {
                                self.active_paint_controls(stroke, index, remove, cx)
                                    .into_any_element()
                            } else {
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .child(self.paint_layer_value_row(stroke, index, cx)),
                                    )
                                    .child(remove)
                                    .into_any_element()
                            })
                    }),
            )
    }
}
