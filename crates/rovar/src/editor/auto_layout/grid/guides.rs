use super::*;
use gpui::{PathBuilder, TextRun, canvas};

impl Workspace {
    pub(in crate::editor) fn grid_guides(&self) -> impl IntoElement + use<> {
        let grid = self.grid_guide_target().and_then(|id| {
            self.auto_layout
                .grid
                .tracks
                .borrow()
                .get(&id)
                .cloned()
                .map(|mut tracks| {
                    if let Some(rect) = self.world_rect(id) {
                        tracks.bounds = rect;
                    }
                    (tracks, self.object_rotation(id))
                })
        });
        let view = self.view;
        canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                let Some((grid, angle)) = &grid else { return };
                let rect = grid.bounds;
                let screen = |x, y| {
                    let p = crate::scene::rotation::around(
                        point(rect.x + x, rect.y + y),
                        crate::scene::rotation::center(rect),
                        *angle,
                    );
                    bounds.origin + view.screen(p).map(px)
                };
                let mut path = PathBuilder::stroke(px(1.));
                for (axis, tracks, limit, cross_limit) in [
                    (0, &grid.columns, rect.width, rect.height),
                    (1, &grid.rows, rect.height, rect.width),
                ] {
                    let other = if axis == 0 { &grid.rows } else { &grid.columns };
                    let lo = other.first().map_or(0., |t| t[0]).clamp(0., cross_limit);
                    let hi = other
                        .last()
                        .map_or(cross_limit, |t| t[1])
                        .clamp(lo, cross_limit);
                    let mut last = None;
                    for (index, track) in tracks.iter().enumerate() {
                        for position in track {
                            if *position < 0. || *position > limit || last == Some(*position) {
                                continue;
                            }
                            last = Some(*position);
                            let (a, b) = if axis == 0 {
                                (screen(*position, lo), screen(*position, hi))
                            } else {
                                (screen(lo, *position), screen(hi, *position))
                            };
                            path.move_to(a);
                            path.line_to(b);
                        }
                        if (track[1] - track[0]) * view.zoom < 28. || track[0] >= limit {
                            continue;
                        }
                        let middle = (track[0] + track[1].min(limit)) / 2.;
                        let label = (index + 1).to_string();
                        let text = window.text_system().shape_line(
                            label.clone().into(),
                            px(10.),
                            &[TextRun {
                                len: label.len(),
                                font: window.text_style().font(),
                                color: rgb(ACCENT).into(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        );
                        let origin = if axis == 0 {
                            screen(middle, lo) - point(text.width / 2., px(15.))
                        } else {
                            screen(lo, middle) - point(text.width + px(5.), px(7.))
                        };
                        let _ =
                            text.paint(origin, px(14.), gpui::TextAlign::Left, None, window, cx);
                    }
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, gpui::rgba(0xb4a2ee66));
                }
            },
        )
        .absolute()
        .size_full()
    }

    pub(super) fn grid_guide_target(&self) -> Option<usize> {
        if !self.auto_layout.grid.show_guides || self.preview.is_some() {
            return None;
        }
        let selected = self.layout_target()?;
        let id = if self
            .hierarchy
            .layouts
            .get(&selected)
            .is_some_and(|l| l.axis == Axis::Grid)
        {
            selected
        } else {
            self.grid_item_target()?;
            self.layer_parent(selected)?
        };
        self.layer_info(id)
            .filter(|(own, parent)| !self.effective_layer(*own, *parent).hidden)
            .map(|_| id)
    }
}
