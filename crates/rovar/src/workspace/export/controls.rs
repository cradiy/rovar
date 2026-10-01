use super::*;
use crate::component_export::Preset;
use uic::components::{
    input::{Input, InputAppearance},
    popover::{Popover, PopoverPlacement, PopoverState},
};

#[derive(Default)]
pub(super) struct Controls {
    selection: (String, BTreeSet<usize>),
    rows: Vec<Row>,
    preview_open: bool,
    preview: Option<Arc<gpui::RenderImage>>,
    preview_error: bool,
    preview_revision: Option<u64>,
    preview_task: Option<gpui::Task<()>>,
}

struct Row {
    format: Entity<DropdownState>,
    scale: Entity<DropdownState>,
    options: Entity<PopoverState>,
    suffix: Entity<TextInput>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    fn common_export_presets(&self) -> Option<Vec<Preset>> {
        let ids = self.selection_ids();
        let mut values = ids
            .iter()
            .map(|id| self.hierarchy.exports.get(id).cloned().unwrap_or_default());
        let first = values.next().unwrap_or_default();
        values.all(|value| value == first).then_some(first)
    }

    fn edit_export_presets(
        &mut self,
        group: Option<Group>,
        edit: impl Fn(&mut Vec<Preset>),
        cx: &mut Context<Self>,
    ) {
        let before = self.snapshot_hierarchy();
        for id in self.selection_ids() {
            if !self.layer_editable(id) {
                continue;
            }
            let presets = self.hierarchy.exports.entry(id).or_default();
            edit(presets);
            if presets.is_empty() {
                self.hierarchy.exports.remove(&id);
            }
        }
        if let Change::Hierarchy { value, .. } = &before
            && *value != self.hierarchy
        {
            if group.is_none() {
                self.history.borrow_mut().break_group();
            }
            self.history.borrow_mut().record(vec![before], group);
            cx.notify();
        }
    }

    pub(in crate::workspace) fn close_export_menus(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for row in &self.export.controls.rows {
            row.format.update(cx, |menu, cx| menu.close(window, cx));
            row.scale.update(cx, |menu, cx| menu.close(window, cx));
            row.options.update(cx, |menu, cx| menu.close(window, cx));
        }
    }

    pub(in crate::workspace) fn sync_export_controls(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selection = (self.pages.active.clone(), self.selection_ids());
        let presets = self.common_export_presets().unwrap_or_default();
        if self.export.controls.selection != selection {
            self.close_export_menus(window, cx);
            self.export.controls = Controls {
                selection: selection.clone(),
                ..Default::default()
            };
        }
        if self.export.controls.rows.len() != presets.len() {
            self.close_export_menus(window, cx);
            self.export.controls.rows.clear();
            for index in 0..presets.len() {
                let suffix = cx.new(TextInput::new);
                let target = selection.clone();
                let changed = cx.subscribe(&suffix, move |this, _, event: &InputEvent, cx| {
                    if this.export.controls.selection != target {
                        return;
                    }
                    if let InputEvent::Change(value) = event {
                        let suffix: String = value
                            .chars()
                            .filter(|c| !c.is_control() && !"/\\:*?\"<>|".contains(*c))
                            .take(100)
                            .collect();
                        this.edit_export_presets(
                            Some(Group::ExportSuffix(
                                target.1.iter().copied().collect(),
                                index,
                            )),
                            |presets| {
                                if let Some(preset) = presets.get_mut(index) {
                                    preset.suffix = suffix.clone();
                                }
                            },
                            cx,
                        );
                    } else {
                        this.history.borrow_mut().break_group();
                    }
                });
                let blur = cx.on_blur(&suffix.focus_handle(cx), window, |this, _, _| {
                    this.history.borrow_mut().break_group()
                });
                self.export.controls.rows.push(Row {
                    format: cx.new(|cx| DropdownState::new(window, cx)),
                    scale: cx.new(|cx| DropdownState::new(window, cx)),
                    options: cx.new(|cx| PopoverState::new(window, cx)),
                    suffix,
                    _subscriptions: vec![changed, blur],
                });
            }
        }
        for (row, preset) in self.export.controls.rows.iter().zip(&presets) {
            if row.suffix.read(cx).value().as_ref() != preset.suffix {
                row.suffix
                    .update(cx, |input, cx| input.set_value(preset.suffix.clone(), cx));
            }
        }
        self.sync_export_preview(window, cx);
    }

