use super::*;

impl Workspace {
    pub(super) fn create_at(
        &mut self,
        tool: DrawTool,
        origin: Point<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.exit_vector_edit(cx);
        let (width, height) = match tool {
            DrawTool::Board => (400., 300.),
            DrawTool::Text => (200., 48.),
            DrawTool::Shape(kind) if kind.is_line() => (100., 0.),
            DrawTool::Shape(_) => (100., 100.),
        };
        let mut rect = Rect {
            x: origin.x,
            y: origin.y,
            width,
            height,
        };
        if tool == DrawTool::Board {
            self.add_artboard(rect, cx);
            return;
        }
        let board = self.board_at(origin);
        let parent = self.parent_origin(board);
        rect.x -= parent.x;
        rect.y -= parent.y;
        match tool {
            DrawTool::Text => self.add_text(board, rect, window, cx),
            DrawTool::Shape(kind) => {
                let id = self.next_id;
                self.next_id += 1;
                let mut shape = Shape::new(id, board, kind, rect);
                if kind.is_line() {
                    shape.fill_enabled = false;
                    shape.stroke.enabled = true;
                    shape.stroke.width = 2.;
                    shape.set_path(&[point(rect.x, rect.y), point(rect.x + width, rect.y)]);
                }
                if kind.is_path() || kind.is_polygon() {
                    shape.stroke.align = crate::scene::shape::StrokeAlign::Center;
                }
                self.shapes.push(shape);
                self.history.borrow_mut().record(
                    vec![Change::Shape {
                        id,
                        index: self.shapes.len() - 1,
                        value: None,
                    }],
                    None,
                );
                self.select_shape(id, cx);
            }
            DrawTool::Board => unreachable!(),
        }
    }
}
