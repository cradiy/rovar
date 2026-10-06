use crate::scene::layer::LayerState;
use gpui::*;

pub(super) fn shader() -> EffectShader {
    EffectShader::wgsl_two_images(include_str!("blend.wgsl"))
}

pub(in crate::editor) type Layer = (
    LayerState,
    Vec<AnyElement>,
    Option<crate::scene::effects::backdrop::Filter>,
);

/// Each non-default layer composites against preceding artwork in its scope.
/// The editor background and deferred selection controls are outside that scope.
pub(in crate::editor) fn compose(layers: impl IntoIterator<Item = Layer>) -> Vec<AnyElement> {
    let mut result = Vec::new();
    for (state, elements, backdrop) in layers {
        if let Some(backdrop) = backdrop.filter(|_| !result.is_empty()) {
            let foreground = div()
                .absolute()
                .inset_0()
                .children(std::mem::take(&mut result))
                .into_any_element();
            result.push(
                Blended {
                    background: None,
                    foreground,
                    state: LayerState::default(),
                    backdrop: Some(backdrop),
                }
                .into_any_element(),
            );
        }
        if elements.is_empty() {
            continue;
        }
        if !state.composited() {
            result.extend(elements);
            continue;
        }
        let background = (!state.blend.is_normal() && !result.is_empty()).then(|| {
            div()
                .absolute()
                .inset_0()
                .children(std::mem::take(&mut result))
                .into_any_element()
        });
        let foreground = div()
            .absolute()
            .inset_0()
            .children(elements)
            .into_any_element();
        result.push(
            Blended {
                background,
                foreground,
                state,
                backdrop: None,
            }
            .into_any_element(),
        );
    }
    result
}

struct Blended {
    background: Option<AnyElement>,
    foreground: AnyElement,
    state: LayerState,
    backdrop: Option<crate::scene::effects::backdrop::Filter>,
}

impl IntoElement for Blended {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Blended {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut children: Vec<_> = self
            .background
            .iter_mut()
            .map(|el| el.request_layout(window, cx))
            .collect();
        children.push(self.foreground.request_layout(window, cx));
        let style = Style {
            position: Position::Absolute,
            size: size(relative(1.).into(), relative(1.).into()),
            ..Default::default()
        };
        (window.request_layout(style, children, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(background) = &mut self.background {
            window.prepaint_subtree_effect(|window| {
                background.prepaint(window, cx);
            });
        }
        window.prepaint_subtree_effect(|window| {
            self.foreground.prepaint(window, cx);
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
        let capture = bounds.intersect(&window.content_mask().bounds);
        if capture.is_empty() {
            return;
        }
        if let Some(backdrop) = self.backdrop {
            let pass = backdrop.pass(bounds.origin, capture, window.raster_scale_factor());
            window.with_subtree_effect_chain(capture, &[pass], 1., |window| {
                self.foreground.paint(window, cx)
            });
            return;
        }
        if self.background.is_none() || !window.supports_subtree_effects() {
            if let Some(background) = &mut self.background {
                background.paint(window, cx);
            }
            window.with_subtree_effect_chain(capture, &[], self.state.opacity, |window| {
                self.foreground.paint(window, cx)
            });
            return;
        }
        window.with_subtree_pair(
            capture,
            shader(),
            EffectUniforms::default().with_slot(
                0,
                [self.state.blend as u32 as f32, self.state.opacity, 0., 0.],
            ),
            0.,
            1.,
            |input, window| match input {
                SubtreeInput::First => self.background.as_mut().unwrap().paint(window, cx),
                SubtreeInput::Second => self.foreground.paint(window, cx),
            },
        );
    }
}
