use super::*;
use crate::{
    i18n::t,
    scene::{blend::Mode, layer::LayerState},
    ui::theme::Color,
};
use uic::components::dropdown::{DropdownPlacement, dropdown};
pub(super) mod paint;
#[cfg(test)]
mod tests;

impl Workspace {
    pub(super) fn set_blend(&mut self, blend: Mode, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_read_only() {
            return;
        }
        self.suspend(window, cx);
        let mut changes = Vec::new();
        for id in self.selection_ids() {
            if !self.layer_editable(id) {
                continue;
            }
            if let Some(layer) = self.layer_state_mut(id)
                && layer.blend != blend
            {
                changes.push(Change::Layer { id, value: *layer });
                layer.blend = blend;
            }
        }
        self.history.borrow_mut().record(changes, None);
        cx.notify();
    }

    pub(super) fn edit_layer_opacity(&mut self, value: &str) -> bool {
        let Ok(value) = value.trim().parse::<f32>() else {
            return false;
        };
        if !value.is_finite() || !(0. ..=100.).contains(&value) || self.preview_read_only() {
            return false;
        }
        let ids = self.selection_ids();
        let mut changes = Vec::new();
        for id in &ids {
            if !self.layer_editable(*id) {
                continue;
            }
            if let Some(layer) = self.layer_state_mut(*id)
                && layer.opacity != value / 100.
            {
                changes.push(Change::Layer {
                    id: *id,
                    value: *layer,
                });
                layer.opacity = value / 100.;
            }
        }
        self.history.borrow_mut().record(
            changes,
            Some(Group::SelectionProperty(
                ids.into_iter().collect(),
                Property::LayerOpacity,
            )),
        );
        true
    }

    pub(super) fn appearance_controls(&self, cx: &mut Context<Self>) -> Div {
        let mut modes = self
            .selection_ids()
            .into_iter()
            .filter_map(|id| self.layer_info(id).map(|(l, _)| l.blend));
        let selected = modes
            .next()
            .filter(|first| modes.all(|mode| mode == *first));
        inspector::inspector_section(t("appearance"))
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        div().flex_1().min_w_0().child(
                            dropdown(&self.inspector.blend_menu)
                                .placement(DropdownPlacement::BottomStart)
                                .w(px(190.))
                                .min_w(px(190.))
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
                                        .id("blend-menu")
                                        .debug_selector(|| "blend-menu".into())
                                        .h(px(32.))
                                        .w_full()
                                        .min_w_0()
                                        .px(px(8.))
                                        .rounded(px(6.))
                                        .border_1()
                                        .border_color(BORDER.color())
                                        .bg(Color::Input.color())
                                        .flex()
                                        .items_center()
                                        .gap(px(6.))
                                        .cursor_pointer()
                                        .text_size(px(12.))
                                        .text_color(TEXT.color())
                                        .hover(|s| s.bg(Color::Hover.color()))
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .truncate()
                                                .child(selected.map_or(t("mixed"), Mode::label)),
                                        )
                                        .child(icon(LucideIcons::ChevronDown, 12.).flex_shrink_0()),
                                )
                                .menu(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                                        .children(Mode::ALL.map(|mode| {
                                            let current = selected == Some(mode);
                                            div()
                                                .id(mode.key())
                                                .debug_selector(move || mode.key().into())
                                                .h(px(28.))
                                                .px(px(8.))
                                                .rounded(px(5.))
                                                .flex()
                                                .items_center()
                                                .gap(px(8.))
                                                .cursor_pointer()
                                                .when(current, |el| {
                                                    el.bg(Color::Selected.color())
                                                        .text_color(ACCENT.color())
                                                })
                                                .hover(|s| s.bg(Color::Hover.color()))
                                                .child(div().w(px(14.)).when(current, |el| {
                                                    el.child(icon(LucideIcons::Check, 13.))
                                                }))
                                                .child(mode.label())
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.set_blend(mode, window, cx)
                                                    },
                                                ))
                                        })),
                                ),
                        ),
                    )
                    .child(div().w(px(78.)).flex_shrink_0().child(self.property_field(
                        18,
                        t("opacity"),
                        cx,
                    ))),
            )
            .when(
                self.selected_shape()
                    .is_some_and(|s| s.kind.supports_corners()),
                |el| el.child(self.shape_corner_controls(cx)),
            )
    }

    pub(super) fn blend_state(&self, id: usize) -> LayerState {
        self.layer_info(id)
            .map_or_else(LayerState::default, |(layer, _)| layer)
    }
}
