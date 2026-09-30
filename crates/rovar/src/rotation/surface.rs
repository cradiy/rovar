use gpui::{
    App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement, LayoutId,
    Pixels, Size, TransformationMatrix, Window, px, size,
};

/// Rotate pixels and input together, leaving room for the object's handles and
/// rotated corners without changing its layout bounds.
pub fn surface<E: IntoElement>(
    element: E,
    angle: f32,
    width: f32,
    height: f32,
    overflow: f32,
) -> impl IntoElement {
    if angle == 0. {
        return element.into_any_element();
    }
    let (s, c) = angle.to_radians().sin_cos();
    let extent = (c.abs() * width + s.abs() * height - width)
        .max(s.abs() * width + c.abs() * height - height)
        .max(0.)
        / 2.;
    let padding = (extent + 36.).max(overflow);
    let x = width / 2. + padding;
    let y = height / 2. + padding;
    let inner = gpui_effects::transform_group(
        element,
        TransformationMatrix {
            rotation_scale: [[c, -s], [s, c]],
            translation: [x - c * x + s * y, y - s * x - c * y],
        },
    );
    RotationCapture {
        inner,
        capture_size: size(px(width + padding * 2.), px(height + padding * 2.)),
    }
    .into_any_element()
}

// The child keeps its original Taffy layout. Only the transform viewport expands,
// so overflow (selection handles, strokes and rotated corners) stays visible.
struct RotationCapture<E: Element> {
    inner: E,
    capture_size: Size<Pixels>,
}
impl<E: Element> RotationCapture<E> {
    fn capture_bounds(&self, bounds: Bounds<Pixels>) -> Bounds<Pixels> {
        // Keep the matrix's local pivot on the resolved layout center, including
        // any device-pixel snapping applied to the authored width and height.
        Bounds::new(
            bounds.center() - (self.capture_size / 2.).into(),
            self.capture_size,
        )
    }
}
impl<E: Element> IntoElement for RotationCapture<E> {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl<E: Element> Element for RotationCapture<E> {
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
        self.inner.prepaint(
            id,
            inspector,
            self.capture_bounds(bounds),
            state,
            window,
            cx,
        )
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
        self.inner.paint(
            id,
            inspector,
            self.capture_bounds(bounds),
            state,
            prepaint,
            window,
            cx,
        );
    }
}
