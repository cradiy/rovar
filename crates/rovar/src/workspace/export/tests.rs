use super::*;
use crate::{
    document::Loaded,
    layer::LayerGroup,
    text::{
        TextStyle,
        styles::{StyleRun, StyledText},
    },
    workspace::tests::{click, draw, open},
};
use gpui::{TestAppContext, VisualTestContext};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

#[gpui::test]
fn selection_export_separates_roots_expands_groups_and_clips_board_children(
    cx: &mut TestAppContext,
) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.boards.clear();
            this.shapes.clear();
            this.add_artboard(rect(100., 200., 100., 80.), cx);
            let board = this.boards[0].id;
            this.shapes = vec![
                Shape::new(
                    10,
                    Some(board),
                    ShapeKind::Rectangle,
                    rect(-10., 10., 50., 30.),
                ),
                Shape::new(11, None, ShapeKind::Ellipse, rect(300., 400., 40., 40.)),
                Shape::new(12, None, ShapeKind::Rectangle, rect(500., 400., 20., 20.)),
            ];
            this.shapes[2].layer.hidden = true;
            this.next_id = 30;
            this.set_selection(BTreeSet::from([board, 11]), cx);
            let jobs = this.component_export_jobs(window, cx).unwrap();
            assert_eq!(jobs.len(), 2);
            assert_eq!(jobs[0].order, vec![board, 10]);
            assert_eq!(jobs[0].bounds, this.boards[0].rect);
            assert_eq!(jobs[0].clips[&10], this.boards[0].rect);
            assert_eq!(jobs[1].order, vec![11]);
            this.set_selection(BTreeSet::from([10]), cx);
            let jobs = this.component_export_jobs(window, cx).unwrap();
            assert_eq!(jobs[0].order, vec![10]);
            assert!(jobs[0].clips.is_empty());
            assert_eq!(jobs[0].bounds, rect(90., 210., 50., 30.));
            this.hierarchy.groups.insert(
                20,
                LayerGroup {
                    name: "Group".into(),
                    board: None,
                    layer: Default::default(),
                },
            );
            this.hierarchy.parents.extend([(11, 20), (12, 20)]);
            this.set_selection(BTreeSet::from([20]), cx);
            let jobs = this.component_export_jobs(window, cx).unwrap();
            assert_eq!(jobs.len(), 1);
            assert_eq!(jobs[0].name, "Group");
            assert_eq!(jobs[0].order, vec![11]);
            assert_eq!(jobs[0].bounds, rect(300., 400., 40., 40.));
            this.shapes[1].kind = ShapeKind::Video;
            assert!(this.component_export_jobs(window, cx).is_err());
        })
        .unwrap();
}

#[gpui::test]
fn component_export_dialog_cancellation_and_batch_capture(cx: &mut TestAppContext) {
    let window = open(cx);
    let output = tempfile::tempdir().unwrap();
    window
        .update(cx, |this, _, cx| {
            this.shapes = vec![
                Shape::new(1, None, ShapeKind::Ellipse, rect(100., 100., 40., 40.)),
                Shape::new(2, None, ShapeKind::Rectangle, rect(200., 100., 60., 40.)),
            ];
            for shape in &mut this.shapes {
                shape.name = "Same".into();
            }
            this.next_id = 3;
            this.set_selection(BTreeSet::from([1]), cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "export-submit");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.is_exporting());
            assert!(!this.can_transfer());
        })
        .unwrap();
    visual.cx.simulate_new_path_selection(|_| None);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(!this.is_exporting());
            assert!(this.export.status.is_none());
            this.set_selection(BTreeSet::from([1, 2]), cx);
        })
        .unwrap();
    click(&mut visual, "export-scale");
    let choice = visual.debug_bounds("export-scale-2").unwrap();
    assert!(choice.bottom() < visual.debug_bounds("export-scale").unwrap().top());
    click(&mut visual, "export-scale-2");
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.export.scale, 2)
        })
        .unwrap();
    click(&mut visual, "export-submit");
    // Changing selection and geometry while the chooser is open does not change the captured export.
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.shapes[0].rect.width = 90.;
            this.set_selection(BTreeSet::from([2]), cx);
        })
        .unwrap();
    visual
        .cx
        .simulate_path_prompt_response(|_| Some(vec![output.path().into()]));
    draw(&mut visual);
    assert_eq!(
        image::open(output.path().join("Same.png")).unwrap().width(),
        80
    );
    assert_eq!(
        image::open(output.path().join("Same (1).png"))
            .unwrap()
            .width(),
        120
    );
    click(&mut visual, "export-format");
    click(&mut visual, "export-format-svg");
    assert!(visual.debug_bounds("export-scale").is_none());
    click(&mut visual, "export-submit");
    visual
        .cx
        .simulate_new_path_selection(|_| Some(output.path().join("shape")));
    draw(&mut visual);
    let svg = std::fs::read_to_string(output.path().join("shape.svg")).unwrap();
    assert!(svg.contains("<path"));
    assert!(!svg.contains("<image"));
}

