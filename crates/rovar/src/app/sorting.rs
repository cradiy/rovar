use super::*;
use crate::ui::theme::Color;
use uic::components::dropdown::{DropdownPlacement, dropdown};
use web_time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum FileSort {
    #[default]
    CreatedNewest,
    CreatedOldest,
    ModifiedNewest,
    ModifiedOldest,
}

pub(super) fn creation_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl FileSort {
    fn label(self) -> &'static str {
        match self {
            Self::CreatedNewest => "sort-created-newest",
            Self::CreatedOldest => "sort-created-oldest",
            Self::ModifiedNewest => "sort-modified-newest",
            Self::ModifiedOldest => "sort-modified-oldest",
        }
    }

    pub(super) fn sort(self, files: &mut [Recent]) {
        files.sort_by(|a, b| {
            let order = match self {
                Self::CreatedNewest => b.created.cmp(&a.created),
                Self::CreatedOldest => a.created.cmp(&b.created),
                Self::ModifiedNewest => b.modified.cmp(&a.modified),
                Self::ModifiedOldest => a.modified.cmp(&b.modified),
            };
            order.then_with(|| a.path.cmp(&b.path))
        });
    }
}

impl Studio {
    pub(super) fn home_file_sort(&self) -> FileSort {
        if self.all_files {
            self.file_sort
        } else {
            FileSort::ModifiedNewest
        }
    }

    pub(super) fn file_sort_control(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        dropdown(&self.sort_menu)
            .placement(DropdownPlacement::BottomEnd)
            .w(px(232.))
            .p(px(4.))
            .rounded(px(9.))
            .bg(Color::Panel.color())
            .border_color(Color::Text.color().opacity(0.0863))
            .text_color(TEXT.color())
            .text_size(px(12.))
            .trigger(
                div()
                    .id("file-sort-menu")
                    .debug_selector(|| "file-sort-menu".into())
                    .h(px(32.))
                    .mb(px(8.))
                    .px(px(8.))
                    .rounded(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .text_size(px(12.))
                    .text_color(MUTED.color())
                    .cursor_pointer()
                    .hover(|style| {
                        style
                            .bg(Color::Text.color().opacity(0.0314))
                            .text_color(TEXT.color())
                    })
                    .child(t(self.file_sort.label()))
                    .child(icon(LucideIcons::ChevronDown, 12.)),
            )
            .menu(
                div().flex().flex_col().children(
                    [
                        FileSort::CreatedNewest,
                        FileSort::CreatedOldest,
                        FileSort::ModifiedNewest,
                        FileSort::ModifiedOldest,
                    ]
                    .map(|sort| {
                        div()
                            .id(sort.label())
                            .debug_selector(move || sort.label().into())
                            .h(px(32.))
                            .px(px(8.))
                            .rounded(px(5.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .cursor_pointer()
                            .hover(|style| style.bg(Color::Accent.color().opacity(0.1569)))
                            .child(div().w(px(16.)).when(self.file_sort == sort, |el| {
                                el.child(icon(LucideIcons::Check, 14.).text_color(ACCENT.color()))
                            }))
                            .child(t(sort.label()))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.file_sort = sort;
                                this.set_home_page(0, cx);
                                this.sort_menu.update(cx, |menu, cx| menu.close(window, cx));
                                cx.notify();
                            }))
                    }),
                ),
            )
    }
}
