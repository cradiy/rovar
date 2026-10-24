use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Key {
    pub stroke: bool,
    pub layer: usize,
    pub index: usize,
}

pub(super) struct RowInput {
    pub key: Key,
    pub input: Entity<TextInput>,
    pub invalid: bool,
    last: String,
    pending: Vec<gpui::SharedString>,
    _subscriptions: Vec<Subscription>,
}

#[derive(Clone)]
pub(super) struct Field {
    pub key: Key,
    pub input: Entity<TextInput>,
    pub invalid: bool,
    pub value: String,
    pub mode: FillMode,
    pub swatch: gpui::Background,
    pub image: crate::scene::image_fill::ImageFill,
    pub active: bool,
}

impl Workspace {
    pub(super) fn row_value(
        &self,
        key: Key,
    ) -> Option<(
        String,
        FillMode,
        gpui::Background,
        crate::scene::image_fill::ImageFill,
    )> {
        let (mode, gradient, color, image, width, opacity) = if self.selected_shape.is_some() {
            let s = self.paint_shape_at(
                if key.stroke { 0 } else { key.layer },
                if key.stroke { key.layer } else { 0 },
            )?;
            (
                s.paint_mode(key.stroke),
                s.paint_gradient(key.stroke).clone(),
                if key.stroke { s.stroke.color } else { s.color },
                s.image_fill.clone(),
                s.stroke.width,
                if key.stroke {
                    s.stroke.opacity
                } else {
                    s.fill_opacity
                },
            )
        } else {
            let b = self.paint_board_at(key.layer)?;
            (
                b.fill_mode,
                b.gradient.clone(),
                b.color,
                b.image_fill.clone(),
                0.,
                b.fill_opacity,
            )
        };
        let active = key.layer == self.paint_layer_index(key.stroke);
        let stop = if active {
            self.inspector.paint_stops[key.stroke as usize]
        } else {
            0
        };
        let stop = if active && key.stroke == self.inspector.stroke_editing {
            self.inspector.active_stop
        } else {
            stop
        };
        let color = match mode {
            FillMode::Linear => gradient.stop(stop).unwrap_or(&gradient.stops()[0]).color,
            FillMode::Points(g) => g.points[stop.min(3)].color,
            _ => color,
        };
        let value = match key.index {
            5 | 16 => {
                if mode == FillMode::Image {
                    t("shape-image").into()
                } else {
                    hex(color)
                }
            }
            6 | 17 => number(
                100. * opacity
                    * match mode {
                        FillMode::Solid => color.a,
                        FillMode::Image => image.opacity,
                        _ => 1.,
                    },
            ),
            14 => number(width),
            _ => return None,
        };
        let swatch = if mode == FillMode::Linear {
            gradient.background()
        } else {
            color.into()
        };
        Some((value, mode, swatch.opacity(opacity), image))
    }

