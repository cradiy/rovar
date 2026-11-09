use super::*;
use gpui::{
    AnyElement, ContentMask, EffectShader, EffectUniforms, Element, ElementId, GlobalElementId,
    HitboxBehavior, InspectorElementId, LayoutId, SubtreeInput,
};

#[cfg(all(test, not(target_family = "wasm")))]
mod tests;

fn mode(effect: TransitionEffect) -> f32 {
    match effect {
        TransitionEffect::Slide => 0.,
        TransitionEffect::Push => 1.,
        TransitionEffect::ZoomIn => 2.,
        TransitionEffect::ZoomOut => 3.,
        TransitionEffect::Rotate => 4.,
        TransitionEffect::Flip => 5.,
        TransitionEffect::Iris => 6.,
        TransitionEffect::Blinds => 7.,
        TransitionEffect::Pixelate => 8.,
        TransitionEffect::Noise => unreachable!("noise uses the built-in dissolve"),
    }
}

fn shader() -> EffectShader {
    static SHADER: std::sync::OnceLock<EffectShader> = std::sync::OnceLock::new();
    SHADER
        .get_or_init(|| EffectShader::wgsl_two_images(include_str!("effects.wgsl")))
        .clone()
}

fn uniforms(effect: TransitionEffect, direction: WipeDirection, progress: f32) -> EffectUniforms {
    let axis = match direction {
        WipeDirection::Left => [-1., 0., 0., 0.],
        WipeDirection::Right => [1., 0., 0., 0.],
        WipeDirection::Up => [0., -1., 0., 0.],
        WipeDirection::Down => [0., 1., 0., 0.],
    };
    EffectUniforms::new()
        .with_slot(0, [progress, mode(effect), 0., 0.])
        .with_slot(1, axis)
}

/// The same bounded two-subtree capture used by gpui_effects transitions.
/// Only the active interval reaches this element; endpoints paint a single revision.
pub(super) struct Blend {
    inputs: [AnyElement; 2],
    effect: TransitionEffect,
    direction: WipeDirection,
    progress: f32,
}

impl Blend {
    pub fn new(
        from: impl IntoElement,
        to: impl IntoElement,
        effect: TransitionEffect,
        direction: WipeDirection,
        progress: f32,
    ) -> Self {
        Self {
            inputs: [
                div()
                    .id("from")
                    .absolute()
                    .inset_0()
                    .size_full()
                    .child(from)
                    .into_any_element(),
                div()
                    .id("to")
                    .absolute()
                    .inset_0()
                    .size_full()
                    .child(to)
                    .into_any_element(),
            ],
            effect,
            direction,
            progress,
        }
    }
}

impl IntoElement for Blend {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Blend {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let children = self
            .inputs
            .iter_mut()
            .map(|input| input.request_layout(window, cx))
            .collect::<Vec<_>>();
        let style = gpui::Style {
            size: gpui::size(gpui::relative(1.).into(), gpui::relative(1.).into()),
            ..Default::default()
        };
        (window.request_layout(style, children, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.prepaint_subtree_effect(|window| {
                for input in &mut self.inputs {
                    input.prepaint(window, cx);
                }
            });
            window.insert_hitbox(bounds, HitboxBehavior::BlockMouse);
        });
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.with_subtree_pair(
                bounds,
                shader(),
                uniforms(self.effect, self.direction, self.progress),
                0.,
                1.,
                |input, window| {
                    self.inputs[usize::from(input == SubtreeInput::Second)].paint(window, cx);
                },
            );
        });
    }
}
