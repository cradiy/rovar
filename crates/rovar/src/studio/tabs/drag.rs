use super::*;
use gpui::{DragSourceWindowPolicy, SystemDragOptions};

impl Studio {
    pub(in crate::studio) fn begin_tab_drag(
        &mut self,
        payload: DragTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.dismiss_tab_preview(cx);
        self.dragging = Some(payload.token);
        self.strip.drag = Some(payload.clone());
        self.strip.snap_index = None;
        self.strip.snap_left = None;
        if let Some(editor) = self
            .tabs
            .iter()
            .find(|tab| tab.token == payload.token)
            .and_then(|tab| tab.editor.clone())
        {
            editor.update(cx, |editor, cx| editor.suspend(window, cx));
        }
        cx.notify();
    }

    fn preview_state(
        payload: &DragTab,
        detached: bool,
        hidden: bool,
        pointer_y: f32,
        lock: bool,
        cx: &mut gpui::App,
    ) {
        if let Some(preview) = payload.preview.borrow().clone() {
            preview.update(cx, |preview, cx| {
                let y = if lock {
                    locked_y(pointer_y, preview.cursor_offset_y)
                } else {
                    0.
                };
                if preview.detached != detached
                    || preview.hidden != hidden
                    || (preview.y_offset - y).abs() > 0.01
                {
                    preview.detached = detached;
                    preview.hidden = hidden;
                    preview.y_offset = y;
                    cx.notify();
                }
            });
        }
    }

    pub(in crate::studio) fn tab_drag_moved(
        &mut self,
        event: &gpui::DragMoveEvent<DragTab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_tab_drag(event.drag(cx).clone(), event.event.position, window, cx);
    }

    pub(in crate::studio) fn move_tab_drag(
        &mut self,
        payload: DragTab,
        position: gpui::Point<gpui::Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let native = payload.transaction.borrow().native;
        let viewport = f32::from(window.viewport_size().width);
        let x = f32::from(position.x);
        let y = f32::from(position.y);
        let snapped = self.strip.snap_index.is_some();
        let over = over_strip(x, y, viewport, native, snapped);
        let mut changed = self
            .strip
            .drag
            .as_ref()
            .is_none_or(|drag| !drag.same(&payload));
        self.strip.drag = Some(payload.clone());
        if over {
            if native {
                let count = self.tabs.len() + 1;
                let width = self.tab_width(viewport, count);
                let cursor = payload.transaction.borrow().cursor_offset_x;
                let max_left = (viewport - self.tab_right_reserved() - width).max(self.tab_left());
                let left = (x - cursor).clamp(self.tab_left(), max_left)
                    - self.tab_left()
                    - f32::from(self.tab_scroll.offset().x);
                let index = insertion_index(left + width * 0.5, width, count).min(self.tabs.len());
                changed |= self.strip.snap_index != Some(index)
                    || self
                        .strip
                        .snap_left
                        .is_none_or(|old| (old - left).abs() > 0.01);
                self.strip.snap_index = Some(index);
                self.strip.snap_left = Some(left);
                Self::preview_state(&payload, false, true, y, true, cx);
            } else if payload.transaction.borrow().owner == Some((self.window_id, payload.token)) {
                self.strip.snap_index = None;
                self.strip.snap_left = None;
                Self::preview_state(&payload, false, false, y, true, cx);
                if let Some(old) = self.tabs.iter().position(|tab| tab.token == payload.token) {
                    let width = self.tab_width(viewport, self.tabs.len());
                    let index = insertion_index(
                        x - self.tab_left() - f32::from(self.tab_scroll.offset().x),
                        width,
                        self.tabs.len(),
                    );
                    if old != index {
                        let tab = self.tabs.remove(old);
                        self.tabs.insert(index, tab);
                        changed = true;
                    }
                }
            }
        } else if !native
            && payload.transaction.borrow().owner == Some((self.window_id, payload.token))
        {
            Self::preview_state(&payload, true, false, y, false, cx);
            let source_window = if self.tabs.len() == 1 {
                DragSourceWindowPolicy::HideWhileNative
            } else {
                DragSourceWindowPolicy::KeepVisible
            };
            match window.promote_active_drag_to_system_with_options(
                SystemDragOptions {
                    source_window,
                    ..Default::default()
                },
                cx,
            ) {
                Ok(_) => {
                    payload.transaction.borrow_mut().native = true;
                    self.detach_owned_tab(&payload, window, cx);
                    changed = true;
                }
                Err(error) => {
                    self.error = Some(error.to_string());
                    changed = true;
                }
            }
        } else {
            changed |= self.strip.snap_index.is_some() || self.strip.snap_left.is_some();
            self.strip.snap_index = None;
            self.strip.snap_left = None;
            Self::preview_state(&payload, true, false, y, false, cx);
        }
        if changed {
            cx.notify();
        }
    }

