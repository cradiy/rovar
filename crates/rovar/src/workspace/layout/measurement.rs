use super::*;
use gpui::{TextRun, canvas, fill, outline, size};

const GUIDE_COLOR: u32 = 0xf28bd9;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Dimension {
    pub axis: usize,
    pub start: f32,
    pub end: f32,
    pub cross: f32,
}

pub(super) fn interval(rect: Rect, axis: usize) -> (f32, f32) {
    if axis == 0 {
        (rect.x, rect.x + rect.width)
    } else {
        (rect.y, rect.y + rect.height)
    }
}

fn distances(source: Rect, target: Rect) -> Vec<Dimension> {
    let mut result = Vec::new();
    for axis in 0..2 {
        let (a, b) = interval(source, axis);
        let (c, d) = interval(target, axis);
        let (lo, hi) = interval(source, 1 - axis);
        let (other_lo, other_hi) = interval(target, 1 - axis);
        let cross = if lo.max(other_lo) <= hi.min(other_hi) {
            (lo.max(other_lo) + hi.min(other_hi)) / 2.
        } else {
            (lo + hi) / 2.
        };
        let spans = if b <= c {
            vec![(b, c)]
        } else if d <= a {
            vec![(d, a)]
        } else if (a >= c && b <= d) || (c >= a && d <= b) {
            vec![(a.min(c), a.max(c)), (b.min(d), b.max(d))]
        } else if (a - c).abs() <= (b - d).abs() {
            vec![(a.min(c), a.max(c))]
        } else {
            vec![(b.min(d), b.max(d))]
        };
        for (start, end) in spans {
            if end - start > 0.001 {
                result.push(Dimension {
                    axis,
                    start,
                    end,
                    cross,
                });
            }
        }
    }
    result
}

impl Workspace {
    pub(in crate::workspace) fn update_measurement(
        &mut self,
        position: Point<Pixels>,
        alt: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let target = (|| {
            if !alt
                || !self.focus.is_focused(window)
                || self.gesture.is_some()
                || self.space_down
                || self.toolbar.hand
                || self.draw_tool.is_some()
                || self.vector_edit.is_some()
                || self.colors.dialog.is_some()
                || self.assets.dialog.is_some()
                || uic::components::context_menu::is_open(cx)
            {
                return None;
            }
            let bounds = self.bounds.get();
            if !bounds.contains(&position) {
                return None;
            }
            let local = (position - bounds.origin).map(f32::from);
            let (left, right) = self.canvas_insets();
            if local.x < left
                || local.x > f32::from(bounds.size.width) - right
                || local.y < 56.
                || local.y > f32::from(bounds.size.height) - 76.
            {
                return None;
            }
            let selected = self.selection_ids();
            if selected.is_empty() {
                return None;
            }
            let excluded = self.descendants(&selected);
            let p = self.view.world(local);
            self.canvas_layer_order().into_iter().rev().find(|id| {
                !excluded.contains(id)
                    && self.world_rect(*id).is_some_and(|rect| {
                        let p = crate::rotation::around(
                            p,
                            crate::rotation::center(rect),
                            -self.object_rotation(*id),
                        );
                        p.x >= rect.x
                            && p.x <= rect.x + rect.width
                            && p.y >= rect.y
                            && p.y <= rect.y + rect.height
                    })
            })
        })();
        if self.measure_target != target {
            self.measure_target = target;
            cx.notify();
        }
    }

    pub(in crate::workspace) fn measurement_overlay(&self) -> impl IntoElement + use<> {
        let pair = self.measure_target.and_then(|id| {
            let ids = self.selection_ids();
            if self.gesture.is_some()
                || self.space_down
                || ids.contains(&id)
                || self
                    .layer_info(id)
                    .is_none_or(|(own, parent)| self.effective_layer(own, parent).hidden)
            {
                return None;
            }
            let source = ids
                .into_iter()
                .filter_map(|id| self.world_bounds(id))
                .reduce(union)?;
            Some((source, self.world_bounds(id)?))
        });
        dimension_overlay(
            self.view,
            pair.map_or_else(Vec::new, |(source, target)| distances(source, target)),
            pair.map(|(_, target)| target),
        )
    }
}

/// Paint-only guides keep a fixed screen-space weight and never intercept input.
pub(super) fn dimension_overlay(
    view: Viewport,
    dimensions: Vec<Dimension>,
    target: Option<Rect>,
) -> impl IntoElement + use<> {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, cx| {
            let screen = |p| bounds.origin + view.screen(p).map(px);
            let color = rgb(GUIDE_COLOR);
            if let Some(rect) = target {
                window.paint_quad(outline(
                    Bounds::new(
                        screen(point(rect.x, rect.y)),
                        size(px(rect.width * view.zoom), px(rect.height * view.zoom)),
                    ),
                    color,
                    gpui::BorderStyle::Solid,
                ));
            }
            for dimension in &dimensions {
                let Dimension {
                    axis,
                    start,
                    end,
                    cross,
                } = *dimension;
                let (a, b) = if axis == 0 {
                    (screen(point(start, cross)), screen(point(end, cross)))
                } else {
                    (screen(point(cross, start)), screen(point(cross, end)))
                };
                let line = Bounds::new(a, size((b.x - a.x).max(px(1.)), (b.y - a.y).max(px(1.))));
                window.paint_quad(fill(line, color));
                for p in [a, b] {
                    let (offset, length) = if axis == 0 {
                        (point(px(0.), px(-3.)), size(px(1.), px(7.)))
                    } else {
                        (point(px(-3.), px(0.)), size(px(7.), px(1.)))
                    };
                    window.paint_quad(fill(Bounds::new(p + offset, length), color));
                }
                let value = (end - start).abs();
                let label = if (value - value.round()).abs() < 0.05 {
                    format!("{value:.0}")
                } else {
                    format!("{value:.1}")
                };
                let text = window.text_system().shape_line(
                    label.clone().into(),
                    px(11.),
                    &[TextRun {
                        len: label.len(),
                        font: window.text_style().font(),
                        color: color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                );
                let label_size = size(text.width + px(10.), px(19.));
                let center = a + (b - a) / 2.;
                let origin = if axis == 0 {
                    center - point(label_size.width / 2., px(23.))
                } else {
                    center + point(px(5.), -label_size.height / 2.)
                };
                window.paint_quad(
                    fill(Bounds::new(origin, label_size), rgb(0x302333)).corner_radii(px(4.)),
                );
                let _ = text.paint(
                    origin + point(px(5.), px(1.)),
                    px(17.),
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            }
        },
    )
    .absolute()
    .size_full()
}

#[cfg(test)]
mod tests;
