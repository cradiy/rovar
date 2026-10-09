use super::*;
use crate::editor::tests::{click, draw, open};
use gpui::{TestAppContext, VisualTestContext};

#[gpui::test]
fn small_image_import_keeps_intrinsic_size_and_survives_copy_delete_and_source_removal(
    cx: &mut TestAppContext,
) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("generated.png");
    image::RgbaImage::from_pixel(8, 4, image::Rgba([30, 100, 190, 255]))
        .save(&path)
        .unwrap();
    let window = open(cx);
    window
        .update(cx, |this, _, cx| {
            this.view.zoom = 2.;
            this.view.pan = point(83., -42.);
            this.add_artboard(
                Rect {
                    x: 100.,
                    y: 100.,
                    width: 1000.,
                    height: 1000.,
                },
                cx,
            );
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "import-media");
    visual
        .cx
        .simulate_path_prompt_response(|_| Some(vec![path.clone()]));
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.shapes.len(), 1);
            let shape = this.selected_shape().unwrap();
            assert_eq!((shape.rect.width, shape.rect.height), (8., 4.));
            assert_eq!(shape.board, Some(this.boards[0].id));
            let center = this.view.screen(
                crate::scene::rotation::center(shape.rect) + this.parent_origin(shape.board),
            );
            let size = this.bounds.get().size.map(f32::from);
            let (left, right) = this.canvas_insets();
            assert!((center.x - (left + size.width - right) / 2.).abs() < 0.001);
            assert!((center.y - size.height / 2.).abs() < 0.001);
            assert!(this.draw_tool.is_none());
            assert!(this.draft.is_none());
            assert_eq!(this.history.borrow().undo_len(), 2);
        })
        .unwrap();
    assert!(visual.debug_bounds("shape-fill").is_none());
    std::fs::remove_file(&path).unwrap();
    window
        .update(&mut visual.cx, |this, window, cx| {
            let before = this.selected_shape().unwrap().clone();
            assert_eq!(before.kind, ShapeKind::Image);
            assert!((before.rect.width / before.rect.height - 2.).abs() < 0.001);
            assert!(before.layer.aspect_locked);
            assert!(!this.can_flip());
            this.enter_vector_edit(before.id, window, cx);
            assert!(this.vector_edit.is_none());
            this.duplicate_selection(window, cx);
            assert_eq!(this.shapes.len(), 2);
            assert!(Arc::ptr_eq(
                before.media.as_ref().unwrap(),
                this.selected_shape().unwrap().media.as_ref().unwrap()
            ));
            this.delete_selected(cx);
            assert_eq!(this.shapes.len(), 1);
            this.replay_history(false, window, cx);
            assert_eq!(this.shapes.len(), 2);
            this.replay_history(false, window, cx);
            assert_eq!(this.shapes, vec![before]);
            this.replay_history(false, window, cx);
            assert!(this.shapes.is_empty());
            this.replay_history(true, window, cx);
            assert_eq!(this.shapes.len(), 1);
        })
        .unwrap();
    draw(&mut visual);
}

