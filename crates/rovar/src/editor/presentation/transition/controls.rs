use super::*;
use crate::scene::presentation::Easing;
use uic::components::input::{Input, InputAppearance};

type Target = (usize, usize, Trigger, Transition);

pub(in crate::editor::presentation) struct Controls {
    duration: Entity<TextInput>,
    target: RefCell<Option<Target>>,
    invalid: Cell<bool>,
    easing_menu: Entity<DropdownState>,
    _subscriptions: Vec<Subscription>,
}

impl Controls {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let duration = cx.new(TextInput::new);
        let submit = cx.subscribe_in(
            &duration,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if matches!(event, InputEvent::Submit(_)) {
                    this.apply_transition_duration(window, cx);
                }
            },
        );
        let blur = cx.on_blur(&duration.focus_handle(cx), window, |this, window, cx| {
            this.apply_transition_duration(window, cx)
        });
        Self {
            duration,
            target: RefCell::new(None),
            invalid: Cell::new(false),
            easing_menu: cx.new(|cx| DropdownState::new(window, cx)),
            _subscriptions: vec![submit, blur],
        }
    }
}

pub(super) fn label(easing: Easing) -> &'static str {
    t(match easing {
        Easing::Linear => "prototype-easing-linear",
        Easing::EaseIn => "prototype-easing-in",
        Easing::EaseOut => "prototype-easing-out",
        Easing::EaseInOut => "prototype-easing-in-out",
    })
}

fn curve(easing: Easing) -> impl IntoElement {
    gpui::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let mut path = gpui::PathBuilder::stroke(px(1.5));
            for i in 0..=20 {
                let t = i as f32 / 20.;
                let point = bounds.origin
                    + point(
                        bounds.size.width * t,
                        bounds.size.height * (1. - easing.sample(t)),
                    );
                if i == 0 {
                    path.move_to(point);
                } else {
                    path.line_to(point);
                }
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, ACCENT.color());
            }
        },
    )
    .w(px(28.))
    .h(px(15.))
    .flex_shrink_0()
}

impl Workspace {
    pub(in crate::editor) fn commit_transition_duration(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .presentation
            .timing
            .duration
            .focus_handle(cx)
            .is_focused(window)
        {
            self.apply_transition_duration(window, cx);
        }
    }

