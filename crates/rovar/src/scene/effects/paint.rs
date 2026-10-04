use super::Shadow;
use gpui::{Bounds, EffectShader, EffectUniforms, Pixels, SubtreeEffectPass, Window, px};

/// Paint callbacks contain artwork primitives only, never interactive elements.
/// Each shadow starts with the same source alpha, so stacked shadows do not cast
/// shadows on each other. The caller paints the original artwork afterwards.
pub(crate) fn paint_shadows(
    bounds: Bounds<Pixels>,
    shadows: &[Shadow],
    zoom: f32,
    window: &mut Window,
    mut source: impl FnMut(&mut Window),
) {
    if !window.supports_subtree_effects() {
        return;
    }
    let scale = zoom * window.raster_scale_factor();
    for shadow in shadows.iter().rev().filter(|s| s.visible()) {
        let mut passes = Vec::with_capacity(5);
        let mut pass = |shader: &'static str, slots: &[[f32; 4]]| {
            let mut uniforms = EffectUniforms::default();
            for (i, slot) in slots.iter().enumerate() {
                uniforms.set_slot(i, *slot);
            }
            passes.push(SubtreeEffectPass {
                shader: EffectShader::wgsl_image(shader),
                uniforms,
                time: 0.,
                images: Default::default(),
                bloom: None,
                feedback: None,
                distance_field: None,
                particles: None,
                particle_transition: None,
            });
        };
        if shadow.spread != 0. {
            for axis in [[1., 0.], [0., 1.]] {
                pass(
                    include_str!("spread.wgsl"),
                    &[[axis[0], axis[1], shadow.spread * scale, 0.]],
                );
            }
        }
        if shadow.blur > 0. {
            for axis in [[1., 0.], [0., 1.]] {
                pass(
                    include_str!("blur.wgsl"),
                    &[[axis[0], axis[1], shadow.blur * 0.5 * scale, 0.]],
                );
            }
        }
        pass(
            include_str!("color.wgsl"),
            &[
                [shadow.x * scale, shadow.y * scale, 0., 0.],
                [
                    shadow.color.r,
                    shadow.color.g,
                    shadow.color.b,
                    shadow.color.a,
                ],
            ],
        );
        window.with_subtree_effect_chain(
            bounds.dilate(px(shadow.padding() * zoom)),
            &passes,
            1.,
            |window| source(window),
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn shadow_shaders_validate_with_the_gpui_image_contract() {
        for source in [
            include_str!("spread.wgsl"),
            include_str!("blur.wgsl"),
            include_str!("color.wgsl"),
        ] {
            let source = gpui::compose_subtree_effect_wgsl(&gpui::EffectShader::wgsl_image(source));
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