#[gpui::test]
fn large_image_fits_visible_canvas_and_corner_edits_survive_undo(cx: &mut TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("generated-square.png");
    image::RgbaImage::from_pixel(1080, 1080, image::Rgba([30, 100, 190, 255]))
        .save(&path)
        .unwrap();
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    for zoom in [0.5, 1., 2.] {
        window
            .update(&mut visual.cx, |this, _, cx| {
                this.view.zoom = zoom;
                this.sidebar.collapsed = false;
                this.view.pan = point(-240., 125.);
                cx.notify();
            })
            .unwrap();
        click(&mut visual, "import-media");
        visual
            .cx
            .simulate_path_prompt_response(|_| Some(vec![path.clone()]));
        draw(&mut visual);
        window
            .update(&mut visual.cx, |this, _, _| {
                let shape = this.selected_shape().unwrap();
                let rect = shape.rect;
                let p = this.view.screen(point(rect.x, rect.y));
                let size = this.bounds.get().size.map(f32::from);
                let (left, right) = this.canvas_insets();
                assert!(p.x > left && p.x + rect.width * zoom < size.width - right);
                assert!(p.y > 80. && p.y + rect.height * zoom < size.height - 80.);
                assert!(rect.width < 1080. && rect.width > 100.);
                assert!((rect.width * zoom - 320.).abs() < 0.001);
                assert_eq!(rect.width, rect.height);
                let asset = shape.media.as_ref().unwrap();
                assert_eq!((asset.width, asset.height), (1080, 1080));
                assert_eq!(this.view.zoom, zoom);
            })
            .unwrap();
    }
    click(&mut visual, "property-9");
    visual.simulate_keystrokes("secondary-a 2 4");
    click(&mut visual, "corners-independent");
    click(&mut visual, "property-11");
    visual.simulate_keystrokes("secondary-a 4 8");
    click(&mut visual, "corners-unified");
    window
        .update(&mut visual.cx, |this, window, cx| {
            let before = this.selected_shape().unwrap().clone();
            assert_eq!(before.radius, 24.);
            assert_eq!(before.corners, Some([24., 48., 24., 24.]));
            assert_eq!(before.displayed_radii(), [24.; 4]);
            this.replay_history(false, window, cx);
            assert_eq!(
                this.selected_shape().unwrap().displayed_radii(),
                [24., 48., 24., 24.]
            );
            this.replay_history(true, window, cx);
            assert_eq!(this.selected_shape().unwrap(), &before);
        })
        .unwrap();
    draw(&mut visual);
}

#[gpui::test]
fn cancelled_invalid_and_stale_imports_do_not_create_objects(cx: &mut TestAppContext) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("invalid.png");
    std::fs::write(&path, b"not an image").unwrap();
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "import-media");
    visual.cx.simulate_path_prompt_response(|_| None);
    draw(&mut visual);
    click(&mut visual, "import-media");
    visual
        .cx
        .simulate_path_prompt_response(|_| Some(vec![path.clone()]));
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(this.media.error.is_some());
            assert!(this.draw_tool.is_none());
            assert!(!this.media.importing);
        })
        .unwrap();
    click(&mut visual, "import-media");
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.choose_tool(toolbar::Tool::Star, window, cx)
        })
        .unwrap();
    visual
        .cx
        .simulate_path_prompt_response(|_| Some(vec![path]));
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert_eq!(this.draw_tool, Some(DrawTool::Shape(ShapeKind::Star)));
            assert!(this.media.error.is_none());
            assert!(this.shapes.is_empty());
            assert_eq!(this.history.borrow().undo_len(), 0);
        })
        .unwrap();
}

#[gpui::test]
fn hidden_video_pauses_and_deletion_drops_playback_without_putting_it_in_history(
    cx: &mut TestAppContext,
) {
    use gpui_media::{MediaCapabilities, MediaPlaybackSession, MediaResult, PlaybackTimeline};
    use std::sync::Mutex;
    #[derive(Default)]
    struct Calls {
        play: usize,
        pause: usize,
        dropped: bool,
    }
    struct Session(Arc<Mutex<Calls>>);
    impl Drop for Session {
        fn drop(&mut self) {
            self.0.lock().unwrap().dropped = true;
        }
    }
    impl MediaPlaybackSession for Session {
        fn capabilities(&self) -> MediaCapabilities {
            MediaCapabilities::default()
        }
        fn play(&mut self) -> MediaResult<()> {
            self.0.lock().unwrap().play += 1;
            Ok(())
        }
        fn pause(&mut self) -> MediaResult<()> {
            self.0.lock().unwrap().pause += 1;
            Ok(())
        }
        fn timeline(&self) -> PlaybackTimeline {
            PlaybackTimeline::new(Duration::ZERO, Some(Duration::from_secs(10)), true)
        }
    }
    let calls = Arc::new(Mutex::new(Calls::default()));
    let window = open(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    let (sink, output) = MediaOutputSink::channel();
    window
        .update(&mut visual.cx, |this, window, cx| {
            // A generated one-pixel poster; no decoder or local video is needed.
            let coded = size(gpui::DevicePixels(1), gpui::DevicePixels(1));
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
            this.view.zoom = 0.25;
            this.add_media(
                Arc::new(MediaAsset {
                    path: "generated.mp4".into(),
                    source: crate::media::Source::file(
                        tempfile::NamedTempFile::new().unwrap().into_temp_path(),
                    ),
                    hash: String::new(),
                    width: 1920,
                    height: 1080,
                    content: std::sync::OnceLock::from(Ok(MediaContent::Video(Arc::new(poster)))),
                    video: true,
                }),
                window,
                cx,
            );
            let shape = this.selected_shape().unwrap();
            assert_eq!((shape.rect.width, shape.rect.height), (1280., 720.));
            assert_eq!(shape.board, None);
            assert!(this.draw_tool.is_none());
            assert_eq!(this.history.borrow().undo_len(), 1);
            let view = cx.new(|cx| VideoPreview::new(Box::new(Session(calls.clone())), output, cx));
            let controls = cx.new(|cx| VideoControls::new(view.clone(), cx));
            view.update(cx, |v, cx| v.toggle(cx));
            this.media.videos.insert(1, VideoRuntime { view, controls });
        })
        .unwrap();
    draw(&mut visual);
    assert_eq!(calls.lock().unwrap().play, 1);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.shapes[0].layer.hidden = true;
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    assert_eq!(calls.lock().unwrap().pause, 1);
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.shapes[0].layer.hidden = false;
            this.select_shape(1, cx);
            this.delete_selected(cx);
        })
        .unwrap();
    draw(&mut visual);
    assert!(calls.lock().unwrap().dropped);
    assert!(!sink.emit(gpui_media::MediaBackendEvent::Ready));
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.replay_history(false, window, cx);
            assert_eq!(this.shapes.len(), 1);
            assert!(this.media.videos.is_empty());
        })
        .unwrap();
}

