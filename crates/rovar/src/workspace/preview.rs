use super::selection::Marquee;
use super::*;
use std::collections::BTreeSet;

#[derive(Default)]
pub(super) struct State {
    editable: bool,
    pan: Option<(Point<Pixels>, Point<f32>)>,
}

impl Workspace {
    pub(crate) fn enable_preview(&mut self, editable: bool) {
        self.preview = Some(State {
            editable,
            ..Default::default()
        });
        self.draw_tool = None;
        self.toolbar.hand = false;
        self.space_down = false;
    }

    pub(crate) fn disable_preview(
        &mut self,
        views: pages::Views,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.suspend(window, cx);
        // View restoration activates the stored page. Park the latest local
        // edits first so restoring the camera never restores an older document.
        self.park_page(cx);
        self.preview = None;
        self.space_down = false;
        self.restore_page_views(views, window, cx);
        cx.notify();
    }

    pub(super) fn preview_read_only(&self) -> bool {
        self.preview.as_ref().is_some_and(|state| !state.editable)
    }

    pub(crate) fn preview_page(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.switch_page(id, window, cx);
        self.focus_canvas(window, cx);
    }

    /// Only selection and copying are available on the server snapshot. This
    /// deliberately does not dispatch the editor's cut/paste/history shortcuts.
    pub(crate) fn preview_key(&mut self, event: &gpui::KeyDownEvent, cx: &mut Context<Self>) {
        if !self.preview_read_only() {
            return;
        }
        let command = event.keystroke.modifiers.control || event.keystroke.modifiers.platform;
        match (event.keystroke.key.as_str(), command) {
            ("c", true) => self.copy_selection(cx),
            ("a", true) => self.set_selection(self.canvas_layer_order().into_iter().collect(), cx),
            ("space", false) => self.space_down = true,
            _ => return,
        }
        cx.notify();
    }

    fn preview_pointer(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_canvas(window, cx);
        if event.button == MouseButton::Middle
            || (event.button == MouseButton::Left && self.space_down)
        {
            self.preview.as_mut().unwrap().pan = Some((event.position, self.view.pan));
        } else if event.button == MouseButton::Left {
            let position = self.board_point(None, event.position);
            let hit = self.canvas_layer_order().into_iter().rev().find(|id| {
                self.layer_editable(*id)
                    && self.world_rect(*id).is_some_and(|rect| {
                        let p = crate::rotation::around(
                            position,
                            crate::rotation::center(rect),
                            -self.object_rotation(*id),
                        );
                        p.x >= rect.x
                            && p.y >= rect.y
                            && p.x <= rect.x + rect.width
                            && p.y <= rect.y + rect.height
                    })
            });
            if let Some(id) = hit {
                let id = if event.click_count >= 2
                    || event.modifiers.alt
                    || event.modifiers.control
                    || event.modifiers.platform
                {
                    id
                } else {
                    self.group_target(id)
                };
                if event.modifiers.shift {
                    self.toggle_selection(id, cx);
                } else {
                    self.set_selection(BTreeSet::from([id]), cx);
                }
            } else {
                let initial = self.selection_ids();
                self.marquee = Some(Marquee {
                    start: position,
                    rect: creation::drag_rect(position, position, false),
                    initial,
                });
                self.marquee_additive = event.modifiers.shift;
                if !event.modifiers.shift {
                    self.select(None, cx);
                }
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    pub(super) fn preview_canvas(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let bounds = self.bounds.clone();
        div()
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(WORKSPACE))
            .text_color(rgb(TEXT))
            .font_family(crate::ui_font::family(cx))
            .on_paint_before_children(move |rect, _, _, _| bounds.set(rect))
            .child(self.scene_content(cx))
            // Cover all editable scene children; selection is handled here so
            // text editing, resize handles, drop and context menus cannot mutate
            // the server snapshot.
            .child(
                div()
                    .id("comparison-canvas")
                    .debug_selector(|| "comparison-canvas".into())
                    .absolute()
                    .inset_0()
                    .occlude()
                    .track_focus(&self.focus)
                    .when(self.space_down, |el| el.cursor(gpui::CursorStyle::OpenHand))
                    .on_any_mouse_down(cx.listener(Self::preview_pointer))
                    .on_key_up(cx.listener(|this, event: &gpui::KeyUpEvent, _, cx| {
                        if event.keystroke.key == "space" {
                            this.space_down = false;
                            cx.notify();
                        }
                    }))
                    .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                        if event.pressed_button.is_none() {
                            this.preview.as_mut().unwrap().pan = None;
                            this.marquee = None;
                        }
                        if let Some((start, pan)) = this.preview.as_ref().unwrap().pan {
                            let delta = event.position - start;
                            this.view.pan = pan + point(f32::from(delta.x), f32::from(delta.y));
                            cx.notify();
                        } else if this.marquee.is_some() {
                            this.move_marquee(event.position, cx);
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.preview.as_mut().unwrap().pan = None;
                            this.marquee = None;
                            cx.notify();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Middle,
                        cx.listener(|this, _, _, _| {
                            this.preview.as_mut().unwrap().pan = None;
                        }),
                    )
                    .on_scroll_wheel(cx.listener(Self::scroll_canvas))
                    .on_pinch(cx.listener(Self::pinch_canvas)),
            )
            .child(self.view_controls(cx))
    }
}
