use super::*;
use crate::i18n::t;
use crate::image_fill::{ImageFill, ImageFit};

impl Workspace {
    pub(in crate::workspace) fn current_image_fill(&self) -> Option<&ImageFill> {
        if self.selected_text.is_some() || (self.selected_shape.is_some() && self.stroke_editing) {
            return None;
        }
        self.selected_shape()
            .map(|s| &s.image_fill)
            .or_else(|| self.selected_board().map(|b| &b.image_fill))
    }

    fn import_image_fill(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.selected_shape.or(self.selected) else {
            return;
        };
        self.image_fill_request += 1;
        self.image_fill_loading = None;
        cx.notify();
        let request = self.image_fill_request;
        let dialog = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(t("image-background-choose").into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = dialog.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let Ok(true) = this.update(cx, |this, cx| {
                if this.image_fill_request != request {
                    return false;
                }
                this.image_fill_loading = Some(id);
                cx.notify();
                true
            }) else {
                return;
            };
            let asset = cx
                .background_executor()
                .spawn(async move { crate::media::MediaAsset::load_image(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.image_fill_request != request {
                    return;
                }
                this.image_fill_loading = None;
                if !this.layer_editable(id) {
                    cx.notify();
                    return;
                }
                match asset {
                    Ok(asset) => {
                        if let Some(index) = this
                            .shapes
                            .iter()
                            .position(|s| s.id == id && s.fill_mode == FillMode::Image)
                        {
                            let before = this.shapes[index].clone();
                            this.shapes[index].image_fill.asset = Some(asset);
                            this.history.borrow_mut().record(
                                vec![Change::Shape {
                                    id,
                                    index,
                                    value: Some(before),
                                }],
                                None,
                            );
                        } else if let Some(index) = this
                            .boards
                            .iter()
                            .position(|b| b.id == id && b.fill_mode == FillMode::Image)
                        {
                            let before = this.boards[index].clone();
                            this.boards[index].image_fill.asset = Some(asset);
                            this.history.borrow_mut().record(
                                vec![Change::Board {
                                    id,
                                    index,
                                    value: Some(before),
                                }],
                                None,
                            );
                        }
                        this.sync_fields(cx);
                    }
                    Err(error) => {
                        this.media_error = Some(crate::i18n::message(
                            "image-background-error",
                            &[("error", format!("{error:#}"))],
                        ))
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn image_fill_controls(&self, cx: &mut Context<Self>) -> Div {
        let Some(fill) = self.current_image_fill() else {
            return div();
        };
        let loading = self.image_fill_loading == self.selected_shape.or(self.selected);
        div()
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div().flex().gap(px(6.)).children(
                    [
                        (ImageFit::Cover, "image-fill-cover", t("fill")),
                        (ImageFit::Contain, "image-fill-contain", t("image-fit")),
                    ]
                    .into_iter()
                    .map(|(fit, id, label)| {
                        div()
                            .id(id)
                            .debug_selector(move || id.into())
                            .flex_1()
                            .h(px(30.))
                            .rounded(px(6.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .bg(rgb(if fill.fit == fit { 0x353044 } else { 0x282b33 }))
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.history.borrow_mut().break_group();
                                if this.selected_shape.is_some() {
                                    this.edit_shape(|s| s.image_fill.fit = fit);
                                } else {
                                    this.edit_board(None, |b| b.image_fill.fit = fit);
                                }
                                cx.notify();
                            }))
                    }),
                ),
            )
            .child(
                div()
                    .relative()
                    .h(px(190.))
                    .rounded(px(8.))
                    .overflow_hidden()
                    .bg(rgb(0xffffff))
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .bg(gpui::checkerboard(rgb(0xd9dce2), 8.)),
                    )
                    .child(fill.element().absolute().inset_0()),
            )
            .child(
                div()
                    .id("image-fill-import")
                    .debug_selector(|| "image-fill-import".into())
                    .h(px(32.))
                    .rounded(px(6.))
                    .bg(rgb(0x353044))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .child(icon(LucideIcons::Image, 16.))
                    .child(if loading {
                        t("loading")
                    } else if fill.asset.is_some() {
                        t("image-replace")
                    } else {
                        t("image-choose")
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.import_image_fill(cx))),
            )
            .child(self.paint_input_field(6, t("opacity"), cx))
    }
}
