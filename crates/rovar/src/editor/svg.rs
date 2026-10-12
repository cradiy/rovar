use super::*;
use crate::document::Page;
use crate::scene::layer::LayerGroup;
#[cfg(test)]
mod tests;
use crate::{
    asset::{AssetContent, FileAsset},
    i18n::t,
    ui::theme::Color,
};
use std::sync::Arc;

pub(in crate::editor) struct Pending {
    pub asset: Arc<FileAsset>,
    pub page: String,
    pub rect: Rect,
}

#[derive(Default)]
pub(super) struct State {
    pub pending: Option<Pending>,
    pub loading: bool,
    pub error: Option<String>,
    request: u64,
    focus: Option<gpui::FocusHandle>,
}

impl Workspace {
    pub(super) fn svg_properties(&self) -> Div {
        let Some(asset) = self.selected_shape().and_then(|s| s.asset.as_ref()) else {
            return div();
        };
        inspector::inspector_section(t("svg-source")).child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    crate::scene::image_fill::ImageFill {
                        asset: Some(asset.clone()),
                        fit: crate::scene::image_fill::ImageFit::Contain,
                        ..Default::default()
                    }
                    .element()
                    .size(px(40.)),
                )
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(12.))
                        .truncate()
                        .child(asset.name()),
                ),
        )
    }

    pub(super) fn import_svg_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.import_svg_at(None, window, cx);
    }

    pub(super) fn import_svg_at(
        &mut self,
        origin: Option<Point<f32>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.finish_bezier(cx);
        self.exit_vector_edit(cx);
        self.draw_tool = None;
        self.seal_text_edits(cx);
        self.svg.request += 1;
        self.svg.pending = None;
        self.svg.error = None;
        self.svg.focus = Some(cx.focus_handle());
        let request = self.svg.request;
        let page = self.pages.active.clone();
        let view = self.view;
        let size = self.bounds.get().size.map(f32::from);
        let (left, right) = self.canvas_insets();
        let dialog = crate::platform::prompt_for_paths(
            cx,
            gpui::PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(t("svg-import").into()),
            },
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = dialog.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            if this
                .update_in(cx, |this, window, cx| {
                    if this.svg.request != request {
                        return false;
                    }
                    this.svg.loading = true;
                    this.svg.focus.as_ref().unwrap().focus(window, cx);
                    cx.notify();
                    true
                })
                .ok()
                != Some(true)
            {
                return;
            }
            let result = if crate::svg::is_svg(&path) {
                FileAsset::load_async(path, cx.background_executor()).await
            } else {
                Err(anyhow::anyhow!("Choose an SVG file"))
            };
            let _ = this.update_in(cx, |this, window, cx| {
                if this.svg.request != request {
                    return;
                }
                this.svg.loading = false;
                match result {
                    Ok(asset) => {
                        let Some(AssetContent::Svg(svg)) = asset.content() else {
                            return;
                        };
                        let [width, height] = svg.size;
                        let scale = (0.8 * (size.width - left - right).max(1.) / view.zoom / width)
                            .min(0.8 * (size.height - 160.).max(1.) / view.zoom / height)
                            .min(320. / view.zoom / width.max(height))
                            .min(1.);
                        let center =
                            view.world(point((left + size.width - right) / 2., size.height / 2.));
                        let rect = Rect {
                            x: origin.map_or(center.x - width * scale / 2., |p| p.x),
                            y: origin.map_or(center.y - height * scale / 2., |p| p.y),
                            width: width * scale,
                            height: height * scale,
                        };
                        this.import_svg(Pending { asset, page, rect }, window, cx);
                    }
                    Err(error) => this.svg.error = Some(error.to_string()),
                }
                if this.svg.error.is_some() {
                    this.svg.focus.as_ref().unwrap().focus(window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
    pub(super) fn import_svg(
        &mut self,
        pending: Pending,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(pending.asset.content(), Some(AssetContent::Svg(svg)) if svg.editable.is_ok()) {
            self.svg.pending = Some(pending);
            self.svg
                .focus
                .get_or_insert_with(|| cx.focus_handle())
                .focus(window, cx);
            cx.notify();
        } else {
            self.insert_svg(pending, false, window, cx);
        }
    }

    fn insert_svg(
        &mut self,
        pending: Pending,
        convert: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(AssetContent::Svg(svg)) = pending.asset.content() else {
            return;
        };
        let active = self.pages.active == pending.page;
        let Some(mut page) = (if active {
            Some(self.snapshot_page(cx).0)
        } else {
            self.pages
                .entries
                .iter()
                .find(|p| p.page.id == pending.page)
                .map(|p| p.page.clone())
        }) else {
            return;
        };
        let before = self.page_edit(std::slice::from_ref(&pending.page), cx);
        let (board, rect) = if active {
            self.parent_for_rect(None, pending.rect)
        } else {
            (None, pending.rect)
        };
        let root = if let Some(nodes) = svg.editable.as_ref().ok().filter(|_| convert) {
            let root = add_group(&mut page, &pending.asset.name(), 1., board, None);
            insert_nodes(&mut page, nodes, board, root, rect, svg.size);
            root
        } else {
            let id = page.next_id;
            page.next_id += 1;
            let mut shape = Shape::new(id, board, ShapeKind::Svg, rect);
            shape.name = pending.asset.name();
            shape.fill_enabled = false;
            shape.layer.aspect_locked = true;
            shape.asset = Some(pending.asset.clone());
            page.shapes.push(shape);
            page.assets.push(crate::document::AssetUse {
                object: id,
                fill: false,
                hash: pending.asset.hash.clone(),
            });
            id
        };
        if active {
            self.apply_component_page(page, window, cx);
            self.set_selection(std::collections::BTreeSet::from([root]), cx);
            self.focus.focus(window, cx);
        } else if let Some(state) = self
            .pages
            .entries
            .iter_mut()
            .find(|p| p.page.id == pending.page)
        {
            state.page = page;
        }
        self.record_page_edit(before);
        cx.notify();
    }

    pub(super) fn svg_status(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let convertible = self.svg.pending.is_some();
        let reason = if convertible {
            t("svg-import-convert-description").into()
        } else {
            self.svg
                .error
                .clone()
                .unwrap_or_else(|| t("svg-import-loading").into())
        };
        div()
            .id("svg-import-backdrop")
            .occlude()
            .track_focus(self.svg.focus.as_ref().unwrap_or(&self.focus))
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape"
                    || (this.svg.pending.is_some() && event.keystroke.key == "enter")
                {
                    this.svg.request += 1;
                    this.svg.loading = false;
                    this.svg.error = None;
                    this.focus.focus(window, cx);
                    if let Some(pending) = this.svg.pending.take() {
                        this.insert_svg(pending, false, window, cx);
                    }
                    cx.notify();
                }
                cx.stop_propagation();
                window.prevent_default();
            }))
            .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(Color::Overlay.color())
            .child(
                div()
                    .id("svg-import-status")
                    .debug_selector(|| "svg-import-status".into())
                    .occlude()
                    .w(px(420.))
                    .p(px(20.))
                    .rounded(px(12.))
                    .bg(Color::Panel.color())
                    .border_1()
                    .border_color(BORDER.color())
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .text_size(px(12.))
                    .text_color(TEXT.color())
                    .on_any_mouse_down(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t(if convertible {
                                "svg-import-convert-title"
                            } else {
                                "svg-import"
                            })),
                    )
                    .children(
                        self.svg
                            .pending
                            .as_ref()
                            .map(|p| div().truncate().child(p.asset.name())),
                    )
                    .child(div().text_color(MUTED.color()).child(reason))
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .child(
                                div()
                                    .id(if convertible {
                                        "svg-import-material"
                                    } else {
                                        "svg-import-cancel"
                                    })
                                    .debug_selector(move || {
                                        if convertible {
                                            "svg-import-material"
                                        } else {
                                            "svg-import-cancel"
                                        }
                                        .into()
                                    })
                                    .px(px(12.))
                                    .h(px(32.))
                                    .rounded(px(6.))
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .bg(Color::Input.color())
                                    .child(t(if convertible {
                                        "svg-import-keep-material"
                                    } else if self.svg.loading {
                                        "cancel"
                                    } else {
                                        "close"
                                    }))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.svg.request += 1;
                                        this.svg.loading = false;
                                        this.svg.error = None;
                                        this.focus.focus(window, cx);
                                        if let Some(pending) = this.svg.pending.take() {
                                            this.insert_svg(pending, false, window, cx);
                                        }
                                        cx.notify();
                                    })),
                            )
                            .when(convertible, |el| {
                                el.child(
                                    div()
                                        .id("svg-import-convert")
                                        .debug_selector(|| "svg-import-convert".into())
                                        .px(px(12.))
                                        .h(px(32.))
                                        .rounded(px(6.))
                                        .flex()
                                        .items_center()
                                        .cursor_pointer()
                                        .bg(Color::Selected.color())
                                        .child(t("svg-import-convert"))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            if let Some(pending) = this.svg.pending.take() {
                                                this.insert_svg(pending, true, window, cx);
                                            }
                                            cx.notify();
                                        })),
                                )
                            }),
                    ),
            )
    }
}

