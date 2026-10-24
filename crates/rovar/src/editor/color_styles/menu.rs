use super::*;
use crate::ui::theme::Color;
use gpui::App;
use std::{cell::Cell, ops::Range, rc::Rc};
use uic::components::{
    dropdown::{DropdownState, dropdown},
    input::Input,
    popover::{PopoverDismissReason, PopoverEvent},
};

#[cfg(test)]
mod tests;

#[derive(Clone, PartialEq, Eq)]
struct Target {
    generation: usize,
    page: String,
    object: uuid::Uuid,
    layer: usize,
    stroke: bool,
    text_range: Option<(Range<usize>, bool)>,
}

#[derive(Clone)]
struct Draft {
    target: Target,
    style: ColorStyle,
    scope: Scope,
}

#[derive(Clone, Copy)]
struct Layout {
    bounds: Bounds<Pixels>,
    viewport: gpui::Size<Pixels>,
    anchor_height: Pixels,
}

pub(super) struct Browser {
    query: Entity<TextInput>,
    name: Entity<TextInput>,
    scopes: [Entity<DropdownState>; 2],
    target: Option<Target>,
    draft: Option<Draft>,
    error: Option<String>,
    layout: Rc<Cell<Option<Layout>>>,
    scroll: gpui::ScrollHandle,
}

impl Browser {
    pub(super) fn new(
        window: &mut Window,
        cx: &mut Context<Workspace>,
        menus: &[Entity<PopoverState>; 2],
        subscriptions: &mut Vec<Subscription>,
    ) -> Self {
        let query = cx.new(|cx| TextInput::new(cx).placeholder(t("color-style-search")));
        let name = cx.new(TextInput::new);
        subscriptions.push(cx.observe(&query, |_, _, cx| cx.notify()));
        subscriptions.push(cx.observe(&name, |_, _, cx| cx.notify()));
        subscriptions.push(
            cx.subscribe_in(&name, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::Submit(_)) {
                    this.save_inline_color_style(window, cx);
                }
            }),
        );
        for (index, menu) in menus.iter().enumerate() {
            subscriptions.push(
                cx.subscribe_in(menu, window, move |this, _, event, window, cx| {
                    if *event == PopoverEvent::Opened {
                        this.colors.browser.target = this.color_style_target(index == 1, cx);
                        this.colors.browser.draft = None;
                        this.colors
                            .browser
                            .scroll
                            .set_offset(gpui::Point::default());
                        this.colors.browser.error = None;
                        this.colors.browser.layout.set(None);
                        this.colors
                            .browser
                            .query
                            .update(cx, |input, cx| input.set_value("", cx));
                        this.colors.menu[1 - index].update(cx, |menu, cx| {
                            menu.dismiss(PopoverDismissReason::Programmatic, false, window, cx);
                        });
                        for popup in &this.inspector.paint_popovers {
                            popup.update(cx, |state, cx| {
                                state.dismiss(
                                    PopoverDismissReason::Programmatic,
                                    false,
                                    window,
                                    cx,
                                );
                            });
                        }
                    }
                    cx.notify();
                }),
            );
        }
        Self {
            query,
            name,
            scopes: std::array::from_fn(|_| cx.new(|cx| DropdownState::new(window, cx))),
            target: None,
            draft: None,
            error: None,
            layout: Default::default(),
            scroll: gpui::ScrollHandle::new(),
        }
    }
}

impl Workspace {
    fn color_style_target(&self, stroke: bool, cx: &App) -> Option<Target> {
        self.color_source(stroke, cx)?;
        let (object, text_range) = if let Some(shape) = self.selected_shape() {
            (shape.uid, None)
        } else if let Some(text) = self.selected_text() {
            let editor = text.editor.read(cx);
            (
                text.uid,
                Some((editor.style_range(), !editor.has_style_selection())),
            )
        } else {
            (self.selected_board()?.uid, None)
        };
        Some(Target {
            generation: self.pages.generation,
            page: self.pages.active.clone(),
            object,
            layer: self.paint_layer_index(stroke),
            stroke,
            text_range,
        })
    }

