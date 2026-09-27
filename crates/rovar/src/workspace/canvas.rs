use super::*;
use crate::i18n::t;
use gpui::{CursorStyle, DispatchPhase, HitboxBehavior, MouseMoveEvent, MouseUpEvent, canvas};

// During a sidebar drag only the chrome changes. Reuse this subtree's layout,
// text shaping and paint commands; normal document interactions render it afresh.
pub(super) struct CanvasScene {
    workspace: gpui::WeakEntity<Workspace>,
}
impl CanvasScene {
    pub fn new(workspace: gpui::WeakEntity<Workspace>) -> Self {
        Self { workspace }
    }
}
impl Render for CanvasScene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.workspace
            .update(cx, |workspace, cx| {
                workspace.scene_content(cx).into_any_element()
            })
            .unwrap_or_else(|_| div().into_any_element())
    }
}

impl Workspace {
    pub(super) fn editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds_state = self.bounds.clone();
        let capture = self.capture.clone();
        let weak = cx.entity().downgrade();
        div()
            .id("editor-area")
            .on_drop(cx.listener(|this, drag: &assets::AssetDrag, window, cx| {
                this.drop_asset(drag, window, cx)
            }))
            .debug_selector(|| "editor-area".into())
            .track_focus(&self.focus)
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .bg(rgb(WORKSPACE))
            .when(self.space_down, |el| el.cursor(CursorStyle::OpenHand))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    if this.space_down {
                        this.begin(
                            GestureKind::Pan {
                                original: this.view.pan,
                            },
                            event.position,
                            event.button,
                            window,
                            cx,
                        );
                    } else {
                        this.start_marquee(event, window, cx);
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.open_context_menu(None, false, event.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.begin(
                        GestureKind::Pan {
                            original: this.view.pan,
                        },
                        event.position,
                        event.button,
                        window,
                        cx,
                    );
                }),
            )
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if !this.focus.is_focused(window) {
                    return;
                }
                if this.selection_key(event, window, cx) {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "space" => this.space_down = true,
                    "escape" => {
                        this.toolbar.hand = false;
                        this.cancel_gesture(window, cx);
                        this.discard_bezier();
                        this.draw_tool = None;
                        this.media_request += 1;
                        this.media_loading = false;
                        this.media_error = None;
                    }
                    "enter" if this.draw_tool == Some(DrawTool::Shape(ShapeKind::Bezier)) => {
                        if this.gesture.is_some() {
                            this.finish_gesture(window, cx);
                        }
                        this.finish_bezier(cx);
                        this.draw_tool = None;
                    }
                    "backspace" | "delete" if this.bezier_draft.is_some() => {
                        this.bezier_history(false, window, cx)
                    }
                    "delete" | "backspace" if this.gesture.is_none() => {
                        this.delete_selected(cx);
                    }
                    _ => {
                        if this.toolbar_shortcut(event, window, cx) {
                            cx.stop_propagation();
                        }
                        return;
                    }
                }
                cx.stop_propagation();
                cx.notify();
            }))
            .on_key_up(cx.listener(|this, event: &gpui::KeyUpEvent, _, cx| {
                if event.keystroke.key == "space" {
                    this.space_down = false;
                    cx.notify();
                }
            }))
            .on_scroll_wheel(cx.listener(Self::scroll_canvas))
            .child(
                canvas(
                    move |bounds, window, _| {
                        bounds_state.set(bounds);
                        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                        if capture.get().is_some() && window.captured_hitbox() == capture.get() {
                            window.capture_pointer(hitbox.id);
                        }
                        capture.set(Some(hitbox.id));
                        hitbox
                    },
                    move |_, _, window, _| {
                        let moving = weak.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            let _ = moving.update(cx, |this, cx| {
                                if let Some(gesture) = this.gesture {
                                    if event.pressed_button == Some(gesture.button) {
                                        this.snapping.bypass = event.modifiers.alt;
                                        this.move_gesture(
                                            event.position,
                                            event.modifiers.shift,
                                            cx,
                                        );
                                    } else {
                                        this.cancel_gesture(window, cx);
                                    }
                                    cx.stop_propagation();
                                }
                                this.update_bezier_hover(event.position, cx);
                                this.update_vector_hover(event.position, cx);
                            });
                        });
                        let ending = weak.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                            if phase != DispatchPhase::Capture {
                                return;
                            }
                            let _ = ending.update(cx, |this, cx| {
                                if this.gesture.is_some_and(|g| g.button == event.button) {
                                    this.snapping.bypass = event.modifiers.alt;
                                    this.move_gesture(event.position, event.modifiers.shift, cx);
                                    this.finish_gesture(window, cx);
                                    this.update_bezier_hover(event.position, cx);
                                    this.update_vector_hover(event.position, cx);
                                    cx.stop_propagation();
                                    cx.notify();
                                }
                            });
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
            .child(
                if self
                    .gesture
                    .is_some_and(|g| matches!(g.kind, GestureKind::Panel { .. }))
                {
                    self.scene
                        .clone()
                        .cached(div().absolute().size_full().style().clone())
                        .into_any_element()
                } else {
                    self.scene.clone().into_any_element()
                },
            )
            .when(self.draw_tool.is_some() || self.toolbar.hand, |el| {
                el.child(
                    div()
                        .id("drawing-surface")
                        .debug_selector(|| "drawing-surface".into())
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_scroll_wheel(cx.listener(Self::scroll_canvas))
                        .cursor(if self.toolbar.hand {
                            CursorStyle::OpenHand
                        } else {
                            CursorStyle::Crosshair
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                                if this.space_down || this.toolbar.hand {
                                    this.begin(
                                        GestureKind::Pan {
                                            original: this.view.pan,
                                        },
                                        event.position,
                                        event.button,
                                        window,
                                        cx,
                                    );
                                } else {
                                    this.snapping.bypass = event.modifiers.alt;
                                    this.start_drawing(event.position, window, cx);
                                }
                            }),
                        ),
                )
            })
            .child(self.view_controls(cx))
            .child(self.sidebar(cx))
            .child(self.properties(cx))
            .child(self.tool_bar(cx))
            .when(self.media_loading || self.media_error.is_some(), |el| {
                el.child(self.media_status(cx))
            })
            .when(self.vector_edit.is_some(), |el| {
                el.child(self.vector_toolbar(cx))
            })
    }

    fn scroll_canvas(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_none() {
            self.vector_hover = None;
            let delta = event.delta.pixel_delta(px(20.));
            if event.modifiers.control {
                let local = event.position - self.bounds.get().origin;
                let zoom = self.view.zoom * (f32::from(delta.y) * 0.004).exp();
                self.view
                    .zoom_at(point(f32::from(local.x), f32::from(local.y)), zoom);
            } else {
                self.view.pan += point(f32::from(delta.x), f32::from(delta.y));
            }
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn canvas_grid(&self, pixels: bool) -> impl IntoElement + use<> {
        let view = self.view;
        canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let spacing = if pixels {
                    view.zoom
                } else {
                    (24. * view.zoom).max(12.)
                };
                let offset = point(
                    view.pan.x.rem_euclid(spacing),
                    view.pan.y.rem_euclid(spacing),
                );
                // One primitive regardless of the number of visible grid cells.
                let scale = window.scale_factor();
                let _ = window.paint_effect(
                    gpui::PaintEffect::new(
                        bounds,
                        gpui::EffectShader::wgsl(include_str!("grid.wgsl")),
                    )
                    .uniforms(
                        gpui::EffectUniforms::default()
                            .with_slot(
                                0,
                                [spacing * scale, offset.x * scale, offset.y * scale, scale],
                            )
                            .with_slot(
                                1,
                                [
                                    if pixels { 1. } else { 0. },
                                    ((view.zoom - 8.) / 4.).clamp(0., 1.),
                                    0.,
                                    0.,
                                ],
                            ),
                    ),
                );
            },
        )
        .absolute()
        .size_full()
    }

    fn scene_content(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .absolute()
            .size_full()
            .when(self.view.zoom < 8., |el| el.child(self.canvas_grid(false)))
            .children(self.content_elements(cx))
            // Paint over artwork too, while keeping selection and guides on top.
            // Canvas does not insert a hitbox, so editing remains unobstructed.
            .when(self.view.zoom >= 8., |el| el.child(self.canvas_grid(true)))
            .children(self.bezier_hover_preview())
            .children(self.creation_preview())
            .child(self.selection_overlay())
            .child(self.snap_guides())
            .children(self.vector_hover_preview(cx))
            .when(
                self.boards.is_empty()
                    && self.shapes.is_empty()
                    && self.texts.is_empty()
                    && self.draw_tool.is_none(),
                |el| {
                    el.child(
                        div()
                            .absolute()
                            .inset_0()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .gap(px(14.))
                            .child(icon(LucideIcons::Frame, 32.).text_color(rgb(0x555b69)))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(rgb(MUTED))
                                    .child(t("canvas-empty")),
                            ),
                    )
                },
            )
    }

    pub(super) fn board_element(
        &self,
        board: &Artboard,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = board.id;
        let position = self.view.screen(point(board.rect.x, board.rect.y));
        let width = board.rect.width * self.view.zoom;
        let height = board.rect.height * self.view.zoom;
        let selected = self.selected == Some(id)
            && self.selected_text.is_none()
            && self.selected_shape.is_none();
        div()
            .id(("artboard", id))
            .debug_selector(move || format!("artboard-{id}"))
            .absolute()
            .left(px(position.x))
            .top(px(position.y))
            .w(px(width))
            .h(px(height))
            .cursor(CursorStyle::OpenHand)
            .when(!self.layer_editable(id), |el| el.cursor(CursorStyle::Arrow))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    if this.space_down {
                        this.begin(
                            GestureKind::Pan {
                                original: this.view.pan,
                            },
                            event.position,
                            event.button,
                            window,
                            cx,
                        );
                    } else {
                        if !this.layer_editable(id) {
                            this.focus.focus(window, cx);
                            this.select(None, cx);
                            cx.stop_propagation();
                            return;
                        }
                        if this.selection_pointer(id, event, window, cx) {
                            return;
                        }
                        this.select(Some(id), cx);
                        if let Some(board) = this.selected_board() {
                            this.begin(
                                GestureKind::Move {
                                    id,
                                    original: board.rect,
                                },
                                event.position,
                                event.button,
                                window,
                                cx,
                            );
                        }
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.open_context_menu(Some(id), false, event.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(board.surface(self.view.zoom))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(-26.))
                    .h(px(24.))
                    .max_w(px(width.max(100.)))
                    .overflow_hidden()
                    .text_size(px(11.))
                    .text_color(rgb(if selected { ACCENT } else { MUTED }))
                    .child(board.name.clone()),
            )
            .when(selected, |el| {
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .border_1()
                        .border_color(rgb(ACCENT)),
                )
                .children(Handle::ALL.into_iter().enumerate().map(|(index, handle)| {
                    let x = (handle.0 as f32 + 1.) * 0.5 * width;
                    let y = (handle.1 as f32 + 1.) * 0.5 * height;
                    let cursor = resize_cursor(handle);
                    let corner = handle.0 != 0 && handle.1 != 0;
                    let (left, top, hit_width, hit_height) = if handle.0 == 0 {
                        (6., y - 6., (width - 12.).max(0.), 12.)
                    } else if handle.1 == 0 {
                        (x - 6., 6., 12., (height - 12.).max(0.))
                    } else {
                        (x - 6., y - 6., 12., 12.)
                    };
                    div()
                        .id(("resize-handle", index))
                        .debug_selector(move || format!("handle-{index}"))
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .w(px(hit_width))
                        .h(px(hit_height))
                        .cursor(cursor)
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(corner, |el| {
                            el.child(
                                div()
                                    .debug_selector(move || format!("board-corner-{index}"))
                                    .size(px(7.))
                                    .bg(rgb(0xffffff))
                                    .border_1()
                                    .border_color(rgb(ACCENT)),
                            )
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                                if this.space_down {
                                    this.begin(
                                        GestureKind::Pan {
                                            original: this.view.pan,
                                        },
                                        event.position,
                                        event.button,
                                        window,
                                        cx,
                                    );
                                } else if let Some(board) = this.selected_board() {
                                    this.begin(
                                        GestureKind::Resize {
                                            id,
                                            original: board.rect,
                                            handle,
                                        },
                                        event.position,
                                        event.button,
                                        window,
                                        cx,
                                    );
                                }
                            }),
                        )
                }))
            })
    }

    fn view_controls(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let button = |id: &'static str, label: gpui::SharedString| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .h(px(28.))
                .px(px(10.))
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(BORDER)))
                .flex()
                .items_center()
                .child(label)
        };
        div()
            .absolute()
            .top(px(16.))
            .right(px(self.canvas_insets().1))
            .p(px(4.))
            .rounded(px(8.))
            .bg(rgb(PANEL))
            .border_1()
            .border_color(rgb(BORDER))
            .text_size(px(11.))
            .flex()
            .items_center()
            .gap(px(2.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                button("zoom-out", "−".into())
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_center(0.8, cx))),
            )
            .child(
                button(
                    "zoom-reset",
                    format!("{:.0}%", self.view.zoom * 100.).into(),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.zoom_center(1. / this.view.zoom, cx);
                })),
            )
            .child(
                button("zoom-in", "+".into())
                    .on_click(cx.listener(|this, _, _, cx| this.zoom_center(1.25, cx))),
            )
            .child(
                button("zoom-fit", t("zoom-fit").into())
                    .on_click(cx.listener(|this, _, _, cx| this.fit_selected(cx))),
            )
    }

    fn zoom_center(&mut self, factor: f32, cx: &mut Context<Self>) {
        if self.gesture.is_some() {
            return;
        }
        let size = self.bounds.get().size;
        self.view.zoom_at(
            point(f32::from(size.width) / 2., f32::from(size.height) / 2.),
            self.view.zoom * factor,
        );
        cx.notify();
    }
}
