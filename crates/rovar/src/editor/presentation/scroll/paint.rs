use super::*;
use gpui::{App, ContentMask, Element, ElementId, GlobalElementId, InspectorElementId, LayoutId};

#[derive(Clone, Copy)]
pub(in crate::editor::presentation) struct Bar {
    pub container: usize,
    pub vertical: bool,
    pub bounds: Bounds<f32>,
    pub scale: f32,
}

impl Workspace {
    pub(in crate::editor) fn playback_scrolled_element(
        &self,
        id: usize,
        element: gpui::AnyElement,
    ) -> gpui::AnyElement {
        let Some(node) = self
            .presentation
            .scroll
            .nodes
            .get(&id)
            .filter(|_| self.presentation.scroll.active)
        else {
            return element;
        };
        if node.offset == point(0., 0.) && node.clip.is_none() {
            return element;
        }
        let view = self.view;
        Surface {
            child: div().absolute().inset_0().child(element).into_any_element(),
            offset: node.offset.map(|v| px(-v * view.zoom)),
            clip: node.clip.map(|clip| {
                Bounds::new(
                    view.screen(clip.origin).map(px),
                    clip.size.map(|v| px(v * view.zoom)),
                )
            }),
        }
        .into_any_element()
    }

    fn playback_bars(&self, root: usize, zoom: f32) -> Vec<Bar> {
        let padding = 3. / zoom;
        let mut bars = Vec::new();
        let order = self
            .presentation
            .scroll
            .index
            .as_ref()
            .map(|index| index.order.as_slice())
            .unwrap_or_default();
        for &id in order {
            let Some(node) = self.presentation.scroll.nodes.get(&id) else {
                continue;
            };
            if id != root && !node.ancestors.contains(&root) {
                continue;
            }
            let Some(range) = node.range else {
                continue;
            };
            let viewport = node.displayed();
            for vertical in [false, true] {
                let (min, max, offset, size) = if vertical {
                    (
                        range.min.y,
                        range.max.y,
                        self.presentation.scroll.offsets[&id].y,
                        viewport.size.height,
                    )
                } else {
                    (
                        range.min.x,
                        range.max.x,
                        self.presentation.scroll.offsets[&id].x,
                        viewport.size.width,
                    )
                };
                if max <= min || size <= padding * 4. {
                    continue;
                }
                let track = size - padding * 2.;
                let length = (track * size / (size + max - min))
                    .max(24. / zoom)
                    .min(track * 0.85);
                let scale = (max - min) / (track - length);
                let position = padding + (offset - min) / scale;
                let rect = if vertical {
                    Bounds::new(
                        viewport.origin
                            + point(viewport.size.width - padding - 5. / zoom, position),
                        gpui::size(5. / zoom, length),
                    )
                } else {
                    Bounds::new(
                        viewport.origin
                            + point(position, viewport.size.height - padding - 5. / zoom),
                        gpui::size(length, 5. / zoom),
                    )
                };
                if node
                    .clip
                    .is_none_or(|clip| !clip.intersect(&rect).is_empty())
                {
                    bars.push(Bar {
                        container: id,
                        vertical,
                        bounds: rect,
                        scale,
                    });
                }
            }
        }
        bars
    }

    pub(in crate::editor::presentation) fn playback_scrollbar(
        &self,
        world: Point<f32>,
    ) -> Option<Bar> {
        let p = self.presentation.playback.as_ref()?;
        let root = p.overlay.map_or(p.current, |o| o.target);
        self.playback_bars(root, self.playback_coordinates().0.zoom)
            .into_iter()
            .rev()
            .find(|bar| {
                let clip = self.presentation.scroll.nodes[&bar.container].clip;
                clip.is_none_or(|clip| clip.contains(&world)) && bar.bounds.contains(&world)
            })
    }

    pub(in crate::editor::presentation) fn playback_scrollbars(
        &self,
        root: usize,
    ) -> Vec<gpui::AnyElement> {
        if !self.presentation.scroll.active {
            return Vec::new();
        }
        self.playback_bars(root, self.view.zoom)
            .into_iter()
            .map(|bar| {
                let node = &self.presentation.scroll.nodes[&bar.container];
                let origin = self.view.screen(bar.bounds.origin);
                let element = div()
                    .debug_selector(move || {
                        format!(
                            "playback-scrollbar-{}-{}",
                            bar.container,
                            if bar.vertical {
                                "vertical"
                            } else {
                                "horizontal"
                            }
                        )
                    })
                    .absolute()
                    .left(px(origin.x))
                    .top(px(origin.y))
                    .w(px(bar.bounds.size.width * self.view.zoom))
                    .h(px(bar.bounds.size.height * self.view.zoom))
                    .rounded_full()
                    .bg(gpui::rgba(0x8c939eaa))
                    .into_any_element();
                Surface {
                    child: div().absolute().inset_0().child(element).into_any_element(),
                    offset: point(px(0.), px(0.)),
                    clip: node.clip.map(|clip| {
                        Bounds::new(
                            self.view.screen(clip.origin).map(px),
                            clip.size.map(|v| px(v * self.view.zoom)),
                        )
                    }),
                }
                .into_any_element()
            })
            .collect()
    }
}

struct Surface {
    child: gpui::AnyElement,
    offset: Point<Pixels>,
    clip: Option<Bounds<Pixels>>,
}

impl IntoElement for Surface {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for Surface {
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
        (self.child.request_layout(window, cx), ())
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
        let mask = self.clip.map(|clip| ContentMask {
            bounds: Bounds::new(bounds.origin + clip.origin, clip.size),
        });
        window.with_content_mask(mask, |window| {
            window.with_element_offset(self.offset, |window| self.child.prepaint(window, cx))
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
        let mask = self.clip.map(|clip| ContentMask {
            bounds: Bounds::new(bounds.origin + clip.origin, clip.size),
        });
        window.with_content_mask(mask, |window| self.child.paint(window, cx));
    }
}
