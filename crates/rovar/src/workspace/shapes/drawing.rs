use super::*;
pub(super) use crate::bezier::distance_to_segment as segment_distance;
use crate::shape::StrokeAlign;

pub(in crate::workspace) struct Draft {
    pub shape: Shape,
    points: Vec<Point<f32>>,
}

impl Workspace {
    pub(in crate::workspace) fn start_drawing(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(kind) = self.draw_tool else {
            return;
        };
        if matches!(kind, DrawTool::Board | DrawTool::Text) {
            let start = self.board_point(None, position);
            self.box_draft = Some(BoxDraft {
                kind,
                start,
                rect: Rect {
                    x: start.x,
                    y: start.y,
                    width: 0.,
                    height: 0.,
                },
            });
            self.begin(GestureKind::Draw, position, MouseButton::Left, window, cx);
            let start = self.snap_point(start, [true, true], None);
            if let Some(draft) = &mut self.box_draft {
                draft.start = start;
                draft.rect.x = start.x;
                draft.rect.y = start.y;
            }
            return;
        }
        let DrawTool::Shape(kind) = kind else {
            return;
        };
        if kind.is_media() {
            return;
        }
        if kind == ShapeKind::Bezier {
            self.place_bezier(position, window, cx);
            return;
        }
        let board = self.board_at(self.board_point(None, position));
        let p = self.board_point(board, position);
        let mut shape = Shape::new(
            0,
            board,
            kind,
            Rect {
                x: p.x,
                y: p.y,
                width: 0.,
                height: 0.,
            },
        );
        if kind.is_path() {
            shape.fill_enabled = false;
            shape.stroke.enabled = true;
            shape.stroke.align = StrokeAlign::Center;
            shape.stroke.width = if kind.is_line() { 2. } else { 3. };
            if let Some(previous) = self.selected_shape().filter(|s| s.kind == kind) {
                shape.stroke = previous.stroke.clone();
                shape.stroke.enabled = true;
            }
            shape.set_path(&[p, p]);
        }
        if kind.is_polygon() {
            shape.stroke.align = StrokeAlign::Center;
        }
        self.draft = Some(Draft {
            shape,
            points: vec![p],
        });
        self.begin(GestureKind::Draw, position, MouseButton::Left, window, cx);
        if kind != ShapeKind::Pen {
            let origin = self.parent_origin(board);
            let p = self.snap_point(p + origin, [true, true], None) - origin;
            let draft = self.draft.as_mut().unwrap();
            draft.points[0] = p;
            draft.shape.rect.x = p.x;
            draft.shape.rect.y = p.y;
            if kind.is_line() {
                draft.shape.set_path(&[p, p]);
            }
        }
    }
    pub(in crate::workspace) fn move_drawing(&mut self, position: Point<Pixels>, shift: bool) {
        let moved = self.gesture.is_none_or(|g| {
            (f32::from(position.x - g.start.x).abs() + f32::from(position.y - g.start.y).abs())
                >= 2.
        });
        if let Some(draft) = &self.box_draft {
            let start = draft.start;
            let end = if moved {
                self.board_point(None, position)
            } else {
                start
            };
            let rect = self.snap_box_rect(start, end, shift);
            self.box_draft.as_mut().unwrap().rect = rect;
            return;
        }
        let Some(draft) = self.draft.as_ref() else {
            return;
        };
        let board = draft.shape.board;
        let kind = draft.shape.kind;
        let start = draft.points[0];
        let origin = self.parent_origin(board);
        let mut p = if moved || kind == ShapeKind::Pen {
            self.board_point(board, position)
        } else {
            start
        };
        if !kind.is_path() {
            let mut rect = self.snap_box_rect(start + origin, p + origin, shift);
            rect.x -= origin.x;
            rect.y -= origin.y;
            self.draft.as_mut().unwrap().shape.rect = rect;
            return;
        } else if kind.is_line() {
            if shift {
                p = snap_line(start, p);
            }
            if moved {
                p = self.snap_point(p + origin, [true, true], shift.then_some(p - start)) - origin;
            }
            self.draft.as_mut().unwrap().shape.set_path(&[start, p]);
            return;
        }
        let draft = self.draft.as_mut().unwrap();
        let last = *draft.points.last().unwrap();
        if distance(last, p) * self.view.zoom < 0.75 {
            return;
        }
        // Discard only redundant middle samples, preserving reversals and corners.
        if draft.points.len() >= 2 {
            let a = draft.points[draft.points.len() - 2];
            let ab = last - a;
            let bc = p - last;
            if ab.x * bc.x + ab.y * bc.y >= 0.
                && segment_distance(last, a, p) * self.view.zoom < 0.35
            {
                draft.points.pop();
            }
        }
        draft.points.push(p);
        // Bound tessellation cost for exceptionally long uninterrupted strokes.
        if draft.points.len() > 4096 {
            let last = *draft.points.last().unwrap();
            draft.points = draft.points.iter().step_by(2).copied().collect();
            if draft.points.last() != Some(&last) {
                draft.points.push(last);
            }
        }
        draft.shape.set_path(&draft.points);
    }
    pub(in crate::workspace) fn finish_drawing(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = self.box_draft.take() {
            if draft.rect.width.min(draft.rect.height) * self.view.zoom >= 2. {
                self.draw_tool = None;
                if draft.kind == DrawTool::Board {
                    self.add_artboard(draft.rect, cx);
                } else {
                    let (board, rect) = self.parent_for_rect(None, draft.rect);
                    self.add_text(board, rect, window, cx);
                }
            }
            cx.notify();
            return;
        }
        self.shape_paths.borrow_mut().remove(&0);
        let Some(mut draft) = self.draft.take() else {
            return;
        };
        let shape = &draft.shape;
        let length = if !shape.kind.is_path() {
            shape.rect.width.min(shape.rect.height)
        } else if shape.kind.is_line() {
            distance(shape.path_point(0), shape.path_point(1))
        } else {
            draft.points.windows(2).map(|p| distance(p[0], p[1])).sum()
        };
        if length * self.view.zoom < 2. {
            cx.notify();
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        draft.shape.id = id;
        draft.shape.name = format!("{} {id}", draft.shape.kind.label());
        (draft.shape.board, draft.shape.rect) =
            self.parent_for_rect(draft.shape.board, draft.shape.rect);
        self.shapes.push(draft.shape);
        self.history.borrow_mut().record(
            vec![Change::Shape {
                id,
                index: self.shapes.len() - 1,
                value: None,
            }],
            None,
        );
        self.draw_tool = None;
        self.select_shape(id, cx);
    }
    pub(in crate::workspace) fn move_line_endpoint(
        &mut self,
        id: usize,
        end: usize,
        position: Point<Pixels>,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(board) = self.shapes.iter().find(|s| s.id == id).map(|s| s.board) else {
            return;
        };
        let origin = self.parent_origin(board);
        let mut p = self.board_point(board, position);
        let Some(before) = self.path_before.as_ref() else {
            return;
        };
        let before = before.clone();
        let anchor = before.display_path_point(1 - end);
        if shift {
            p = snap_line(anchor, p);
        }
        // A click on the endpoint must not jump to a nearby target.
        let moved = self.gesture.is_none_or(|g| {
            f32::from(position.x - g.start.x).abs() + f32::from(position.y - g.start.y).abs() >= 3.
        });
        if !moved {
            return;
        }
        p = self.snap_point(p + origin, [true, true], shift.then_some(p - anchor)) - origin;
        if let Some(shape) = self.shapes.iter_mut().find(|s| s.id == id) {
            let pivot = crate::rotation::center(before.rect);
            p = crate::rotation::around(p, pivot, -before.layer.rotation);
            *shape = before;
            shape.set_endpoint(end, p);
            shape.preserve_rotation_pivot(pivot);
        }
        self.sync_fields(cx);
    }
    pub(super) fn line_handles(
        &self,
        shape: &Shape,
        outset: f32,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::Stateful<Div>> {
        let id = shape.id;
        shape
            .points
            .0
            .iter()
            .take(2)
            .enumerate()
            .map(|(end, p)| {
                div()
                    .id(("line-end", end))
                    .debug_selector(move || format!("line-end-{end}"))
                    .absolute()
                    .left(px(outset + p.x * shape.rect.width * self.view.zoom - 6.))
                    .top(px(outset + p.y * shape.rect.height * self.view.zoom - 6.))
                    .size(px(12.))
                    .cursor(CursorStyle::Crosshair)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .size(px(8.))
                            .rounded_full()
                            .bg(rgb(0xffffff))
                            .border_1()
                            .border_color(rgb(ACCENT)),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                            let event = &gpui::MouseDownEvent {
                                position: window.raw_mouse_position(),
                                ..event.clone()
                            };
                            if this.space_down {
                                this.begin(
                                    GestureKind::Pan {
                                        original: this.view.pan,
                                    },
                                    event.position,
                                    event.button,
                                    window,
                                    cx,
                                );
                            } else {
                                this.path_before = this.shapes.iter().find(|s| s.id == id).cloned();
                                this.begin(
                                    GestureKind::LineEnd { id, end },
                                    event.position,
                                    event.button,
                                    window,
                                    cx,
                                );
                            }
                        }),
                    )
            })
            .collect()
    }
}

fn distance(a: Point<f32>, b: Point<f32>) -> f32 {
    (a.x - b.x).hypot(a.y - b.y)
}
pub(super) fn snap_line(origin: Point<f32>, p: Point<f32>) -> Point<f32> {
    let delta = p - origin;
    let step = std::f32::consts::FRAC_PI_4;
    let angle = (delta.y.atan2(delta.x) / step).round() * step;
    let length = delta.x.hypot(delta.y);
    let x = angle.cos();
    let y = angle.sin();
    origin
        + point(
            if x.abs() < 0.00001 { 0. } else { length * x },
            if y.abs() < 0.00001 { 0. } else { length * y },
        )
}

#[cfg(test)]
mod tests;
