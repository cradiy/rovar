use super::*;
use crate::{
    i18n::t,
    scene::auto_layout::{Align, Axis, Container, Mode, Sizing},
};
use gpui::AnyElement;
use uic::components::{
    context_menu::{self, ContextMenuItem},
    input::{Input, InputAppearance},
};
mod constraints;
#[cfg(test)]
mod tests;

pub(super) struct State {
    pub revision: Option<u64>,
    #[cfg(test)]
    pub reflows: usize,
    target: Option<(String, usize)>,
    inputs: Vec<Entity<TextInput>>,
    _subscriptions: Vec<Subscription>,
}
impl State {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let mut subscriptions = Vec::new();
        let inputs = (0..6)
            .map(|index| {
                let input = cx.new(TextInput::new);
                subscriptions.push(cx.subscribe_in(
                    &input,
                    window,
                    move |this, _, event: &InputEvent, _, cx| {
                        if matches!(event, InputEvent::Submit(_)) {
                            this.apply_layout_number(index, cx);
                        }
                    },
                ));
                subscriptions.push(cx.on_blur(
                    &input.focus_handle(cx),
                    window,
                    move |this, _, cx| this.apply_layout_number(index, cx),
                ));
                input
            })
            .collect();
        Self {
            revision: None,
            #[cfg(test)]
            reflows: 0,
            target: None,
            inputs,
            _subscriptions: subscriptions,
        }
    }
}

