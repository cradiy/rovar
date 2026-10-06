use super::{Effect, ShadowKind};
use gpui::{Bounds, EffectShader, EffectUniforms, Pixels, SubtreeEffectPass, Window, px};

/// Paint callbacks contain artwork primitives only, never interactive elements.
/// Each shadow starts with the same source alpha, so stacked shadows do not cast
/// shadows on each other. Drop shadows are painted before the original artwork;
/// inner shadows are painted after it.
pub(crate) fn paint_shadows(
    bounds: Bounds<Pixels>,
    effects: &[Effect],
    kind: ShadowKind,
    zoom: f32,
    window: &mut Window,
    mut source: impl FnMut(&mut Window),
) {
    if !window.supports_subtree_effects() {
        return;
    }
    let scale = zoom * window.raster_scale_factor();
    for shadow in effects
        .iter()
        .rev()
        .filter_map(Effect::shadow)
        .filter(|s| s.visible() && s.kind == kind)
    {
        let inner = kind == ShadowKind::Inner;
        let channel = if inner { 1. } else { 0. };
        let mut passes = Vec::with_capacity(6);
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
        if inner {
            pass(include_str!("mask.wgsl"), &[]);
        }
        if shadow.spread != 0. {
            for axis in [[1., 0.], [0., 1.]] {
                pass(
                    include_str!("spread.wgsl"),
                    &[[
                        axis[0],
                        axis[1],
                        shadow.spread * scale * if inner { -1. } else { 1. },
                        channel,
                    ]],
                );
            }
        }
        if shadow.blur > 0. {
            for axis in [[1., 0.], [0., 1.]] {
                pass(
                    include_str!("blur.wgsl"),
                    &[[axis[0], axis[1], shadow.blur * 0.5 * scale, channel]],
                );
            }
        }
        pass(
            include_str!("color.wgsl"),
            &[
                [shadow.x * scale, shadow.y * scale, 0., channel],
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

/// Compose shadows and contour light around the source, then blur the whole layer.
pub(crate) fn paint_effects(
    bounds: Bounds<Pixels>,
    effects: &[Effect],
    zoom: f32,
    window: &mut Window,
    mut source: impl FnMut(&mut Window),
) {
    let radius = super::blur_radius(effects);
    let mut paint = |window: &mut Window| {
        paint_shadows(bounds, effects, ShadowKind::Drop, zoom, window, &mut source);
        super::glow::paint(bounds, effects, zoom, window, &mut source);
        source(window);
        paint_shadows(
            bounds,
            effects,
            ShadowKind::Inner,
            zoom,
            window,
            &mut source,
        );
    };
    if radius == 0. || !window.supports_subtree_effects() {
        paint(window);
        return;
    }
    let sigma = radius * 0.5 * zoom * window.raster_scale_factor();
    let passes = [[1., 0.], [0., 1.]].map(|axis| {
        let mut uniforms = EffectUniforms::default();
        uniforms.set_slot(0, [axis[0], axis[1], sigma, 0.]);
        SubtreeEffectPass {
            shader: EffectShader::wgsl_image(include_str!("layer_blur.wgsl")),
            uniforms,
            time: 0.,
            images: Default::default(),
            bloom: None,
            feedback: None,
            distance_field: None,
            particles: None,
            particle_transition: None,
        }
    });
    window.with_subtree_effect_chain(
        bounds.dilate(px((super::padding(effects) + 2.) * zoom)),
        &passes,
        1.,
        paint,
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn shadow_shaders_validate_with_the_gpui_image_contract() {
        for source in [
            include_str!("spread.wgsl"),
            include_str!("blur.wgsl"),
            include_str!("color.wgsl"),
            include_str!("mask.wgsl"),
            include_str!("layer_blur.wgsl"),
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