    pub(super) fn configured_export_jobs(
        &self,
        window: &Window,
        cx: &gpui::App,
    ) -> Result<Vec<Job>> {
        let roots: BTreeSet<_> = self
            .selection_ids()
            .into_iter()
            .filter(|id| {
                self.hierarchy
                    .exports
                    .get(id)
                    .is_some_and(|presets| !presets.is_empty())
            })
            .collect();
        let jobs = self.export_jobs(roots.clone(), window, cx)?;
        Ok(roots
            .into_iter()
            .zip(jobs)
            .flat_map(|(id, job)| {
                self.hierarchy.exports[&id].iter().map(move |preset| {
                    let mut job = job.clone();
                    job.preset = preset.clone();
                    job
                })
            })
            .collect())
    }

    pub(in crate::workspace) fn export_properties(&self, cx: &mut Context<Self>) -> Div {
        let presets = self.common_export_presets();
        let has_presets = self.selection_ids().iter().any(|id| {
            self.hierarchy
                .exports
                .get(id)
                .is_some_and(|p| !p.is_empty())
        });
        let videos = self.export_video_count();
        let only_videos = videos > 0 && videos == self.selection_ids().len();
        let label = if self.export.busy {
            t("export-progress").to_owned()
        } else if self.selection_ids().len() == 1 {
            crate::i18n::message(
                "export-named",
                &[(
                    "name",
                    self.layer_name(*self.selection_ids().first().unwrap(), cx),
                )],
            )
        } else {
            t("export-selection").into()
        };
        div()
            .debug_selector(|| "export-properties".into())
            .flex_shrink_0()
            .p(px(14.))
            .border_b_1()
            .border_color(rgb(BORDER))
            .flex()
            .flex_col()
            .gap(px(8.))
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(t("export-action")),
                    )
                    .child(
                        export_icon("export-add", t("export-add"), LucideIcons::Plus).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.edit_export_presets(
                                    None,
                                    |presets| presets.push(Preset::default()),
                                    cx,
                                );
                                this.inspector_scroll.scroll_to_bottom();
                            }),
                        ),
                    ),
            )
            .when(presets.is_none(), |el| {
                el.child(div().text_color(rgb(MUTED)).child(t("export-mixed")))
            })
            .children(
                presets
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .enumerate()
                    .map(|(index, preset)| self.export_preset_row(index, preset, only_videos, cx)),
            )
            .when(has_presets, |el| {
                el.child(
                    div()
                        .id("export-submit")
                        .debug_selector(|| "export-submit".into())
                        .h(px(30.))
                        .w_full()
                        .px(px(10.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(rgb(BORDER))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(TEXT))
                        .child(div().min_w_0().truncate().child(label))
                        .when(!self.export.busy, |el| {
                            el.cursor_pointer()
                                .hover(|s| s.bg(rgb(BORDER)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.export_selection(None, window, cx)
                                }))
                        }),
                )
                .when(videos > 0 && !only_videos, |el| {
                    el.child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(MUTED))
                            .child(t("export-video-original-note")),
                    )
                })
                .when(self.selection_ids().len() == 1 && !only_videos, |el| {
                    el.child(self.export_preview_control(cx))
                })
            })
    }

    fn export_preset_row(
        &self,
        index: usize,
        preset: &Preset,
        video: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let row = &self.export.controls.rows[index];
        let format_menu = row.format.clone();
        let scale_menu = row.scale.clone();
        let format = export_menu(&row.format)
            .trigger(export_trigger(row_id("export-format", index), preset.format.label()).w_full())
            .menu(
                div()
                    .flex()
                    .flex_col()
                    .children([Format::Png, Format::Svg].into_iter().map(|format| {
                        let menu = format_menu.clone();
                        export_choice(format.label(), format.label(), preset.format == format)
                            .debug_selector(move || {
                                row_id(&format!("export-format-{}", format.extension()), index)
                            })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.edit_export_presets(
                                    None,
                                    |presets| presets[index].format = format,
                                    cx,
                                );
                                menu.update(cx, |menu, cx| menu.close(window, cx));
                            }))
                    })),
            );
        let scale = export_menu(&row.scale)
            .trigger(
                export_trigger(row_id("export-scale", index), format!("{}×", preset.scale))
                    .w_full(),
            )
            .menu(
                div()
                    .flex()
                    .flex_col()
                    .children([1, 2, 3, 4].into_iter().map(|scale| {
                        let menu = scale_menu.clone();
                        export_choice(
                            ("export-scale-choice", scale as usize),
                            format!("{scale}×"),
                            preset.scale == scale,
                        )
                        .debug_selector(move || row_id(&format!("export-scale-{scale}"), index))
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.edit_export_presets(
                                    None,
                                    |presets| presets[index].scale = scale,
                                    cx,
                                );
                                menu.update(cx, |menu, cx| menu.close(window, cx));
                            },
                        ))
                    })),
            );
        let suffix = row.suffix.clone();
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .when(!video && preset.format == Format::Png, |el| {
                el.child(
                    div()
                        .w(px(66.))
                        .flex_shrink_0()
                        .rounded(px(5.))
                        .bg(rgb(0x282b33))
                        .child(scale),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .rounded(px(5.))
                    .bg(rgb(0x282b33))
                    .when(!video, |el| el.child(format))
                    .when(video, |el| {
                        el.child(
                            div()
                                .debug_selector(|| "export-video-format".into())
                                .h(px(30.))
                                .px(px(10.))
                                .flex()
                                .items_center()
                                .child(self.video_export_label()),
                        )
                    }),
            )
            .child(
                Popover::new(&row.options)
                    .label(t("export-options"))
                    .placement(PopoverPlacement::LeftStart)
                    .gap(px(12.))
                    .p(px(14.))
                    .rounded(px(10.))
                    .bg(rgb(PANEL))
                    .border_1()
                    .border_color(rgb(BORDER))
                    .shadow_lg()
                    .trigger(export_icon(
                        gpui::SharedString::from(row_id("export-options", index)),
                        t("export-options"),
                        LucideIcons::Ellipsis,
                    ))
                    .content(move |_, _| {
                        div()
                            .w(px(230.))
                            .flex()
                            .flex_col()
                            .gap(px(12.))
                            .text_size(px(12.))
                            .text_color(rgb(TEXT))
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(t("export-options")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.))
                                    .child(
                                        div()
                                            .flex_shrink_0()
                                            .text_color(rgb(MUTED))
                                            .child(t("export-suffix")),
                                    )
                                    .child(
                                        div()
                                            .debug_selector(move || row_id("export-suffix", index))
                                            .flex_1()
                                            .min_w_0()
                                            .child(
                                                Input::new(&suffix)
                                                    .w_full()
                                                    .h(px(30.))
                                                    .px(px(8.))
                                                    .rounded(px(5.))
                                                    .bg(rgb(WORKSPACE))
                                                    .border_color(rgb(BORDER))
                                                    .text_size(px(12.))
                                                    .text_color(rgb(TEXT))
                                                    .appearance(InputAppearance {
                                                        focus_border: rgb(ACCENT).into(),
                                                        caret: rgb(ACCENT).into(),
                                                        selection: gpui::rgba(0xb4a2ee44).into(),
                                                        caret_height: px(16.),
                                                        ..Default::default()
                                                    }),
                                            ),
                                    ),
                            )
                    }),
            )
            .child(
                export_icon(
                    gpui::SharedString::from(row_id("export-remove", index)),
                    t("export-remove"),
                    LucideIcons::Minus,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.edit_export_presets(
                        None,
                        |presets| {
                            presets.remove(index);
                        },
                        cx,
                    )
                })),
            )
    }

    fn export_preview_control(&self, cx: &mut Context<Self>) -> Div {
        let open = self.export.controls.preview_open;
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .id("export-preview-toggle")
                    .debug_selector(|| "export-preview-toggle".into())
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .child(
                        icon(
                            if open {
                                LucideIcons::ChevronDown
                            } else {
                                LucideIcons::ChevronRight
                            },
                            12.,
                        )
                        .text_color(rgb(MUTED)),
                    )
                    .child(t("export-preview"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.export.controls.preview_open = !this.export.controls.preview_open;
                        if this.export.controls.preview_open {
                            this.inspector_scroll.scroll_to_bottom();
                        }
                        cx.notify();
                    })),
            )
            .when(open, |el| {
                el.child(
                    div()
                        .debug_selector(|| "export-preview".into())
                        .h(px(140.))
                        .w_full()
                        .rounded(px(6.))
                        .overflow_hidden()
                        .bg(gpui::checkerboard(rgb(0x30343d), 6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when_some(self.export.controls.preview.clone(), |el, image| {
                            el.child(
                                gpui::img(image)
                                    .size_full()
                                    .object_fit(gpui::ObjectFit::Contain),
                            )
                        })
                        .when(self.export.controls.preview.is_none(), |el| {
                            el.child(div().text_size(px(11.)).text_color(rgb(MUTED)).child(t(
                                if self.export.controls.preview_error {
                                    "export-preview-failed"
                                } else {
                                    "export-preview-loading"
                                },
                            )))
                        }),
                )
            })
    }

    fn sync_export_preview(&mut self, window: &Window, cx: &mut Context<Self>) {
        let revision = self.history.borrow().revision();
        let active = self.export.controls.preview_open
            && self.preview.is_none()
            && !self.panels.right_collapsed
            && self.selection_ids().len() == 1
            && self.export_video_count() == 0
            && self.common_export_presets().is_some_and(|p| !p.is_empty());
        if !active {
            self.export.controls.preview_task = None;
            self.export.controls.preview = None;
            self.export.controls.preview_revision = None;
            return;
        }
        if self.export.controls.preview_revision == Some(revision) {
            return;
        }
        self.export.controls.preview_revision = Some(revision);
        self.export.controls.preview_error = false;
        self.export.controls.preview = None;
        let job = self
            .component_export_jobs(window, cx)
            .and_then(|jobs| jobs.into_iter().next().context("Empty preview"));
        let selection = self.export.controls.selection.clone();
        self.export.controls.preview_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(150))
                .await;
            let result = cx
                .background_executor()
                .spawn(async move {
                    let job = job?;
                    crate::raster::prepare().await?;
                    let svg = job.svg()?;
                    let scale = (320. / job.bounds.width.max(job.bounds.height).max(1.)).min(1.);
                    let size = [
                        (job.bounds.width * scale).ceil().max(1.) as u32,
                        (job.bounds.height * scale).ceil().max(1.) as u32,
                    ];
                    let options = crate::scene_render::render_options(!job.text.is_empty())?;
                    let png = crate::raster::render(&svg, size, scale, false, false, &options)?;
                    let mut pixels = image::load_from_memory(&png)?.into_rgba8();
                    for pixel in pixels.pixels_mut() {
                        pixel.0.swap(0, 2);
                    }
                    anyhow::Ok(Arc::new(gpui::RenderImage::new(vec![image::Frame::new(
                        pixels,
                    )])))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.export.controls.selection == selection
                    && this.export.controls.preview_revision == Some(revision)
                {
                    this.export.controls.preview_error = result.is_err();
                    this.export.controls.preview = result.ok();
                    cx.notify();
                }
            });
        }));
    }
}

fn export_icon(
    id: impl Into<gpui::SharedString>,
    label: &'static str,
    glyph: LucideIcons,
) -> gpui::Stateful<Div> {
    let id = id.into();
    div()
        .id(id.clone())
        .debug_selector(move || id.to_string())
        .aria_label(label)
        .size(px(24.))
        .flex_shrink_0()
        .rounded(px(5.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(rgb(BORDER)))
        .child(icon(glyph, 17.).text_color(rgb(MUTED)))
}

fn row_id(prefix: &str, index: usize) -> String {
    if index == 0 {
        prefix.into()
    } else {
        format!("{prefix}-{index}")
    }
}

#[cfg(test)]
mod tests;
