use super::*;
use crate::i18n::t;
use crate::ui::theme::Color;
use uic::components::{
    input::{Input, InputAppearance},
    popover::{Popover, PopoverEvent, PopoverPlacement, PopoverState},
};

pub(super) struct ZoomMenu {
    pub popover: Entity<PopoverState>,
    input: Entity<TextInput>,
    invalid: bool,
}

impl ZoomMenu {
    pub fn new(
        window: &mut Window,
        cx: &mut Context<Workspace>,
        subscriptions: &mut Vec<Subscription>,
    ) -> Self {
        let popover = cx.new(|cx| PopoverState::new(window, cx));
        let input = cx.new(TextInput::new);
        subscriptions.push(cx.subscribe_in(
            &popover,
            window,
            |this, _, event: &PopoverEvent, window, cx| {
                if *event == PopoverEvent::Opened {
                    this.zoom_menu.invalid = false;
                    let value = format!("{}%", (this.view.zoom * 10000.).round() / 100.);
                    this.zoom_menu
                        .input
                        .update(cx, |input, cx| input.set_value(value, cx));
                    let input = this.zoom_menu.input.clone();
                    let popover = this.zoom_menu.popover.clone();
                    window.on_next_frame(move |window, cx| {
                        if popover.read(cx).is_open() {
                            input.focus_handle(cx).focus(window, cx);
                            window.on_next_frame(move |window, cx| {
                                if input.focus_handle(cx).is_focused(window)
                                    && let Ok(action) =
                                        cx.build_action("text_input::SelectAll", None)
                                {
                                    window.dispatch_action(action, cx);
                                }
                            });
                        }
                    });
                }
                cx.notify();
            },
        ));
        subscriptions.push(cx.subscribe_in(
            &input,
            window,
            |this, _, event: &InputEvent, window, cx| {
                this.zoom_menu.invalid = false;
                if let InputEvent::Submit(value) = event {
                    if let Some(zoom) = parse_percentage(value) {
                        this.zoom_center(zoom / this.view.zoom, cx);
                        this.zoom_menu
                            .popover
                            .update(cx, |state, cx| state.close(window, cx));
                    } else {
                        this.zoom_menu.invalid = true;
                    }
                }
                cx.notify();
            },
        ));
        Self {
            popover,
            input,
            invalid: false,
        }
    }
}

fn parse_percentage(value: &str) -> Option<f32> {
    let value = value.trim();
    value
        .strip_suffix('%')
        .unwrap_or(value)
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.)
        .map(|value| (value / 100.).clamp(0.1, 256.))
}

#[derive(Clone, Copy)]
enum ZoomAction {
    In,
    Out,
    FitContent,
    FitSelection,
    Scale(f32),
}

