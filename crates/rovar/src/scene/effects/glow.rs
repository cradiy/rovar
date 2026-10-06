use super::{Effect, MAX_RADIUS};
use gpui::{Bounds, EffectShader, EffectUniforms, Pixels, Rgba, SubtreeEffectPass, Window, px};

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Glow {
    pub enabled: bool,
    #[serde(with = "crate::document::rgba")]
    pub color: Rgba,
    pub radius: f32,
    pub edge_width: f32,
    pub intensity: f32,
    pub threshold: f32,
}

impl Default for Glow {
    fn default() -> Self {
        Self {
            enabled: true,
            color: gpui::rgb(0x76deff),
            radius: 18.,
            edge_width: 1.5,
            intensity: 1.3,
            threshold: 0.5,
        }
    }
}

impl Glow {
    pub fn visible(&self) -> bool {
        self.enabled && self.radius > 0. && self.intensity > 0. && self.color.a > 0.
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            (0. ..=MAX_RADIUS).contains(&self.radius)
                && (0. ..=self.radius).contains(&self.edge_width)
                && (0. ..=4.).contains(&self.intensity)
                && (0.001..=0.999).contains(&self.threshold)
                && [self.color.r, self.color.g, self.color.b, self.color.a]
                    .into_iter()
                    .all(|v| (0. ..=1.).contains(&v)),
            "Invalid contour glow"
        );
        Ok(())
    }

    pub fn pass(&self, scale: f32) -> SubtreeEffectPass {
        SubtreeEffectPass {
            shader: gpui_effects::subtree_identity_shader(),
            uniforms: EffectUniforms::new()
                .with_slot(0, [self.color.r, self.color.g, self.color.b, self.color.a])
                .with_slot(1, [self.radius * scale, self.edge_width * scale, 0., 0.])
                .with_slot(2, [self.intensity, 0., 0., 0.]),
            distance_field: Some(gpui::SubtreeDistanceFieldPass {
                composite: EffectShader::wgsl_two_images(include_str!("glow.wgsl")),
                threshold: self.threshold,
            }),
            time: 0.,
            images: Default::default(),
            bloom: None,
            feedback: None,
            particles: None,
            particle_transition: None,
        }
    }

    /// GPUI contour-light falloff, evaluated in document units for export.
    pub fn alpha(&self, distance: f32, pixel_size: f32) -> f32 {
        if !self.visible() || distance >= self.radius {
            return 0.;
        }
        let smooth = |a: f32, b: f32, v: f32| {
            let t = ((v - a) / (b - a)).clamp(0., 1.);
            t * t * (3. - 2. * t)
        };
        let core = 1. - smooth(0., self.edge_width.max(pixel_size * 0.5), distance);
        let spread = distance / self.radius;
        let halo = (-4. * spread * spread).exp() * (1. - smooth(0.7, 1., spread));
        (1. - (-self.intensity * (core * 1.8 + halo * 0.45)).exp()) * self.color.a
    }
}

pub fn padding(effects: &[Effect]) -> f32 {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Glow(g) if g.visible() => Some(g.radius),
            _ => None,
        })
        .fold(0., f32::max)
}

pub fn paint(
    bounds: Bounds<Pixels>,
    effects: &[Effect],
    zoom: f32,
    window: &mut Window,
    mut source: impl FnMut(&mut Window),
) {
    if !window.supports_subtree_effects() {
        return;
    }
    for effect in effects.iter().rev() {
        if let Effect::Glow(g) = effect
            && g.visible()
        {
            let pass = g.pass(zoom * window.raster_scale_factor());
            window.with_subtree_effect_chain(
                bounds.dilate(px((g.radius + 2.) * zoom)),
                &[pass],
                1.,
                &mut source,
            );
        }
    }
}
