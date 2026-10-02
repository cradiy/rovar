use super::*;
use crate::i18n::t;
use std::collections::BTreeSet;
mod measurement;
mod snapping;
mod spacing;
pub(super) use snapping::Snapping;
pub(super) use spacing::State as Spacing;

#[derive(Clone, Copy, Debug)]
pub(super) enum LayoutAction {
    Left,
    CenterX,
    Right,
    Top,
    CenterY,
    Bottom,
    DistributeX,
    DistributeY,
}
impl LayoutAction {
    pub(super) const ALL: [Self; 8] = [
        Self::Left,
        Self::CenterX,
        Self::Right,
        Self::Top,
        Self::CenterY,
        Self::Bottom,
        Self::DistributeX,
        Self::DistributeY,
    ];
    pub(super) fn details(self) -> (&'static str, &'static str, LucideIcons) {
        match self {
            Self::Left => (
                "align-left",
                t("align-left"),
                LucideIcons::AlignStartVertical,
            ),
            Self::CenterX => (
                "align-center-x",
                t("align-center-x"),
                LucideIcons::AlignCenterVertical,
            ),
            Self::Right => (
                "align-right",
                t("align-right"),
                LucideIcons::AlignEndVertical,
            ),
            Self::Top => (
                "align-top",
                t("align-top"),
                LucideIcons::AlignStartHorizontal,
            ),
            Self::CenterY => (
                "align-center-y",
                t("align-center-y"),
                LucideIcons::AlignCenterHorizontal,
            ),
            Self::Bottom => (
                "align-bottom",
                t("align-bottom"),
                LucideIcons::AlignEndHorizontal,
            ),
            Self::DistributeX => ("distribute-x", t("distribute-x"), LucideIcons::Columns3),
            Self::DistributeY => ("distribute-y", t("distribute-y"), LucideIcons::Rows3),
        }
    }
    fn horizontal(self) -> bool {
        matches!(
            self,
            Self::Left | Self::CenterX | Self::Right | Self::DistributeX
        )
    }
    fn distribute(self) -> bool {
        matches!(self, Self::DistributeX | Self::DistributeY)
    }
}

pub(super) fn union(a: Rect, b: Rect) -> Rect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    Rect {
        x,
        y,
        width: (a.x + a.width).max(b.x + b.width) - x,
        height: (a.y + a.height).max(b.y + b.height) - y,
    }
}

impl Workspace {
    fn alignment_frame(&self) -> Option<Rect> {
        let ids = self.selection_ids();
        if ids.len() > 1 {
            return ids
                .into_iter()
                .filter_map(|id| self.world_bounds(id))
                .reduce(union);
        }
        let id = *ids.first()?;
        let board = self
            .hierarchy
            .groups
            .get(&id)
            .and_then(|g| g.board)
            .or_else(|| self.object_rect(id).and_then(|(p, _)| p))?;
        self.boards.iter().find(|b| b.id == board).map(|b| b.rect)
    }
    pub(super) fn can_layout(&self, action: LayoutAction) -> bool {
        if action.distribute() {
            self.selection_ids().len() >= 3
        } else {
            self.alignment_frame().is_some()
        }
    }
    // Move each selected root as a unit. Frames already carry their children.
    fn translate_root(&mut self, id: usize, delta: Point<f32>) {
        if self.hierarchy.groups.contains_key(&id)
            && self.hierarchy.layouts.contains_key(&id)
            && let Some((parent, mut rect)) = self.object_rect(id)
        {
            rect.x += delta.x;
            rect.y += delta.y;
            self.set_object_rect(id, parent, rect);
            return;
        }
        let all = self.descendants(&BTreeSet::from([id]));
        let objects: Vec<_> = all
            .iter()
            .filter(|id| !self.hierarchy.groups.contains_key(id))
            .filter(|id| {
                !self
                    .ancestors(**id)
                    .iter()
                    .any(|p| all.contains(p) && self.boards.iter().any(|b| b.id == *p))
            })
            .filter_map(|id| {
                self.object_rect(*id)
                    .map(|(parent, rect)| (*id, parent, rect))
            })
            .collect();
        for (id, parent, mut rect) in objects {
            rect.x += delta.x;
            rect.y += delta.y;
            self.set_object_rect(id, parent, rect);
        }
    }
    pub(super) fn arrange_selection(&mut self, action: LayoutAction, cx: &mut Context<Self>) {
        if !self.can_layout(action) {
            return;
        }
        self.seal_text_edits(cx);
        let before = self.before_geometry();
        let mut rects: Vec<_> = self
            .selection_ids()
            .into_iter()
            .filter_map(|id| self.world_bounds(id).map(|r| (id, r)))
            .collect();
        let horizontal = action.horizontal();
        let start = |r: Rect| if horizontal { r.x } else { r.y };
        let length = |r: Rect| if horizontal { r.width } else { r.height };
        let frame = self.alignment_frame().unwrap();
        if action.distribute() {
            rects.sort_by(|a, b| start(a.1).total_cmp(&start(b.1)).then(a.0.cmp(&b.0)));
            let last = rects.last().unwrap().1;
            let first = rects[0].1;
            let gap = (start(last) + length(last)
                - start(first)
                - rects.iter().map(|(_, r)| length(*r)).sum::<f32>())
                / (rects.len() - 1) as f32;
            let mut at = start(first) + length(first) + gap;
            for &(id, rect) in rects.iter().skip(1).take(rects.len() - 2) {
                let delta = at - start(rect);
                self.translate_root(
                    id,
                    if horizontal {
                        point(delta, 0.)
                    } else {
                        point(0., delta)
                    },
                );
                at += length(rect) + gap;
            }
        } else {
            for (id, rect) in rects {
                let delta = match action {
                    LayoutAction::Left => frame.x - rect.x,
                    LayoutAction::CenterX => frame.x + (frame.width - rect.width) / 2. - rect.x,
                    LayoutAction::Right => frame.x + frame.width - rect.x - rect.width,
                    LayoutAction::Top => frame.y - rect.y,
                    LayoutAction::CenterY => frame.y + (frame.height - rect.height) / 2. - rect.y,
                    LayoutAction::Bottom => frame.y + frame.height - rect.y - rect.height,
                    _ => unreachable!(),
                };
                self.translate_root(
                    id,
                    if horizontal {
                        point(delta, 0.)
                    } else {
                        point(0., delta)
                    },
                );
            }
        }
        // Explicit alignment preserves parentage; only pointer drops reparent.
        if self.batch_changed(&before, cx) {
            self.history.borrow_mut().record(before, None);
        }
        self.sync_fields(cx);
        cx.notify();
    }
    pub(super) fn alignment_controls(&self, cx: &mut Context<Self>) -> Div {
        let mut row = div().flex().items_center().gap(px(2.)).flex_shrink_0();
        for action in LayoutAction::ALL {
            let (id, label, glyph) = action.details();
            let enabled = self.can_layout(action);
            row = row.child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .flex_1()
                    .min_w_0()
                    .h(px(28.))
                    .rounded(px(5.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .opacity(if enabled { 1. } else { 0.28 })
                    .when(enabled, |el| {
                        el.cursor_pointer().hover(|s| s.bg(gpui::rgba(0xb4a2ee22)))
                    })
                    .tooltip(move |_, cx| cx.new(|_| toolbar::ToolTip(label.into())).into())
                    .child(icon(glyph, 15.))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.focus.focus(window, cx);
                        this.arrange_selection(action, cx);
                    })),
            );
        }
        row
    }
}
#[cfg(test)]
pub(super) mod tests;
