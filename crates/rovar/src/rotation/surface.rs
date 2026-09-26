use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, PointerTransform, Window, px,
};

/// Capture only this object, including its handles. Identity objects bypass the
/// capture entirely. Pointer mapping also runs on the headless test platform.
pub fn surface<E: IntoElement>(
    element: E,
    angle: f32,
    width: f32,
    height: f32,
    overflow: f32,
) -> impl IntoElement {
    let (s, c) = angle.to_radians().sin_cos();
    let extent = (c.abs() * width + s.abs() * height - width)
        .max(s.abs() * width + c.abs() * height - height)
        .max(0.)
        / 2.;
    let padding = px((extent + 36.).max(overflow));
    let inner = gpui_effects::subtree_effect(
        element,
        gpui::EffectShader::wgsl_image(include_str!("rotate.wgsl")),
    )
    .uniform(0, [s, c, 0., 0.])
    .capture_padding(padding)
    .enabled(angle != 0.);
    PointerScope {
        inner,
        angle,
        padding,
    }
}

struct PointerScope<E: Element> {
    inner: E,
    angle: f32,
    padding: Pixels,
}
impl<E: Element> IntoElement for PointerScope<E> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl<E: Element> Element for PointerScope<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;
    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        self.inner.source_location()
    }
    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let angle = self.angle;
        if angle == 0. {
            return self
                .inner
                .prepaint(id, inspector, bounds, state, window, cx);
        }
        let pivot = bounds.center();
        let transform = PointerTransform::new(move |p, _, _| super::pixels(p, pivot, -angle));
        window.with_pointer_transform(bounds.dilate(self.padding), transform, |window| {
            self.inner
                .prepaint(id, inspector, bounds, state, window, cx)
        })
    }
    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let angle = self.angle;
        if angle == 0. {
            self.inner
                .paint(id, inspector, bounds, state, prepaint, window, cx);
            return;
        }
        let pivot = bounds.center();
        let transform = PointerTransform::new(move |p, _, _| super::pixels(p, pivot, -angle));
        window.with_pointer_transform(bounds.dilate(self.padding), transform, |window| {
            self.inner
                .paint(id, inspector, bounds, state, prepaint, window, cx)
        });
    }
}
