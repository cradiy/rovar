use super::*;
use crate::scene::presentation::property::{self, Align, Direction, Property};
use std::cell::RefCell;
use uic::components::{
    dropdown::{DropdownPlacement, dropdown},
    input::{Input, InputAppearance},
};

type Target = (usize, usize, Trigger, Action);

pub(super) struct State {
    input: Entity<TextInput>,
    target: RefCell<Option<Target>>,
    menus: [Entity<DropdownState>; 2],
    _subscriptions: Vec<Subscription>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let input = cx.new(TextInput::new);
        let submit = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Submit(_)) {
                this.apply_property_value(window, cx);
            }
        });
        let blur = cx.on_blur(&input.focus_handle(cx), window, |this, window, cx| {
            this.apply_property_value(window, cx)
        });
        Self {
            input,
            target: RefCell::new(None),
            menus: std::array::from_fn(|_| cx.new(|cx| DropdownState::new(window, cx))),
            _subscriptions: vec![submit, blur],
        }
    }
}

fn label(property: &Property) -> &'static str {
    use Property::*;
    t(match property {
        Visible(_) => "prototype-property-visible",
        Opacity(_) => "prototype-property-opacity",
        X(_) => "prototype-property-x",
        Y(_) => "prototype-property-y",
        Width(_) => "prototype-property-width",
        Height(_) => "prototype-property-height",
        Rotation(_) => "prototype-property-rotation",
        Fill(_) => "prototype-property-fill",
        FillEnabled(_) => "prototype-property-fill-enabled",
        Stroke(_) => "prototype-property-stroke",
        StrokeWidth(_) => "prototype-property-stroke-width",
        StrokeEnabled(_) => "prototype-property-stroke-enabled",
        Radius(_) => "prototype-property-radius",
        Content(_) => "prototype-property-content",
        FontFamily(_) => "prototype-property-font-family",
        FontSize(_) => "prototype-property-font-size",
        FontWeight(_) => "prototype-property-font-weight",
        LineHeight(_) => "prototype-property-line-height",
        LetterSpacing(_) => "prototype-property-letter-spacing",
        TextAlign(_) => "prototype-property-text-align",
        Gap(_) => "prototype-property-gap",
        LineGap(_) => "prototype-property-line-gap",
        Padding(_) => "prototype-property-padding",
        LayoutDirection(_) => "prototype-property-layout-direction",
        LayoutWrap(_) => "prototype-property-layout-wrap",
    })
}

fn value(property: &Property) -> String {
    if let Property::Fill(paint) | Property::Stroke(paint) = property {
        return paint.color.clone();
    }
    serde_json::to_value(property)
        .unwrap()
        .get("value")
        .map_or_else(String::new, |v| {
            v.as_str().map_or_else(|| v.to_string(), str::to_owned)
        })
}

fn choices(property: &Property) -> Vec<(Property, &'static str)> {
    use crate::scene::presentation::property::Boolean::{Set, Toggle};
    use Property::*;
    match property {
        Visible(_) => vec![
            (Visible(Set(true)), t("prototype-show")),
            (Visible(Set(false)), t("prototype-hide")),
            (Visible(Toggle), t("prototype-toggle")),
        ],
        FillEnabled(_) => vec![
            (FillEnabled(Set(true)), t("prototype-enabled")),
            (FillEnabled(Set(false)), t("prototype-disabled")),
            (FillEnabled(Toggle), t("prototype-toggle")),
        ],
        StrokeEnabled(_) => vec![
            (StrokeEnabled(Set(true)), t("prototype-enabled")),
            (StrokeEnabled(Set(false)), t("prototype-disabled")),
            (StrokeEnabled(Toggle), t("prototype-toggle")),
        ],
        LayoutWrap(_) => vec![
            (LayoutWrap(Set(true)), t("prototype-enabled")),
            (LayoutWrap(Set(false)), t("prototype-disabled")),
            (LayoutWrap(Toggle), t("prototype-toggle")),
        ],
        _ => Vec::new(),
    }
}

