use super::*;
use crate::{
    component_export::{self, Format, Job},
    document::Document,
    i18n::t,
};
use anyhow::{Context as _, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use uic::components::dropdown::{Dropdown, DropdownPlacement, DropdownState, dropdown};

pub(super) struct ExportState {
    pub busy: bool,
    pub status: Option<String>,
    failure: Option<String>,
    scale: u32,
    format: Format,
    format_menu: Entity<DropdownState>,
    menu: Entity<DropdownState>,
}
impl ExportState {
    pub fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        Self {
            busy: false,
            status: None,
            failure: None,
            scale: 1,
            format: Format::Png,
            format_menu: cx.new(|cx| DropdownState::new(window, cx)),
            menu: cx.new(|cx| DropdownState::new(window, cx)),
        }
    }
}

impl Workspace {
    pub(crate) fn is_exporting(&self) -> bool {
        self.export.busy
    }

    pub(crate) fn take_export_failure(&mut self) -> Option<String> {
        self.export.failure.take()
    }

    fn export_bounds(&self, id: usize) -> Option<Rect> {
        let mut r = self.world_rect(id)?;
        if let Some(shape) = self.shapes.iter().find(|s| s.id == id) {
            let width = if shape.stroke.enabled {
                if shape.kind.is_path() || shape.kind.is_polygon() {
                    shape.stroke.width / 2.
                } else {
                    shape.stroke.outset()
                }
            } else {
                0.
            };
            if shape.kind == ShapeKind::Arrow {
                let mut shape = shape.clone();
                shape.rect = r;
                for node in shape.editable_nodes() {
                    let right = (r.x + r.width).max(node.anchor.x);
                    let bottom = (r.y + r.height).max(node.anchor.y);
                    r.x = r.x.min(node.anchor.x);
                    r.y = r.y.min(node.anchor.y);
                    r.width = right - r.x;
                    r.height = bottom - r.y;
                }
            }
            r.x -= width;
            r.y -= width;
            r.width += 2. * width;
            r.height += 2. * width;
        }
        let rotation = self.object_rotation(id);
        let pivot = crate::rotation::center(self.world_rect(id)?);
        let center = crate::rotation::around(crate::rotation::center(r), pivot, rotation);
        r = crate::rotation::bounds(r, rotation);
        r.x = center.x - r.width / 2.;
        r.y = center.y - r.height / 2.;
        r.width = r.width.max(1.);
        r.height = r.height.max(1.);
        Some(r)
    }