fn add_group(
    page: &mut Page,
    name: &str,
    opacity: f32,
    board: Option<usize>,
    parent: Option<usize>,
) -> usize {
    let id = page.next_id;
    page.next_id += 1;
    page.hierarchy.groups.insert(
        id,
        LayerGroup {
            uid: uuid::Uuid::new_v4(),
            name: if name.is_empty() {
                "SVG group".into()
            } else {
                name.into()
            },
            board,
            boolean: None,
            mask: None,
            layer: crate::scene::layer::LayerState {
                opacity,
                ..Default::default()
            },
        },
    );
    if let Some(parent) = parent {
        page.hierarchy.parents.insert(id, parent);
    }
    page.hierarchy.order.push(id);
    id
}

fn insert_nodes(
    page: &mut Page,
    nodes: &[rovar_svg::Node],
    board: Option<usize>,
    parent: usize,
    rect: Rect,
    size: [f32; 2],
) {
    for node in nodes {
        match node {
            rovar_svg::Node::Group {
                name,
                opacity,
                children,
            } => {
                let id = add_group(page, name, *opacity, board, Some(parent));
                insert_nodes(page, children, board, id, rect, size);
            }
            rovar_svg::Node::Path {
                name,
                color,
                points,
                closed,
                stroke_width,
            } => {
                let id = page.next_id;
                page.next_id += 1;
                let mut shape = Shape::new(id, board, ShapeKind::Bezier, rect);
                if !name.is_empty() {
                    shape.name = name.clone();
                }
                let transform = |p: [f32; 2]| {
                    point(
                        rect.x + p[0] * rect.width / size[0],
                        rect.y + p[1] * rect.height / size[1],
                    )
                };
                let nodes: Vec<_> = points
                    .iter()
                    .map(|p| crate::scene::bezier::Node {
                        anchor: transform(p.anchor),
                        incoming: transform(p.incoming),
                        outgoing: transform(p.outgoing),
                        smooth: false,
                    })
                    .collect();
                shape.set_bezier(&nodes, *closed);
                shape.color = gpui::Rgba {
                    r: color[0],
                    g: color[1],
                    b: color[2],
                    a: color[3],
                };
                if let Some(width) = stroke_width {
                    shape.fill_enabled = false;
                    shape.stroke.enabled = true;
                    shape.stroke.align = crate::scene::shape::StrokeAlign::Center;
                    shape.stroke.width = width * rect.width / size[0];
                    shape.stroke.color = shape.color;
                }
                page.shapes.push(shape);
                page.hierarchy.parents.insert(id, parent);
                page.hierarchy.order.push(id);
            }
        }
    }
}