impl Workspace {
    pub(super) fn fix_layout_size(
        &mut self,
        id: usize,
        before: Rect,
        after: Rect,
    ) -> Option<Change> {
        let sizing = self.hierarchy.sizing.get(&id)?;
        let width = before.width != after.width && sizing.width != Mode::Fixed;
        let height = before.height != after.height && sizing.height != Mode::Fixed;
        let constraints = before != after && sizing.constraints.is_some();
        if !width && !height && !constraints {
            return None;
        }
        let change = self.snapshot_hierarchy();
        let sizing = self.hierarchy.sizing.get_mut(&id).unwrap();
        if width {
            sizing.width = Mode::Fixed;
        }
        if height {
            sizing.height = Mode::Fixed;
        }
        self.refresh_constraints(id);
        Some(change)
    }
    pub(super) fn is_layout_flow_item(&self, id: usize) -> bool {
        self.is_auto_layout_child(id) && !self.hierarchy.sizing.get(&id).is_some_and(|s| s.absolute)
    }
    pub(super) fn is_auto_layout_child(&self, id: usize) -> bool {
        self.layer_parent(id)
            .is_some_and(|p| self.hierarchy.layouts.contains_key(&p))
    }
    pub(super) fn reorder_layout_item(&mut self, id: usize) -> Option<Change> {
        if !self.is_layout_flow_item(id) {
            return None;
        }
        let parent = self.layer_parent(id).unwrap();
        let axis = self.hierarchy.layouts[&parent].axis;
        let mut order = self.ordered_children(Some(parent));
        let mut flow: Vec<_> = order
            .iter()
            .copied()
            .filter(|id| self.is_layout_flow_item(*id))
            .collect();
        if self.hierarchy.layouts[&parent].wrap {
            let moved = self.world_rect(id)?;
            flow.retain(|item| *item != id);
            let spans = |r: Rect| {
                if axis == Axis::Horizontal {
                    (r.x + r.width / 2., r.y, r.y + r.height)
                } else {
                    (r.y + r.height / 2., r.x, r.x + r.width)
                }
            };
            let (main, start, end) = spans(moved);
            let cross = (start + end) / 2.;
            let mut lines: Vec<(f32, f32, usize, usize)> = Vec::new();
            for (index, item) in flow.iter().enumerate() {
                let (_, start, end) = spans(self.world_rect(*item)?);
                if let Some(line) = lines
                    .last_mut()
                    .filter(|line| start < line.1 && end > line.0)
                {
                    line.0 = line.0.min(start);
                    line.1 = line.1.max(end);
                    line.3 = index + 1;
                } else {
                    lines.push((start, end, index, index + 1));
                }
            }
            let at = lines
                .iter()
                .min_by(|a, b| {
                    let distance = |line: &(f32, f32, usize, usize)| {
                        (cross - cross.clamp(line.0, line.1)).abs()
                    };
                    distance(a).total_cmp(&distance(b))
                })
                .map_or(0, |line| {
                    (line.2..line.3)
                        .find(|index| {
                            self.world_rect(flow[*index])
                                .is_some_and(|r| spans(r).0 > main)
                        })
                        .unwrap_or(line.3)
                });
            flow.insert(at, id);
        } else {
            flow.sort_by(|a, b| {
                let key = |id| {
                    self.world_rect(id).map_or(0., |r| {
                        if axis == Axis::Horizontal {
                            r.x + r.width / 2.
                        } else {
                            r.y + r.height / 2.
                        }
                    })
                };
                key(*a).total_cmp(&key(*b))
            });
        }
        let mut next = flow.into_iter();
        for item in &mut order {
            if self.is_layout_flow_item(*item) {
                *item = next.next().unwrap();
            }
        }
        if order == self.ordered_children(Some(parent)) {
            return None;
        }
        let before = self.snapshot_hierarchy();
        self.set_sibling_order(&order);
        Some(before)
    }
    pub(super) fn finish_layout_item_move(&mut self, id: usize) -> Option<Change> {
        if !self.is_auto_layout_child(id) {
            return None;
        }
        let parent = self.layer_parent(id)?;
        let bounds = self.world_rect(parent)?;
        let rect = self.world_rect(id)?;
        let center = point(rect.x + rect.width / 2., rect.y + rect.height / 2.);
        if center.x >= bounds.x
            && center.x <= bounds.x + bounds.width
            && center.y >= bounds.y
            && center.y <= bounds.y + bounds.height
        {
            return self.reorder_layout_item(id);
        }
        let before = self.snapshot_hierarchy();
        self.hierarchy.parents.remove(&id);
        if self.hierarchy.groups.contains_key(&id) {
            self.reparent_group(id);
        } else if !self.boards.iter().any(|b| b.id == id) {
            let board = self.board_at(center);
            let origin = self.parent_origin(board);
            self.set_object_rect(
                id,
                board,
                Rect {
                    x: rect.x - origin.x,
                    y: rect.y - origin.y,
                    ..rect
                },
            );
        }
        // Preserve the dropped size. Inside another auto-layout board this
        // becomes a free-positioned child instead of snapping into its flow.
        let absolute = self
            .layer_parent(id)
            .is_some_and(|p| self.hierarchy.layouts.contains_key(&p));
        let sizing = self.hierarchy.sizing.entry(id).or_default();
        if sizing.width == Mode::Fill {
            sizing.width = Mode::Fixed;
        }
        if sizing.height == Mode::Fill {
            sizing.height = Mode::Fixed;
        }
        sizing.absolute = absolute;
        if *sizing == Sizing::default() {
            self.hierarchy.sizing.remove(&id);
        }
        if self.selected_text == Some(id) || self.selected_shape == Some(id) {
            self.selected = self.object_rect(id).and_then(|(board, _)| board);
        }
        Some(before)
    }
    fn layout_target(&self) -> Option<usize> {
        let ids = self.selection_ids();
        (ids.len() == 1).then(|| *ids.first().unwrap())
    }
    pub(super) fn reflow_layout(&mut self, cx: &mut Context<Self>) {
        let revision = self.history.borrow().revision();
        let resizing_frame = self.gesture.is_some_and(|g| {
            matches!(g.kind,
            GestureKind::Resize { id, .. } if self.hierarchy.sizing.get(&id).is_none_or(|s|
                s.constraints.is_none() && s.width == Mode::Fixed && s.height == Mode::Fixed))
        });
        if self.gesture.is_some_and(|g| {
            !resizing_frame && !matches!(g.kind, GestureKind::LayoutProperty { .. })
        }) || (!resizing_frame && self.auto_layout.revision == Some(revision))
        {
            return;
        }
        self.auto_layout.revision = (!resizing_frame).then_some(revision);
        if self.hierarchy.layouts.is_empty() && self.hierarchy.sizing.is_empty() {
            return;
        }
        #[cfg(test)]
        {
            self.auto_layout.reflows += 1;
        }
        let selected = self.layout_target();
        let before = selected.and_then(|id| self.world_rect(id));
        let (page, _) = self.snapshot_page(cx);
        self.boards = page.boards;
        self.shapes = page.shapes;
        for text in page.texts {
            if let Some(item) = self.texts.iter_mut().find(|t| t.id == text.id) {
                item.rect = text.rect;
            }
        }
        self.hierarchy = page.hierarchy;
        self.refresh_layout_inputs(cx);
        if selected.and_then(|id| self.world_rect(id)) != before {
            for index in 1..=4 {
                if !self.inspector.invalid[index] {
                    self.sync_field(index, cx);
                }
            }
        }
    }
    pub(super) fn sync_layout_inputs(&mut self, cx: &mut Context<Self>) {
        let target = self.layout_input_target();
        if self.auto_layout.target == target {
            return;
        }
        self.auto_layout.target = target;
        self.refresh_layout_inputs(cx);
    }
    fn layout_input_target(&self) -> Option<(String, usize)> {
        self.layout_target()
            .map(|id| (self.pages.active.clone(), id))
    }
    fn refresh_layout_inputs(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.layout_target() else {
            return;
        };
        let Some(layout) = self.hierarchy.layouts.get(&id) else {
            return;
        };
        let values = [
            layout.gap,
            layout.padding[0],
            layout.padding[1],
            layout.padding[2],
            layout.padding[3],
            layout.line_gap,
        ];
        for (input, value) in self.auto_layout.inputs.iter().zip(values) {
            input.update(cx, |input, cx| {
                input.set_value(inspector::number(value), cx)
            });
        }
    }
    pub(super) fn enable_auto_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.suspend(window, cx);
        if !self.can_auto_layout() {
            return;
        }
        let before = self.page_edit(std::slice::from_ref(&self.pages.active), cx);
        if self.can_group() {
            let suppressed = self.history.borrow().suppressed;
            self.history.borrow_mut().suppressed = true;
            self.group_selection(cx);
            self.history.borrow_mut().suppressed = suppressed;
        }
        let Some(id) = self.layout_target() else {
            return;
        };
        if !self.layer_editable(id) {
            return;
        }
        let group = self.hierarchy.groups.get(&id);
        if group.is_none() && !self.boards.iter().any(|b| b.id == id) {
            return;
        }
        if self.hierarchy.layouts.remove(&id).is_none() {
            let mut rect = self.world_rect(id).unwrap();
            let origin = self.parent_origin(group.and_then(|g| g.board));
            rect.x -= origin.x;
            rect.y -= origin.y;
            rect.width = rect.width.max(1.);
            rect.height = rect.height.max(1.);
            self.hierarchy.layouts.insert(id, Container::new(rect));
            if group.is_some() {
                let width = if self.sizing_enabled(id, 0, Mode::Hug) {
                    Mode::Hug
                } else {
                    Mode::Fixed
                };
                let height = if self.sizing_enabled(id, 1, Mode::Hug) {
                    Mode::Hug
                } else {
                    Mode::Fixed
                };
                self.hierarchy.sizing.insert(
                    id,
                    Sizing {
                        width,
                        height,
                        ..self.hierarchy.sizing.get(&id).copied().unwrap_or_default()
                    },
                );
            }
        } else {
            if let Some(sizing) = self.hierarchy.sizing.get_mut(&id) {
                sizing.width = Mode::Fixed;
                sizing.height = Mode::Fixed;
                if *sizing == Sizing::default() {
                    self.hierarchy.sizing.remove(&id);
                }
            }
        }
        for child in self.ordered_children(Some(id)) {
            self.refresh_constraints(child);
        }
        self.record_page_edit(before);
        self.auto_layout.revision = None;
        self.reflow_layout(cx);
        self.refresh_layout_inputs(cx);
        cx.notify();
    }
    pub(super) fn can_auto_layout(&self) -> bool {
        let ids = self.selection_ids();
        !ids.is_empty()
            && self.common_parent(&ids).is_some()
            && ids.iter().all(|id| self.layer_editable(*id))
            && (self.can_group()
                || self.layout_target().is_some_and(|id| {
                    self.hierarchy.groups.contains_key(&id)
                        || self.boards.iter().any(|b| b.id == id)
                }))
    }
    pub(super) fn has_auto_layout(&self) -> bool {
        self.layout_target()
            .is_some_and(|id| self.hierarchy.layouts.contains_key(&id))
    }
    fn edit_layout(&mut self, edit: impl FnOnce(&mut Container), cx: &mut Context<Self>) {
        let Some(id) = self.layout_target().filter(|id| self.layer_editable(*id)) else {
            return;
        };
        let before = self.snapshot_hierarchy();
        if let Some(layout) = self.hierarchy.layouts.get_mut(&id) {
            let previous = layout.clone();
            edit(layout);
            if *layout != previous {
                if layout.wrap {
                    let sizing = self.hierarchy.sizing.entry(id).or_default();
                    let main = if layout.axis == Axis::Horizontal {
                        &mut sizing.width
                    } else {
                        &mut sizing.height
                    };
                    if *main == Mode::Hug {
                        *main = Mode::Fixed;
                    }
                }
                self.history.borrow_mut().record(vec![before], None);
            }
        }
        self.auto_layout.revision = None;
        self.reflow_layout(cx);
        self.refresh_layout_inputs(cx);
        cx.notify();
    }
    fn apply_layout_number(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.auto_layout.target != self.layout_input_target() {
            return;
        }
        let value = self.auto_layout.inputs[index]
            .read(cx)
            .value()
            .parse::<f32>();
        if let Ok(value) = value
            && value.is_finite()
            && (0. ..=crate::scene::artboard::MAX_SIZE).contains(&value)
        {
            self.edit_layout(
                |layout| match index {
                    0 => layout.gap = value,
                    5 => layout.line_gap = value,
                    _ => layout.padding[index - 1] = value,
                },
                cx,
            );
        }
        self.refresh_layout_inputs(cx);
    }
    fn choose_sizing(&mut self, id: usize, axis: usize, mode: Mode, cx: &mut Context<Self>) {
        if !self.layer_editable(id) || !self.sizing_enabled(id, axis, mode) {
            return;
        }
        let sizing = self.hierarchy.sizing.get(&id).copied().unwrap_or_default();
        if (if axis == 0 {
            sizing.width
        } else {
            sizing.height
        }) == mode
        {
            return;
        }
        let before = self.snapshot_hierarchy();
        let sizing = self.hierarchy.sizing.entry(id).or_default();
        if axis == 0 {
            sizing.width = mode
        } else {
            sizing.height = mode
        }
        self.history.borrow_mut().record(vec![before], None);
        self.auto_layout.revision = None;
        self.reflow_layout(cx);
        self.refresh_layout_inputs(cx);
        cx.notify();
    }
    fn toggle_layout_absolute(&mut self, id: usize, cx: &mut Context<Self>) {
        if !self.layer_editable(id) {
            return;
        }
        let before = self.snapshot_hierarchy();
        let parent = self
            .layer_parent(id)
            .and_then(|parent| self.hierarchy.sizing.get(&parent))
            .copied()
            .unwrap_or_default();
        let sizing = self.hierarchy.sizing.entry(id).or_default();
        sizing.absolute = !sizing.absolute;
        sizing.constraints = None;
        if !sizing.absolute {
            if sizing.width == Mode::Fill && parent.width == Mode::Hug {
                sizing.width = Mode::Fixed;
            }
            if sizing.height == Mode::Fill && parent.height == Mode::Hug {
                sizing.height = Mode::Fixed;
            }
        }
        self.history.borrow_mut().record(vec![before], None);
        cx.notify();
    }
    fn sizing_enabled(&self, id: usize, axis: usize, mode: Mode) -> bool {
        if mode != Mode::Fixed
            && self
                .hierarchy
                .sizing
                .get(&id)
                .and_then(|s| s.constraints)
                .is_some_and(|c| {
                    matches!(
                        if axis == 0 { c.horizontal } else { c.vertical },
                        crate::scene::auto_layout::Constraint::Stretch
                            | crate::scene::auto_layout::Constraint::Scale
                    )
                })
        {
            return false;
        }
        let size_mode = |id| {
            let sizing = self.hierarchy.sizing.get(&id).copied().unwrap_or_default();
            if axis == 0 {
                sizing.width
            } else {
                sizing.height
            }
        };
        match mode {
            Mode::Fixed => true,
            Mode::Hug => {
                if self
                    .hierarchy
                    .layouts
                    .get(&id)
                    .is_some_and(|l| l.wrap && (l.axis == Axis::Horizontal) == (axis == 0))
                {
                    return false;
                }
                (self.hierarchy.layouts.contains_key(&id) || self.texts.iter().any(|t| t.id == id))
                    && !self.ordered_children(Some(id)).iter().any(|child| {
                        self.is_layout_flow_item(*child) && size_mode(*child) == Mode::Fill
                    })
            }
            Mode::Fill => {
                self.is_layout_flow_item(id)
                    && self
                        .layer_parent(id)
                        .is_some_and(|p| size_mode(p) != Mode::Hug)
            }
        }
    }
    fn sizing_menu(&mut self, id: usize, axis: usize, window: &mut Window, cx: &mut Context<Self>) {
        let mut menu = super::context_menu::menu(170., "sizing-menu");
        let hug = self.sizing_enabled(id, axis, Mode::Hug);
        let fill = self.sizing_enabled(id, axis, Mode::Fill);
        let sizing = self.hierarchy.sizing.get(&id).copied().unwrap_or_default();
        let current = if axis == 0 {
            sizing.width
        } else {
            sizing.height
        };
        for (mode, label, enabled) in [
            (Mode::Fixed, "layout-fixed", true),
            (Mode::Hug, "layout-hug", hug),
            (Mode::Fill, "layout-fill", fill),
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
                            .child(div().flex_1().child(t(label)))
                            .when(current == mode, |el| {
                                el.child(icon(LucideIcons::Check, 13.))
                            })
                    },
                    move |_, cx| {
                        let _ = weak.update(cx, |this, cx| this.choose_sizing(id, axis, mode, cx));
                    },
                )
                .disabled(!enabled),
            );
        }
        let _ = context_menu::show(menu, window.mouse_position(), window, cx);
    }
    pub(super) fn sizing_control(&self, axis: usize, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.layout_target()?;
        if !self.hierarchy.layouts.contains_key(&id)
            && !self
                .layer_parent(id)
                .is_some_and(|p| self.hierarchy.layouts.contains_key(&p))
            && !self.texts.iter().any(|t| t.id == id)
        {
            return None;
        }
        let sizing = self.hierarchy.sizing.get(&id).copied().unwrap_or_default();
        let mode = if axis == 0 {
            sizing.width
        } else {
            sizing.height
        };
        let label = t(match mode {
            Mode::Fixed => "layout-fixed",
            Mode::Hug => "layout-hug",
            Mode::Fill => "layout-fill",
        });
        Some(
            div()
                .id(("layout-sizing", axis))
                .debug_selector(move || format!("layout-sizing-{axis}"))
                .w(px(24.))
                .h_full()
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(5.))
                .cursor_pointer()
                .text_color(rgb(if mode == Mode::Fixed { MUTED } else { ACCENT }))
                .hover(|s| s.bg(gpui::rgba(0xb4a2ee18)).text_color(rgb(TEXT)))
                .tooltip(move |_, cx| cx.new(|_| toolbar::ToolTip(label.into())).into())
                .child(icon(LucideIcons::ChevronDown, 12.))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.sizing_menu(id, axis, window, cx);
                    cx.stop_propagation();
                }))
                .into_any_element(),
        )
    }
    pub(super) fn layout_position_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.layout_target()?;
        if !self
            .layer_parent(id)
            .is_some_and(|p| self.hierarchy.layouts.contains_key(&p))
        {
            return None;
        }
        let absolute = self.hierarchy.sizing.get(&id).is_some_and(|s| s.absolute);
        Some(
            div()
                .id("layout-absolute")
                .h(px(24.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(11.))
                .cursor_pointer()
                .text_color(rgb(if absolute { ACCENT } else { MUTED }))
                .hover(|s| s.text_color(rgb(TEXT)))
                .child(icon(
                    if absolute {
                        LucideIcons::Check
                    } else {
                        LucideIcons::Move
                    },
                    13.,
                ))
                .child(t("layout-absolute"))
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_layout_absolute(id, cx)))
                .into_any_element(),
        )
    }
    pub(super) fn auto_layout_controls(&self, cx: &mut Context<Self>) -> Div {
        let target = self.layout_target();
        let layout = target.and_then(|id| self.hierarchy.layouts.get(&id));
        if !self.can_auto_layout() {
            return div();
        }
        let toggle = div()
            .id("toggle-auto-layout")
            .debug_selector(|| "toggle-auto-layout".into())
            .h(px(24.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(11.))
            .text_color(rgb(MUTED))
            .cursor_pointer()
            .hover(|s| s.text_color(rgb(ACCENT)))
            .child(icon(
                if layout.is_some() {
                    LucideIcons::Minus
                } else {
                    LucideIcons::Plus
                },
                13.,
            ))
            .child(t(if layout.is_some() {
                "layout-disable"
            } else {
                "layout-enable"
            }))
            .on_click(cx.listener(|this, _, window, cx| this.enable_auto_layout(window, cx)));
        let mut section = if layout.is_some() {
            inspector::inspector_section(t("auto-layout")).child(toggle)
        } else {
            div().flex_shrink_0().px(px(14.)).py(px(8.)).child(toggle)
        }
        .text_size(px(12.))
        .font_weight(FontWeight::NORMAL)
        .line_height(px(16.));
        if let Some(layout) = layout {
            section = section.child(div().flex().gap(px(6.)).children(
                [Axis::Horizontal, Axis::Vertical].into_iter().map(|axis| {
                    let active = axis == layout.axis;
                    div()
                        .id(if axis == Axis::Horizontal {
                            "layout-horizontal"
                        } else {
                            "layout-vertical"
                        })
                        .h(px(28.))
                        .flex_1()
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .gap(px(6.))
                        .cursor_pointer()
                        .bg(rgb(if active { 0x383044 } else { 0x282b33 }))
                        .text_color(rgb(if active { ACCENT } else { TEXT }))
                        .child(icon(
                            if axis == Axis::Horizontal {
                                LucideIcons::ArrowRight
                            } else {
                                LucideIcons::ArrowDown
                            },
                            14.,
                        ))
                        .child(t(if axis == Axis::Horizontal {
                            "layout-horizontal"
                        } else {
                            "layout-vertical"
                        }))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.edit_layout(|l| l.axis = axis, cx)
                        }))
                }),
            ));
            section = section.child(
                div()
                    .id("layout-wrap")
                    .debug_selector(|| "layout-wrap".into())
                    .h(px(28.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .cursor_pointer()
                    .bg(rgb(if layout.wrap { 0x383044 } else { 0x282b33 }))
                    .text_color(rgb(if layout.wrap { ACCENT } else { TEXT }))
                    .child(t("layout-wrap"))
                    .child(icon(
                        if layout.wrap {
                            LucideIcons::Check
                        } else {
                            LucideIcons::Plus
                        },
                        14.,
                    ))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_layout_wrap(cx))),
            );
            for cross in [false, true] {
                section = section.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(div().flex_1().text_color(rgb(MUTED)).child(t(if cross {
                            "layout-cross"
                        } else {
                            "layout-main"
                        })))
                        .children(
                            [Align::Start, Align::Center, Align::End]
                                .into_iter()
                                .enumerate()
                                .map(|(index, align)| {
                                    let active =
                                        if cross { layout.cross } else { layout.main } == align;
                                    div()
                                        .id((
                                            if cross { "layout-cross" } else { "layout-main" },
                                            index,
                                        ))
                                        .size(px(26.))
                                        .rounded(px(5.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .bg(rgb(if active { 0x383044 } else { 0x282b33 }))
                                        .text_color(rgb(if active { ACCENT } else { MUTED }))
                                        .child(icon(
                                            if (layout.axis == Axis::Horizontal) != cross {
                                                [
                                                    LucideIcons::AlignStartVertical,
                                                    LucideIcons::AlignCenterVertical,
                                                    LucideIcons::AlignEndVertical,
                                                ][index]
                                            } else {
                                                [
                                                    LucideIcons::AlignStartHorizontal,
                                                    LucideIcons::AlignCenterHorizontal,
                                                    LucideIcons::AlignEndHorizontal,
                                                ][index]
                                            },
                                            14.,
                                        ))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.edit_layout(
                                                |l| {
                                                    if cross {
                                                        l.cross = align
                                                    } else {
                                                        l.main = align
                                                    }
                                                },
                                                cx,
                                            )
                                        }))
                                }),
                        ),
                );
            }
            section = section.child(
                div().flex().flex_col().gap(px(6.)).children(
                    [
                        (0, "layout-gap"),
                        (5, "layout-line-gap"),
                        (1, "layout-padding-top"),
                        (2, "layout-padding-right"),
                        (3, "layout-padding-bottom"),
                        (4, "layout-padding-left"),
                    ]
                    .into_iter()
                    .filter(|(index, _)| *index != 5 || layout.wrap)
                    .map(|(index, label)| self.layout_number(index, t(label), cx)),
                ),
            );
        }
        section
    }
    pub(super) fn layout_number_value(&self, index: usize) -> Option<f32> {
        let layout = self.hierarchy.layouts.get(&self.layout_target()?)?;
        Some(match index {
            0 => layout.gap,
            5 => layout.line_gap,
            _ => layout.padding[index - 1],
        })
    }
    fn begin_layout_scrub(
        &mut self,
        index: usize,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_some()
            || !self
                .layout_target()
                .is_some_and(|id| self.layer_editable(id))
        {
            return;
        }
        if let Some(focused) = self
            .auto_layout
            .inputs
            .iter()
            .position(|input| input.focus_handle(cx).is_focused(window))
        {
            self.apply_layout_number(focused, cx);
        }
        let Some(original) = self.layout_number_value(index) else {
            return;
        };
        self.begin(
            GestureKind::LayoutProperty { index, original },
            event.position,
            event.button,
            window,
            cx,
        );
        self.history.borrow_mut().begin_preview();
    }
    pub(super) fn scrub_layout_number(
        &mut self,
        index: usize,
        original: f32,
        delta: f32,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        if delta.abs() < 3. && !self.history.borrow().can_merge(None) {
            return;
        }
        let value = (original + (delta * if shift { 10. } else { 1. }).round())
            .clamp(0., crate::scene::artboard::MAX_SIZE);
        if self.layout_number_value(index) == Some(value) {
            return;
        }
        self.edit_layout(
            |layout| match index {
                0 => layout.gap = value,
                5 => layout.line_gap = value,
                _ => layout.padding[index - 1] = value,
            },
            cx,
        );
    }
    pub(super) fn finish_layout_scrub(&mut self, commit: bool, cx: &mut Context<Self>) {
        let changes = self.history.borrow_mut().end_preview(commit);
        self.restore_batch(&changes, cx);
        self.auto_layout.revision = None;
        self.reflow_layout(cx);
        cx.notify();
    }
    fn layout_number(&self, index: usize, label: &'static str, cx: &mut Context<Self>) -> Div {
        let editable = self
            .layout_target()
            .is_some_and(|id| self.layer_editable(id));
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(
                div()
                    .id(("layout-number-label", index))
                    .debug_selector(move || format!("layout-drag-{index}"))
                    .flex_1()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .text_color(rgb(MUTED))
                    .when(editable, |el| {
                        el.cursor(gpui::CursorStyle::ResizeLeftRight)
                            .hover(|s| s.text_color(rgb(TEXT)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event, window, cx| {
                                    this.begin_layout_scrub(index, event, window, cx)
                                }),
                            )
                    })
                    .child(label),
            )
            .child(
                Input::new(&self.auto_layout.inputs[index])
                    .w(px(70.))
                    .h(px(26.))
                    .px(px(6.))
                    .py_0()
                    .rounded(px(5.))
                    .text_size(px(12.))
                    .text_color(rgb(TEXT))
                    .bg(rgb(0x282b33))
                    .border_color(rgb(BORDER))
                    .appearance(InputAppearance {
                        focus_border: rgb(ACCENT).into(),
                        caret: rgb(ACCENT).into(),
                        selection: gpui::rgba(0xb4a2ee44).into(),
                        ..Default::default()
                    }),
            )
    }
}