    pub(super) fn component_export_jobs(
        &self,
        window: &Window,
        cx: &gpui::App,
    ) -> Result<Vec<Job>> {
        let (json, assets) = self.snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)?;
        let doc = Document::decode(&json)?;
        let json = Arc::new(json);
        let assets = Arc::new(assets);
        let mut jobs = Vec::new();
        for root in self.selection_ids() {
            let ids = self.descendants(&BTreeSet::from([root]));
            let order: Vec<_> = self
                .paint_order()
                .into_iter()
                .filter(|id| ids.contains(id))
                .collect();
            ensure!(!order.is_empty(), "{}", t("export-empty-selection"));
            if let Some(shape) = doc
                .shapes
                .iter()
                .find(|s| s.id == root && s.kind == ShapeKind::Video)
            {
                let asset = doc
                    .assets
                    .iter()
                    .find(|a| a.object == root && !a.fill)
                    .context("Missing video asset")?;
                let source = assets
                    .iter()
                    .find(|a| a.hash == asset.hash)
                    .context("Missing video source")?
                    .clone();
                jobs.push(Job {
                    original: Some(source),
                    name: self.layer_name(root, cx),
                    json: json.clone(),
                    assets: assets.clone(),
                    order,
                    bounds: shape.rect,
                    clips: BTreeMap::new(),
                    text: BTreeMap::new(),
                });
                continue;
            }
            ensure!(
                !doc.shapes
                    .iter()
                    .any(|s| order.contains(&s.id) && s.kind == ShapeKind::Video),
                "{}",
                t("export-video-unsupported")
            );
            let boards: BTreeMap<_, _> = doc
                .boards
                .iter()
                .filter(|b| order.contains(&b.id))
                .map(|b| (b.id, b.rect))
                .collect();
            let clips: BTreeMap<_, _> = doc
                .shapes
                .iter()
                .map(|s| (s.id, s.board))
                .chain(doc.texts.iter().map(|s| (s.id, s.board)))
                .filter(|(id, _)| order.contains(id))
                .filter_map(|(id, parent)| parent.and_then(|p| boards.get(&p)).map(|r| (id, *r)))
                .collect();
            let mut bounds = boards.get(&root).copied();
            if bounds.is_none() {
                for id in &order {
                    if clips.contains_key(id) {
                        continue;
                    }
                    if let Some(r) = self.export_bounds(*id) {
                        bounds = Some(bounds.map_or(r, |b| {
                            let x = b.x.min(r.x);
                            let y = b.y.min(r.y);
                            Rect {
                                x,
                                y,
                                width: (b.x + b.width).max(r.x + r.width) - x,
                                height: (b.y + b.height).max(r.y + r.height) - y,
                            }
                        }));
                    }
                }
            }
            let text = doc
                .texts
                .iter()
                .filter(|t| order.contains(&t.id))
                .map(|t| {
                    (
                        t.id,
                        crate::text::export_fragments(&t.content, &t.styles, t.rect, window),
                    )
                })
                .collect();
            jobs.push(Job {
                original: None,
                name: self.layer_name(root, cx),
                json: json.clone(),
                assets: assets.clone(),
                order,
                bounds: bounds.ok_or_else(|| anyhow::anyhow!(t("export-empty-selection")))?,
                clips,
                text,
            });
        }
        ensure!(!jobs.is_empty(), "{}", t("export-empty-selection"));
        Ok(jobs)
    }

    pub(super) fn export_selection(
        &mut self,
        format: Format,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.export.busy {
            return;
        }
        self.suspend(window, cx);
        let jobs = match self.component_export_jobs(window, cx) {
            Ok(jobs) => jobs,
            Err(error) => {
                self.export.status = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let filename = match jobs[0].output_name(format) {
            Ok(name) => name,
            Err(error) => {
                self.export.status = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let extension = jobs[0].extension(format).unwrap();
        let batch = jobs.len() > 1;
        let scale = self.export.scale;
        self.export.busy = true;
        self.export.status = None;
        self.export.failure = None;
        self.export
            .menu
            .update(cx, |menu, cx| menu.close(window, cx));
        self.export
            .format_menu
            .update(cx, |menu, cx| menu.close(window, cx));
        let directory = dirs::document_dir().unwrap_or_else(std::env::temp_dir);
        let file_dialog = (!batch).then(|| cx.prompt_for_new_path(&directory, Some(&filename)));
        let folder_dialog = batch.then(|| {
            cx.prompt_for_paths(gpui::PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some(t("export-folder").into()),
            })
        });
        cx.spawn_in(window, async move |this, cx| {
            let result: Result<Option<std::path::PathBuf>> = if let Some(dialog) = file_dialog {
                dialog
                    .await
                    .map_err(anyhow::Error::from)
                    .and_then(std::convert::identity)
            } else {
                folder_dialog
                    .unwrap()
                    .await
                    .map_err(anyhow::Error::from)
                    .and_then(|result| {
                        result.map(|paths| paths.and_then(|paths| paths.into_iter().next()))
                    })
            };
            let result = match result {
                Ok(Some(mut path)) => {
                    if !batch && path.extension().is_none() {
                        path.set_extension(&extension);
                    }
                    cx.background_executor().spawn(async move {
                        component_export::write(jobs, path, format, scale, batch)
                    }).await.map(Some)
                }
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                this.export.busy = false;
                this.export.status = match result {
                    Ok(Some(paths)) => Some(crate::i18n::count("export-complete", paths.len())),
                    Ok(None) => None,
                    Err(error) => {
                        let message =
                            crate::i18n::message("export-failed", &[("error", error.to_string())]);
                        this.export.failure = Some(message.clone());
                        Some(message)
                    }
                };
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn export_video_count(&self) -> usize {
        let ids = self.selection_ids();
        self.shapes
            .iter()
            .filter(|s| ids.contains(&s.id) && s.kind == ShapeKind::Video)
            .count()
    }

    fn video_export_label(&self) -> String {
        let ids = self.selection_ids();
        let formats: BTreeSet<_> = self
            .shapes
            .iter()
            .filter(|s| ids.contains(&s.id) && s.kind == ShapeKind::Video)
            .filter_map(|s| s.media.as_ref())
            .filter_map(|asset| asset.path.extension())
            .map(|ext| ext.to_string_lossy().to_uppercase())
            .collect();
        if formats.len() == 1 {
            formats.into_iter().next().unwrap()
        } else {
            t("export-original").into()
        }
    }

    pub(super) fn export_controls(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.export.busy;
        let enabled = !busy && !self.selection_ids().is_empty();
        let videos = self.export_video_count();
        let only_videos = videos > 0 && videos == self.selection_ids().len();
        let selected_format = self.export.format;
        let format =
            export_menu(&self.export.format_menu)
                .trigger(export_trigger("export-format", selected_format.label()).w_full())
                .menu(div().flex().flex_col().children(
                    [Format::Png, Format::Svg].into_iter().map(|format| {
                        export_choice(format.label(), format.label(), selected_format == format)
                            .debug_selector(move || format!("export-format-{}", format.extension()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.export.format = format;
                                this.export
                                    .format_menu
                                    .update(cx, |menu, cx| menu.close(window, cx));
                                this.export
                                    .menu
                                    .update(cx, |menu, cx| menu.close(window, cx));
                                cx.notify();
                            }))
                    }),
                ));
        let scale = export_menu(&self.export.menu)
            .trigger(export_trigger("export-scale", format!("{}×", self.export.scale)).w(px(62.)))
            .menu(
                div()
                    .flex()
                    .flex_col()
                    .children([1, 2, 4].into_iter().map(|scale| {
                        export_choice(
                            ("export-scale-choice", scale as usize),
                            format!("{scale}×"),
                            self.export.scale == scale,
                        )
                        .debug_selector(move || format!("export-scale-{scale}"))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.export.scale = scale;
                                this.export
                                    .menu
                                    .update(cx, |menu, cx| menu.close(window, cx));
                                cx.notify();
                            },
                        ))
                    })),
            );
        div()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(10.))
            .p(px(14.))
            .border_t_1()
            .border_color(rgb(BORDER))
            .text_size(px(12.))
            .text_color(rgb(TEXT))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(gpui::rgba(0xffffff04))
                            .when(only_videos, |el| {
                                el.child(
                                    div()
                                        .id("export-video-format")
                                        .debug_selector(|| "export-video-format".into())
                                        .h(px(30.))
                                        .px(px(10.))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .child(icon(LucideIcons::Film, 13.).text_color(rgb(MUTED)))
                                        .child(self.video_export_label()),
                                )
                            })
                            .when(!only_videos, |el| {
                                el.child(div().flex_1().min_w_0().child(format))
                            })
                            .when(!only_videos && selected_format == Format::Png, |el| {
                                el.child(div().w(px(1.)).h(px(14.)).bg(rgb(BORDER)))
                                    .child(scale)
                            }),
                    )
                    .child(
                        div()
                            .id("export-submit")
                            .debug_selector(|| "export-submit".into())
                            .w(px(80.))
                            .h(px(32.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(gpui::rgba(0xb4a2ee35))
                            .bg(gpui::rgba(0xb4a2ee18))
                            .text_color(rgb(ACCENT))
                            .font_weight(FontWeight::MEDIUM)
                            .opacity(if enabled { 1. } else { 0.35 })
                            .child(t("export-action"))
                            .when(enabled, |el| {
                                el.cursor_pointer()
                                    .hover(|s| {
                                        s.bg(gpui::rgba(0xb4a2ee30))
                                            .border_color(gpui::rgba(0xb4a2ee65))
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.export_selection(selected_format, window, cx)
                                    }))
                            }),
                    ),
            )
            .when(videos > 0 && !only_videos, |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(t("export-video-original-note")),
                )
            })
            .when(busy, |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(MUTED))
                        .child(t("export-progress")),
                )
            })
            .when_some(self.export.status.clone(), |el, status| {
                el.child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(11.))
                                .text_color(rgb(MUTED))
                                .child(status),
                        )
                        .child(
                            div()
                                .id("export-dismiss")
                                .cursor_pointer()
                                .child(icon(LucideIcons::X, 13.))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.export.status = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
    }
}

