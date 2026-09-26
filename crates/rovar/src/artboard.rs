use gpui::{
    Background, Div, Point, Rgba, checkerboard, div, linear_color_stop, multi_linear_gradient,
    point, prelude::*, px, rgb,
};

pub const MIN_SIZE: f32 = 1.;
pub const MAX_SIZE: f32 = 100_000.;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Artboard {
    pub id: usize,
    pub layer: crate::layer::LayerState,
    pub name: String,
    pub rect: Rect,
    #[serde(with = "crate::document::rgba")]
    pub color: Rgba,
    pub fill_mode: FillMode,
    pub gradient: LinearGradient,
    pub image_fill: crate::image_fill::ImageFill,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FillMode {
    #[default]
    Solid,
    Linear,
    Image,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GradientStop {
    pub id: usize,
    pub position: f32,
    #[serde(with = "crate::document::rgba")]
    pub color: Rgba,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LinearGradient {
    pub angle: f32,
    #[serde(with = "crate::document::gradient_kind")]
    pub kind: gpui::GradientKind,
    stops: Vec<GradientStop>,
    next_id: usize,
}

impl Default for LinearGradient {
    fn default() -> Self {
        Self {
            angle: 90.,
            kind: gpui::GradientKind::Linear,
            stops: vec![
                GradientStop {
                    id: 0,
                    position: 0.,
                    color: rgb(0xffffff),
                },
                GradientStop {
                    id: 1,
                    position: 1.,
                    color: rgb(0xd9d9d9),
                },
            ],
            next_id: 2,
        }
    }
}

impl LinearGradient {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (2..=4).contains(&self.stops.len()),
            "Invalid gradient stop count"
        );
        let mut ids = std::collections::BTreeSet::new();
        for stop in &self.stops {
            anyhow::ensure!(
                ids.insert(stop.id)
                    && stop.id < self.next_id
                    && stop.position.is_finite()
                    && (0. ..=1.).contains(&stop.position),
                "Invalid gradient stop"
            );
        }
        anyhow::ensure!(self.angle.is_finite(), "Invalid gradient angle");
        Ok(())
    }
    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    pub fn stop(&self, id: usize) -> Option<&GradientStop> {
        self.stops.iter().find(|s| s.id == id)
    }

    pub fn stop_mut(&mut self, id: usize) -> Option<&mut GradientStop> {
        self.stops.iter_mut().find(|s| s.id == id)
    }

    pub fn set_position(&mut self, id: usize, position: f32) -> bool {
        if !position.is_finite() || !(0. ..=1.).contains(&position) {
            return false;
        }
        let Some(stop) = self.stop_mut(id) else {
            return false;
        };
        stop.position = position;
        self.stops.sort_by(|a, b| a.position.total_cmp(&b.position));
        true
    }

    pub fn add_stop(&mut self) -> Option<usize> {
        if self.stops.len() >= 4 {
            return None;
        }
        // Split the largest interval using the same sRGB interpolation as
        // the GPUI gradient, so insertion preserves its appearance.
        let pair = self.stops.windows(2).max_by(|a, b| {
            (a[1].position - a[0].position).total_cmp(&(b[1].position - b[0].position))
        })?;
        let (a, b) = (pair[0], pair[1]);
        let alpha = (a.color.a + b.color.a) * 0.5;
        let mix = |x: f32, y: f32| (x + y) * 0.5;
        let color = Rgba {
            r: mix(a.color.r, b.color.r),
            g: mix(a.color.g, b.color.g),
            b: mix(a.color.b, b.color.b),
            a: alpha,
        };
        let id = self.next_id;
        self.next_id += 1;
        self.stops.push(GradientStop {
            id,
            position: (a.position + b.position) * 0.5,
            color,
        });
        self.stops.sort_by(|a, b| a.position.total_cmp(&b.position));
        Some(id)
    }

    pub fn remove_stop(&mut self, id: usize) -> bool {
        if self.stops.len() <= 2 {
            return false;
        }
        let count = self.stops.len();
        self.stops.retain(|s| s.id != id);
        self.stops.len() != count
    }

    pub fn reverse(&mut self) {
        for stop in &mut self.stops {
            stop.position = 1. - stop.position;
        }
        self.stops.reverse();
    }

    pub fn background(&self) -> Background {
        let stops: Vec<_> = self
            .stops
            .iter()
            .map(|s| linear_color_stop(s.color, s.position))
            .collect();
        (match stops.as_slice() {
            [a, b] => multi_linear_gradient(self.angle, [*a, *b]),
            [a, b, c] => multi_linear_gradient(self.angle, [*a, *b, *c]),
            [a, b, c, d] => multi_linear_gradient(self.angle, [*a, *b, *c, *d]),
            _ => unreachable!("gradient mutations preserve 2–4 stops"),
        })
        .gradient_kind(self.kind)
    }
}

impl Artboard {
    pub fn background(&self) -> Background {
        match self.fill_mode {
            FillMode::Solid => self.color.into(),
            FillMode::Linear => self.gradient.background(),
            FillMode::Image => gpui::rgba(0).into(),
        }
    }

    pub fn editable_color(&self, stop: usize) -> Rgba {
        if self.fill_mode == FillMode::Linear {
            self.gradient
                .stop(stop)
                .unwrap_or(&self.gradient.stops()[0])
                .color
        } else {
            self.color
        }
    }

    pub fn editable_color_mut(&mut self, stop: usize) -> Option<&mut Rgba> {
        if self.fill_mode == FillMode::Linear {
            self.gradient.stop_mut(stop).map(|s| &mut s.color)
        } else {
            Some(&mut self.color)
        }
    }
    /// Only the background is painted here. Future child content must not be
    /// clipped by the artboard during editing.
    pub fn surface(&self, zoom: f32) -> Div {
        div()
            .relative()
            .w(px(self.rect.width * zoom))
            .h(px(self.rect.height * zoom))
            .bg(rgb(0xffffff))
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .bg(checkerboard(rgb(0xd9dce2), 8.)),
            )
            .child(div().absolute().inset_0().bg(self.background()))
            .when(self.fill_mode == FillMode::Image, |el| {
                el.child(self.image_fill.element().absolute().inset_0())
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub pan: Point<f32>,
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            pan: point(0., 0.),
            zoom: 1.,
        }
    }
}

impl Viewport {
    pub fn screen(&self, world: Point<f32>) -> Point<f32> {
        point(
            world.x * self.zoom + self.pan.x,
            world.y * self.zoom + self.pan.y,
        )
    }

    pub fn world(&self, screen: Point<f32>) -> Point<f32> {
        point(
            (screen.x - self.pan.x) / self.zoom,
            (screen.y - self.pan.y) / self.zoom,
        )
    }

    pub fn zoom_at(&mut self, anchor: Point<f32>, zoom: f32) {
        let world = self.world(anchor);
        self.zoom = zoom.clamp(0.1, 256.);
        self.pan = point(
            anchor.x - world.x * self.zoom,
            anchor.y - world.y * self.zoom,
        );
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Handle(pub i8, pub i8);

impl Handle {
    pub const ALL: [Self; 8] = [
        Self(-1, -1),
        Self(0, -1),
        Self(1, -1),
        Self(1, 0),
        Self(1, 1),
        Self(0, 1),
        Self(-1, 1),
        Self(-1, 0),
    ];

    pub fn resize(self, start: Rect, delta: Point<f32>) -> Rect {
        let mut result = start;
        if self.0 != 0 {
            result.width = (start.width + delta.x * self.0 as f32).clamp(MIN_SIZE, MAX_SIZE);
            if self.0 < 0 {
                result.x = start.x + start.width - result.width;
            }
        }
        if self.1 != 0 {
            result.height = (start.height + delta.y * self.1 as f32).clamp(MIN_SIZE, MAX_SIZE);
            if self.1 < 0 {
                result.y = start.y + start.height - result.height;
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_reorders_by_position_without_changing_stop_identity() {
        let mut gradient = LinearGradient::default();
        gradient.stop_mut(1).unwrap().color.a = 0.;
        assert!(gradient.set_position(1, 0.75));
        let inserted = gradient.add_stop().unwrap();
        let original_color = gradient.stop(inserted).unwrap().color;
        assert_eq!(original_color.a, 0.5);
        assert!(gradient.set_position(inserted, 1.));
        assert_eq!(gradient.stops().last().unwrap().id, inserted);
        assert_eq!(gradient.stop(inserted).unwrap().color, original_color);
        let snapshot = gradient.clone();
        for position in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
            assert!(!gradient.set_position(inserted, position));
            assert_eq!(gradient, snapshot);
        }
        // Equal-position stops are valid hard transitions in the renderer.
        assert!(gradient.set_position(1, 1.));
        let _ = gradient.background();
        let fourth = gradient.add_stop().unwrap();
        assert!(gradient.add_stop().is_none());
        assert!(gradient.remove_stop(fourth));
        assert!(gradient.remove_stop(inserted));
        assert!(!gradient.remove_stop(0));
        assert_eq!(gradient.stops().len(), 2);
    }

    #[test]
    fn resize_crossing_opposite_edge_clamps_without_moving_that_edge() {
        let start = Rect {
            x: -20.,
            y: 30.,
            width: 200.,
            height: 100.,
        };
        let result = Handle(-1, -1).resize(start, point(500., 400.));
        assert_eq!(
            result,
            Rect {
                x: 179.,
                y: 129.,
                width: 1.,
                height: 1.
            }
        );
        let right = Handle(1, 0).resize(start, point(80., 999.));
        assert_eq!(
            right,
            Rect {
                width: 280.,
                ..start
            }
        );
    }

    #[test]
    fn zoom_preserves_world_point_under_pointer_even_at_limits() {
        let mut view = Viewport {
            pan: point(-370., 54.),
            zoom: 0.7,
        };
        let anchor = point(233., 410.);
        let before = view.world(anchor);
        for (zoom, expected) in [
            (2.5, 2.5),
            (16., 16.),
            (1024., 256.),
            (0.001, 0.1),
            (1., 1.),
        ] {
            view.zoom_at(anchor, zoom);
            assert_eq!(view.zoom, expected);
            let after = view.screen(before);
            // At maximum zoom, f32 screen coordinates still stay within 0.05 px.
            assert!((after.x - anchor.x).abs() < 0.05);
            assert!((after.y - anchor.y).abs() < 0.05);
        }
    }
}
