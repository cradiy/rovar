use super::*;
use crate::i18n::t;
use crate::layer::LayerState;

impl Workspace {
    pub(in crate::workspace) fn layer_info(
        &self,
        id: usize,
    ) -> Option<(LayerState, Option<usize>)> {
        let own = self
            .hierarchy
            .groups
            .get(&id)
            .map(|g| g.layer)
            .or_else(|| self.boards.iter().find(|b| b.id == id).map(|b| b.layer))
            .or_else(|| self.shapes.iter().find(|s| s.id == id).map(|s| s.layer))
            .or_else(|| self.texts.iter().find(|t| t.id == id).map(|t| t.layer))?;
        Some((own, self.layer_parent(id)))
    }

    pub(in crate::workspace) fn layer_state_mut(&mut self, id: usize) -> Option<&mut LayerState> {
        if let Some(group) = self.hierarchy.groups.get_mut(&id) {
            return Some(&mut group.layer);
        }
        if let Some(board) = self.boards.iter_mut().find(|b| b.id == id) {
            return Some(&mut board.layer);
        }
        if let Some(shape) = self.shapes.iter_mut().find(|s| s.id == id) {
            return Some(&mut shape.layer);
        }
        self.texts
            .iter_mut()
            .find(|t| t.id == id)
            .map(|t| &mut t.layer)
    }

    pub(in crate::workspace) fn effective_layer(
        &self,
        mut own: LayerState,
        parent: Option<usize>,
    ) -> LayerState {
        let mut parent = parent;
        while let Some(id) = parent {
            let Some((state, next)) = self.layer_info(id) else {
                break;
            };
            own.hidden |= state.hidden;
            own.locked |= state.locked;
            parent = next;
        }
        own
    }

    pub(in crate::workspace) fn layer_editable(&self, id: usize) -> bool {
        self.layer_info(id)
            .is_some_and(|(own, parent)| self.effective_layer(own, parent).editable())
    }

    fn toggle_layer(
        &mut self,
        id: usize,
        visibility: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((_, parent)) = self.layer_info(id) else {
            return;
        };
        let inherited = self.effective_layer(LayerState::default(), parent);
        if if visibility {
            inherited.hidden
        } else {
            inherited.locked
        } {
            return;
        }
        self.choose_tool(toolbar::Tool::Move, window, cx);
        let Some(layer) = self.layer_state_mut(id) else {
            return;
        };
        let before = *layer;
        if visibility {
            layer.hidden = !layer.hidden;
        } else {
            layer.locked = !layer.locked;
        }
        self.history
            .borrow_mut()
            .record(vec![Change::Layer { id, value: before }], None);
        let ids = self.selection_ids();
        self.set_selection(ids, cx);
        cx.notify();
    }

    pub(super) fn layer_controls(
        &self,
        id: usize,
        own: LayerState,
        parent: Option<usize>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let inherited = self.effective_layer(LayerState::default(), parent);
        let effective = self.effective_layer(own, parent);
        div()
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .flex()
            .items_center()
            .gap(px(2.))
            .flex_shrink_0()
            .children([false, true].map(|visibility| {
                let (active, disabled, glyph, label) = if visibility {
                    (
                        effective.hidden,
                        inherited.hidden,
                        if effective.hidden {
                            LucideIcons::EyeOff
                        } else {
                            LucideIcons::Eye
                        },
                        if inherited.hidden {
                            t("parent-hidden")
                        } else if own.hidden {
                            t("show-layer")
                        } else {
                            t("hide-layer")
                        },
                    )
                } else {
                    (
                        effective.locked,
                        inherited.locked,
                        if effective.locked {
                            LucideIcons::Lock
                        } else {
                            LucideIcons::LockOpen
                        },
                        if inherited.locked {
                            t("parent-locked")
                        } else if own.locked {
                            t("unlock-layer")
                        } else {
                            t("lock-layer")
                        },
                    )
                };
                let action = if visibility { "visibility" } else { "lock" };
                div()
                    .id((action, id))
                    .debug_selector(move || format!("layer-{action}-{id}"))
                    .size(px(22.))
                    .flex_shrink_0()
                    .rounded(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(!disabled, |el| {
                        el.cursor_pointer().hover(|s| s.bg(rgba(0xffffff18)))
                    })
                    .tooltip(move |_, cx| cx.new(|_| toolbar::ToolTip(label.into())).into())
                    .child(icon(glyph, 14.).text_color(rgb(if active && !disabled {
                        ACCENT
                    } else {
                        MUTED
                    })))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.toggle_layer(id, visibility, window, cx);
                    }))
            }))
    }
}