fn export_menu(state: &Entity<DropdownState>) -> Dropdown {
    dropdown(state)
        .placement(DropdownPlacement::TopStart)
        .priority(1100)
        .w(px(120.))
        .min_w(px(120.))
        .p(px(4.))
        .rounded(px(8.))
        .shadow_lg()
        .bg(rgb(0x252730))
        .border_color(rgb(BORDER))
        .text_size(px(12.))
        .text_color(rgb(TEXT))
}

fn export_trigger(id: &'static str, label: impl Into<gpui::SharedString>) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .h(px(30.))
        .px(px(10.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .rounded(px(5.))
        .cursor_pointer()
        .hover(|s| s.bg(gpui::rgba(0xffffff08)))
        .child(label.into())
        .child(icon(LucideIcons::ChevronDown, 11.).text_color(rgb(MUTED)))
}

fn export_choice(
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    selected: bool,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .h(px(28.))
        .px(px(8.))
        .flex()
        .items_center()
        .justify_between()
        .rounded(px(4.))
        .cursor_pointer()
        .hover(|s| s.bg(gpui::rgba(0xb4a2ee18)))
        .child(label.into())
        .child(
            icon(LucideIcons::Check, 12.)
                .text_color(rgb(ACCENT))
                .opacity(if selected { 1. } else { 0. }),
        )
}

#[cfg(test)]
mod tests;
