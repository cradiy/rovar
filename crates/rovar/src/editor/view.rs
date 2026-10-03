//! Workspace composition and top-level keyboard routing.

use super::*;

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.reflow_layout(cx);
        self.sync_components(window, cx);
        self.sync_layout_inputs(cx);
        self.sync_video_visibility(cx);
        self.sync_export_controls(window, cx);
        self.load_visible_media(window, cx);
        if self.preview_read_only() {
            return self.preview_canvas(window, cx).into_any_element();
        }
        self.panels.window_width = f32::from(window.viewport_size().width);
        let drag_cursor = self.gesture_cursor();
        div()
            .relative()
            .when_some(drag_cursor, |el, cursor| {
                // Every drag keeps its interaction cursor outside the starting hitbox. The
                // request disappears on the first frame after release/cancel.
                el.on_paint_before_children(move |_, _, window, _| {
                    window.set_window_cursor_style(cursor);
                })
            })
            .on_modifiers_changed(cx.listener(
                |this, event: &gpui::ModifiersChangedEvent, window, cx| {
                    this.update_measurement(
                        window.mouse_position(),
                        event.modifiers.alt,
                        window,
                        cx,
                    );
                    if this.gesture.is_some_and(|g| {
                        matches!(
                            g.kind,
                            GestureKind::SelectionMove
                                | GestureKind::CornerRadius
                                | GestureKind::SelectionResize { .. }
                                | GestureKind::Rotate { .. }
                                | GestureKind::Move { .. }
                                | GestureKind::Resize { .. }
                                | GestureKind::Text { .. }
                                | GestureKind::Shape { .. }
                                | GestureKind::Draw
                                | GestureKind::LineEnd { .. }
                        )
                    }) {
                        this.snapping.bypass = event.modifiers.alt;
                        this.move_gesture(window.mouse_position(), event.modifiers.shift, cx);
                    }
                },
            ))
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if this.image_crop.is_some() {
                    match event.keystroke.key.as_str() {
                        "escape" => this.finish_image_crop(false, window, cx),
                        "enter" => this.finish_image_crop(true, window, cx),
                        _ => {}
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if this.corner_editor.editing() {
                    if event.keystroke.key == "escape" {
                        this.finish_corner_input(false, cx);
                        this.focus.focus(window, cx);
                        cx.stop_propagation();
                    }
                    return;
                }
                if this.spacing.editing() {
                    if event.keystroke.key == "escape" {
                        this.finish_spacing_input(false, cx);
                        this.focus.focus(window, cx);
                        cx.stop_propagation();
                    }
                    return;
                }
                if let Some(id) = this.pages.delete.clone() {
                    if event.keystroke.key == "escape" {
                        this.pages.delete = None;
                        cx.notify();
                    } else if event.keystroke.key == "enter" {
                        this.delete_page(&id, window, cx);
                    }
                    cx.stop_propagation();
                    return;
                }
                if this.pages.renaming.is_some() {
                    if event.keystroke.key == "escape" {
                        this.finish_page_rename(false, window, cx);
                        cx.stop_propagation();
                    }
                    return;
                }
                if this.components.rename.is_some() {
                    if event.keystroke.key == "escape" {
                        this.finish_variant_rename(false, window, cx);
                        cx.stop_propagation();
                    }
                    return;
                }
                if this.colors.dialog.is_some() {
                    if event.keystroke.key == "escape" {
                        if this.gesture.is_some_and(|g| {
                            matches!(
                                g.kind,
                                GestureKind::ColorStyleProperty { .. }
                                    | GestureKind::ColorStyleStop { .. }
                                    | GestureKind::GradientMidpoint { style: true, .. }
                                    | GestureKind::GradientSeam { style: true, .. }
                            )
                        }) {
                            this.cancel_gesture(window, cx);
                        } else if !this.cancel_style_input(window, cx) {
                            this.close_color_dialog(window, cx);
                        }
                        cx.stop_propagation();
                    }
                    return;
                }
                if this.assets.dialog.is_some() {
                    if event.keystroke.key == "escape" {
                        this.cancel_asset_dialog(window, cx);
                        cx.stop_propagation();
                    } else if event.keystroke.key == "enter"
                        && matches!(
                            this.assets.dialog,
                            Some(assets::Dialog::Delete(_) | assets::Dialog::DocumentDelete(_))
                        )
                    {
                        this.confirm_asset_dialog(window, cx);
                        cx.stop_propagation();
                    }
                    return;
                }
                if uic::components::context_menu::is_open(cx) {
                    if this.pick_hover.take().is_some() {
                        cx.notify();
                    }
                    return;
                }
                if this.gesture.is_some_and(|g| {
                    matches!(
                        g.kind,
                        GestureKind::LayerSort
                            | GestureKind::CornerRadius
                            | GestureKind::SelectionResize { .. }
                            | GestureKind::Spacing { .. }
                            | GestureKind::Property { .. }
                            | GestureKind::FillGradientStop { .. }
                            | GestureKind::GradientMidpoint { style: false, .. }
                            | GestureKind::GradientSeam { style: false, .. }
                            | GestureKind::LayoutProperty { .. }
                            | GestureKind::Panel { .. }
                            | GestureKind::MultiProperty { .. }
                    )
                }) {
                    if event.keystroke.key == "escape"
                        || ((event.keystroke.modifiers.control
                            || event.keystroke.modifiers.platform)
                            && event.keystroke.key == "z")
                    {
                        this.cancel_gesture(window, cx);
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape"
                    && this.inspector.gradient_menu.read(cx).is_open()
                {
                    this.inspector
                        .gradient_menu
                        .update(cx, |state, cx| state.close(window, cx));
                    cx.stop_propagation();
                } else if event.keystroke.key == "escape"
                    && this
                        .inspector
                        .paint_popovers
                        .iter()
                        .any(|popover| popover.read(cx).is_open())
                {
                    for popover in &this.inspector.paint_popovers {
                        popover.update(cx, |state, cx| state.close(window, cx));
                    }
                    cx.stop_propagation();
                } else {
                    this.history_key(event, window, cx);
                }
            }))
            .on_any_mouse_down(cx.listener(|this, _, window, cx| {
                this.close_tool_menus(window, cx);
            }))
            .size_full()
            .overflow_hidden()
            .bg(rgb(PANEL))
            .text_color(rgb(TEXT))
            .font_family(crate::ui::font::family(cx))
            .flex()
            .flex_col()
            .child(div().flex_1().min_h_0().flex().child(self.editor(cx)))
            .child(uic::components::context_menu::layer(cx))
            .when(self.assets.dialog.is_some(), |el| {
                el.child(self.asset_dialog(cx))
            })
            .when(self.colors.dialog.is_some(), |el| {
                el.child(self.color_style_dialog(window, cx))
            })
            .when(self.pages.delete.is_some(), |el| {
                el.child(self.page_delete_dialog(cx))
            })
            .into_any_element()
    }
}
