use super::*;
use crate::scene::auto_layout::Limits;
use crate::ui::theme::Color;

pub(super) struct State {
    pub expanded: bool,
    pub inputs: Vec<Entity<TextInput>>,
    invalid: [bool; 4],
    _subscriptions: Vec<Subscription>,
}

impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let mut subscriptions = Vec::new();
        let inputs = (0..4)
            .map(|index| {
                let input = cx.new(TextInput::new);
                subscriptions.push(cx.subscribe_in(
                    &input,
                    window,
                    move |this, _, event: &InputEvent, _, cx| {
                        if matches!(event, InputEvent::Submit(_)) {
                            this.apply_size_limit(index, cx);
                        }
                    },
                ));
                subscriptions.push(cx.on_blur(
                    &input.focus_handle(cx),
                    window,
                    move |this, _, cx| this.apply_size_limit(index, cx),
                ));
                input
            })
            .collect();
        Self {
            expanded: false,
            inputs,
            invalid: [false; 4],
            _subscriptions: subscriptions,
        }
    }
}

impl Workspace {
    fn size_limit_target(&self) -> Option<usize> {
        self.layout_target().filter(|id| {
            !self.hierarchy.groups.contains_key(id) || self.hierarchy.layouts.contains_key(id)
        })
    }

    pub(super) fn refresh_limit_inputs(&mut self, cx: &mut Context<Self>) {
        let limits = self
            .size_limit_target()
            .and_then(|id| self.hierarchy.sizing.get(&id))
            .map(|s| s.limits)
            .unwrap_or_default();
        self.auto_layout.limits.invalid = [false; 4];
        for (input, value) in self.auto_layout.limits.inputs.iter().zip([
            limits.min_width,
            limits.max_width,
            limits.min_height,
            limits.max_height,
        ]) {
            input.update(cx, |input, cx| {
                input.set_placeholder(t("layout-no-limit"));
                input.set_value(value.map(inspector::number).unwrap_or_default(), cx);
            });
        }
    }

    pub(super) fn apply_size_limit(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.auto_layout.target != self.layout_input_target() {
            return;
        }
        let Some(id) = self
            .size_limit_target()
            .filter(|id| self.layer_editable(*id))
        else {
            return;
        };
        let raw = self.auto_layout.limits.inputs[index]
            .read(cx)
            .value()
            .to_string();
        let value = if raw.trim().is_empty() {
            Ok(None)
        } else {
            raw.trim().parse::<f32>().map(Some)
        };
        let Ok(value) = value else {
            self.auto_layout.limits.invalid[index] = true;
            cx.notify();
            return;
        };
        let mut limits = self
            .hierarchy
            .sizing
            .get(&id)
            .map(|s| s.limits)
            .unwrap_or_default();
        match index {
            0 => limits.min_width = value,
            1 => limits.max_width = value,
            2 => limits.min_height = value,
            3 => limits.max_height = value,
            _ => unreachable!(),
        }
        if limits.validate().is_err() {
            self.auto_layout.limits.invalid[index] = true;
            cx.notify();
            return;
        }
        self.set_size_limits(id, limits, cx);
    }

    fn set_size_limits(&mut self, id: usize, limits: Limits, cx: &mut Context<Self>) {
        if !self.layer_editable(id) {
            return;
        }
        let current = self
            .hierarchy
            .sizing
            .get(&id)
            .map(|s| s.limits)
            .unwrap_or_default();
        if current != limits {
            let before = self.page_edit(std::slice::from_ref(&self.pages.active), cx);
            let sizing = self.hierarchy.sizing.entry(id).or_default();
            sizing.limits = limits;
            if *sizing == Sizing::default() {
                self.hierarchy.sizing.remove(&id);
            }
            self.record_page_edit(before);
            self.auto_layout.revision = None;
            self.reflow_layout(cx);
        }
        self.refresh_limit_inputs(cx);
        cx.notify();
    }

