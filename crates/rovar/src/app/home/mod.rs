use super::*;
use crate::ui::theme::Color;
use gpui::{FontWeight, linear_color_stop, linear_gradient};
use uic::components::input::Input;

mod artwork;
mod pagination;

const PAGE_SIZE: usize = 6;

impl Studio {
    pub(super) fn home(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        // value() includes IME preedit; only committed Change events update the query.
        let search = self.search_query.as_str();
        let session = self.session.borrow();
        let mut files: Vec<_> = session
            .recent
            .iter()
            .filter(|file| {
                self.source_matches(&file.path, cx)
                    && (self.all_files || !session.removed_recent.contains(&file.path))
                    && file.title.to_lowercase().contains(search)
            })
            .cloned()
            .collect();
        self.home_file_sort().sort(&mut files);
        let total = files.len();
        let pages = total.div_ceil(PAGE_SIZE).max(1);
        let page = self.home_page.min(pages - 1);
        if page != self.home_page {
            self.home_page = page;
            self.home_scroll.set_offset(gpui::point(px(0.), px(0.)));
        }
        let width = (f32::from(window.viewport_size().width) - 96.).clamp(320., 1160.);
        let columns = if width >= 940. { 3. } else { 2. };
        let card_width = (width - (columns - 1.) * 20.) / columns;
        let content = div()
            .id("home")
            .debug_selector(|| "home".into())
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.home_scroll)
            .bg(Color::Workspace.color())
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .w(px(width))
                    .flex_shrink_0()
                    .pt(px(44.))
                    .pb(px(48.))
                    .flex()
                    .flex_col()
                    .gap(px(32.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(28.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(Color::Text.color())
                                    .child(t("home-title")),
                            )
                            .child(self.source_control(cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(20.))
                            .child(self.start_card(true, cx))
                            .child(self.start_card(false, cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(22.))
                            .child(
                                div()
                                    .min_h(px(46.))
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(24.))
                                    .border_b_1()
                                    .border_color(Color::Text.color().opacity(0.0471))
                                    .children(
                                        [
                                            (false, "home-recent", "recent-files"),
                                            (true, "home-all-files", "all-files"),
                                        ]
                                        .map(
                                            |(all_files, id, label)| {
                                                let active = self.all_files == all_files;
                                                div()
                                                    .id(id)
                                                    .debug_selector(move || id.into())
                                                    .h_full()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.))
                                                    .border_b_2()
                                                    .border_color(if active {
                                                        ACCENT.color()
                                                    } else {
                                                        Color::Transparent.color()
                                                    })
                                                    .text_size(px(13.))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(if active {
                                                        Color::Text.color()
                                                    } else {
                                                        Color::Muted.color()
                                                    })
                                                    .cursor_pointer()
                                                    .hover(|s| s.text_color(Color::Text.color()))
                                                    .child(t(label))
                                                    .when(active, |el| {
                                                        el.child(
                                                            div()
                                                                .px(px(6.))
                                                                .py(px(1.))
                                                                .rounded(px(5.))
                                                                .text_size(px(10.))
                                                                .bg(Color::Accent
                                                                    .color()
                                                                    .opacity(0.0941))
                                                                .text_color(ACCENT.color())
                                                                .child(files.len().to_string()),
                                                        )
                                                    })
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.all_files = all_files;
                                                        this.set_home_page(0, cx);
                                                    }))
                                            },
                                        ),
                                    )
                                    .child(div().flex_1())
                                    .when(self.all_files, |el| el.child(self.file_sort_control(cx)))
                                    .child(
                                        div()
                                            .w(px(220.))
                                            .h(px(32.))
                                            .mb(px(8.))
                                            .px(px(10.))
                                            .flex()
                                            .items_center()
                                            .gap(px(8.))
                                            .rounded(px(8.))
                                            .border_1()
                                            .border_color(Color::Text.color().opacity(0.0510))
                                            .bg(Color::Text.color().opacity(0.0157))
                                            .text_color(Color::Muted.color())
                                            .child(icon(LucideIcons::Search, 14.))
                                            .child(
                                                Input::new(&self.search)
                                                    .w_full()
                                                    .h(px(30.))
                                                    .px(px(0.))
                                                    .text_size(px(12.))
                                                    .bg(Color::Transparent.color())
                                                    .border_0()
                                                    .text_color(TEXT.color())
                                                    .appearance(
                                                        uic::components::input::InputAppearance {
                                                            focus_border: Color::Transparent
                                                                .color()
                                                                .into(),
                                                            ..crate::ui::theme::input_appearance()
                                                        },
                                                    ),
                                            ),
                                    ),
                            )
                            .when(files.is_empty(), |el| {
                                el.child(
                                    div()
                                        .h(px(220.))
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .justify_center()
                                        .gap(px(16.))
                                        .text_color(Color::Muted.color())
                                        .child(
                                            div()
                                                .size(px(48.))
                                                .rounded(px(14.))
                                                .bg(Color::Accent.color().opacity(0.0392))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(icon(LucideIcons::Files, 22.)),
                                        )
                                        .child(div().text_size(px(13.)).child(t(
                                            if search.is_empty() {
                                                "home-empty"
                                            } else {
                                                "home-no-results"
                                            },
                                        ))),
                                )
                            })
                            .child(
                                div().flex().flex_wrap().gap(px(20.)).children(
                                    files
                                        .into_iter()
                                        .skip(page * PAGE_SIZE)
                                        .take(PAGE_SIZE)
                                        .enumerate()
                                        .map(|(index, file)| {
                                            self.file_card(index, file, card_width, cx)
                                        }),
                                ),
                            ),
                    ),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(Color::Workspace.color())
            .child(content)
            .when(total > 0, |el| {
                el.child(self.home_pagination(total, width, cx))
            })
    }

    fn start_card(&self, primary: bool, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let id = if primary { "home-new" } else { "home-open" };
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex_1()
            .min_w_0()
            .h(px(176.))
            .relative()
            .overflow_hidden()
            .rounded(px(16.))
            .border_1()
            .border_color(if primary {
                Color::Accent.color().opacity(0.1961)
            } else {
                Color::Text.color().opacity(0.0745)
            })
            .bg(linear_gradient(
                110.,
                linear_color_stop(
                    if primary {
                        Color::Selected.color()
                    } else {
                        Color::Surface.color()
                    },
                    0.,
                ),
                linear_color_stop(
                    if primary {
                        Color::Selected.color()
                    } else {
                        Color::Input.color()
                    },
                    1.,
                ),
            ))
            .hover(|s| {
                s.border_color(if primary {
                    Color::Accent.color().opacity(0.5333)
                } else {
                    Color::Text.color().opacity(0.2078)
                })
            })
            .cursor_pointer()
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top_0()
                    .w(px(230.))
                    .h_full()
                    .child(artwork::start_art(primary)),
            )
            .child(
                div()
                    .absolute()
                    .inset_0()
                    .p(px(24.))
                    .flex()
                    .flex_col()
                    .justify_between()
                    .child(
                        div()
                            .size(px(32.))
                            .rounded(px(9.))
                            .border_1()
                            .border_color(Color::Text.color().opacity(0.0824))
                            .bg(Color::Text.color().opacity(0.0392))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(if primary {
                                Color::Accent.color()
                            } else {
                                Color::Muted.color()
                            })
                            .child(icon(
                                if primary {
                                    LucideIcons::Plus
                                } else {
                                    LucideIcons::FolderOpen
                                },
                                18.,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_end()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(5.))
                                    .child(
                                        div()
                                            .text_size(px(19.))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(Color::Text.color())
                                            .child(t(if primary {
                                                "new-document"
                                            } else {
                                                "open-document"
                                            })),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(Color::Muted.color())
                                            .child(t(if primary {
                                                "home-new-detail"
                                            } else {
                                                "home-open-detail"
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(Color::Accent.color().opacity(0.4392))
                                    .child(crate::ui::shortcuts::label(if primary {
                                        "Mod+N"
                                    } else {
                                        "Mod+O"
                                    })),
                            ),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                if primary {
                    this.new_document(window, cx)
                } else {
                    this.open_dialog(window, cx)
                }
            }))
    }

    fn file_card(
        &self,
        index: usize,
        file: Recent,
        width: f32,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let path = file.path.clone();
        let menu_path = path.clone();
        let delete = self.all_files;
        let preview_width = (width - 40.).min((width * 0.57 - 38.) * 560. / 336.);
        let preview = file
            .preview
            .as_ref()
            .map(|name| self.directory.join("previews").join(name))
            .filter(|path| rovar_storage::exists(path));
        div()
            .id(("recent-file", index))
            .debug_selector(move || format!("recent-file-{index}"))
            .w(px(width))
            .rounded(px(13.))
            .overflow_hidden()
            .border_1()
            .border_color(Color::Text.color().opacity(0.0706))
            .bg(Color::Panel.color())
            .cursor_pointer()
            .hover(|s| {
                s.border_color(Color::Accent.color().opacity(0.5333))
                    .bg(Color::Hover.color())
            })
            .child(
                div()
                    .h(px(width * 0.57))
                    .rounded_tl(px(12.))
                    .rounded_tr(px(12.))
                    .relative()
                    .overflow_hidden()
                    .bg(Color::Panel.color())
                    .child(artwork::preview_grid())
                    .when(preview.is_none(), |el| {
                        el.child(
                            div()
                                .absolute()
                                .inset_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(artwork::blank_canvas()),
                        )
                    })
                    .when_some(preview, |el, path| {
                        el.child(
                            div()
                                .absolute()
                                .inset(px(18.))
                                .rounded(px(5.))
                                .overflow_hidden()
                                .border_1()
                                .border_color(Color::Text.color().opacity(0.0824))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(
                                    gpui::img(crate::platform::preview_image(path))
                                        .w(px(preview_width))
                                        .h(px(preview_width * 336. / 560.))
                                        .flex_shrink_0()
                                        .object_fit(gpui::ObjectFit::Contain),
                                ),
                        )
                    }),
            )
            .child(
                div()
                    .p(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .size(px(34.))
                            .flex_shrink_0()
                            .rounded(px(9.))
                            .bg(Color::Accent.color().opacity(0.0588))
                            .text_color(Color::Accent.color())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon(LucideIcons::PenTool, 16.)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(Color::Text.color())
                                    .truncate()
                                    .child(file.title),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(Color::Muted.color())
                                    .text_ellipsis()
                                    .child(edited_time(file.modified)),
                            ),
                    ),
            )
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    this.document_menu(menu_path.clone(), Some(delete), event.position, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_click(
                cx.listener(move |this, _, window, cx| this.open_path(path.clone(), window, cx)),
            )
    }
}

pub(super) fn edited_time(modified: u64) -> String {
    let seconds = now().saturating_sub(modified);
    let (key, count) = match seconds {
        0..60 => return t("edited-now").into(),
        60..3600 => ("edited-minutes", seconds / 60),
        3600..86400 => ("edited-hours", seconds / 3600),
        _ => ("edited-days", seconds / 86400),
    };
    crate::i18n::count(key, count as usize)
}
