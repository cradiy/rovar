use super::*;
use crate::scene::auto_layout::{Constraint, Constraints};

impl Workspace {
    fn constraint_geometry(&self, id: usize) -> Option<(Rect, [f32; 2])> {
        let parent = self.layer_parent(id)?;
        if self.is_layout_flow_item(id)
            || !(self.boards.iter().any(|b| b.id == parent)
                || self.hierarchy.layouts.contains_key(&parent))
        {
            return None;
        }
        let bounds = self.world_rect(parent)?;
        let mut rect = self.world_rect(id)?;
        rect.x -= bounds.x;
        rect.y -= bounds.y;
        Some((rect, [bounds.width.max(1.), bounds.height.max(1.)]))
    }

    pub(in crate::editor) fn refresh_constraints(&mut self, id: usize) {
        let geometry = self.constraint_geometry(id);
        if let Some(sizing) = self.hierarchy.sizing.get_mut(&id)
            && let Some(constraints) = &mut sizing.constraints
        {
            if let Some((rect, parent_size)) = geometry {
                constraints.rect = rect;
                constraints.parent_size = parent_size;
            } else {
                sizing.constraints = None;
            }
        }
    }

    pub(super) fn choose_constraint(
        &mut self,
        id: usize,
        axis: usize,
        mode: Constraint,
        cx: &mut Context<Self>,
    ) {
        if !self.layer_editable(id) {
            return;
        }
        let Some((rect, parent_size)) = self.constraint_geometry(id) else {
            return;
        };
        let current = self
            .hierarchy
            .sizing
            .get(&id)
            .and_then(|s| s.constraints)
            .map_or(Constraint::Start, |c| {
                if axis == 0 { c.horizontal } else { c.vertical }
            });
        if current == mode {
            return;
        }
        let before = self.snapshot_hierarchy();
        let sizing = self.hierarchy.sizing.entry(id).or_default();
        let constraints = sizing.constraints.get_or_insert(Constraints {
            horizontal: Constraint::Start,
            vertical: Constraint::Start,
            rect,
            parent_size,
        });
        constraints.rect = rect;
        constraints.parent_size = parent_size;
        if axis == 0 {
            constraints.horizontal = mode;
        } else {
            constraints.vertical = mode;
        }
        if matches!(mode, Constraint::Stretch | Constraint::Scale) {
            if axis == 0 {
                sizing.width = Mode::Fixed;
            } else {
                sizing.height = Mode::Fixed;
            }
        }
        self.history.borrow_mut().record(vec![before], None);
        self.auto_layout.revision = None;
        self.reflow_layout(cx);
        cx.notify();
    }

    pub(in crate::editor) fn constraint_controls(&self, cx: &mut Context<Self>) -> Div {
        if self.grid_item_target().is_some() {
            return self.grid_item_controls(cx);
        }
        let Some(id) = self
            .layout_target()
            .filter(|id| self.constraint_geometry(*id).is_some())
        else {
            return div();
        };
        let constraints = self.hierarchy.sizing.get(&id).and_then(|s| s.constraints);
        inspector::inspector_section(t("layout-constraints"))
            .text_size(px(12.))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(16.))
            .children([0, 1].map(|axis| {
                let mode = constraints.map_or(Constraint::Start, |c| {
                    if axis == 0 { c.horizontal } else { c.vertical }
                });
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().flex_1().text_color(rgb(MUTED)).child(t(if axis == 0 {
                        "layout-horizontal"
                    } else {
                        "layout-vertical"
                    })))
                    .child(
                        div()
                            .id(("layout-constraint", axis))
                            .debug_selector(move || format!("layout-constraint-{axis}"))
                            .w(px(132.))
                            .h(px(28.))
                            .px(px(8.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .bg(rgb(0x282b33))
                            .text_size(px(12.))
                            .text_color(rgb(TEXT))
                            .cursor_pointer()
                            .child(t(constraint_label(mode, axis)))
                            .child(icon(LucideIcons::ChevronDown, 12.).text_color(rgb(MUTED)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.constraint_menu(id, axis, window, cx)
                            })),
                    )
            }))
    }

    fn constraint_menu(
        &mut self,
        id: usize,
        axis: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .hierarchy
            .sizing
            .get(&id)
            .and_then(|s| s.constraints)
            .map_or(Constraint::Start, |c| {
                if axis == 0 { c.horizontal } else { c.vertical }
            });
        let mut menu = super::super::context_menu::menu(180., "constraints-menu");
        for mode in [
            Constraint::Start,
            Constraint::End,
            Constraint::Center,
            Constraint::Stretch,
            Constraint::Scale,
        ] {
            let weak = cx.entity().downgrade();
            menu = menu.item(
                ContextMenuItem::action_with(
                    move |_, _| {
                        div()
                            .w_full()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().flex_1().child(t(constraint_label(mode, axis))))
                            .when(current == mode, |el| {
                                el.child(icon(LucideIcons::Check, 13.))
                            })
                    },
                    move |_, cx| {
                        let _ =
                            weak.update(cx, |this, cx| this.choose_constraint(id, axis, mode, cx));
                    },
                )
                .disabled(!self.layer_editable(id)),
            );
        }
        let _ = context_menu::show(menu, window.mouse_position(), window, cx);
    }

    pub(super) fn toggle_layout_wrap(&mut self, cx: &mut Context<Self>) {
        self.edit_layout(|layout| layout.wrap = !layout.wrap, cx);
    }
}

fn constraint_label(mode: Constraint, axis: usize) -> &'static str {
    match (mode, axis) {
        (Constraint::Start, 0) => "constraint-left",
        (Constraint::Start, _) => "constraint-top",
        (Constraint::End, 0) => "constraint-right",
        (Constraint::End, _) => "constraint-bottom",
        (Constraint::Stretch, 0) => "constraint-left-right",
        (Constraint::Stretch, _) => "constraint-top-bottom",
        (Constraint::Center, _) => "constraint-center",
        (Constraint::Scale, _) => "constraint-scale",
    }
}