    fn current_style_paint(&self, stroke: bool, cx: &App) -> Option<(FillMode, ColorStyle, f32)> {
        let (color, _) = self.color_source(stroke, cx)?;
        let (mode, opacity) = if let Some(shape) = self.paint_shape() {
            (
                shape.paint_mode(stroke),
                if stroke {
                    shape.stroke.opacity
                } else {
                    shape.fill_opacity
                },
            )
        } else if let Some(text) = self.selected_text() {
            (text.editor.read(cx).effective_style().fill_mode, 1.)
        } else {
            let board = self.paint_board()?;
            (board.fill_mode, board.fill_opacity)
        };
        Some((
            mode,
            ColorStyle {
                name: String::new(),
                color,
                gradient: self.selected_gradient(stroke, cx),
            },
            opacity,
        ))
    }

    fn begin_inline_color_style(
        &mut self,
        stroke: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.color_style_target(stroke, cx) else {
            return;
        };
        if self.colors.browser.target.as_ref() != Some(&target) {
            return;
        }
        let Some((FillMode::Solid | FillMode::Linear, style, _)) =
            self.current_style_paint(stroke, cx)
        else {
            return;
        };
        self.colors.browser.draft = Some(Draft {
            target,
            style,
            scope: self.colors.scope,
        });
        self.colors.browser.error = None;
        self.colors
            .browser
            .name
            .update(cx, |input, cx| input.set_value("", cx));
        self.colors.browser.name.focus_handle(cx).focus(window, cx);
        self.colors.browser.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn cancel_inline_color_style(
        &mut self,
        stroke: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.colors.browser.draft = None;
        self.colors.browser.error = None;
        self.colors
            .browser
            .scroll
            .set_offset(gpui::Point::default());
        self.colors.menu[usize::from(stroke)]
            .focus_handle(cx)
            .focus(window, cx);
        cx.notify();
    }

    fn save_inline_color_style(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(draft) = self.colors.browser.draft.clone() else {
            return;
        };
        let current = self.current_style_paint(draft.target.stroke, cx);
        if self.color_style_target(draft.target.stroke, cx).as_ref() != Some(&draft.target)
            || current
                .as_ref()
                .is_none_or(|(_, style, _)| *style != draft.style)
        {
            self.colors.browser.error = Some(t("color-style-target-changed").into());
            cx.notify();
            return;
        }
        let mut style = draft.style;
        style.name = self.colors.browser.name.read(cx).value().trim().into();
        if style.validate().is_err() {
            self.colors.browser.error = Some(t("color-style-invalid-name").into());
            cx.notify();
            return;
        }
        let id = uuid::Uuid::new_v4().to_string();
        if draft.scope == Scope::Local {
            let library = self
                .assets
                .library
                .clone()
                .filter(|l| l.read(cx).ready && !l.read(cx).busy);
            let result = library
                .ok_or_else(|| anyhow::anyhow!(t("color-style-library-unavailable")))
                .and_then(|library| {
                    library.update(cx, |l, cx| l.set_color(id.clone(), Some(style.clone()), cx))
                });
            if let Err(error) = result {
                self.colors.browser.error = Some(error.to_string());
                cx.notify();
                return;
            }
        } else {
            self.seal_text_edits(cx);
            let before = self.page_edit(std::slice::from_ref(&self.pages.active), cx);
            let mut page = self.snapshot_page(cx).0;
            self.colors.palette.insert(id.clone(), style.clone());
            if let Some(shape) = page
                .shapes
                .iter_mut()
                .find(|s| s.uid == draft.target.object)
            {
                let layer = draft.target.layer;
                let reference = if draft.target.stroke {
                    if layer == 0 {
                        &mut shape.stroke.color_style
                    } else {
                        &mut shape.stroke_layers[layer - 1].color_style
                    }
                } else if layer == 0 {
                    &mut shape.color_style
                } else {
                    &mut shape.fill_layers[layer - 1].color_style
                };
                *reference = Some(id.clone());
            } else if let Some(board) = page
                .boards
                .iter_mut()
                .find(|b| b.uid == draft.target.object)
            {
                if draft.target.layer == 0 {
                    board.color_style = Some(id.clone());
                } else {
                    board.fill_layers[draft.target.layer - 1].color_style = Some(id.clone());
                }
            } else if let Some(text) = page.texts.iter_mut().find(|t| t.uid == draft.target.object)
                && let Some((range, whole)) = draft.target.text_range
            {
                text.styles.apply(
                    range,
                    &StyleChange::ColorStyle(Some(id.clone()), style),
                    whole,
                );
            }
            self.apply_component_page(page, window, cx);
            self.record_page_edit(before);
        }
        self.colors.scope = draft.scope;
        self.colors.selected = Some((draft.scope, id));
        self.colors
            .browser
            .query
            .update(cx, |input, cx| input.set_value("", cx));
        self.cancel_inline_color_style(draft.target.stroke, window, cx);
        self.sync_fields(cx);
    }

    fn style_scope_control(&self, save: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let scope = if save {
            self.colors
                .browser
                .draft
                .as_ref()
                .map_or(self.colors.scope, |d| d.scope)
        } else {
            self.colors.scope
        };
        let state = self.colors.browser.scopes[usize::from(save)].clone();
        dropdown(&state)
            .priority(1200)
            .w(px(190.))
            .rounded(px(10.))
            .bg(Color::Panel.color())
            .border_color(Color::Text.color().opacity(0.08))
            .shadow_lg()
            .trigger(
                div()
                    .id(if save {
                        "style-save-location"
                    } else {
                        "style-source"
                    })
                    .debug_selector(move || {
                        if save {
                            "style-save-location"
                        } else {
                            "style-source"
                        }
                        .into()
                    })
                    .h(px(32.))
                    .px(px(8.))
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .text_color(TEXT.color())
                    .cursor_pointer()
                    .when(save, |el| el.w(px(298.)).bg(Color::Input.color()))
                    .hover(|s| s.bg(Color::Text.color().opacity(0.04)))
                    .child(div().flex_1().child(t(if scope == Scope::Document {
                        "assets-document"
                    } else {
                        "assets-local"
                    })))
                    .child(icon(GpuiIcons::LucideChevronDown, 12.).text_color(MUTED.color())),
            )
            .menu(
                div().flex().flex_col().gap(px(2.)).children(
                    [
                        (Scope::Document, "assets-document"),
                        (Scope::Local, "assets-local"),
                    ]
                    .map(|(choice, key)| {
                        let state = state.clone();
                        div()
                            .id(key)
                            .debug_selector(move || key.into())
                            .h(px(32.))
                            .px(px(8.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(px(12.))
                            .text_color(TEXT.color())
                            .cursor_pointer()
                            .hover(|s| s.bg(Color::Text.color().opacity(0.05)))
                            .child(div().flex_1().child(t(key)))
                            .when(scope == choice, |el| {
                                el.child(
                                    icon(GpuiIcons::LucideCheck, 13.).text_color(ACCENT.color()),
                                )
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if save {
                                    if let Some(draft) = &mut this.colors.browser.draft {
                                        draft.scope = choice;
                                    }
                                    this.colors.browser.error = None;
                                } else {
                                    this.colors.scope = choice;
                                }
                                state.update(cx, |state, cx| state.close(window, cx));
                                cx.notify();
                            }))
                    }),
                ),
            )
    }

    fn style_browser_input(&self, name: bool, stroke: bool, cx: &App) -> impl IntoElement + use<> {
        let input = if name {
            &self.colors.browser.name
        } else {
            &self.colors.browser.query
        };
        let focus = input.focus_handle(cx);
        let parent_focus = self.colors.menu[usize::from(stroke)].focus_handle(cx);
        let layout = self.colors.browser.layout.clone();
        div()
            .id(if name {
                "style-inline-name"
            } else {
                "style-search"
            })
            .debug_selector(move || {
                if name {
                    "style-inline-name"
                } else {
                    "style-search"
                }
                .into()
            })
            .w_full()
            .h(px(34.))
            .on_mouse_down_out(move |event, window, cx| {
                if event.button == MouseButton::Left
                    && focus.is_focused(window)
                    && layout
                        .get()
                        .is_some_and(|l| l.bounds.contains(&event.position))
                {
                    parent_focus.focus(window, cx);
                }
            })
            .child(
                Input::new(input)
                    .w_full()
                    .h_full()
                    .px(px(10.))
                    .rounded(px(8.))
                    .text_size(px(12.))
                    .text_color(TEXT.color())
                    .bg(if name {
                        Color::Input.color()
                    } else {
                        Color::Transparent.color()
                    })
                    .border_color(Color::Text.color().opacity(if name { 0.08 } else { 0. }))
                    .appearance(crate::ui::theme::input_appearance())
                    .blur_on_click_outside(false),
            )
    }

    pub(super) fn color_style_menu(
        &self,
        stroke: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let source = self.color_source(stroke, cx);
        let linked = source.as_ref().and_then(|s| s.1.as_ref());
        let scope = self.colors.scope;
        let palette = self.color_palette(scope, cx);
        let count = palette.len();
        let query = self
            .colors
            .browser
            .query
            .read(cx)
            .value()
            .trim()
            .to_lowercase();
        let mut entries: Vec<_> = palette
            .into_iter()
            .filter(|(_, style)| {
                style.name.to_lowercase().contains(&query)
                    || color_hex(style.color).to_lowercase().contains(&query)
            })
            .collect();
        entries.sort_by(|a, b| {
            a.1.name
                .to_lowercase()
                .cmp(&b.1.name.to_lowercase())
                .then(a.0.cmp(&b.0))
        });
        let valid_target = self
            .color_style_target(stroke, cx)
            .is_some_and(|target| self.colors.browser.target.as_ref() == Some(&target));
        let expanded = self.colors.browser.draft.is_some();
        let current = self.current_style_paint(stroke, cx);
        let can_save = valid_target
            && current
                .as_ref()
                .is_some_and(|(mode, _, _)| matches!(mode, FillMode::Solid | FillMode::Linear));
        let viewport = window.viewport_size();
        let old = self
            .colors
            .browser
            .layout
            .get()
            .filter(|l| l.viewport == viewport);
        let available = old
            .map_or(viewport.height - px(32.), |l| {
                viewport.height - l.bounds.top() - px(16.)
            })
            .max(px(120.));
        let grid_height = px((entries.len().div_ceil(2) as f32 * 100. - 10.).max(88.))
            .min(px(290.))
            .min((available - px(if expanded { 410. } else { 224. })).max(px(88.)));
        // Position the popup with room for its inline form from the first frame.
        // Its visible surface can then grow down without moving the heading.
        let anchor_height = old
            .map_or(grid_height + px(480.), |l| l.anchor_height)
            .min(viewport.height - px(32.));
        let mut panel = div()
            .id("color-style-panel")
            .debug_selector(|| "color-style-panel".into())
            .w(px(336.))
            .max_h(available)
            .overflow_y_scroll()
            .track_scroll(&self.colors.browser.scroll)
            .flex_shrink_0()
            .p(px(18.))
            .rounded(px(16.))
            .border_1()
            .border_color(Color::Text.color().opacity(0.09))
            .bg(Color::Panel.color())
            .shadow_lg()
            .text_color(TEXT.color())
            .text_size(px(12.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" && this.colors.browser.draft.is_some() {
                        this.cancel_inline_color_style(stroke, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .debug_selector(|| "color-style-heading".into())
                            .child(t("color-style-panel-title")),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(MUTED.color())
                            .child(format!(
                                "{} {:02}",
                                t(if stroke { "stroke" } else { "fill" }),
                                self.paint_layer_index(stroke) + 1
                            )),
                    )
                    .child(
                        inspector::icon_button(
                            "close-color-styles",
                            t("close"),
                            GpuiIcons::LucideX,
                            false,
                        )
                        .size(px(24.))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.colors.menu[usize::from(stroke)]
                                    .update(cx, |m, cx| m.close(window, cx))
                            },
                        )),
                    ),
            )
            .child(style_divider())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.style_scope_control(false, cx))
                    .child(div().text_size(px(10.)).text_color(MUTED.color()).child(
                        crate::i18n::message("color-style-count", &[("count", count.to_string())]),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(icon(GpuiIcons::LucideSearch, 14.).text_color(MUTED.color()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(self.style_browser_input(false, stroke, cx)),
                    ),
            )
            .child(
                div()
                    .id("color-style-grid")
                    .debug_selector(|| "color-style-grid".into())
                    .h(grid_height)
                    .flex_shrink_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .when(entries.is_empty(), |el| {
                        el.child(
                            div()
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(MUTED.color())
                                .child(t(if query.is_empty() {
                                    "color-styles-empty"
                                } else {
                                    "color-style-no-matches"
                                })),
                        )
                    })
                    .children(entries.chunks(2).map(|row| {
                        div()
                            .flex()
                            .gap(px(10.))
                            .flex_shrink_0()
                            .children(row.iter().map(|(id, style)| {
                                let id = id.clone();
                                let active = scope == Scope::Document && linked == Some(&id);
                                div()
                                    .id(gpui::SharedString::from(format!("pick-color-{id}")))
                                    .debug_selector({
                                        let id = id.clone();
                                        move || format!("pick-color-{id}")
                                    })
                                    .w(px(144.))
                                    .h(px(90.))
                                    .p(px(5.))
                                    .rounded(px(10.))
                                    .border_1()
                                    .border_color(if active {
                                        ACCENT.color().opacity(0.8)
                                    } else {
                                        Color::Text.color().opacity(0.07)
                                    })
                                    .bg(if active {
                                        Color::Selected.color()
                                    } else {
                                        Color::Text.color().opacity(0.025)
                                    })
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(Color::Text.color().opacity(0.06)))
                                    .child(
                                        div()
                                            .relative()
                                            .w_full()
                                            .h(px(52.))
                                            .rounded(px(6.))
                                            .overflow_hidden()
                                            .bg(gpui::checkerboard(Color::Checker.color(), 6.))
                                            .child(
                                                div().absolute().inset_0().bg(style.background()),
                                            )
                                            .when(active, |el| {
                                                el.child(
                                                    div()
                                                        .absolute()
                                                        .top(px(5.))
                                                        .right(px(5.))
                                                        .size(px(20.))
                                                        .rounded_full()
                                                        .bg(Color::Panel.color())
                                                        .flex()
                                                        .items_center()
                                                        .justify_center()
                                                        .child(
                                                            icon(GpuiIcons::LucideCheck, 12.)
                                                                .text_color(ACCENT.color()),
                                                        ),
                                                )
                                            }),
                                    )
                                    .child(
                                        div()
                                            .px(px(4.))
                                            .text_size(px(11.))
                                            .truncate()
                                            .child(style.name.clone()),
                                    )
                                    .when(valid_target, |el| {
                                        el.on_click(cx.listener(move |this, _, window, cx| {
                                            if this.colors.browser.target
                                                == this.color_style_target(stroke, cx)
                                            {
                                                this.apply_color_style(
                                                    scope, &id, stroke, window, cx,
                                                );
                                            }
                                        }))
                                    })
                            }))
                    })),
            )
            .child(style_divider());
        if let Some((mode, style, opacity)) = current {
            let description = if mode == FillMode::Solid {
                color_hex(style.color)[1..7].to_owned()
            } else {
                t(if mode == FillMode::Linear {
                    "gradient"
                } else if mode == FillMode::Image {
                    "shape-image"
                } else {
                    "gradient-points"
                })
                .into()
            };
            let alpha = opacity
                * if mode == FillMode::Solid {
                    style.color.a
                } else {
                    1.
                };
            panel = panel.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(super::view::paint_swatch(
                        style.background().opacity(opacity),
                        28.,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(div().text_size(px(9.)).text_color(MUTED.color()).child(t(
                                if stroke {
                                    "color-style-current-stroke"
                                } else {
                                    "color-style-current"
                                },
                            )))
                            .child(div().truncate().text_size(px(11.)).child(format!(
                                "{description} · {}%",
                                inspector::number(alpha * 100.)
                            ))),
                    )
                    .when(!expanded, |el| {
                        el.child(
                            style_button(
                                if stroke {
                                    "save-stroke-style"
                                } else {
                                    "save-fill-style"
                                },
                                "color-style-save-current",
                                false,
                            )
                            .opacity(if can_save { 1. } else { 0.4 })
                            .when(can_save, |el| {
                                el.on_click(cx.listener(move |this, _, window, cx| {
                                    this.begin_inline_color_style(stroke, window, cx)
                                }))
                            }),
                        )
                    })
                    .when(expanded, |el| {
                        el.child(
                            inspector::icon_button(
                                "cancel-inline-style-icon",
                                t("cancel"),
                                GpuiIcons::LucideX,
                                false,
                            )
                            .size(px(26.))
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.cancel_inline_color_style(stroke, window, cx)
                                },
                            )),
                        )
                    }),
            );
        }
        if let Some(draft) = &self.colors.browser.draft {
            let local_ready = draft.scope == Scope::Document
                || self
                    .assets
                    .library
                    .as_ref()
                    .is_some_and(|l| l.read(cx).ready && !l.read(cx).busy);
            panel = panel.child(
                div()
                    .id("inline-style-form")
                    .debug_selector(|| "inline-style-form".into())
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(style_caption("color-style-name"))
                    .child(self.style_browser_input(true, stroke, cx))
                    .child(div().mt(px(6.)).child(style_caption("color-style-save-to")))
                    .child(self.style_scope_control(true, cx))
                    .when_some(self.colors.browser.error.clone(), |el, error| {
                        el.child(
                            div()
                                .text_size(px(11.))
                                .text_color(Color::Danger.color())
                                .child(error),
                        )
                    })
                    .when(!local_ready, |el| {
                        el.child(style_caption("color-style-library-unavailable"))
                    })
                    .child(
                        div()
                            .mt(px(8.))
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                style_button("cancel-inline-style", "cancel", false).on_click(
                                    cx.listener(move |this, _, window, cx| {
                                        this.cancel_inline_color_style(stroke, window, cx)
                                    }),
                                ),
                            )
                            .child(
                                style_button("confirm-inline-style", "color-style-save", true)
                                    .opacity(if local_ready { 1. } else { 0.4 })
                                    .when(local_ready, |el| {
                                        el.on_click(cx.listener(|this, _, window, cx| {
                                            this.save_inline_color_style(window, cx)
                                        }))
                                    }),
                            ),
                    ),
            );
        }
        panel = panel.child(style_caption(if !valid_target {
            "color-style-target-changed"
        } else if !can_save {
            "color-style-unsupported"
        } else if scope == Scope::Document {
            "color-style-document-hint"
        } else {
            "color-style-library-hint"
        }));
        if linked.is_some() && !expanded {
            panel = panel.child(
                style_button("detach-color-style", "color-style-detach", false).on_click(
                    cx.listener(move |this, _, _, cx| {
                        if this.colors.browser.target == this.color_style_target(stroke, cx) {
                            this.detach_color_style(stroke, cx);
                        }
                    }),
                ),
            );
        }
        let layout = self.colors.browser.layout.clone();
        div()
            .w(px(336.))
            .flex()
            .flex_col()
            .min_h(anchor_height)
            .child(div().flex_shrink_0().child(panel).on_paint_before_children(
                move |bounds, _, _, _| {
                    layout.set(Some(Layout {
                        bounds,
                        viewport,
                        anchor_height: anchor_height.max(bounds.size.height),
                    }));
                },
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if this
                        .colors
                        .browser
                        .layout
                        .get()
                        .is_some_and(|l| !l.bounds.contains(&event.position))
                    {
                        this.colors.menu[usize::from(stroke)]
                            .update(cx, |state, cx| state.close(window, cx));
                    }
                }),
            )
    }
}

fn style_divider() -> Div {
    div()
        .h(px(1.))
        .flex_shrink_0()
        .bg(Color::Text.color().opacity(0.065))
}

fn style_caption(key: &'static str) -> Div {
    div()
        .text_size(px(10.))
        .text_color(MUTED.color())
        .child(t(key))
}

fn style_button(id: &'static str, key: &'static str, primary: bool) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(28.))
        .px(px(10.))
        .rounded(px(8.))
        .bg(if primary {
            ACCENT.color()
        } else {
            Color::Text.color().opacity(0.045)
        })
        .text_color(if primary {
            Color::OnAccent.color()
        } else {
            ACCENT.color()
        })
        .text_size(px(11.))
        .font_weight(FontWeight::MEDIUM)
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.opacity(0.85))
        .child(t(key))
}