#[gpui::test]
fn exported_text_keeps_wrapping_rich_styles_and_alignment(cx: &mut TestAppContext) {
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            let mut doc = crate::document::Document::decode(
                &this
                    .snapshot_document(&uuid::Uuid::new_v4().to_string(), cx)
                    .unwrap()
                    .0,
            )
            .unwrap();
            let content = "Hello world\nSecond line";
            let style = TextStyle {
                family: "DejaVu Sans".into(),
                size: 20.,
                align: gpui::TextAlign::Right,
                ..Default::default()
            };
            let red = TextStyle {
                color: rgb(0xff0000),
                ..style.clone()
            };
            doc.pages[0].texts = vec![crate::document::Text {
                id: 10,
                board: None,
                rect: rect(100., 100., 80., 180.),
                layer: Default::default(),
                content: content.into(),
                styles: StyledText {
                    default: style.clone(),
                    runs: vec![
                        StyleRun {
                            range: 0..5,
                            style: red,
                        },
                        StyleRun {
                            range: 5..content.len(),
                            style,
                        },
                    ],
                },
            }];
            doc.pages[0].next_id = 11;
            this.load_document(
                Loaded {
                    needs_upgrade: false,
                    json: serde_json::to_vec(&doc).unwrap(),
                    assets: BTreeMap::new(),
                },
                window,
                cx,
            )
            .unwrap();
            this.set_selection(BTreeSet::from([10]), cx);
            let jobs = this.component_export_jobs(window, cx).unwrap();
            let text = &jobs[0].text[&10];
            assert!(text.len() >= 3);
            assert_eq!(text[0].style.color, rgb(0xff0000));
            assert!(text.iter().any(|f| f.x > 1.));
            assert!(
                text.iter()
                    .map(|f| f.baseline as i32)
                    .collect::<BTreeSet<_>>()
                    .len()
                    >= 3
            );
            let options = crate::scene_render::render_options(true).unwrap();
            let svg = String::from_utf8(jobs[0].render(Format::Svg, 1, &options).unwrap()).unwrap();
            assert!(!svg.contains("<text"));
            assert!(svg.contains("<path"));
            assert!(svg.contains("#ff0000"));
        })
        .unwrap();
}

#[gpui::test]
fn video_selection_exports_original_and_hides_image_options(cx: &mut TestAppContext) {
    use crate::media::{MediaAsset, MediaContent};
    let cache = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    let bytes = b"original video bytes with audio";
    std::fs::write(&cache, bytes).unwrap();
    let window = open(cx);
    window
        .update(cx, |this, _, cx| {
            let coded = gpui::size(gpui::DevicePixels(1), gpui::DevicePixels(1));
            let poster = gpui::SurfaceFrame::new(
                gpui::SurfaceHandle::new(),
                0,
                coded,
                Bounds::new(Default::default(), coded),
                coded,
                gpui::SurfaceFormat::Bgra8,
                [gpui::SurfacePlane::with_offset(vec![128_u8; 4], 0, 4)],
                gpui::SurfaceColorInfo::default(),
            )
            .unwrap();
            let mut video = Shape::new(1, None, ShapeKind::Video, rect(100., 100., 180., 320.));
            video.name = "clip.mp4".into();
            video.media = Some(Arc::new(MediaAsset {
                path: "/missing-original/clip.mp4".into(),
                source: crate::media::Source::file(cache),
                hash: "a".repeat(64),
                width: 1440,
                height: 2560,
                content: std::sync::OnceLock::from(Ok(MediaContent::Video(Arc::new(poster)))),
                video: true,
            }));
            this.shapes = vec![
                video,
                Shape::new(2, None, ShapeKind::Rectangle, rect(400., 100., 40., 30.)),
            ];
            this.next_id = 3;
            this.set_selection(BTreeSet::from([1]), cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    assert!(visual.debug_bounds("export-video-format").is_some());
    assert!(visual.debug_bounds("export-format").is_none());
    assert!(visual.debug_bounds("export-scale").is_none());
    let output = tempfile::tempdir().unwrap();
    click(&mut visual, "export-submit");
    visual
        .cx
        .simulate_new_path_selection(|_| Some(output.path().join("video")));
    draw(&mut visual);
    assert_eq!(
        std::fs::read(output.path().join("video.mp4")).unwrap(),
        bytes
    );
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.open_context_menu(Some(1), true, point(px(500.), px(300.)), window, cx);
        })
        .unwrap();
    draw(&mut visual);
    assert!(visual.debug_bounds("context-export-video").is_some());
    visual.simulate_keystrokes("escape");
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.set_selection(BTreeSet::from([1, 2]), cx);
            let jobs = this.component_export_jobs(window, cx).unwrap();
            assert!(jobs[0].original.is_some());
            assert!(jobs[1].original.is_none());
            this.hierarchy.groups.insert(
                3,
                LayerGroup {
                    name: "Group".into(),
                    board: None,
                    layer: Default::default(),
                },
            );
            this.hierarchy.parents.extend([(1, 3), (2, 3)]);
            this.next_id = 4;
            this.set_selection(BTreeSet::from([3]), cx);
            assert!(this.component_export_jobs(window, cx).is_err());
        })
        .unwrap();
}
