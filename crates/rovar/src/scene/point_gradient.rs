use gpui::{Point, Rgba, point, rgb};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GradientPoint {
    pub position: Point<f32>,
    #[serde(with = "crate::document::rgba")]
    pub color: Rgba,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PointGradient {
    pub points: [GradientPoint; 4],
}

impl Default for PointGradient {
    fn default() -> Self {
        Self {
            points: [
                (0.15, 0.2, 0xffb178),
                (0.8, 0.15, 0xf36c98),
                (0.25, 0.85, 0x6053cc),
                (0.85, 0.8, 0x91cce5),
            ]
            .map(|(x, y, color)| GradientPoint {
                position: point(x, y),
                color: rgb(color),
                radius: 0.65,
            }),
        }
    }
}

impl PointGradient {
    pub fn validate(&self) -> anyhow::Result<()> {
        for p in self.points {
            anyhow::ensure!(
                [
                    p.position.x,
                    p.position.y,
                    p.color.r,
                    p.color.g,
                    p.color.b,
                    p.color.a
                ]
                .into_iter()
                .all(|v| v.is_finite() && (0. ..=1.).contains(&v))
                    && (0.01..=4.).contains(&p.radius),
                "Invalid gradient point"
            );
        }
        Ok(())
    }

    pub fn gpu_points(&self) -> [gpui_effects::GradientPoint; 4] {
        self.points
            .map(|p| gpui_effects::GradientPoint::new(p.position, p.color).radius(p.radius))
    }

    pub fn paint(&self, bounds: gpui::Bounds<gpui::Pixels>, window: &mut gpui::Window) {
        let _ = window.paint_effect(
            gpui::PaintEffect::new(bounds, gpui_effects::point_gradient_shader())
                .uniforms(gpui_effects::point_gradient_uniforms(self.gpu_points())),
        );
    }

    /// Same normalized, premultiplied blend as the GPU surface, without dithering.
    pub fn sample(&self, uv: Point<f32>, width: f32, height: f32) -> Rgba {
        let edge = width.min(height).max(f32::EPSILON);
        let distances = self.points.map(|p| {
            let x = (uv.x - p.position.x) * width / edge / p.radius;
            let y = (uv.y - p.position.y) * height / edge / p.radius;
            2. * (x * x + y * y)
        });
        let nearest = distances.into_iter().fold(f32::INFINITY, f32::min);
        let mut rgba = [0.; 4];
        let mut total = 0.;
        for (p, distance) in self.points.iter().zip(distances) {
            let weight = (nearest - distance).exp();
            total += weight;
            for (target, source) in rgba.iter_mut().zip([
                p.color.r * p.color.a,
                p.color.g * p.color.a,
                p.color.b * p.color.a,
                p.color.a,
            ]) {
                *target += source * weight;
            }
        }
        if rgba[3] <= 0. {
            return gpui::rgba(0);
        }
        Rgba {
            r: rgba[0] / rgba[3],
            g: rgba[1] / rgba[3],
            b: rgba[2] / rgba[3],
            a: rgba[3] / total,
        }
    }

    pub fn set_number(
        &mut self,
        index: usize,
        property: super::property::Property,
        value: f32,
    ) -> bool {
        use super::property::Property::*;
        let Some(p) = self.points.get_mut(index) else {
            return false;
        };
        match property {
            PointX if (0. ..=100.).contains(&value) => p.position.x = value / 100.,
            PointY if (0. ..=100.).contains(&value) => p.position.y = value / 100.,
            PointRadius if (1. ..=400.).contains(&value) => p.radius = value / 100.,
            _ => return false,
        }
        true
    }
}
