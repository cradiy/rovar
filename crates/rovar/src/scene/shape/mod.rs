use crate::i18n::t;
use crate::scene::artboard::{FillMode, LinearGradient, Rect};
use gpui::{Background, Point, Rgba, point, rgb};
use std::rc::Rc;
mod vector;
pub(crate) use vector::arrow_wings;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ShapeKind {
    Rectangle,
    Ellipse,
    Polygon,
    Star,
    Arrow,
    Image,
    Video,
    Line,
    Pen,
    Bezier,
}

impl ShapeKind {
    pub fn is_path(self) -> bool {
        matches!(self, Self::Line | Self::Arrow | Self::Pen | Self::Bezier)
    }
    pub fn is_line(self) -> bool {
        matches!(self, Self::Line | Self::Arrow)
    }
    pub fn is_polygon(self) -> bool {
        matches!(self, Self::Polygon | Self::Star)
    }
    pub fn is_media(self) -> bool {
        matches!(self, Self::Image | Self::Video)
    }
    pub fn supports_corners(self) -> bool {
        matches!(self, Self::Rectangle | Self::Image)
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => t("shape-rectangle"),
            Self::Ellipse => t("shape-ellipse"),
            Self::Polygon => t("shape-polygon"),
            Self::Star => t("shape-star"),
            Self::Arrow => t("shape-arrow"),
            Self::Image => t("shape-image"),
            Self::Video => t("shape-video"),
            Self::Line => t("shape-line"),
            Self::Pen => t("tool-pencil"),
            Self::Bezier => t("tool-pen"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum StrokeAlign {
    Inside,
    Center,
    Outside,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Stroke {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_style: Option<String>,
    pub enabled: bool,
    pub width: f32,
    pub align: StrokeAlign,
    #[serde(with = "crate::document::rgba")]
    pub color: Rgba,
    pub fill_mode: FillMode,
    pub gradient: LinearGradient,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            color_style: None,
            enabled: false,
            width: 1.,
            align: StrokeAlign::Inside,
            color: rgb(0x27232f),
            fill_mode: FillMode::Solid,
            gradient: LinearGradient::default(),
        }
    }
}

impl Stroke {
    pub fn background(&self) -> Background {
        match self.fill_mode {
            FillMode::Solid => self.color.into(),
            FillMode::Linear => self.gradient.background(),
            FillMode::Image => gpui::rgba(0).into(),
        }
    }
    pub fn outset(&self) -> f32 {
        if !self.enabled {
            return 0.;
        }
        match self.align {
            StrokeAlign::Inside => 0.,
            StrokeAlign::Center => self.width / 2.,
            StrokeAlign::Outside => self.width,
        }
    }
}

/// Immutable normalized points shared by rendering and history snapshots.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct PathPoints(pub Rc<Vec<Point<f32>>>);
impl PartialEq for PathPoints {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0) || self.0 == other.0
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Shape {
    #[serde(default, skip_serializing_if = "uuid::Uuid::is_nil")]
    pub uid: uuid::Uuid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_style: Option<String>,
    pub id: usize,
    pub layer: crate::scene::layer::LayerState,
    pub board: Option<usize>,
    pub name: String,
    pub kind: ShapeKind,
    pub vertices: usize,
    pub inner_radius: f32,
    pub mirrored: [bool; 2],
    #[serde(skip)]
    pub media: Option<std::sync::Arc<crate::media::MediaAsset>>,
    #[serde(default)]
    pub media_placement: crate::scene::image_fill::Placement,
    pub image_fill: crate::scene::image_fill::ImageFill,
    pub points: PathPoints,
    pub nodes: crate::scene::bezier::Nodes,
    pub closed: bool,
    pub rect: Rect,
    pub radius: f32,
    pub independent_corners: bool,
    // Clockwise from top left. Initialized on first switch, retained thereafter.
    pub corners: Option<[f32; 4]>,
    pub fill_enabled: bool,
    pub stroke: Stroke,
    #[serde(with = "crate::document::rgba")]
    pub color: Rgba,
    pub fill_mode: FillMode,
    pub gradient: LinearGradient,
}

impl Shape {
    pub fn new(id: usize, board: Option<usize>, kind: ShapeKind, rect: Rect) -> Self {
        Self {
            uid: uuid::Uuid::new_v4(),
            color_style: None,
            id,
            layer: Default::default(),
            board,
            kind,
            vertices: if kind == ShapeKind::Star { 5 } else { 3 },
            inner_radius: 0.45,
            mirrored: [false; 2],
            media: None,
            media_placement: Default::default(),
            image_fill: Default::default(),
            points: PathPoints::default(),
            nodes: Default::default(),
            closed: false,
            rect,
            name: crate::i18n::message(
                "object-name",
                &[("kind", kind.label().into()), ("id", id.to_string())],
            ),
            radius: 0.,
            independent_corners: false,
            corners: None,
            fill_enabled: true,
            stroke: Stroke::default(),
            color: rgb(0xd9d9d9),
            fill_mode: FillMode::Solid,
            gradient: LinearGradient::default(),
        }
    }
    pub fn background(&self) -> Background {
        match self.fill_mode {
            FillMode::Solid => self.color.into(),
            FillMode::Linear => self.gradient.background(),
            FillMode::Image => gpui::rgba(0).into(),
        }
    }
    pub fn set_path(&mut self, points: &[Point<f32>]) {
        if points.is_empty() {
            return;
        }
        let mut min = points[0];
        let mut max = min;
        for p in points {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
        let floor = if self.kind.is_line() { 0. } else { 1. };
        self.rect = Rect {
            x: min.x,
            y: min.y,
            width: (max.x - min.x).max(floor),
            height: (max.y - min.y).max(floor),
        };
        self.points = PathPoints(Rc::new(
            points
                .iter()
                .map(|p| {
                    point(
                        if self.rect.width > 0. {
                            (p.x - min.x) / self.rect.width
                        } else {
                            0.
                        },
                        if self.rect.height > 0. {
                            (p.y - min.y) / self.rect.height
                        } else {
                            0.
                        },
                    )
                })
                .collect(),
        ));
    }
    pub fn path_point(&self, index: usize) -> Point<f32> {
        let p = self.points.0[index];
        point(
            self.rect.x + p.x * self.rect.width,
            self.rect.y + p.y * self.rect.height,
        )
    }
    pub fn display_path_point(&self, index: usize) -> Point<f32> {
        crate::scene::rotation::around(
            self.path_point(index),
            crate::scene::rotation::center(self.rect),
            self.layer.rotation,
        )
    }
    /// Recomputing a path's bounds moves its pivot. Offset its box so unchanged
    /// nodes remain in the same displayed positions under the existing angle.
    pub fn preserve_rotation_pivot(&mut self, original_center: Point<f32>) {
        let center = crate::scene::rotation::center(self.rect);
        let offset =
            crate::scene::rotation::around(center, original_center, self.layer.rotation) - center;
        self.rect.x += offset.x;
        self.rect.y += offset.y;
    }
    pub fn set_endpoint(&mut self, index: usize, p: Point<f32>) {
        let mut points = [self.path_point(0), self.path_point(1)];
        points[index] = p;
        self.set_path(&points);
    }
    pub fn can_fill(&self) -> bool {
        !self.kind.is_media()
            && (!self.kind.is_path() || (self.kind == ShapeKind::Bezier && self.closed))
    }
    pub fn world_nodes(&self) -> Vec<crate::scene::bezier::Node> {
        self.nodes
            .0
            .iter()
            .map(|node| {
                node.map(|p| {
                    point(
                        self.rect.x + p.x * self.rect.width,
                        self.rect.y + p.y * self.rect.height,
                    )
                })
            })
            .collect()
    }
    pub fn set_bezier(&mut self, nodes: &[crate::scene::bezier::Node], closed: bool) {
        self.closed = closed;
        let bounds = crate::scene::bezier::extrema(nodes, closed);
        self.set_path(&bounds);
        self.nodes = crate::scene::bezier::Nodes(Rc::new(
            nodes
                .iter()
                .map(|node| {
                    node.map(|p| {
                        point(
                            (p.x - self.rect.x) / self.rect.width,
                            (p.y - self.rect.y) / self.rect.height,
                        )
                    })
                })
                .collect(),
        ));
    }
    pub fn paint_mode(&self, stroke: bool) -> FillMode {
        if stroke {
            self.stroke.fill_mode
        } else {
            self.fill_mode
        }
    }
    pub fn paint_gradient(&self, stroke: bool) -> &LinearGradient {
        if stroke {
            &self.stroke.gradient
        } else {
            &self.gradient
        }
    }
    pub fn paint_gradient_mut(&mut self, stroke: bool) -> &mut LinearGradient {
        if stroke {
            &mut self.stroke.gradient
        } else {
            &mut self.gradient
        }
    }
    pub fn paint_color(&self, stop: usize, stroke: bool) -> Rgba {
        let gradient = self.paint_gradient(stroke);
        if self.paint_mode(stroke) == FillMode::Linear {
            gradient.stop(stop).unwrap_or(&gradient.stops()[0]).color
        } else if stroke {
            self.stroke.color
        } else {
            self.color
        }
    }
    pub fn paint_color_mut(&mut self, stop: usize, stroke: bool) -> Option<&mut Rgba> {
        if self.paint_mode(stroke) == FillMode::Linear {
            self.paint_gradient_mut(stroke)
                .stop_mut(stop)
                .map(|s| &mut s.color)
        } else if stroke {
            Some(&mut self.stroke.color)
        } else {
            Some(&mut self.color)
        }
    }
    pub fn set_independent_corners(&mut self, independent: bool) {
        if independent {
            self.corners.get_or_insert([self.radius; 4]);
        }
        self.independent_corners = independent;
    }
    pub fn displayed_radii(&self) -> [f32; 4] {
        let radii = if self.independent_corners {
            self.corners.unwrap_or([self.radius; 4])
        } else {
            [self.radius; 4]
        };
        // Clamp each corner to half the short edge, preserving stored values.
        radii.map(|r| r.min(self.rect.width / 2.).min(self.rect.height / 2.))
    }
}
