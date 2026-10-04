use super::*;

impl Workspace {
    pub(in crate::editor) fn selection_labels(&self, cx: &gpui::App) -> Div {
        // While inspecting a candidate, its name takes precedence over the
        // current selection so overlapping objects do not stack their labels.
        let ids = self
            .pick_hover
            .filter(|_| uic::components::context_menu::is_open(cx))
            .map_or_else(|| self.selection_ids(), |id| BTreeSet::from([id]));
        div()
            .absolute()
            .inset_0()
            .children(ids.into_iter().filter_map(|id| {
                // Frames already have persistent canvas labels.
                if self.boards.iter().any(|board| board.id == id)
                    || self
                        .layer_info(id)
                        .is_none_or(|(own, parent)| self.effective_layer(own, parent).hidden)
                {
                    return None;
                }
                let rect = self.world_bounds(id)?;
                let position = self.view.screen(point(rect.x, rect.y));
                Some(
                    div()
                        .debug_selector(move || format!("selection-name-{id}"))
                        .absolute()
                        .left(px(position.x))
                        .top(px(position.y - 26.))
                        .h(px(24.))
                        .max_w(px((rect.width * self.view.zoom).clamp(100., 240.)))
                        .truncate()
                        .text_size(px(11.))
                        .text_color(ACCENT.color())
                        .child(self.layer_name(id, cx)),
                )
            }))
    }
}
