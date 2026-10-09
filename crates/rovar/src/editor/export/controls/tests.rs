use super::*;
use crate::editor::tests::{click, create, draw, open};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn export_presets_follow_objects_through_undo_storage_and_clipboard(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1200.)));
    create(&mut visual, "add-rectangle");
    assert!(visual.debug_bounds("export-submit").is_none());
    let before = window
        .update(&mut visual.cx, |this, _, _| {
            this.history.borrow().undo_len()
        })
        .unwrap();
    click(&mut visual, "export-add");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.hierarchy.exports[&1], vec![Preset::default()]);
            assert_eq!(this.history.borrow().undo_len(), before + 1);
        })
        .unwrap();
    click(&mut visual, "export-scale");
    click(&mut visual, "export-scale-2");
    click(&mut visual, "export-options");
    click(&mut visual, "export-suffix");
    visual.simulate_input("@2x");
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    click(&mut visual, "export-add");
    click(&mut visual, "export-format-1");
    click(&mut visual, "export-format-svg-1");
    let presets = window
        .update(&mut visual.cx, |this, _, _| {
            let presets = this.hierarchy.exports[&1].clone();
            assert_eq!(presets.len(), 2);
            assert_eq!(presets[0].scale, 2);
            assert_eq!(presets[0].suffix, "@2x");
            assert_eq!(presets[1].format, Format::Svg);
            presets
        })
        .unwrap();
    click(&mut visual, "export-remove-1");
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.focus.focus(window, cx);
            assert_eq!(this.hierarchy.exports[&1].len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("secondary-z");
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, window, cx| {
            assert_eq!(this.hierarchy.exports[&1], presets);
            let (json, sources) = this
                .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                .unwrap();
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("exports.rovar");
            crate::document::save_as(&path, &json, &sources, &cx.text_system().clone()).unwrap();
            let document =
                crate::document::Document::decode(&crate::document::load(&path).unwrap().json)
                    .unwrap();
            assert_eq!(document.pages[0].hierarchy.exports[&1], presets);
            this.duplicate_selection(window, cx);
            let copy = this.selected_shape.unwrap();
            assert_ne!(copy, 1);
            assert_eq!(this.hierarchy.exports[&copy], presets);
            this.hierarchy.exports.get_mut(&copy).unwrap()[0].scale = 3;
            assert_eq!(this.hierarchy.exports[&1][0].scale, 2);
            let saved = this.component_snapshot(cx).unwrap();
            let component = crate::document::Document::decode(&saved.json).unwrap();
            assert_eq!(
                component.pages[0]
                    .hierarchy
                    .exports
                    .keys()
                    .copied()
                    .collect::<Vec<_>>(),
                vec![copy]
            );
            let extracted = crate::scene::components::extract(&component.pages[0], copy).unwrap();
            let placed = crate::scene::components::place(
                &extracted,
                &BTreeMap::from([(copy, 100)]),
                [0., 0.],
                None,
            );
            assert_eq!(placed.hierarchy.exports[&100][0].scale, 3);
            placed.validate().unwrap();
            this.set_selection(BTreeSet::from([1, copy]), cx);
            assert!(this.common_export_presets().is_none());
        })
        .unwrap();
}

#[gpui::test]
fn configured_export_writes_each_format_scale_and_suffix(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    create(&mut visual, "add-rectangle");
    let jobs = window
        .update(&mut visual.cx, |this, window, cx| {
            this.shapes[0].name = "Icon".into();
            this.shapes[0].rect = Rect {
                x: 0.,
                y: 0.,
                width: 30.,
                height: 20.,
            };
            this.edit_export_presets(
                None,
                |presets| {
                    *presets = vec![
                        Preset {
                            suffix: "@2x".into(),
                            scale: 2,
                            ..Default::default()
                        },
                        Preset {
                            suffix: "-vector".into(),
                            format: Format::Svg,
                            ..Default::default()
                        },
                    ];
                },
                cx,
            );
            this.configured_export_jobs(window, cx).unwrap()
        })
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let paths = exporter::write(jobs, directory.path().into(), true).unwrap();
    assert_eq!(
        paths,
        vec![
            directory.path().join("Icon@2x.png"),
            directory.path().join("Icon-vector.svg")
        ]
    );
    assert_eq!(image::open(&paths[0]).unwrap().width(), 60);
    assert_eq!(image::open(&paths[0]).unwrap().height(), 40);
    assert!(
        std::fs::read_to_string(&paths[1])
            .unwrap()
            .contains("<path")
    );
}

#[gpui::test]
fn export_preview_is_lazy_and_cleared_when_selection_changes(cx: &mut TestAppContext) {
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_resize(size(px(1280.), px(1200.)));
    create(&mut visual, "add-rectangle");
    click(&mut visual, "export-add");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.export.controls.preview_task.is_none());
        })
        .unwrap();
    click(&mut visual, "export-preview-toggle");
    visual
        .cx
        .executor()
        .advance_clock(std::time::Duration::from_millis(200));
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(this.export.controls.preview.is_some());
            this.select(None, cx);
        })
        .unwrap();
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.export.controls.preview.is_none());
            assert!(this.export.controls.preview_task.is_none());
        })
        .unwrap();
    assert!(visual.debug_bounds("export-properties").is_none());
}