impl Workspace {
    pub(super) fn inspector_modes(
        &self,
        collapse: impl IntoElement,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let compact = self.panels.width(panels::Side::Right) < 280.;
        let row = div()
            .h(px(48.))
            .px(px(8.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(
                div()
                    .id("inspector-modes")
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(
                        div()
                            .id("inspector-design-tab")
                            .debug_selector(|| "inspector-design-tab".into())
                            .h(px(28.))
                            .px(px(6.))
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .when(!self.presentation.tab, |el| el.bg(Color::Selected.color()))
                            .text_color(if self.presentation.tab {
                                MUTED.color()
                            } else {
                                ACCENT.color()
                            })
                            .cursor_pointer()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_presentation_tab(false, window, cx)
                            }))
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .flex()
                            .items_center()
                            .child(t("design")),
                    )
                    .child(
                        div()
                            .id("inspector-prototype-tab")
                            .debug_selector(|| "inspector-prototype-tab".into())
                            .h(px(28.))
                            .px(px(6.))
                            .flex_shrink_0()
                            .rounded(px(6.))
                            .when(self.presentation.tab, |el| el.bg(Color::Selected.color()))
                            .text_color(if self.presentation.tab {
                                ACCENT.color()
                            } else {
                                MUTED.color()
                            })
                            .text_size(px(12.))
                            .font_weight(FontWeight::MEDIUM)
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .child(t("prototype"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_presentation_tab(true, window, cx)
                            })),
                    ),
            )
            .child(
                inspector::icon_button("present", t("present"), LucideIcons::Play, false)
                    .size(px(24.))
                    .opacity(if self.presentation_start().is_some() {
                        1.
                    } else {
                        0.4
                    })
                    .on_click(
                        cx.listener(|this, _, window, cx| this.start_presentation(window, cx)),
                    ),
            )
            .when(!compact, |el| el.child(self.zoom_control(cx)))
            .child(collapse);
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .child(row)
            .when(compact, |el| {
                el.child(
                    div()
                        .px(px(8.))
                        .pb(px(6.))
                        .flex()
                        .justify_end()
                        .child(self.zoom_control(cx)),
                )
            })
    }

    fn zoom_control(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        Popover::new(&self.zoom_menu.popover)
            .label(t("menu-zoom"))
            .placement(PopoverPlacement::BottomEnd)
            .gap(px(8.))
            .p(px(6.))
            .rounded(px(10.))
            .bg(PANEL.color())
            .border_1()
            .border_color(BORDER.color())
            .shadow_lg()
            .trigger(
                div()
                    .id("inspector-zoom")
                    .debug_selector(|| "inspector-zoom".into())
                    .h(px(28.))
                    .px(px(4.))
                    .rounded(px(6.))
                    .text_size(px(12.))
                    .text_color(TEXT.color())
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .cursor_pointer()
                    .hover(|s| s.bg(BORDER.color()))
                    .when(self.zoom_menu.popover.read(cx).is_open(), |el| {
                        el.bg(BORDER.color())
                    })
                    .child(format!("{:.0}%", self.view.zoom * 100.))
                    .child(icon(LucideIcons::ChevronDown, 12.).text_color(MUTED.color())),
            )
            .content(move |_, cx| {
                div().children(weak.update(cx, |this, cx| this.zoom_options(cx)).ok())
            })
    }

    fn zoom_options(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let row = |id: &'static str, label: String, action: ZoomAction, enabled: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .h(px(30.))
                .px(px(10.))
                .rounded(px(5.))
                .flex()
                .items_center()
                .text_size(px(12.))
                .text_color(if enabled { TEXT.color() } else { MUTED.color() })
                .child(label)
                .when(enabled, |el| {
                    el.cursor_pointer()
                        .hover(|s| s.bg(BORDER.color()))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            match action {
                                ZoomAction::In => this.zoom_center(1.25, cx),
                                ZoomAction::Out => this.zoom_center(0.8, cx),
                                ZoomAction::FitContent => this.fit_content(false, cx),
                                ZoomAction::FitSelection => this.fit_content(true, cx),
                                ZoomAction::Scale(zoom) => {
                                    this.zoom_center(zoom / this.view.zoom, cx)
                                }
                            }
                            this.zoom_menu
                                .popover
                                .update(cx, |state, cx| state.close(window, cx));
                        }))
                })
        };
        div()
            .debug_selector(|| "zoom-menu".into())
            .w(px(220.))
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "zoom-input".into())
                    .p(px(4.))
                    .pb(px(8.))
                    .child(
                        Input::new(&self.zoom_menu.input)
                            .w_full()
                            .h(px(32.))
                            .px(px(9.))
                            .rounded(px(6.))
                            .text_size(px(12.))
                            .text_color(TEXT.color())
                            .bg(WORKSPACE.color())
                            .border_color(BORDER.color())
                            .appearance(InputAppearance {
                                focus_border: if self.zoom_menu.invalid {
                                    Color::Danger.color()
                                } else {
                                    ACCENT.color()
                                }
                                .into(),
                                caret_height: px(16.),
                                ..crate::ui::theme::input_appearance()
                            }),
                    ),
            )
            .when(self.zoom_menu.invalid, |el| {
                el.child(
                    div()
                        .px(px(10.))
                        .pb(px(8.))
                        .text_size(px(11.))
                        .text_color(Color::Danger.color())
                        .child(t("zoom-invalid")),
                )
            })
            .child(div().h(px(1.)).mb(px(4.)).bg(BORDER.color()))
            .child(row(
                "zoom-in",
                t("zoom-in").into(),
                ZoomAction::In,
                self.view.zoom < 256.,
            ))
            .child(row(
                "zoom-out",
                t("zoom-out").into(),
                ZoomAction::Out,
                self.view.zoom > 0.1,
            ))
            .child(row(
                "zoom-fit",
                t("zoom-fit").into(),
                ZoomAction::FitContent,
                !self.paint_order().is_empty(),
            ))
            .child(row(
                "zoom-selection",
                t("zoom-selection").into(),
                ZoomAction::FitSelection,
                !self.selection_ids().is_empty(),
            ))
            .child(div().h(px(1.)).my(px(4.)).bg(BORDER.color()))
            .child(row("zoom-50", "50%".into(), ZoomAction::Scale(0.5), true))
            .child(row(
                "zoom-reset",
                "100%".into(),
                ZoomAction::Scale(1.),
                true,
            ))
            .child(row("zoom-200", "200%".into(), ZoomAction::Scale(2.), true))
    }

    pub(super) fn fit_content(&mut self, selection: bool, cx: &mut Context<Self>) {
        if self.gesture.is_some() {
            return;
        }
        let selected = self.selection_ids();
        let rect = self
            .canvas_layer_order()
            .into_iter()
            .filter(|id| !selection || selected.contains(id))
            .filter_map(|id| self.world_bounds(id))
            .reduce(|a, b| {
                let x = a.x.min(b.x);
                let y = a.y.min(b.y);
                Rect {
                    x,
                    y,
                    width: (a.x + a.width).max(b.x + b.width) - x,
                    height: (a.y + a.height).max(b.y + b.height) - y,
                }
            });
        let Some(rect) = rect else {
            return;
        };
        let size = self.bounds.get().size;
        let (left, right) = self.canvas_insets();
        let width = (f32::from(size.width) - left - right).max(1.);
        let height = f32::from(size.height);
        self.view.zoom = ((width - 48.).max(1.) / rect.width.max(1.))
            .min((height - 144.).max(1.) / rect.height.max(1.))
            .clamp(0.1, 1.);
        self.view.pan = point(
            left + width / 2. - (rect.x + rect.width / 2.) * self.view.zoom,
            height / 2. - 16. - (rect.y + rect.height / 2.) * self.view.zoom,
        );
        cx.notify();
    }
}

#[cfg(test)]
mod tests;
