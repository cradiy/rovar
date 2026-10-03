use super::*;
use crate::scene::auto_layout::Mode;
use gpui::SharedString;
use serde::Deserialize;
use std::sync::LazyLock;
use uic::components::context_menu::{ContextMenu, ContextMenuItem, ContextMenuTrigger};

#[cfg(test)]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Group {
    label: String,
    presets: Vec<Preset>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Preset {
    name: String,
    width: f32,
    height: f32,
}

impl Preset {
    fn dimensions(&self) -> [f32; 2] {
        [self.width, self.height]
    }
}

static GROUPS: LazyLock<Vec<Group>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../../../assets/presets/frames.json"))
        .expect("Invalid bundled frame presets")
});

#[test]
fn bundled_presets_have_unique_names_and_valid_dimensions() {
    let mut names = std::collections::BTreeSet::new();
    assert!(!GROUPS.is_empty());
    for group in GROUPS.iter() {
        assert!(!group.label.trim().is_empty() && !group.presets.is_empty());
        for preset in &group.presets {
            assert!(!preset.name.trim().is_empty() && names.insert(&preset.name));
            assert!(
                preset.dimensions().iter().all(|value| {
                    value.is_finite() && (1. ..=crate::scene::artboard::MAX_SIZE).contains(value)
                }),
                "Invalid dimensions for {}",
                preset.name
            );
        }
    }
}

impl Workspace {
    pub(super) fn frame_preset_control(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let id = self.selected_board().unwrap().id;
        let page = self.pages.active.clone();
        let weak = cx.entity().downgrade();
        ContextMenuTrigger::new(
            div()
                .id("frame-presets")
                .debug_selector(|| "frame-presets".into())
                .h(px(30.))
                .px(px(6.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(TEXT))
                .hover(|s| s.bg(rgb(BORDER)))
                .child(t("artboard"))
                .child(icon(LucideIcons::ChevronDown, 12.).text_color(rgb(MUTED))),
            move |window, cx| {
                weak.update(cx, |this, cx| this.frame_presets(id, &page, window, cx))
                    .unwrap_or_default()
            },
        )
        .id("frame-preset-trigger")
    }

    fn frame_presets(
        &self,
        id: usize,
        page: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> ContextMenu {
        let Some(board) = self
            .selected_board()
            .filter(|board| board.id == id && self.pages.active == page)
        else {
            return ContextMenu::new();
        };
        let current = [board.rect.width, board.rect.height];
        let enabled = self.layer_editable(id);
        let active = GROUPS
            .iter()
            .flat_map(|group| &group.presets)
            .find(|preset| preset.dimensions() == current)
            .map(|preset| &preset.name);
        let item = |label: SharedString, size: [f32; 2]| {
            let checked = active.is_some_and(|name| label.as_ref() == name);
            let weak = cx.entity().downgrade();
            let page = page.to_owned();
            ContextMenuItem::action_with(
                move |_, _| {
                    div()
                        .debug_selector({
                            let label = label.clone();
                            move || format!("frame-preset-{label}")
                        })
                        .h(px(28.))
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(
                            div()
                                .w(px(14.))
                                .flex_shrink_0()
                                .when(checked, |el| el.child(icon(LucideIcons::Check, 13.))),
                        )
                        .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                        .child(div().flex_shrink_0().text_color(rgb(MUTED)).child(format!(
                            "{} × {}",
                            inspector::number(size[0]),
                            inspector::number(size[1])
                        )))
                },
                move |window, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.apply_frame_preset(id, &page, size, window, cx)
                    });
                },
            )
            .disabled(!enabled)
        };
        let mut menu =
            super::super::context_menu::menu(320., "frame-preset-menu")
                .max_h(px(
                    (f32::from(window.viewport_size().height) - 32.).clamp(120., 520.)
                ));
        for (index, group) in GROUPS.iter().enumerate() {
            if index != 0 {
                menu = menu.separator();
            }
            menu = menu.item(ContextMenuItem::action(t(&group.label), |_, _| {}).disabled(true));
            for preset in &group.presets {
                menu = menu.item(item(preset.name.clone().into(), preset.dimensions()));
            }
        }
        menu
    }

    fn apply_frame_preset(
        &mut self,
        id: usize,
        page: &str,
        dimensions: [f32; 2],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pages.active != page || !self.layer_editable(id) || self.preview_read_only() {
            return;
        }
        if !self.selection_ids().iter().copied().eq([id]) {
            return;
        }
        let Some(board) = self.selected_board().filter(|board| board.id == id) else {
            return;
        };
        let sizing = self.hierarchy.sizing.get(&id).copied().unwrap_or_default();
        if [board.rect.width, board.rect.height] == dimensions
            && sizing.width == Mode::Fixed
            && sizing.height == Mode::Fixed
        {
            return;
        }
        self.suspend(window, cx);
        let before = self.page_edit(std::slice::from_ref(&self.pages.active), cx);
        let board = self.boards.iter_mut().find(|board| board.id == id).unwrap();
        board.rect.width = dimensions[0];
        board.rect.height = dimensions[1];
        let sizing = self.hierarchy.sizing.entry(id).or_default();
        sizing.width = Mode::Fixed;
        sizing.height = Mode::Fixed;
        self.refresh_constraints(id);
        self.record_page_edit(before);
        self.auto_layout.revision = None;
        self.reflow_layout(cx);
        self.sync_field(3, cx);
        self.sync_field(4, cx);
        cx.notify();
    }
}
