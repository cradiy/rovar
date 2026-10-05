use super::*;
use crate::{
    i18n::t,
    scene::boolean::{self, Operation},
    ui::theme::Color,
};
use uic::components::dropdown::{DropdownPlacement, dropdown};
#[cfg(test)]
mod tests;

impl Workspace {
    pub(super) fn boolean_geometry(&self, id: usize) -> Option<boolean::Geometry> {
        self.boolean_cache
            .borrow_mut()
            .get(&self.hierarchy, &self.shapes, id)
    }

    fn boolean_target(&self) -> Option<usize> {
        let ids = self.selection_ids();
        (ids.len() == 1)
            .then(|| *ids.first().unwrap())
            .filter(|id| self.hierarchy.groups.contains_key(id))
    }

    pub(super) fn selected_boolean(&self) -> Option<usize> {
        self.boolean_target()
            .filter(|id| boolean::is_boolean(&self.hierarchy, *id))
    }

    pub(super) fn boolean_property_bounds(&self, id: usize) -> Option<Rect> {
        let g = self.boolean_geometry(id)?;
        let mut rect = g.shape.rect;
        let origin = self.parent_origin(g.shape.board);
        rect.x += origin.x;
        rect.y += origin.y;
        if g.contours.is_empty() {
            rect.width = 0.;
            rect.height = 0.;
        }
        Some(rect)
    }

    pub(super) fn boolean_result_empty(&self) -> bool {
        self.selected_boolean().is_some_and(|id| {
            self.boolean_geometry(id)
                .is_none_or(|g| g.contours.is_empty())
        })
    }

    pub(super) fn can_boolean(&self) -> bool {
        if self.preview_read_only() {
            return false;
        }
        let mut ids = self.selection_ids();
        if let Some(id) = self.boolean_target() {
            if !self.layer_editable(id) || self.hierarchy.layouts.contains_key(&id) {
                return false;
            }
            if boolean::is_boolean(&self.hierarchy, id) {
                return true;
            }
            ids = self.ordered_children(Some(id)).into_iter().collect();
        }
        ids.len() >= 2
            && self.common_parent(&ids).is_some()
            && ids.iter().all(|id| {
                self.layer_editable(*id)
                    && (boolean::is_boolean(&self.hierarchy, *id)
                        || self
                            .shapes
                            .iter()
                            .find(|s| s.id == *id)
                            .is_some_and(boolean::supported))
            })
    }

    pub(super) fn apply_boolean(
        &mut self,
        operation: Operation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_boolean() {
            return;
        }
        self.suspend(window, cx);
        if let Some(id) = self.boolean_target() {
            if self.hierarchy.groups[&id].boolean == Some(operation) {
                return;
            }
            let before = self.snapshot_hierarchy();
            let group = self.hierarchy.groups.get_mut(&id).unwrap();
            if Operation::ALL.iter().any(|op| group.name == op.label()) {
                group.name = operation.label().to_owned();
            }
            group.boolean = Some(operation);
            self.history.borrow_mut().record(vec![before], None);
        } else {
            self.group_selection_with(Some(operation), cx);
        }
        self.sync_fields(cx);
        cx.notify();
    }

    pub(super) fn boolean_menu(
        &self,
        toolbar: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let selected = self
            .boolean_target()
            .and_then(|id| self.hierarchy.groups[&id].boolean);
        let menu = &self.toolbar.boolean_menus[usize::from(toolbar)];
        let selector = if toolbar {
            "boolean-toolbar-menu"
        } else {
            "boolean-menu"
        };
        dropdown(menu)
            .placement(if toolbar {
                DropdownPlacement::TopEnd
            } else {
                DropdownPlacement::BottomStart
            })
            .w(px(210.))
            .min_w(px(210.))
            .p(px(5.))
            .rounded(px(10.))
            .bg(Color::Input.color())
            .border_1()
            .border_color(BORDER.color())
            .shadow_lg()
            .text_size(px(12.))
            .text_color(TEXT.color())
            .trigger(
                div()
                    .id(selector)
                    .debug_selector(move || selector.into())
                    .h(px(32.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(TEXT.color())
                    .hover(|s| s.bg(BORDER.color()))
                    .child(icon(LucideIcons::Combine, 17.))
                    .child(selected.map_or(t("boolean-operations"), Operation::label))
                    .child(icon(LucideIcons::ChevronDown, 12.)),
            )
            .menu(
                div()
                    .debug_selector(|| "boolean-options".into())
                    .flex()
                    .flex_col()
                    .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .children(Operation::ALL.into_iter().map(|operation| {
                        let current = selected == Some(operation);
                        div()
                            .id(operation.key())
                            .debug_selector(move || format!("{}-option", operation.key()))
                            .h(px(28.))
                            .px(px(8.))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .when(current, |el| {
                                el.bg(Color::Selected.color()).text_color(ACCENT.color())
                            })
                            .hover(|s| s.bg(Color::Hover.color()))
                            .child(div().w(px(14.)).when(current, |el| {
                                el.child(
                                    icon(LucideIcons::Check, 13.)
                                        .debug_selector(|| "boolean-current-option".into()),
                                )
                            }))
                            .child(operation.label())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_boolean(operation, window, cx)
                            }))
                    })),
            )
    }

    pub(super) fn boolean_controls(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.can_boolean() {
            return None;
        }
        Some(
            inspector::inspector_section(t("boolean-operations"))
                .child(self.boolean_menu(false, cx))
                .when(self.selected_boolean().is_some(), |el| {
                    el.child(
                        div()
                            .id("boolean-release")
                            .debug_selector(|| "boolean-release".into())
                            .h(px(30.))
                            .px(px(8.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .cursor_pointer()
                            .text_size(px(12.))
                            .text_color(MUTED.color())
                            .hover(|s| s.bg(Color::Hover.color()).text_color(TEXT.color()))
                            .child(icon(LucideIcons::Ungroup, 16.))
                            .child(t("boolean-release"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.selected_boolean().is_some() && this.can_boolean() {
                                    this.suspend(window, cx);
                                    this.ungroup_selection(cx);
                                }
                            })),
                    )
                })
                .when(self.boolean_result_empty(), |el| {
                    el.child(
                        div()
                            .debug_selector(|| "boolean-empty-result".into())
                            .text_size(px(12.))
                            .text_color(MUTED.color())
                            .child(t("boolean-empty-result")),
                    )
                })
                .into_any_element(),
        )
    }
}
