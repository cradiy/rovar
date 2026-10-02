use super::super::inspector::{hex, number};
use super::*;
use crate::i18n::t;
use crate::scene::artboard::{MAX_SIZE, MIN_SIZE};
use crate::scene::property::Property::*;

impl Workspace {
    pub(in crate::editor) fn batch_color_supported(&self, cx: &gpui::App) -> bool {
        self.operation_ids().into_iter().all(|id| {
            if let Some(t) = self.texts.iter().find(|t| t.id == id) {
                let e = t.editor.read(cx);
                e.effective_style().fill_mode == FillMode::Solid && !e.mixed(TextProperty::FillMode)
            } else if let Some(s) = self.shapes.iter().find(|s| s.id == id) {
                !s.kind.is_media()
                    && s.paint_mode(!s.can_fill()) == FillMode::Solid
                    && (if s.can_fill() {
                        s.fill_enabled
                    } else {
                        s.stroke.enabled
                    })
            } else {
                self.boards
                    .iter()
                    .any(|b| b.id == id && b.fill_mode == FillMode::Solid)
            }
        })
    }

    pub(in crate::editor) fn multi_picker_color(&self, cx: &gpui::App) -> Option<gpui::Rgba> {
        self.object_color(*self.operation_ids().first()?, cx)
    }

