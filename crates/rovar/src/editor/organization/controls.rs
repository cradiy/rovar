use super::*;
use crate::i18n::t;
use crate::ui::theme::Color;

impl Workspace {
    pub(in crate::editor) fn begin_rename(
        &mut self,
        id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.layer_editable(id) {
            return;
        }
        self.cancel_gesture(window, cx);
        self.seal_text_edits(cx);
        self.set_selection(BTreeSet::from([id]), cx);
        self.sidebar.renaming = Some(id);
        let name = self.layer_name(id, cx);
        self.rename_input.update(cx, |input, cx| {
            input.set_value(name, cx);
        });
        self.rename_input.focus_handle(cx).focus(window, cx);
        let input = self.rename_input.clone();
        // Frame callbacks run before painting. Wait for the new input's dispatch
        // node to be mounted before sending its SelectAll action.
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| {
                if input.focus_handle(cx).is_focused(window)
                    && let Ok(action) = cx.build_action("text_input::SelectAll", None)
                {
                    window.dispatch_action(action, cx);
                }
            });
        });
        cx.notify();
    }

    pub(in crate::editor) fn finish_rename(
        &mut self,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(id) = self.sidebar.renaming.take() else {
            return;
        };
        let name = self.rename_input.read(cx).value().trim().to_owned();
        if commit && self.layer_editable(id) && !name.is_empty() && name != self.layer_name(id, cx)
        {
            let change = if let Some((index, b)) =
                self.boards.iter_mut().enumerate().find(|(_, b)| b.id == id)
            {
                let before = b.clone();
                b.name = name;
                Change::Board {
                    id,
                    index,
                    value: Some(before),
                }
            } else if let Some((index, s)) =
                self.shapes.iter_mut().enumerate().find(|(_, s)| s.id == id)
            {
                let before = s.clone();
                s.name = name;
                Change::Shape {
                    id,
                    index,
                    value: Some(before),
                }
            } else {
                let before = self.snapshot_hierarchy();
                if let Some(g) = self.hierarchy.groups.get_mut(&id) {
                    g.name = name;
                } else {
                    self.hierarchy.names.insert(id, name);
                }
                before
            };
            self.history.borrow_mut().record(vec![change], None);
            self.sync_fields(cx);
        }
        if self.rename_input.focus_handle(cx).is_focused(window) {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    pub(in crate::editor) fn layer_mouse_down(
        &mut self,
        id: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if !self.layer_editable(id) {
            return;
        }
        if event.click_count >= 2 {
            self.begin_rename(id, window, cx);
            return;
        }
        self.choose_tool(toolbar::Tool::Move, window, cx);
        if event.modifiers.shift {
            self.toggle_selection(id, cx);
            return;
        }
        if !self.is_selected(id) {
            self.set_selection(BTreeSet::from([id]), cx);
        }
        let selection = self.selection_ids();
        if self.common_parent(&selection).is_none() {
            return;
        }
        self.layer_drag = Some(LayerDrag {
            ids: selection.into_iter().collect(),
            ..Default::default()
        });
        self.begin(
            GestureKind::LayerSort,
            event.position,
            event.button,
            window,
            cx,
        );
    }

    pub(in crate::editor) fn move_layer_sort(&mut self, position: Point<Pixels>) {
        let Some(gesture) = self.gesture else {
            return;
        };
        if (position.x - gesture.start.x).abs() + (position.y - gesture.start.y).abs() < px(4.) {
            return;
        }
        let Some(drag) = self.layer_drag.as_ref() else {
            return;
        };
        let parent = self.layer_parent(drag.ids[0]);
        let target = self
            .layer_row_bounds
            .borrow()
            .iter()
            .find_map(|(id, bounds)| {
                (bounds.contains(&position)
                    && !drag.ids.contains(id)
                    && self.layer_parent(*id) == parent)
                    .then_some((*id, position.y < bounds.center().y))
            });
        let drag = self.layer_drag.as_mut().unwrap();
        drag.moved = true;
        drag.target = target;
    }

    pub(in crate::editor) fn finish_layer_sort(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.layer_drag.take() else {
            return;
        };
        let Some((target, above)) = drag.target.filter(|_| drag.moved) else {
            return;
        };
        let parent = self.layer_parent(target);
        let mut order = self.ordered_children(parent);
        let original = order.clone();
        let moving: Vec<_> = order
            .iter()
            .copied()
            .filter(|id| drag.ids.contains(id))
            .collect();
        order.retain(|id| !drag.ids.contains(id));
        let Some(at) = order.iter().position(|id| *id == target) else {
            return;
        };
        order.splice(at + usize::from(above)..at + usize::from(above), moving);
        if order != original {
            let before = self.snapshot_hierarchy();
            self.set_sibling_order(&order);
            self.history.borrow_mut().record(vec![before], None);
            cx.notify();
        }
    }

    pub(in crate::editor) fn organization_actions(&self, cx: &mut Context<Self>) -> Div {
        let ids = self.selection_ids();
        let mut row = div()
            .h(px(38.))
            .px(px(10.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(3.))
            .border_b_1()
            .border_color(BORDER.color());
        for (id, label, glyph, action, enabled) in [
            (
                "group-selection",
                t("group-hint"),
                LucideIcons::Group,
                0,
                self.can_group(),
            ),
            (
                "ungroup-selection",
                t("ungroup-hint"),
                LucideIcons::Ungroup,
                1,
                ids.iter().any(|id| self.hierarchy.groups.contains_key(id)),
            ),
            (
                "raise-layer",
                t("raise-hint"),
                LucideIcons::ArrowUp,
                2,
                !ids.is_empty() && self.common_parent(&ids).is_some(),
            ),
            (
                "lower-layer",
                t("lower-hint"),
                LucideIcons::ArrowDown,
                3,
                !ids.is_empty() && self.common_parent(&ids).is_some(),
            ),
            (
                "rename-layer",
                t("rename-hint"),
                LucideIcons::Pencil,
                4,
                ids.len() == 1,
            ),
        ] {
            row = row.child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .size(px(28.))
                    .rounded(px(5.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(if enabled { 1. } else { 0.3 })
                    .when(enabled, |el| {
                        el.cursor_pointer()
                            .hover(|s| s.bg(Color::Accent.color().opacity(0.1333)))
                    })
                    .tooltip(move |_, cx| cx.new(|_| toolbar::ToolTip(label.into())).into())
                    .child(icon(glyph, 15.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !enabled {
                            return;
                        }
                        this.focus.focus(window, cx);
                        match action {
                            0 => this.group_selection(cx),
                            1 => this.ungroup_selection(cx),
                            2 => this.shift_layers(true, false, cx),
                            3 => this.shift_layers(false, false, cx),
                            _ => {
                                if let Some(id) = this.selection_ids().first() {
                                    this.begin_rename(*id, window, cx);
                                }
                            }
                        }
                    })),
            );
        }
        row
    }
}