impl Workspace {
    #[cfg(test)]
    pub(in crate::editor) fn prototype_property_number(
        &self,
        id: usize,
        trigger: Trigger,
    ) -> Option<f32> {
        match self.hierarchy.interactions.get(&id)?.get(trigger)? {
            Action::SetProperty { property, .. } => serde_json::to_value(&property)
                .ok()?
                .get("value")?
                .as_f64()
                .map(|v| {
                    if matches!(property, Property::Opacity(_)) {
                        v as f32 * 100.
                    } else {
                        v as f32
                    }
                }),
            _ => None,
        }
    }

    pub(super) fn prototype_property_targets(&self, cx: &gpui::App) -> Vec<(usize, String)> {
        self.layer_ids()
            .into_iter()
            .map(|id| {
                let mut names = self
                    .ancestors(id)
                    .into_iter()
                    .map(|parent| self.layer_name(parent, cx))
                    .collect::<Vec<_>>();
                names.reverse();
                names.push(self.layer_name(id, cx));
                (id, names.join(" / "))
            })
            .collect()
    }

    fn apply_property_value(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((generation, id, trigger, action)) =
            self.presentation.properties.target.borrow().clone()
        else {
            return;
        };
        if generation != self.pages.generation
            || self.selection_ids() != BTreeSet::from([id])
            || self.presentation.trigger != trigger
            || self
                .hierarchy
                .interactions
                .get(&id)
                .and_then(|i| i.get(trigger))
                .as_ref()
                != Some(&action)
        {
            return;
        }
        let Action::SetProperty { target, property } = action else {
            return;
        };
        let input = self
            .presentation
            .properties
            .input
            .read(cx)
            .value()
            .to_string();
        let mut encoded = serde_json::to_value(&property).unwrap();
        if encoded["value"].is_string() {
            encoded["value"] = input.into();
        } else if let Ok(number) = input.trim().parse::<f32>() {
            encoded["value"] = serde_json::json!(if matches!(property, Property::Opacity(_)) {
                number / 100.
            } else {
                number
            });
        }
        let updated = serde_json::from_value::<Property>(encoded)
            .ok()
            .filter(Property::valid)
            .filter(|property| match property {
                Property::FontFamily(family) => {
                    family == crate::ui::font::SYSTEM
                        || cx
                            .text_system()
                            .all_font_names()
                            .iter()
                            .any(|name| name == family)
                }
                _ => true,
            });
        let selected = updated.unwrap_or(property);
        self.set_prototype_action(
            id,
            trigger,
            Some(Action::SetProperty {
                target,
                property: selected.clone(),
            }),
            window,
            cx,
        );
        self.presentation
            .properties
            .input
            .update(cx, |input, cx| input.set_value(value(&selected), cx));
    }