    fn object_color(&self, id: usize, cx: &gpui::App) -> Option<gpui::Rgba> {
        self.boards
            .iter()
            .find(|b| b.id == id)
            .map(|b| b.color)
            .or_else(|| {
                self.shapes.iter().find(|s| s.id == id).map(|s| {
                    if s.can_fill() {
                        s.color
                    } else {
                        s.stroke.color
                    }
                })
            })
            .or_else(|| {
                self.texts
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.editor.read(cx).effective_style().color)
            })
    }

    fn object_value(&self, id: usize, property: Property, cx: &gpui::App) -> Option<String> {
        if property.is_geometry() {
            let r = self.world_rect(id)?;
            return Some(number(match property {
                X => r.x,
                Y => r.y,
                Width => r.width,
                _ => r.height,
            }));
        }
        if matches!(property, Color | Opacity) {
            if self
                .texts
                .iter()
                .find(|t| t.id == id)
                .is_some_and(|t| t.editor.read(cx).mixed(property.text_style().unwrap()))
            {
                return Some(String::new());
            }
            let c = self.object_color(id, cx)?;
            return Some(if property == Color {
                hex(c)
            } else {
                number(c.a * 100.)
            });
        }
        None
    }

    pub(in crate::editor) fn multi_field_value(
        &self,
        property: Property,
        cx: &gpui::App,
    ) -> Option<String> {
        if !(property.is_geometry() || matches!(property, Color | Opacity))
            || (matches!(property, Color | Opacity) && !self.batch_color_supported(cx))
        {
            return None;
        }
        let property_ids = self.operation_ids();
        let mut values = property_ids
            .iter()
            .map(|id| self.object_value(*id, property, cx));
        let first = values.next()??;
        Some(if values.all(|value| value.as_ref() == Some(&first)) {
            first
        } else {
            String::new()
        })
    }

    pub(in crate::editor) fn multi_can_scrub(&self, property: Property, cx: &gpui::App) -> bool {
        (property.is_geometry() || (property == Opacity && self.batch_color_supported(cx)))
            && self.operation_ids().iter().all(|id| {
                self.object_value(*id, property, cx)
                    .is_some_and(|v| v.parse::<f32>().is_ok())
            })
    }

    fn before_batch_property(&self, property: Property, cx: &gpui::App) -> Vec<Change> {
        self.before_geometry()
            .into_iter()
            .map(|change| match change {
                Change::TextRect { id, .. } if matches!(property, Color | Opacity) => {
                    Change::Text {
                        id,
                        value: self
                            .texts
                            .iter()
                            .find(|t| t.id == id)
                            .unwrap()
                            .editor
                            .read(cx)
                            .snapshot(),
                    }
                }
                other => other,
            })
            .collect()
    }

    fn set_batch_numeric(
        &mut self,
        id: usize,
        property: Property,
        value: f32,
        cx: &mut Context<Self>,
    ) {
        if property.is_geometry() {
            if let Some((parent, mut rect)) = self.object_rect(id) {
                let origin = self.parent_origin(parent);
                match property {
                    X => rect.x = value - origin.x,
                    Y => rect.y = value - origin.y,
                    _ => {
                        editing::set_dimension(
                            &mut rect,
                            property,
                            value,
                            self.layer_info(id).is_some_and(|(s, _)| s.aspect_locked),
                        );
                    }
                }
                self.set_object_rect(id, parent, rect);
                if self.hierarchy.layouts.contains_key(&id) && matches!(property, Width | Height) {
                    let sizing = self.hierarchy.sizing.entry(id).or_default();
                    if property == Width {
                        sizing.width = crate::scene::auto_layout::Mode::Fixed;
                    } else {
                        sizing.height = crate::scene::auto_layout::Mode::Fixed;
                    }
                }
            }
        } else if property == Opacity {
            let mut color = self.object_color(id, cx).unwrap();
            color.a = value / 100.;
            self.set_batch_color(id, color, true, cx);
        }
    }

    fn set_batch_color(
        &mut self,
        id: usize,
        color: gpui::Rgba,
        alpha_only: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(t) = self.texts.iter().find(|t| t.id == id) {
            t.editor.update(cx, |e, cx| {
                e.apply_batch_style(
                    if alpha_only {
                        StyleChange::Opacity(color.a)
                    } else {
                        StyleChange::Color(color)
                    },
                    cx,
                )
            });
        } else {
            let target = if let Some(b) = self.boards.iter_mut().find(|b| b.id == id) {
                b.color_style = None;
                Some(&mut b.color)
            } else {
                self.shapes.iter_mut().find(|s| s.id == id).map(|s| {
                    if s.can_fill() {
                        s.color_style = None;
                        &mut s.color
                    } else {
                        s.stroke.color_style = None;
                        &mut s.stroke.color
                    }
                })
            };
            if let Some(target) = target {
                if alpha_only {
                    target.a = color.a;
                } else {
                    target.r = color.r;
                    target.g = color.g;
                    target.b = color.b;
                }
            }
        }
    }

    pub(in crate::editor) fn edit_multi_field(
        &mut self,
        property: Property,
        value: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !(property.is_geometry() || matches!(property, Color | Opacity))
            || (matches!(property, Color | Opacity) && !self.batch_color_supported(cx))
        {
            return false;
        }
        let before = self.before_batch_property(property, cx);
        if property == Color {
            let value = value.trim().trim_start_matches('#');
            if value.len() != 6 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
                return false;
            }
            let Ok(color) = u32::from_str_radix(value, 16) else {
                return false;
            };
            for id in self.operation_ids() {
                self.set_batch_color(id, rgb(color), false, cx);
            }
        } else {
            let Ok(value) = value.trim().parse::<f32>() else {
                return false;
            };
            let (min, max) = multi_limits(property);
            if !value.is_finite() || !(min..=max).contains(&value) {
                return false;
            }
            for id in self.operation_ids() {
                self.set_batch_numeric(id, property, value, cx);
            }
        }
        if self.batch_changed(&before, cx) {
            self.history.borrow_mut().record(
                before,
                Some(Group::SelectionProperty(
                    self.operation_ids().iter().copied().collect(),
                    property,
                )),
            );
        }
        cx.notify();
        true
    }

    pub(in crate::editor) fn begin_multi_property(
        &mut self,
        property: Property,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.multi_can_scrub(property, cx) {
            return;
        }
        self.batch_values = self
            .operation_ids()
            .iter()
            .map(|id| {
                (
                    *id,
                    self.object_value(*id, property, cx)
                        .unwrap()
                        .parse()
                        .unwrap(),
                )
            })
            .collect();
        self.batch_before = self.before_batch_property(property, cx);
        self.begin(
            GestureKind::MultiProperty { property },
            event.position,
            event.button,
            window,
            cx,
        );
    }

    pub(in crate::editor) fn move_multi_property(
        &mut self,
        property: Property,
        delta: f32,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        if delta.abs() < 3. && !self.batch_changed(&self.batch_before, cx) {
            return;
        }
        let (min, max) = multi_limits(property);
        for (id, original) in self.batch_values.clone() {
            let value = (original + (delta * if shift { 10. } else { 1. }).round()).clamp(min, max);
            self.set_batch_numeric(id, property, value, cx);
        }
        self.sync_fields(cx);
    }

    pub(in crate::editor) fn finish_multi_property(&mut self, cx: &mut Context<Self>) {
        let before = std::mem::take(&mut self.batch_before);
        self.batch_values.clear();
        if self.batch_changed(&before, cx) {
            self.history.borrow_mut().record(before, None);
        }
        self.sync_fields(cx);
    }

    pub(in crate::editor) fn multi_properties(
        &self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let section = super::super::inspector::inspector_section;
        let mut actions =
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(div().flex_1().text_size(px(12.)).child(crate::i18n::count(
                    "selection-count",
                    self.operation_ids().len(),
                )));
        for (id, glyph, label, delete) in [
            (
                "duplicate-selection",
                LucideIcons::Copy,
                t("duplicate-hint"),
                false,
            ),
            (
                "delete-selection",
                LucideIcons::Trash2,
                t("delete-selection"),
                true,
            ),
        ] {
            actions = actions.child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .size(px(28.))
                    .rounded(px(5.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|s| s.bg(gpui::rgba(0xb4a2ee22)))
                    .tooltip(move |_, cx| cx.new(|_| toolbar::ToolTip(label.into())).into())
                    .child(icon(glyph, 15.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus.focus(window, cx);
                        if delete {
                            this.delete_selected(cx);
                        } else {
                            this.duplicate_selection(window, cx);
                        }
                    })),
            );
        }
        div()
            .id("multi-properties")
            .track_scroll(&self.inspector.inspector_scroll)
            .debug_selector(|| "multi-properties".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .child(div().p(px(14.)).flex_shrink_0().child(actions))
            .child(self.auto_layout_controls(cx))
            .child(
                div()
                    .px(px(14.))
                    .pb(px(10.))
                    .flex_shrink_0()
                    .child(self.alignment_controls(cx)),
            )
            .child(
                section(t("position-canvas")).child(
                    div()
                        .flex()
                        .gap(px(8.))
                        .child(self.property_field(1, "X", cx))
                        .child(self.property_field(2, "Y", cx)),
                ),
            )
            .child(
                section(t("object-dimensions"))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(self.property_field(3, t("width"), cx))
                            .child(self.property_field(4, t("height"), cx)),
                    )
                    .children(self.layout_position_control(cx)),
            )
            .when(self.batch_color_supported(cx), |el| {
                el.child(section(t("color")).child(self.paint_value_row(cx)))
            })
            .child(self.export_properties(cx))
    }
}

fn multi_limits(property: Property) -> (f32, f32) {
    match property {
        X | Y => (-1_000_000., 1_000_000.),
        Width | Height => (MIN_SIZE, MAX_SIZE),
        _ => (0., 100.),
    }
}
