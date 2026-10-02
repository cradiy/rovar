use super::*;

impl Workspace {
    pub(in crate::editor) fn selection_resize_controls(&self, cx: &mut Context<Self>) -> Div {
        let overlay = div().absolute().inset_0();
        if self.space_down
            || self.toolbar.hand
            || self.draw_tool.is_some()
            || self
                .gesture
                .is_some_and(|g| !matches!(g.kind, GestureKind::SelectionResize { .. }))
        {
            return overlay;
        }
        let Some(rect) = self.selection_resize_bounds() else {
            return overlay;
        };
        let p = self.view.screen(point(rect.x, rect.y));
        let width = rect.width * self.view.zoom;
        let height = rect.height * self.view.zoom;
        overlay.children(Handle::ALL.into_iter().enumerate().map(|(index, handle)| {
            let x = p.x + (handle.0 as f32 + 1.) * width / 2.;
            let y = p.y + (handle.1 as f32 + 1.) * height / 2.;
            let corner = handle.0 != 0 && handle.1 != 0;
            let (left, top, w, h) = if handle.0 == 0 {
                (p.x + 6., y - 6., (width - 12.).max(0.), 12.)
            } else if handle.1 == 0 {
                (x - 6., p.y + 6., 12., (height - 12.).max(0.))
            } else {
                (x - 6., y - 6., 12., 12.)
            };
            div()
                .id(("selection-resize-handle", index))
                .debug_selector(move || format!("selection-resize-handle-{index}"))
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(w))
                .h(px(h))
                .cursor(resize_cursor(handle))
                .flex()
                .items_center()
                .justify_center()
                .when(corner, |el| {
                    el.child(
                        div()
                            .size(px(7.))
                            .bg(rgb(0xffffff))
                            .border_1()
                            .border_color(rgb(ACCENT)),
                    )
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event, window, cx| {
                        this.begin_selection_resize(handle, event, window, cx);
                    }),
                )
        }))
    }
}