    pub(super) fn property_action_controls(
        &self,
        id: usize,
        trigger: Trigger,
        action: &Option<Action>,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let Some(action @ Action::SetProperty { target, property }) = action else {
            self.presentation.properties.target.replace(None);
            return None;
        };
        let key = Some((self.pages.generation, id, trigger, action.clone()));
        if *self.presentation.properties.target.borrow() != key {
            self.presentation.properties.target.replace(key);
            self.presentation
                .properties
                .input
                .update(cx, |input, cx| input.set_value(value(property), cx));
        }
        let page = self.snapshot_page(cx).0;
        let fields = property::defaults(&page, *target)
            .into_iter()
            .map(|default| {
                let property = if default.key() == property.key() {
                    property.clone()
                } else {
                    default
                };
                let name = label(&property);
                (property, name)
            })
            .collect();
        let mut el = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .child(t("prototype-property")),
            )
            .child(self.property_menu((id, *target), trigger, property, 0, fields, cx));
        if matches!(property, Property::Fill(_) | Property::Stroke(_)) {
            return Some(el.child(self.paint_value_row(cx)));
        }
        if self.action_numeric_property().is_some() {
            return Some(el.child(self.property_field(18, label(property), cx)));
        }
        if matches!(property, Property::FontFamily(_)) {
            return Some(el.child(self.inspector.font_picker.clone()));
        }
        if let Property::TextAlign(align) = property {
            return Some(el.child(self.text_alignment_control(
                Some(match align {
                    Align::Left => gpui::TextAlign::Left,
                    Align::Center => gpui::TextAlign::Center,
                    Align::Right => gpui::TextAlign::Right,
                }),
                cx,
            )));
        }
        if let Property::LayoutDirection(direction) = property {
            use crate::scene::auto_layout::Axis;
            return Some(el.child(self.layout_direction_control(
                match direction {
                    Direction::Horizontal => Axis::Horizontal,
                    Direction::Vertical => Axis::Vertical,
                    Direction::Grid => Axis::Grid,
                },
                cx,
            )));
        }
        let options = choices(property);
        el = el.child(div().text_size(px(12.)).text_color(MUTED.color()).child(t(
            if matches!(property, Property::Opacity(_)) {
                "prototype-property-opacity-value"
            } else {
                "prototype-property-value"
            },
        )));
        if options.is_empty() {
            el = el.child(
                div()
                    .h(px(32.))
                    .w_full()
                    .min_w_0()
                    .rounded(px(6.))
                    .bg(Color::Input.color())
                    .border_1()
                    .border_color(Color::Hover.color())
                    .flex()
                    .items_center()
                    .child(
                        div()
                            .id("prototype-property-input")
                            .debug_selector(|| "prototype-property-input".into())
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .child(
                                Input::new(&self.presentation.properties.input)
                                    .w_full()
                                    .h(px(30.))
                                    .px(px(4.))
                                    .rounded(px(4.))
                                    .bg(Color::Input.color())
                                    .border_color(Color::Input.color())
                                    .text_size(px(12.))
                                    .text_color(TEXT.color())
                                    .appearance(InputAppearance {
                                        caret_height: px(16.),
                                        ..crate::ui::theme::input_appearance()
                                    }),
                            ),
                    ),
            );
        } else {
            el = el.child(self.property_menu((id, *target), trigger, property, 1, options, cx));
        }
        Some(el)
    }

    fn property_menu(
        &self,
        (id, target): (usize, usize),
        trigger: Trigger,
        current: &Property,
        index: usize,
        options: Vec<(Property, &'static str)>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let title = if index == 0 {
            label(current)
        } else {
            options
                .iter()
                .find(|(p, _)| p == current)
                .map_or("", |(_, label)| *label)
        };
        let current = current.clone();
        dropdown(&self.presentation.properties.menus[index])
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
                    .id(("prototype-property-menu", index))
                    .debug_selector(move || format!("prototype-property-menu-{index}"))
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
                    .child(div().flex_1().truncate().child(title))
                    .child(icon(LucideIcons::ChevronDown, 12.)),
            )
            .menu(
                div()
                    .id(("prototype-property-options", index))
                    .max_h(px(300.))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                    .children(
                        options
                            .into_iter()
                            .enumerate()
                            .map(|(i, (property, label))| {
                                let selected = property == current;
                                div()
                                    .id(("prototype-property-option", i))
                                    .debug_selector(move || {
                                        format!("prototype-property-option-{index}-{i}")
                                    })
                                    .h(px(28.))
                                    .flex_shrink_0()
                                    .px(px(8.))
                                    .rounded(px(5.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .cursor_pointer()
                                    .when(selected, |el| {
                                        el.bg(Color::Selected.color()).text_color(ACCENT.color())
                                    })
                                    .hover(|s| s.bg(Color::Hover.color()))
                                    .child(
                                        div().w(px(14.)).children(
                                            selected.then(|| icon(LucideIcons::Check, 14.)),
                                        ),
                                    )
                                    .child(label)
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.presentation.properties.menus[index]
                                            .update(cx, |menu, cx| menu.close(window, cx));
                                        this.set_prototype_action(
                                            id,
                                            trigger,
                                            Some(Action::SetProperty {
                                                target,
                                                property: property.clone(),
                                            }),
                                            window,
                                            cx,
                                        );
                                    }))
                            }),
                    ),
            )
    }
}
