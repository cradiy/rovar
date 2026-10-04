use super::*;
use crate::ui::theme::Color;
use crate::{
    document::export::Format,
    i18n::t,
    render::export::{self as exporter, Job},
};
use anyhow::{Context as _, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use uic::components::dropdown::{Dropdown, DropdownPlacement, DropdownState, dropdown};
mod controls;

pub(super) struct ExportState {
    pub busy: bool,
    pub status: Option<String>,
    failure: Option<String>,
    controls: controls::Controls,
    status_dismiss: Option<gpui::Task<()>>,
}
impl ExportState {
    pub fn new() -> Self {
        Self {
            busy: false,
            status: None,
            failure: None,
            controls: Default::default(),
            status_dismiss: None,
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
        if let Some(effects) = self.hierarchy.effects.get(&id) {
            r = crate::scene::effects::bounds(r, effects);
        }
        let rotation = self.object_rotation(id);
        let pivot = crate::scene::rotation::center(self.world_rect(id)?);
        let center =
            crate::scene::rotation::around(crate::scene::rotation::center(r), pivot, rotation);
        r = crate::scene::rotation::bounds(r, rotation);
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
        self.export_jobs(self.selection_ids(), window, cx)
    }

    fn export_jobs(
        &self,
        roots: BTreeSet<usize>,
        window: &Window,
        cx: &gpui::App,
    ) -> Result<Vec<Job>> {
        let (doc, assets) = self.snapshot_page(cx);
        let json = Arc::new(serde_json::to_vec(&doc)?);
        let assets = Arc::new(assets);
        let mut jobs = Vec::new();
        for root in roots {
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
                    preset: Default::default(),
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
            let mut bounds = boards.get(&root).and_then(|_| self.export_bounds(root));
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
                        crate::scene::text::export_fragments(
                            &t.content,
                            &t.styles,
                            t.rect,
                            window.text_system(),
                        ),
                    )
                })
                .collect();
            jobs.push(Job {
                preset: Default::default(),
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
        quick_format: Option<Format>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.export.busy {
            return;
        }
        self.export.status_dismiss = None;
        self.suspend(window, cx);
        let jobs = match if let Some(format) = quick_format {
            self.component_export_jobs(window, cx).map(|mut jobs| {
                for job in &mut jobs {
                    job.preset.format = format;
                }
                jobs
            })
        } else {
            self.configured_export_jobs(window, cx)
        } {
            Ok(jobs) => jobs,
            Err(error) => {
                self.export.status = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let format = jobs[0].preset.format;
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
        self.export.busy = true;
        self.export.status = None;
        self.export.failure = None;
        self.close_export_menus(window, cx);
        let directory = crate::platform::export_directory();
        let file_dialog =
            (!batch).then(|| crate::platform::prompt_for_new_path(cx, &directory, Some(&filename)));
        let folder_dialog = batch.then(|| {
            crate::platform::prompt_for_paths(
                cx,
                gpui::PathPromptOptions {
                    files: false,
                    directories: true,
                    multiple: false,
                    prompt: Some(t("export-folder").into()),
                },
            )
        });
        cx.spawn_in(window, async move |this, cx| {
            let result: Result<Option<std::path::PathBuf>> = if let Some(dialog) = file_dialog {
                dialog.await.and_then(std::convert::identity)
            } else {
                folder_dialog.unwrap().await.and_then(|result| {
                    result.map(|paths| paths.and_then(|paths| paths.into_iter().next()))
                })
            };
            let result = match result {
                Ok(Some(mut path)) => {
                    if !batch && path.extension().is_none() {
                        path.set_extension(&extension);
                    }
                    cx.background_executor()
                        .spawn(async move {
                            crate::render::raster::prepare().await?;
                            let paths = exporter::write(jobs, path, batch)?;
                            for path in &paths {
                                crate::platform::download(path)?;
                            }
                            Ok::<_, anyhow::Error>(paths)
                        })
                        .await
                        .map(Some)
                }
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                this.export.busy = false;
                let complete = matches!(&result, Ok(Some(_)));
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
                if complete {
                    this.dismiss_export_status_later(cx);
                }
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

    fn dismiss_export_status_later(&mut self, cx: &mut Context<Self>) {
        self.export.status_dismiss = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(4))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.export.status = None;
                cx.notify();
            });
        }));
    }

    pub(super) fn export_feedback(&self, cx: &mut Context<Self>) -> Div {
        let busy = self.export.busy;
        div()
            .debug_selector(|| "export-footer".into())
            .flex()
            .flex_col()
            .flex_shrink_0()
            .gap(px(10.))
            .p(px(14.))
            .border_t_1()
            .border_color(BORDER.color())
            .text_size(px(12.))
            .text_color(TEXT.color())
            .when(busy, |el| {
                el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(MUTED.color())
                        .child(t("export-progress")),
                )
            })
            .when_some(self.export.status.clone(), |el, status| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(11.))
                                .text_color(MUTED.color())
                                .child(status),
                        )
                        .child(
                            div()
                                .id("export-dismiss")
                                .debug_selector(|| "export-dismiss".into())
                                .size(px(24.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(5.))
                                .cursor_pointer()
                                .hover(|el| el.bg(BORDER.color()))
                                .child(icon(LucideIcons::X, 13.))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.export.status = None;
                                    this.export.status_dismiss = None;
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
        .bg(Color::Input.color())
        .border_color(BORDER.color())
        .text_size(px(12.))
        .text_color(TEXT.color())
}

fn export_trigger(id: String, label: impl Into<gpui::SharedString>) -> gpui::Stateful<Div> {
    div()
        .id(gpui::SharedString::from(id.clone()))
        .debug_selector(move || id.clone())
        .h(px(30.))
        .px(px(10.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .rounded(px(5.))
        .cursor_pointer()
        .hover(|s| s.bg(Color::Text.color().opacity(0.0314)))
        .child(label.into())
        .child(icon(LucideIcons::ChevronDown, 11.).text_color(MUTED.color()))
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
        .hover(|s| s.bg(Color::Accent.color().opacity(0.0941)))
        .child(label.into())
        .child(
            icon(LucideIcons::Check, 12.)
                .text_color(ACCENT.color())
                .opacity(if selected { 1. } else { 0. }),
        )
}

#[cfg(test)]
mod tests;