    pub(in crate::editor) fn size_limit_controls(&self, cx: &mut Context<Self>) -> Div {
        let Some(id) = self.size_limit_target() else {
            return div();
        };
        let has_limits = self
            .hierarchy
            .sizing
            .get(&id)
            .is_some_and(|s| !s.limits.is_empty());
        let expanded = self.auto_layout.limits.expanded;
        let section =
            div()
                .flex_shrink_0()
                .px(px(14.))
                .py(px(10.))
                .border_b_1()
                .border_color(BORDER.color())
                .flex()
                .flex_col()
                .text_size(px(12.))
                .font_weight(FontWeight::NORMAL)
                .line_height(px(16.))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .id("toggle-size-limits")
                                .debug_selector(|| "toggle-size-limits".into())
                                .h(px(24.))
                                .flex_1()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .cursor_pointer()
                                .text_color(if has_limits {
                                    TEXT.color()
                                } else {
                                    MUTED.color()
                                })
                                .child(crate::ui::disclosure_icon(
                                    format!("size-limits-chevron-{}-{id}", self.pages.active),
                                    expanded,
                                    13.,
                                    TEXT.color(),
                                ))
                                .child(t("layout-size-limits"))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if let Some(index) =
                                        this.auto_layout.limits.inputs.iter().position(|input| {
                                            input.focus_handle(cx).is_focused(window)
                                        })
                                    {
                                        this.apply_size_limit(index, cx);
                                    }
                                    this.focus.focus(window, cx);
                                    this.auto_layout.limits.expanded =
                                        !this.auto_layout.limits.expanded;
                                    cx.notify();
                                })),
                        )
                        .when(has_limits, |el| {
                            el.child(
                                div()
                                    .id("clear-size-limits")
                                    .size(px(24.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(5.))
                                    .cursor_pointer()
                                    .text_color(MUTED.color())
                                    .hover(|s| s.bg(Color::Input.color()))
                                    .child(icon(LucideIcons::Minus, 13.))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.set_size_limits(id, Limits::default(), cx)
                                    })),
                            )
                        }),
                );
        let fields = self.size_limit_fields();
        section.child(
            gpui_effects::animated_collapse(
                format!("size-limits-{}-{id}", self.pages.active),
                expanded,
                move || fields,
            )
            .duration(crate::ui::DISCLOSURE_DURATION),
        )
    }

    fn size_limit_fields(&self) -> Div {
        let mut section = div().flex().flex_col().gap(px(8.)).pt(px(8.));
        section = section.child(div().flex().gap(px(8.)).pl(px(26.)).children(
            ["layout-minimum", "layout-maximum"].map(|label| {
                div()
                    .flex_1()
                    .text_color(MUTED.color())
                    .text_size(px(10.))
                    .child(t(label))
            }),
        ));
        for (axis, label) in [(0, "W"), (1, "H")] {
            section = section.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().w(px(18.)).text_color(MUTED.color()).child(label))
                    .children((axis * 2..axis * 2 + 2).map(|index| {
                        let invalid = self.auto_layout.limits.invalid[index];
                        div()
                            .id(("size-limit", index))
                            .debug_selector(move || format!("size-limit-{index}"))
                            .flex_1()
                            .min_w_0()
                            .child(
                                Input::new(&self.auto_layout.limits.inputs[index])
                                    .w_full()
                                    .h(px(28.))
                                    .px(px(8.))
                                    .py_0()
                                    .rounded(px(6.))
                                    .text_size(px(12.))
                                    .text_color(TEXT.color())
                                    .bg(Color::Input.color())
                                    .border_1()
                                    .border_color(if invalid {
                                        Color::Danger.color()
                                    } else {
                                        BORDER.color()
                                    })
                                    .appearance(InputAppearance {
                                        focus_border: if invalid {
                                            Color::Danger.color()
                                        } else {
                                            ACCENT.color()
                                        }
                                        .into(),
                                        ..crate::ui::theme::input_appearance()
                                    }),
                            )
                    })),
            );
        }
        section.when(
            self.auto_layout
                .limits
                .invalid
                .iter()
                .any(|invalid| *invalid),
            |el| {
                el.child(
                    div()
                        .text_size(px(10.))
                        .text_color(Color::Danger.color())
                        .child(t("layout-invalid-limits")),
                )
            },
        )
    }
}
