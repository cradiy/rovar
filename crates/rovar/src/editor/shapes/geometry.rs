use crate::scene::shape::{PathPoints, Shape, ShapeKind};
use gpui::{
    FillOptions, FillRule, LineCap, LineJoin, Path, PathBuilder, PathStyle, Pixels, StrokeOptions,
    point, px,
};

/// Screen-space geometry only: moving, changing colors or toggling fill reuses it.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::editor) struct GeometryKey {
    pub kind: ShapeKind,
    pub width: f32,
    pub height: f32,
    pub radii: [f32; 4],
    pub stroke_width: f32,
    pub outset: f32,
    pub points: PathPoints,
    pub nodes: crate::scene::bezier::Nodes,
    pub closed: bool,
    pub zoom: f32,
}

impl GeometryKey {
    pub fn outset(shape: &Shape, zoom: f32) -> f32 {
        if shape.kind == ShapeKind::Arrow {
            (shape.stroke.width * 4.).max(12.) * zoom + 6.
        } else if shape.kind.is_path() || shape.kind.is_polygon() {
            (shape.stroke.width * zoom / 2.).max(6.)
        } else {
            shape.stroke.outset() * zoom
        }
    }

    pub fn new(shape: &Shape, zoom: f32) -> Self {
        Self {
            kind: if shape.kind.is_polygon() {
                ShapeKind::Bezier
            } else {
                shape.kind
            },
            width: shape.rect.width * zoom,
            height: shape.rect.height * zoom,
            radii: shape.displayed_radii().map(|r| r * zoom),
            stroke_width: if shape.stroke.enabled {
                shape.stroke.width * zoom
            } else {
                0.
            },
            outset: Self::outset(shape, zoom),
            points: shape.points.clone(),
            nodes: if shape.kind.is_polygon() {
                crate::scene::bezier::Nodes(std::sync::Arc::new(
                    shape
                        .polygon_points()
                        .into_iter()
                        .map(crate::scene::bezier::Node::corner)
                        .collect(),
                ))
            } else {
                shape.nodes.clone()
            },
            closed: shape.editable_closed(),
            zoom,
        }
    }

    fn contour(&self, builder: &mut PathBuilder, offset: f32) {
        let x = self.outset - offset;
        let y = x;
        let w = self.width + offset * 2.;
        let h = self.height + offset * 2.;
        if w <= 0. || h <= 0. {
            return;
        }
        if self.kind == ShapeKind::Ellipse {
            builder.move_to(point(px(x + w), px(y + h / 2.)));
            builder.arc_to(
                point(px(w / 2.), px(h / 2.)),
                px(0.),
                false,
                true,
                point(px(x), px(y + h / 2.)),
            );
            builder.arc_to(
                point(px(w / 2.), px(h / 2.)),
                px(0.),
                false,
                true,
                point(px(x + w), px(y + h / 2.)),
            );
        } else {
            let [tl, tr, br, bl] = self.radii.map(|r| {
                if r == 0. {
                    0.
                } else {
                    (r + offset).max(0.).min(w / 2.).min(h / 2.)
                }
            });
            builder.move_to(point(px(x + tl), px(y)));
            builder.line_to(point(px(x + w - tr), px(y)));
            arc(builder, tr, x + w, y + tr);
            builder.line_to(point(px(x + w), px(y + h - br)));
            arc(builder, br, x + w - br, y + h);
            builder.line_to(point(px(x + bl), px(y + h)));
            arc(builder, bl, x, y + h - bl);
            builder.line_to(point(px(x), px(y + tl)));
            arc(builder, tl, x + tl, y);
        }
        builder.close();
    }