    fn apply_transition_duration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((generation, id, trigger, expected)) = *self.presentation.timing.target.borrow()
        else {
            return;
        };
        if generation != self.pages.generation || self.prototype_transition(id, trigger) != expected
        {
            return;
        }
        let value = self
            .presentation
            .timing
            .duration
            .read(cx)
            .value()
            .trim()
            .parse::<u16>()
            .ok();
        if let Some(value) = value.filter(|value| expected.with_duration(*value).valid()) {
            self.presentation.timing.invalid.set(false);
            self.set_prototype_transition(id, trigger, expected.with_duration(value), window, cx);
        } else {
            self.presentation.timing.invalid.set(true);
        }
        let duration = self
            .prototype_transition(id, trigger)
            .duration_ms()
            .unwrap_or(0);
        self.presentation
            .timing
            .duration
            .update(cx, |input, cx| input.set_value(duration.to_string(), cx));
        cx.notify();
    }

    pub(in crate::editor::presentation) fn transition_timing_controls(
        &self,
        id: usize,
        trigger: Trigger,
        current: Transition,
        cx: &mut Context<Self>,
    ) -> Div {
        let duration = current.duration_ms().unwrap();
        let state = &self.presentation.timing;
        let key = Some((self.pages.generation, id, trigger, current));
        if *state.target.borrow() != key {
            *state.target.borrow_mut() = key;
            state.invalid.set(false);
            state
                .duration
                .update(cx, |input, cx| input.set_value(duration.to_string(), cx));
        }
        div()
            .mt(px(10.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(authoring::caption(t("prototype-duration")).w(px(68.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h(px(32.))
                            .rounded(px(6.))
                            .bg(Color::Input.color())
                            .flex()
                            .items_center()
                            .child(
                                div()
                                    .id("prototype-duration-input")
                                    .debug_selector(|| "prototype-duration-input".into())
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        Input::new(&state.duration)
                                            .blur_on_click_outside(false)
                                            .w_full()
                                            .h(px(30.))
                                            .px(px(6.))
                                            .rounded(px(4.))
                                            .border_0()
                                            .text_size(px(12.))
                                            .bg(Color::Input.color())
                                            .text_color(if state.invalid.get() {
                                                Color::Danger.color()
                                            } else {
                                                TEXT.color()
                                            })
                                            .appearance(InputAppearance {
                                                caret_height: px(16.),
                                                ..crate::ui::theme::input_appearance()
                                            }),
                                    ),
                            )
                            .child(authoring::caption("ms"))
                            .child(
                                dropdown(&self.presentation.menus[3])
                                    .placement(DropdownPlacement::BottomEnd)
                                    .w(px(140.))
                                    .p(px(5.))
                                    .rounded(px(8.))
                                    .bg(Color::Input.color())
                                    .text_color(TEXT.color())
                                    .border_color(BORDER.color())
                                    .trigger(
                                        div()
                                            .id("prototype-duration")
                                            .debug_selector(|| "prototype-duration".into())
                                            .size(px(28.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(icon(GpuiIcons::LucideChevronDown, 12.)),
                                    )
                                    .menu(div().flex().flex_col().children(
                                        [100u16, 200, 300, 500, 1000, 2000].map(|ms| {
                                            div()
                                                .id(("prototype-duration-option", ms as usize))
                                                .debug_selector(move || {
                                                    format!("prototype-duration-{ms}")
                                                })
                                                .h(px(28.))
                                                .px(px(8.))
                                                .rounded(px(5.))
                                                .flex()
                                                .items_center()
                                                .gap(px(6.))
                                                .cursor_pointer()
                                                .text_size(px(12.))
                                                .hover(|s| s.bg(Color::Hover.color()))
                                                .child(
                                                    icon(GpuiIcons::LucideCheck, 12.).opacity(
                                                        if ms == duration { 1. } else { 0. },
                                                    ),
                                                )
                                                .child(format!("{ms} ms"))
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.presentation.menus[3]
                                                            .update(cx, |menu, cx| {
                                                                menu.close(window, cx)
                                                            });
                                                        let current =
                                                            this.prototype_transition(id, trigger);
                                                        this.set_prototype_transition(
                                                            id,
                                                            trigger,
                                                            current.with_duration(ms),
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                ))
                                        }),
                                    )),
                            ),
                    ),
            )
            .when(state.invalid.get(), |el| {
                el.child(
                    div()
                        .text_size(px(10.))
                        .text_color(Color::Danger.color())
                        .child(t("prototype-duration-range")),
                )
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(authoring::caption(t("prototype-easing")).w(px(68.)))
                    .child(
                        div().flex_1().min_w_0().child(
                            dropdown(&state.easing_menu)
                                .placement(DropdownPlacement::BottomEnd)
                                .w(px(202.))
                                .p(px(5.))
                                .rounded(px(8.))
                                .bg(Color::Input.color())
                                .text_color(TEXT.color())
                                .border_color(BORDER.color())
                                .trigger(
                                    div()
                                        .id("prototype-easing-menu")
                                        .debug_selector(|| "prototype-easing-menu".into())
                                        .h(px(34.))
                                        .px(px(8.))
                                        .rounded(px(6.))
                                        .bg(Color::Input.color())
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .text_size(px(11.))
                                        .child(curve(current.easing()))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .child(label(current.easing())),
                                        )
                                        .child(icon(GpuiIcons::LucideChevronDown, 12.)),
                                )
                                .menu(
                                    div().flex().flex_col().children(
                                        [
                                            Easing::Linear,
                                            Easing::EaseIn,
                                            Easing::EaseOut,
                                            Easing::EaseInOut,
                                        ]
                                        .into_iter()
                                        .enumerate()
                                        .map(
                                            |(i, easing)| {
                                                div()
                                                    .id(("prototype-easing-option", i))
                                                    .debug_selector(move || {
                                                        format!("prototype-easing-{i}")
                                                    })
                                                    .h(px(34.))
                                                    .px(px(8.))
                                                    .rounded(px(5.))
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(10.))
                                                    .text_size(px(12.))
                                                    .cursor_pointer()
                                                    .hover(|s| s.bg(Color::Hover.color()))
                                                    .child(curve(easing))
                                                    .child(div().flex_1().child(label(easing)))
                                                    .children(
                                                        (current.easing() == easing).then(|| {
                                                            icon(GpuiIcons::LucideCheck, 12.)
                                                        }),
                                                    )
                                                    .on_click(cx.listener(
                                                        move |this, _, window, cx| {
                                                            this.commit_transition_duration(
                                                                window, cx,
                                                            );
                                                            this.presentation
                                                                .timing
                                                                .easing_menu
                                                                .update(cx, |menu, cx| {
                                                                    menu.close(window, cx)
                                                                });
                                                            let current = this
                                                                .prototype_transition(id, trigger);
                                                            this.set_prototype_transition(
                                                                id,
                                                                trigger,
                                                                current.with_easing(easing),
                                                                window,
                                                                cx,
                                                            );
                                                        },
                                                    ))
                                            },
                                        ),
                                    ),
                                ),
                        ),
                    ),
            )
    }
}
