use super::Effect;
use crate::scene::{
    artboard::Rect,
    shape::{Shape, ShapeKind},
};
use gpui::{
    Bounds, EffectShader, EffectUniforms, Pixels, Point, SubtreeBloomPass, SubtreeEffectPass,
};

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
}

/// Replaces preceding artwork inside a screen-space region, preserving alpha.
#[derive(Clone, Copy)]
pub(crate) struct Filter {
    pub region: Region,
    pub radius: f32,
    pub opacity: f32,
    pub clip: Option<Rect>,
}

impl Filter {
    pub fn pass(
        self,
        origin: Point<Pixels>,
        capture: Bounds<Pixels>,
        scale: f32,
    ) -> SubtreeEffectPass {
        let offset = origin - capture.origin;
        let x = f32::from(offset.x);
        let y = f32::from(offset.y);
        let r = self.region;
        let (sin, cos) = r.rotation.to_radians().sin_cos();
        let clip = self
            .clip
            .map(|clip| {
                [
                    (x + clip.x) * scale,
                    (y + clip.y) * scale,
                    (x + clip.x + clip.width) * scale,
                    (y + clip.y + clip.height) * scale,
                ]
            })
            .unwrap_or([
                0.,
                0.,
                f32::from(capture.size.width) * scale,
                f32::from(capture.size.height) * scale,
            ]);
        let radius = self.radius * scale;
        SubtreeEffectPass {
            shader: gpui_effects::subtree_identity_shader(),
            uniforms: EffectUniforms::default()
                .with_slot(0, [r.rect.width * scale, r.rect.height * scale, cos, sin])
                .with_slot(1, r.corners.map(|r| r * scale))
                // Slot 2 belongs to the renderer's separable-pass dispatch.
                .with_slot(
                    3,
                    [
                        (x + r.rect.x + r.rect.width * 0.5) * scale,
                        (y + r.rect.y + r.rect.height * 0.5) * scale,
                        if r.ellipse { 1. } else { 0. },
                        self.opacity,
                    ],
                )
                .with_slot(4, clip)
                .with_slot(5, [radius, 0., 0., 0.]),
            time: 0.,
            images: Default::default(),
            feedback: None,
            distance_field: None,
            particles: None,
            particle_transition: None,
            // Retain the original capture for replacement while filtering a
            // second image in two directions, using the compound-pass API.
            bloom: Some(SubtreeBloomPass {
                extract: EffectShader::wgsl_image(include_str!("backdrop/extract.wgsl")),
                blur: EffectShader::wgsl_image(include_str!("backdrop/blur.wgsl")),
                composite: EffectShader::wgsl_two_images(include_str!("background_blur.wgsl")),
                downsample: ((radius / 16.).floor() as u32).clamp(1, 8),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn background_shaders_validate() {
        let shaders = [
            EffectShader::wgsl_image(include_str!("backdrop/extract.wgsl")),
            EffectShader::wgsl_image(include_str!("backdrop/blur.wgsl")),
            EffectShader::wgsl_two_images(include_str!("background_blur.wgsl")),
        ];
        for shader in shaders {
            let source = gpui::compose_subtree_effect_wgsl(&shader);
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