    pub(in crate::editor) fn sync_paint_rows(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = self
            .selected_shape
            .or(self.selected)
            .filter(|_| self.selected_text.is_none() && self.multi_selection.is_empty())
            .map(|id| (self.pages.active.clone(), id));
        if owner != self.inspector.paint_row_owner {
            self.inspector.paint_rows.clear();
            self.inspector.paint_row_owner = owner.clone();
        }
        let Some(owner) = owner else {
            return;
        };
        let mut keys = Vec::new();
        for stroke in [false, true] {
            let (enabled, count) = if let Some(s) = self.selected_shape() {
                if stroke {
                    (s.stroke.enabled, s.stroke_layers.len())
                } else if s.can_fill() {
                    (s.fill_enabled, s.fill_layers.len())
                } else {
                    continue;
                }
            } else if !stroke {
                let Some(b) = self.selected_board() else {
                    continue;
                };
                (b.fill_enabled, b.fill_layers.len())
            } else {
                continue;
            };
            for layer in 0..=count {
                if layer == 0 && !enabled {
                    continue;
                }
                for index in if stroke {
                    &[16, 17, 14][..]
                } else {
                    &[5, 6][..]
                } {
                    keys.push(Key {
                        stroke,
                        layer,
                        index: *index,
                    });
                }
            }
        }
        self.inspector.paint_rows.retain(|r| keys.contains(&r.key));
        for key in keys {
            if !self.inspector.paint_rows.iter().any(|r| r.key == key) {
                let input = cx.new(TextInput::new);
                let request_owner = owner.clone();
                let change = cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
                    if this.inspector.paint_row_owner.as_ref() == Some(&request_owner) {
                        this.edit_paint_row(key, event, cx);
                    }
                });
                let focus = input.read(cx).focus_handle(cx);
                let request_owner = owner.clone();
                let focused = cx.on_focus(&focus, window, move |this, window, cx| {
                    if this.inspector.paint_row_owner.as_ref() == Some(&request_owner)
                        && (this.paint_layer_index(key.stroke) != key.layer
                            || this.inspector.stroke_editing != key.stroke)
                    {
                        this.select_paint_layer(key.stroke, key.layer, window, cx);
                    }
                });
                let request_owner = owner.clone();
                let blur = cx.on_blur(&focus, window, move |this, _, cx| {
                    if this.inspector.paint_row_owner.as_ref() == Some(&request_owner) {
                        if let Some(row) = this.inspector.paint_rows.iter().find(|r| r.key == key)
                            && row.input.read(cx).value().as_ref() != row.last
                        {
                            let value = row.input.read(cx).value();
                            this.edit_paint_row(key, &InputEvent::Submit(value), cx);
                        }
                        this.sync_paint_row(key, true, cx);
                        this.history.borrow_mut().break_group();
                    }
                });
                self.inspector.paint_rows.push(RowInput {
                    key,
                    input,
                    invalid: false,
                    last: String::new(),
                    pending: Vec::new(),
                    _subscriptions: vec![change, focused, blur],
                });
            }
            self.sync_paint_row(key, false, cx);
        }
    }

    fn sync_paint_row(&mut self, key: Key, force: bool, cx: &mut Context<Self>) {
        if !self.inspector.paint_rows.iter().any(|r| r.key == key) {
            return;
        }
        let Some((value, ..)) = self.row_value(key) else {
            return;
        };
        let row = self
            .inspector
            .paint_rows
            .iter_mut()
            .find(|r| r.key == key)
            .unwrap();
        if force || row.last != value {
            row.last = value.clone();
            row.invalid = false;
            let value: gpui::SharedString = value.into();
            if row.input.read(cx).value() != value {
                row.pending.push(value.clone());
                row.input.update(cx, |input, cx| input.set_value(value, cx));
            }
        }
    }

    fn edit_paint_row(&mut self, key: Key, event: &InputEvent, cx: &mut Context<Self>) {
        if self.inspector.paint_row_owner.as_ref()
            != self
                .selected_shape
                .or(self.selected)
                .map(|id| (self.pages.active.clone(), id))
                .as_ref()
        {
            return;
        }
        if self.row_value(key).is_none() {
            return;
        }
        let Some(row) = self.inspector.paint_rows.iter_mut().find(|r| r.key == key) else {
            return;
        };
        let value = event.text();
        if matches!(event, InputEvent::Change(_)) {
            if let Some(i) = row.pending.iter().position(|v| v == value) {
                row.pending.remove(i);
                return;
            }
            if row.input.read(cx).value() != *value {
                return;
            }
        }
        let Some(id) = self
            .selected_shape
            .or(self.selected)
            .filter(|id| self.layer_editable(*id) && !self.preview_read_only())
        else {
            return;
        };
        self.inspector.paint_object = Some(id);
        self.inspector.paint_page = self.pages.active.clone();
        self.inspector.paint_layers[key.stroke as usize] = key.layer;
        self.inspector.stroke_editing = key.stroke;
        let property = self.field_property(key.index).unwrap();
        let stop = self.inspector.active_stop;
        self.history.borrow_mut().set_scope(
            Group::Property {
                id,
                property,
                stop,
                stroke: key.stroke,
            },
            false,
        );
        let valid = if let Some(valid) = self.edit_paint_layer_opacity(key, value) {
            valid
        } else if self.selected_shape.is_some() {
            self.edit_shape_paint_at(
                if key.stroke { 0 } else { key.layer },
                if key.stroke { key.layer } else { 0 },
                |s| properties::apply_shape_field(s, key.stroke, stop, property, value),
            )
            .unwrap_or(false)
        } else {
            self.edit_board_paint_at(key.layer, None, |b| {
                properties::apply_field(b, stop, property, value)
            })
            .unwrap_or(false)
        };
        self.history.borrow_mut().clear_scope();
        let current = self.row_value(key).map(|v| v.0).unwrap_or_default();
        let row = self
            .inspector
            .paint_rows
            .iter_mut()
            .find(|r| r.key == key)
            .unwrap();
        row.invalid = !valid;
        row.last = current;
        if matches!(event, InputEvent::Submit(_)) {
            self.sync_paint_row(key, true, cx);
            self.history.borrow_mut().break_group();
        }
        self.sync_fields(cx);
        cx.notify();
    }

    pub(super) fn row_field(&self, stroke: bool, layer: usize, index: usize) -> Option<Field> {
        let key = Key {
            stroke,
            layer,
            index,
        };
        let row = self.inspector.paint_rows.iter().find(|r| r.key == key)?;
        let (value, mode, swatch, image) = self.row_value(key)?;
        Some(Field {
            key,
            input: row.input.clone(),
            invalid: row.invalid,
            value,
            mode,
            swatch,
            image,
            active: self.paint_layer_index(stroke) == layer,
        })
    }
    pub(super) fn edit_paint_layer_opacity(&mut self, key: Key, value: &str) -> Option<bool> {
        if !matches!(key.index, 6 | 17) {
            return None;
        }
        let Some(value) = value
            .parse::<f32>()
            .ok()
            .filter(|v| (0. ..=100.).contains(v))
        else {
            return Some(false);
        };
        if self.selected_shape.is_some() {
            self.edit_shape_paint_at(
                if key.stroke { 0 } else { key.layer },
                if key.stroke { key.layer } else { 0 },
                |s| {
                    if key.stroke {
                        s.stroke.opacity = value / 100.;
                        if s.stroke.fill_mode == FillMode::Solid {
                            s.stroke.color.a = 1.;
                        }
                    } else {
                        s.fill_opacity = value / 100.;
                        match s.fill_mode {
                            FillMode::Solid => s.color.a = 1.,
                            FillMode::Image => s.image_fill.opacity = 1.,
                            _ => {}
                        }
                    }
                },
            );
        } else {
            self.edit_board_paint_at(key.layer, None, |b| {
                b.fill_opacity = value / 100.;
                match b.fill_mode {
                    FillMode::Solid => b.color.a = 1.,
                    FillMode::Image => b.image_fill.opacity = 1.,
                    _ => {}
                }
            });
        }
        Some(true)
    }

    pub(super) fn simple_paint_opacity_key(&self, index: usize) -> Option<Key> {
        if !matches!(index, 6 | 17)
            || self.action_paint().is_some()
            || self.selected_text.is_some()
            || !self.multi_selection.is_empty()
        {
            return None;
        }
        let stroke = index == 17;
        let mode = if let Some(s) = self.paint_shape() {
            s.paint_mode(stroke)
        } else {
            self.paint_board()?.fill_mode
        };
        matches!(mode, FillMode::Solid | FillMode::Image).then(|| Key {
            stroke,
            layer: self.paint_layer_index(stroke),
            index,
        })
    }
    pub(in crate::editor) fn paint_row_inputs(&self) -> impl Iterator<Item = &Entity<TextInput>> {
        self.inspector.paint_rows.iter().map(|r| &r.input)
    }
    pub(in crate::editor) fn paint_row_focus_removed(
        &self,
        window: &Window,
        cx: &gpui::App,
    ) -> bool {
        self.inspector.paint_rows.iter().any(|r| {
            r.input.focus_handle(cx).is_focused(window)
                && (self.row_value(r.key).is_none()
                    || self.selected_shape().is_some_and(|s| {
                        if r.key.stroke {
                            r.key.layer == 0 && !s.stroke.enabled
                        } else {
                            r.key.layer == 0 && !s.fill_enabled
                        }
                    })
                    || self.selected_board().is_some_and(|b| {
                        self.selected_shape.is_none() && r.key.layer == 0 && !b.fill_enabled
                    }))
        })
    }
}