    pub(in crate::studio) fn tab_drag_exited(
        &mut self,
        _: &gpui::MouseExitEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(payload) = self
            .strip
            .drag
            .clone()
            .filter(|payload| payload.transaction.borrow().native)
        else {
            return;
        };
        self.strip.snap_index = None;
        self.strip.snap_left = None;
        Self::preview_state(&payload, true, false, 0., false, cx);
        cx.notify();
    }

    pub(in crate::studio) fn detach_owned_tab(
        &mut self,
        payload: &DragTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let owner = payload.transaction.borrow().owner;
        let Some((id, token)) = owner.filter(|(id, _)| *id == self.window_id) else {
            return false;
        };
        let Some(index) = self.tabs.iter().position(|tab| tab.token == token) else {
            return false;
        };
        if self.tabs[index].saving || self.tabs[index].loading || self.tabs[index].exporting {
            return false;
        }
        if let Some(editor) = &self.tabs[index].editor {
            editor.update(cx, |editor, cx| editor.suspend(window, cx));
        }
        let tab = self.tabs.remove(index);
        self.strip.slots.remove(&token);
        if self.active == Some(token) {
            self.active = None;
            let neighbor = self
                .tabs
                .get(index.min(self.tabs.len().saturating_sub(1)))
                .map(|tab| tab.token);
            self.select_tab(neighbor, window, cx);
            if neighbor.is_none() {
                self.focus.focus(window, cx);
            }
        }
        let mut transaction = payload.transaction.borrow_mut();
        debug_assert_eq!(transaction.owner, Some((id, token)));
        transaction.owner = None;
        transaction.detached = Some(tab);
        self.dragging = None;
        drop(transaction);
        cx.notify();
        true
    }

    pub(in crate::studio) fn dropped_on_strip(
        &mut self,
        payload: &DragTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if cfg!(target_family = "wasm") {
            return;
        }
        let width = self.tab_width(f32::from(window.viewport_size().width), self.tabs.len() + 1);
        let index = self.strip.snap_index.unwrap_or_else(|| {
            insertion_index(
                f32::from(window.mouse_position().x)
                    - self.tab_left()
                    - f32::from(self.tab_scroll.offset().x),
                width,
                self.tabs.len() + 1,
            )
        });
        self.receive_tab(payload, Some(index), window, cx);
    }

    pub(in crate::studio) fn dropped_as_window(
        &mut self,
        payload: &DragTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let position = window.mouse_position();
        let origin = window.bounds().origin
            + gpui::point(
                position.x - gpui::px(payload.transaction.borrow().cursor_offset_x),
                position.y - gpui::px(TAB_TOP + TAB_HEIGHT * 0.5),
            );
        let payload = payload.clone();
        cx.defer(move |cx| {
            Self::open_detached_tab(&payload, Some(origin), cx);
        });
        cx.stop_propagation();
    }

    pub(in crate::studio) fn reveal_tab(&self, index: usize, window: &Window) {
        let viewport = f32::from(window.viewport_size().width);
        let width = self.tab_width(viewport, self.tabs.len());
        let available = (viewport - self.tab_left() - self.tab_right_reserved() + 32.).max(width);
        let left = slot_left(index, width);
        let offset = f32::from(self.tab_scroll.offset().x);
        let next = if left + offset < 0. {
            -left
        } else if left + width + offset > available {
            available - left - width
        } else {
            offset
        };
        self.tab_scroll
            .set_offset(gpui::point(px(next.min(0.)), px(0.)));
    }
}
