use crate::scene::{artboard::Viewport, mask::Outline};
use gpui::*;
use std::rc::Rc;

pub(super) struct Masked {
    pub id: usize,
    pub child: AnyElement,
    pub outline: Rc<Outline>,
    pub view: Viewport,
}

impl IntoElement for Masked {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Masked {
    type RequestLayoutState = ();
    type PrepaintState = Option<Path<Pixels>>;
    fn id(&self) -> Option<ElementId> {
        Some(("mask", self.id).into())
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
        (self.child.request_layout(window, cx), ())
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        window.prepaint_subtree_effect(|window| {
            self.child.prepaint(window, cx);
        });
        if self.outline.0.is_empty() {
            return None;
        }
        let mut path = window.with_element_state(
            id.expect("mask has an element id"),
            |cached: Option<(Rc<Outline>, Option<Path<Pixels>>)>, _| {
                let cached = cached
                    .filter(|(outline, _)| {
                        Rc::ptr_eq(outline, &self.outline) || *outline == self.outline
                    })
                    .unwrap_or_else(|| (self.outline.clone(), self.path()));
                (cached.1.clone(), cached)
            },
        )?;
        let offset = bounds.origin + point(px(self.view.pan.x), px(self.view.pan.y));
        path.bounds.origin = path.bounds.origin * self.view.zoom + offset;
        path.bounds.size = path.bounds.size.map(|v| v * self.view.zoom);
        for vertex in &mut path.vertices {
            vertex.xy_position = vertex.xy_position * self.view.zoom + offset;
        }
        Some(path)
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        path: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(path) = path else {
            return;
        };
        let capture = path.bounds.intersect(&window.content_mask().bounds);
        if capture.is_empty() {
            return;
        }
        window.with_subtree_pair(
            capture,
            EffectShader::wgsl_two_images(include_str!("../shapes/image_mask.wgsl")),
            Default::default(),
            0.,
            1.,
            |input, window| match input {
                SubtreeInput::First => window.paint_path(path.clone(), white()),
                SubtreeInput::Second => self.child.paint(window, cx),
            },
        );
    }
}

impl Masked {
    fn path(&self) -> Option<Path<Pixels>> {
        let mut builder = PathBuilder::fill().with_style(PathStyle::Fill(
            FillOptions::default().with_fill_rule(FillRule::EvenOdd),
        ));
        for contour in &self.outline.0 {
            for (i, p) in contour.iter().enumerate() {
                let p = point(px(p.x), px(p.y));
                if i == 0 {
                    builder.move_to(p);
                } else {
                    builder.line_to(p);
                }
            }
            builder.close();
        }
        builder.build().ok()
    }
}
