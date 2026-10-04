use super::Effect;
use crate::scene::{
    artboard::Rect,
    shape::{Shape, ShapeKind},
};
use gpui::{Bounds, Pixels, Window, px};

#[cfg(all(test, not(target_family = "wasm")))]
mod gpu_tests;

pub(crate) fn supports_shape(shape: &Shape) -> bool {
    matches!(
        shape.kind,
        ShapeKind::Rectangle | ShapeKind::Ellipse | ShapeKind::Image
    )
}

pub(crate) fn radius(effects: &[Effect]) -> f32 {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::BackgroundBlur {
                enabled: true,
                radius,
            } => Some(radius * radius),
            _ => None,
        })
        .sum::<f32>()
        .sqrt()
}

#[derive(Clone, Copy)]
pub(crate) struct Region {
    pub rect: Rect,
    pub rotation: f32,
    pub corners: [f32; 4],
    pub ellipse: bool,
}

impl Region {
    pub fn bounds(self) -> Rect {
        crate::scene::rotation::bounds(self.rect, self.rotation)
    }

    pub fn paint(self, bounds: Bounds<Pixels>, radius: f32, zoom: f32, window: &mut Window) {
        if radius <= 0. || !window.supports_backdrop_blur() {
            return;
        }
        let scale = zoom * window.raster_scale_factor();
        let (sin, cos) = self.rotation.to_radians().sin_cos();
        let mut uniforms = gpui::EffectUniforms::default();
        uniforms.set_slot(
            0,
            [self.rect.width * scale, self.rect.height * scale, cos, sin],
        );
        uniforms.set_slot(1, self.corners.map(|r| r * scale));
        uniforms.set_slot(
            2,
            [if self.ellipse { 1. } else { 0. }, radius * scale, 0., 0.],
        );
        let clip = window.content_mask().bounds;
        let device_scale = window.raster_scale_factor();
        uniforms.set_slot(
            3,
            [
                f32::from(clip.left()) * device_scale,
                f32::from(clip.top()) * device_scale,
                f32::from(clip.right()) * device_scale,
                f32::from(clip.bottom()) * device_scale,
            ],
        );
        window.paint_backdrop_effect(
            gpui::PaintBackdropEffect::new(
                bounds,
                px(radius * zoom),
                gpui::BackdropShader::wgsl(include_str!("background_blur.wgsl")),
            )
            .uniforms(uniforms),
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn background_shader_validates_for_both_sampler_backends() {
        let shader = gpui::BackdropShader::wgsl(include_str!("background_blur.wgsl"));
        for sampling in [
            gpui::BackdropSampling::Hardware,
            gpui::BackdropSampling::Manual,
        ] {
            let source = gpui::compose_backdrop_shader_wgsl_with_sampling(&shader, sampling);
            let module = naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
    }
}