    fn bezier_contour(&self, path: &mut PathBuilder) {
        let to_pixel = |p: gpui::Point<f32>| {
            point(
                px(self.outset + p.x * self.width),
                px(self.outset + p.y * self.height),
            )
        };
        let Some(first) = self.nodes.0.first() else {
            return;
        };
        path.move_to(to_pixel(first.anchor));
        let segments = self.nodes.0.len().saturating_sub(1)
            + usize::from(self.closed && self.nodes.0.len() > 1);
        for i in 0..segments {
            let a = self.nodes.0[i];
            let b = self.nodes.0[(i + 1) % self.nodes.0.len()];
            path.cubic_bezier_to(
                to_pixel(b.anchor),
                to_pixel(a.outgoing),
                to_pixel(b.incoming),
            );
        }
        if self.closed {
            path.close();
        }
    }
    fn bezier_geometry(self) -> Geometry {
        let mut tolerance = 0.1;
        // Retry with coarser tessellation if exceptionally complex curves exceed u16 indices.
        let (fill, stroke) = loop {
            let mut fill = PathBuilder::fill().with_style(PathStyle::Fill(
                FillOptions::default()
                    .with_fill_rule(FillRule::EvenOdd)
                    .with_tolerance(tolerance),
            ));
            if self.closed && self.nodes.0.len() >= 2 {
                self.bezier_contour(&mut fill);
            }
            let mut stroke =
                PathBuilder::stroke(px(self.stroke_width)).with_style(PathStyle::Stroke(
                    StrokeOptions::default()
                        .with_line_width(self.stroke_width)
                        .with_line_cap(LineCap::Round)
                        .with_line_join(LineJoin::Round)
                        .with_tolerance(tolerance),
                ));
            if self.stroke_width > 0. && self.nodes.0.len() >= 2 {
                self.bezier_contour(&mut stroke);
            }
            match (fill.build(), stroke.build()) {
                (Ok(fill), Ok(stroke)) => break (fill, stroke),
                _ if tolerance < 1_000_000. => tolerance *= 4.,
                _ => {
                    break (
                        Path::new(point(px(0.), px(0.))),
                        Path::new(point(px(0.), px(0.))),
                    );
                }
            }
        };
        let stroke = (self.stroke_width > 0.).then_some(stroke);
        Geometry {
            key: self,
            fill,
            stroke,
        }
    }
    pub fn build(self) -> Geometry {
        if self.kind == ShapeKind::Bezier {
            return self.bezier_geometry();
        }
        if self.kind.is_path() {
            let stroke = (self.stroke_width > 0. && self.points.0.len() >= 2).then(|| {
                // Lyon's builder uses u16 indices. Reserve room for segment bodies
                // and round joins, increasing arc tolerance only for dense, wide paths.
                // The stored centerline points and editable geometry stay unchanged.
                let round_budget = (24_000. / self.points.0.len() as f32).max(4.);
                let tolerance = (self.stroke_width / 2.
                    * (1. - (std::f32::consts::PI / round_budget).cos()))
                .max(0.1);
                let mut path =
                    PathBuilder::stroke(px(self.stroke_width)).with_style(PathStyle::Stroke(
                        StrokeOptions::default()
                            .with_tolerance(tolerance)
                            .with_line_width(self.stroke_width)
                            .with_line_cap(LineCap::Round)
                            .with_line_join(LineJoin::Round),
                    ));
                for (index, p) in self.points.0.iter().enumerate() {
                    let p = point(
                        px(self.outset + p.x * self.width),
                        px(self.outset + p.y * self.height),
                    );
                    if index == 0 {
                        path.move_to(p);
                    } else {
                        path.line_to(p);
                    }
                }
                if self.kind == ShapeKind::Arrow {
                    let a = point(
                        self.points.0[0].x * self.width / self.zoom,
                        self.points.0[0].y * self.height / self.zoom,
                    );
                    let b = point(
                        self.points.0[1].x * self.width / self.zoom,
                        self.points.0[1].y * self.height / self.zoom,
                    );
                    let [left, right] =
                        crate::scene::shape::arrow_wings(a, b, self.stroke_width / self.zoom);
                    let pixel = |p: gpui::Point<f32>| {
                        point(
                            px(self.outset + p.x * self.zoom),
                            px(self.outset + p.y * self.zoom),
                        )
                    };
                    path.move_to(pixel(left));
                    path.line_to(pixel(b));
                    path.line_to(pixel(right));
                }
                path.build().expect("bounded open path")
            });
            return Geometry {
                key: self,
                fill: Path::new(point(px(0.), px(0.))),
                stroke,
            };
        }
        let mut fill = PathBuilder::fill();
        self.contour(&mut fill, 0.);
        let fill = fill.build().expect("bounded shape contour");
        let stroke = (self.stroke_width > 0.).then(|| {
            // Two contours with even-odd fill leave a real transparent hole.
            // A background-colored inner shape would erase underlying objects.
            let mut ring = PathBuilder::fill().with_style(PathStyle::Fill(
                FillOptions::default().with_fill_rule(FillRule::EvenOdd),
            ));
            self.contour(&mut ring, self.outset);
            self.contour(&mut ring, self.outset - self.stroke_width);
            ring.build().expect("bounded shape stroke contours")
        });
        Geometry {
            key: self,
            fill,
            stroke,
        }
    }
}

pub(in crate::editor) struct Geometry {
    pub key: GeometryKey,
    pub fill: Path<Pixels>,
    pub stroke: Option<Path<Pixels>>,
}

fn arc(builder: &mut PathBuilder, radius: f32, x: f32, y: f32) {
    let end = point(px(x), px(y));
    if radius > 0. {
        builder.arc_to(point(px(radius), px(radius)), px(0.), false, true, end);
    } else {
        builder.line_to(end);
    }
}

#[cfg(test)]
mod tests {
    use super::GeometryKey;
    use crate::{
        scene::artboard::Rect,
        scene::shape::{Shape, ShapeKind, StrokeAlign},
    };
    use gpui::{Path, Pixels};

    fn covers(path: &Path<Pixels>, x: f32, y: f32) -> bool {
        path.vertices.as_chunks::<3>().0.iter().any(|triangle| {
            let sides: Vec<_> = (0..3)
                .map(|i| {
                    let a = triangle[i].xy_position;
                    let b = triangle[(i + 1) % 3].xy_position;
                    (f32::from(b.x - a.x) * (y - f32::from(a.y)))
                        - (f32::from(b.y - a.y) * (x - f32::from(a.x)))
                })
                .collect();
            sides.iter().all(|s| *s >= 0.) || sides.iter().all(|s| *s <= 0.)
        })
    }

