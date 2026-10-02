use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DrawTool {
    Board,
    Text,
    Shape(ShapeKind),
}

pub(super) struct BoxDraft {
    pub kind: DrawTool,
    pub start: Point<f32>,
    pub rect: Rect,
}

pub(super) fn drag_rect(start: Point<f32>, end: Point<f32>, square: bool) -> Rect {
    let mut delta = end - start;
    if square {
        let side = delta.x.abs().max(delta.y.abs());
        delta = point(
            if delta.x < 0. { -side } else { side },
            if delta.y < 0. { -side } else { side },
        );
    }
    delta.x = delta.x.clamp(
        -crate::scene::artboard::MAX_SIZE,
        crate::scene::artboard::MAX_SIZE,
    );
    delta.y = delta.y.clamp(
        -crate::scene::artboard::MAX_SIZE,
        crate::scene::artboard::MAX_SIZE,
    );
    Rect {
        x: start.x + delta.x.min(0.),
        y: start.y + delta.y.min(0.),
        width: delta.x.abs(),
        height: delta.y.abs(),
    }
}

impl Workspace {
    pub(super) fn activate_drawing(
        &mut self,
        tool: DrawTool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_gesture(window, cx);
        self.finish_bezier(cx);
        self.seal_text_edits(cx);
        for text in &self.texts {
            text.editor.update(cx, |editor, _| editor.editing = false);
        }
        self.draw_tool = Some(tool);
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(super) fn parent_origin(&self, board: Option<usize>) -> Point<f32> {
        self.boards
            .iter()
            .find(|b| Some(b.id) == board)
            .map_or(point(0., 0.), |b| point(b.rect.x, b.rect.y))
    }

    pub(super) fn board_point(&self, board: Option<usize>, position: Point<Pixels>) -> Point<f32> {
        let local = position - self.bounds.get().origin;
        let world = self
            .view
            .world(point(f32::from(local.x), f32::from(local.y)))
            - self.parent_origin(board);
        point(
            world.x.clamp(-1_000_000., 1_000_000.),
            world.y.clamp(-1_000_000., 1_000_000.),
        )
    }

    pub(super) fn board_at(&self, p: Point<f32>) -> Option<usize> {
        self.paint_order().into_iter().rev().find(|id| {
            self.boards.iter().find(|b| b.id == *id).is_some_and(|b| {
                self.layer_editable(*id)
                    && p.x >= b.rect.x
                    && p.x <= b.rect.x + b.rect.width
                    && p.y >= b.rect.y
                    && p.y <= b.rect.y + b.rect.height
            })
        })
    }

    // Resolve ownership at the end of a drag using the object's center. Convert
    // local coordinates through world space so attaching/detaching never jumps.
    pub(super) fn parent_for_rect(
        &self,
        board: Option<usize>,
        mut rect: Rect,
    ) -> (Option<usize>, Rect) {
        let origin = self.parent_origin(board);
        rect.x += origin.x;
        rect.y += origin.y;
        let parent = self.board_at(point(rect.x + rect.width / 2., rect.y + rect.height / 2.));
        let origin = self.parent_origin(parent);
        rect.x -= origin.x;
        rect.y -= origin.y;
        (parent, rect)
    }

    pub(super) fn reparent_moved(&mut self, gesture: GestureKind, cx: &mut Context<Self>) {
        let id = match gesture {
            GestureKind::Shape { id, .. } | GestureKind::Text { id, .. } => Some(id),
            _ => None,
        };
        if id.is_some_and(|id| self.hierarchy.parents.contains_key(&id)) {
            self.sync_fields(cx);
            return;
        }
        match gesture {
            GestureKind::Shape {
                id,
                handle: None,
                original,
            } => {
                if let Some(index) = self
                    .shapes
                    .iter()
                    .position(|s| s.id == id && s.rect != original)
                {
                    let shape = &self.shapes[index];
                    let (parent, rect) = self.parent_for_rect(shape.board, shape.rect);
                    self.shapes[index].board = parent;
                    self.shapes[index].rect = rect;
                    self.selected = parent;
                }
            }
            GestureKind::Text {
                id,
                handle: None,
                original,
            } => {
                if let Some(index) = self
                    .texts
                    .iter()
                    .position(|t| t.id == id && t.rect != original)
                {
                    let text = &self.texts[index];
                    let (parent, rect) = self.parent_for_rect(text.board, text.rect);
                    self.texts[index].board = parent;
                    self.texts[index].rect = rect;
                    self.selected = parent;
                }
            }
            _ => {}
        }
        if let Some(id) = id {
            self.avoid_component_nesting(id);
        }
        self.sync_fields(cx);
    }

    pub(super) fn creation_preview(&self) -> Option<Div> {
        let draft = self.box_draft.as_ref()?;
        let p = self.view.screen(point(draft.rect.x, draft.rect.y));
        Some(
            div()
                .absolute()
                .left(px(p.x))
                .top(px(p.y))
                .w(px(draft.rect.width * self.view.zoom))
                .h(px(draft.rect.height * self.view.zoom))
                .border_1()
                .border_color(rgb(ACCENT))
                .when(draft.kind == DrawTool::Board, |el| {
                    el.bg(rgb(0xffffff).opacity(0.85))
                }),
        )
    }
}

#[cfg(test)]
mod tests;