#[gpui::test]
fn reopening_displays_scene_before_decoding_only_visible_media(cx: &mut TestAppContext) {
    use crate::document::{self, AssetSource, AssetUse, Document};
    let directory = tempfile::tempdir().unwrap();
    let mut doc = Document::single(crate::document::Page {
        name: "Page 1".into(),
        id: uuid::Uuid::new_v4().to_string(),
        boards: vec![],
        shapes: vec![],
        texts: vec![],
        hierarchy: Default::default(),
        next_id: 3,
        assets: vec![],
    });
    let mut sources = Vec::new();
    for (id, x) in [(1, 400.), (2, 5000.)] {
        let path = directory.path().join(format!("{id}.png"));
        image::RgbaImage::from_pixel(4, 4, image::Rgba([id as u8, 100, 150, 255]))
            .save(&path)
            .unwrap();
        let asset = MediaAsset::load(path).unwrap();
        sources.push(AssetSource {
            hash: asset.hash.clone(),
            name: asset.name(),
            path: asset.source.clone(),
            size: [4, 4],
        });
        doc.pages[0].assets.push(AssetUse {
            object: id,
            fill: false,
            hash: asset.hash.clone(),
        });
        doc.pages[0].shapes.push(Shape::new(
            id,
            None,
            ShapeKind::Image,
            Rect {
                x,
                y: 100.,
                width: 100.,
                height: 100.,
            },
        ));
    }
    let path = directory.path().join("scene.rovar");
    let json = serde_json::to_vec(&doc).unwrap();
    let text_system = cx.update(|cx| cx.text_system().clone());
    document::save_as(&path, &json, &sources, &text_system).unwrap();
    let loaded = document::load(&path).unwrap();
    assert!(loaded.assets.values().all(|asset| asset.is_pending()));
    let window = open(cx);
    window
        .update(cx, |this, window, cx| {
            this.load_document(loaded, window, cx).unwrap();
            this.view.pan = point(0., 0.);
            this.view.zoom = 1.;
            assert_eq!(this.shapes.len(), 2);
            assert!(
                this.shapes
                    .iter()
                    .all(|s| s.media.as_ref().unwrap().is_pending())
            );
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, cx| {
            assert!(!this.shapes[0].media.as_ref().unwrap().is_pending());
            assert!(this.shapes[1].media.as_ref().unwrap().is_pending());
            assert_eq!(this.snapshot_document(&doc.id, cx).unwrap().0, json);
            assert!(!this.can_undo_redo(false));
            this.view.pan.x = -4600.;
            cx.notify();
        })
        .unwrap();
    draw(&mut visual);
    window
        .update(&mut visual.cx, |this, _, _| {
            assert!(!this.shapes[1].media.as_ref().unwrap().is_pending());
            assert!(this.media.error.is_none());
        })
        .unwrap();
}