    #[test]
    fn stroke_mesh_has_transparent_hole_and_correct_placement_even_when_inner_contour_collapses() {
        for kind in [ShapeKind::Rectangle, ShapeKind::Ellipse] {
            let mut shape = Shape::new(
                1,
                None,
                kind,
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 200.,
                    height: 100.,
                },
            );
            shape.stroke.enabled = true;
            shape.stroke.width = 20.;
            for (align, outset) in [
                (StrokeAlign::Inside, 0.),
                (StrokeAlign::Center, 10.),
                (StrokeAlign::Outside, 20.),
            ] {
                shape.stroke.align = align;
                let geometry = GeometryKey::new(&shape, 1.).build();
                let stroke = geometry.stroke.unwrap();
                assert_eq!(f32::from(stroke.bounds.left()), 0.);
                assert!((f32::from(stroke.bounds.right()) - (200. + outset * 2.)).abs() < 0.1);
                assert!((f32::from(geometry.fill.bounds.left()) - outset).abs() < 0.1);
                assert!(covers(&stroke, 1., 50. + outset));
                assert!(!covers(&stroke, 21., 50. + outset));
                assert!(!covers(&stroke, 100. + outset, 50. + outset));
            }
            shape.stroke.align = StrokeAlign::Inside;
            shape.stroke.width = 60.;
            assert!(covers(
                &GeometryKey::new(&shape, 1.).build().stroke.unwrap(),
                100.,
                50.
            ));
            shape.stroke.width = 0.;
            assert!(GeometryKey::new(&shape, 1.).build().stroke.is_none());
        }
    }

    #[test]
    fn bezier_mesh_follows_curve_and_closes_fill_without_using_control_hull() {
        use crate::scene::bezier::Node;
        use gpui::point;
        let mut shape = Shape::new(
            1,
            None,
            ShapeKind::Bezier,
            Rect {
                x: 0.,
                y: 0.,
                width: 1.,
                height: 1.,
            },
        );
        let nodes = [
            Node {
                smooth: false,
                anchor: point(0., 0.),
                incoming: point(0., 0.),
                outgoing: point(0., 100.),
            },
            Node {
                smooth: false,
                anchor: point(200., 0.),
                incoming: point(200., 100.),
                outgoing: point(200., 0.),
            },
        ];
        shape.set_bezier(&nodes, false);
        shape.stroke.enabled = true;
        shape.stroke.width = 2.;
        assert!((shape.rect.height - 75.).abs() < 0.001);
        for zoom in [0.5, 1., 2.] {
            let key = GeometryKey::new(&shape, zoom);
            let padding = key.outset;
            let geometry = key.build();
            let stroke = geometry.stroke.unwrap();
            assert!(covers(&stroke, padding + 100. * zoom, padding + 75. * zoom));
            assert!(!covers(&stroke, padding + 100. * zoom, padding));
            assert!(geometry.fill.vertices.is_empty());
        }
        shape.set_bezier(&nodes, true);
        let key = GeometryKey::new(&shape, 1.);
        let padding = key.outset;
        let fill = key.build().fill;
        assert!(covers(&fill, padding + 100., padding + 35.));
        assert!(!covers(&fill, padding + 100., padding + 90.));
    }

    #[test]
    fn long_wide_pen_stroke_keeps_renderable_geometry() {
        let mut shape = Shape::new(
            1,
            None,
            ShapeKind::Pen,
            Rect {
                x: 0.,
                y: 0.,
                width: 1.,
                height: 1.,
            },
        );
        let points: Vec<_> = (0..4096)
            .map(|i| gpui::point(i as f32 * 2., if i % 2 == 0 { 0. } else { 10. }))
            .collect();
        shape.set_path(&points);
        shape.stroke.enabled = true;
        shape.stroke.width = 50_000.;
        let geometry = GeometryKey::new(&shape, 4.).build();
        let stroke = geometry.stroke.unwrap();
        assert!(!stroke.vertices.is_empty());
        assert!(f32::from(stroke.bounds.size.width) >= 8190. * 4.);
    }

    #[test]
    fn independent_corner_mesh_rounds_only_requested_corners_and_scales_with_zoom() {
        let mut shape = Shape::new(
            1,
            None,
            ShapeKind::Rectangle,
            Rect {
                x: 0.,
                y: 0.,
                width: 200.,
                height: 100.,
            },
        );
        shape.set_independent_corners(true);
        shape.corners = Some([0., 40., 0., 20.]);
        for zoom in [0.5, 1., 2.] {
            let mesh = GeometryKey::new(&shape, zoom).build().fill;
            assert!(covers(&mesh, 1. * zoom, 1. * zoom));
            assert!(!covers(&mesh, 199. * zoom, 1. * zoom));
            assert!(covers(&mesh, 199. * zoom, 99. * zoom));
            assert!(!covers(&mesh, 1. * zoom, 99. * zoom));
            assert!(covers(&mesh, 100. * zoom, 50. * zoom));
        }
    }
}
