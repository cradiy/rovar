use super::*;
use gpui::{TestAppContext, VisualTestContext, size};

#[test]
fn internal_paths_accept_unsaved_documents_in_existing_directories() {
    let directory = tempfile::tempdir().unwrap();
    let documents = directory.path().join("documents");
    std::fs::create_dir_all(&documents).unwrap();
    let name = format!("{}.rovar", uuid::Uuid::new_v4());
    let path = documents.join(&name);
    assert!(files::is_internal(directory.path(), &path));
    let canonical_root = std::fs::canonicalize(directory.path()).unwrap();
    assert!(files::is_internal(&canonical_root, &path));
    std::fs::write(&path, []).unwrap();
    assert!(files::is_internal(directory.path(), &path));
    assert!(!files::is_internal(
        directory.path(),
        &directory.path().join(&name)
    ));
    assert!(!files::is_internal(
        directory.path(),
        &documents.join("invalid.rovar")
    ));
}

#[gpui::test]
fn closing_new_documents_in_a_fresh_workspace_saves_before_closing(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().join("workspace"), window, cx)
    });
    let tab_path = handle
        .update(cx, |studio, window, cx| {
            studio.new_document(window, cx);
            let path = studio.tabs[0].file.path.clone();
            studio.close_tab(studio.tabs[0].token, window, cx);
            path
        })
        .unwrap();
    cx.run_until_parked();
    handle
        .update(cx, |studio, _, _| assert!(studio.tabs.is_empty()))
        .unwrap();
    assert!(crate::document::load(&tab_path).is_ok());
    let window_path = handle
        .update(cx, |studio, window, cx| {
            studio.new_document(window, cx);
            let path = studio.tabs[0].file.path.clone();
            studio.begin_close(window, cx);
            path
        })
        .unwrap();
    cx.run_until_parked();
    assert!(handle.update(cx, |_, _, _| ()).is_err());
    assert!(crate::document::load(&window_path).is_ok());
}

#[gpui::test]
fn autosave_skips_idle_snapshots_but_persists_view_and_content_changes(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    window
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |studio, window, cx| {
            studio.autosave(window, cx);
        })
        .unwrap();
    cx.run_until_parked();
    let (editor, count, path) = window
        .update(cx, |studio, _, cx| {
            let tab = &studio.tabs[0];
            let editor = tab.editor.clone().unwrap();
            let count = editor.read(cx).snapshot_count.get();
            (editor, count, tab.file.path.clone())
        })
        .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
    }
    window
        .update(cx, |studio, window, cx| {
            assert_eq!(editor.read(cx).snapshot_count.get(), count);
            editor.update(cx, |editor, _| editor.restore_view([40., 60., 2.]));
            studio.autosave(window, cx);
            assert_eq!(editor.read(cx).snapshot_count.get(), count);
            assert_eq!(
                studio.tabs[0].file.views.pages[&studio.tabs[0].file.views.active],
                [40., 60., 2.]
            );
            set_rectangle(studio, 75., window, cx);
            let before_save = editor.read(cx).snapshot_count.get();
            studio.autosave(window, cx);
            assert_eq!(editor.read(cx).snapshot_count.get(), before_save + 1);
        })
        .unwrap();
    cx.run_until_parked();
    assert_ne!(std::fs::read(&path).unwrap(), bytes);
    let saved = crate::document::load(&path)
        .unwrap()
        .into_document()
        .unwrap();
    assert_eq!(saved.pages[0].shapes[0].rect.x, 75.);
    window
        .update(cx, |studio, window, cx| {
            let count = editor.read(cx).snapshot_count.get();
            studio.autosave(window, cx);
            assert_eq!(editor.read(cx).snapshot_count.get(), count);
        })
        .unwrap();
}

fn set_rectangle(studio: &mut Studio, x: f32, window: &mut Window, cx: &mut Context<Studio>) {
    let tab = studio
        .tabs
        .iter()
        .find(|tab| Some(tab.token) == studio.active)
        .unwrap();
    let editor = tab.editor.clone().unwrap();
    let (json, _) = editor
        .read(cx)
        .snapshot_document(&tab.document_id, cx)
        .unwrap();
    let mut document = crate::document::Document::decode(&json).unwrap();
    document.pages[0].shapes = vec![crate::shape::Shape::new(
        1,
        None,
        crate::shape::ShapeKind::Rectangle,
        crate::artboard::Rect {
            x,
            y: 20.,
            width: 100.,
            height: 80.,
        },
    )];
    document.pages[0].next_id = 2;
    editor
        .update(cx, |editor, cx| {
            editor.load_document(
                crate::document::Loaded {
                    needs_upgrade: false,
                    json: serde_json::to_vec(&document).unwrap(),
                    assets: Default::default(),
                },
                window,
                cx,
            )
        })
        .unwrap();
}

#[gpui::test]
fn save_is_internal_and_exported_files_are_independent(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let workspace = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let output = external.path().join("Design.rovar");
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(workspace.path().into(), window, cx)
    });
    window
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |studio, window, cx| {
            set_rectangle(studio, 25., window, cx)
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear());
    visual.simulate_keystrokes("ctrl-s");
    visual.cx.run_until_parked();
    assert!(!visual.cx.did_prompt_for_new_path());
    let (internal, id, json) = window
        .update(&mut visual.cx, |studio, _, _| {
            let tab = &studio.tabs[0];
            assert_eq!(
                std::fs::canonicalize(tab.file.path.parent().unwrap()).unwrap(),
                std::fs::canonicalize(workspace.path().join("documents")).unwrap()
            );
            (
                tab.file.path.clone(),
                tab.document_id.clone(),
                tab.last_saved.clone(),
            )
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    let menu = visual.debug_bounds("app-menu").unwrap().center();
    visual.simulate_click(menu, Default::default());
    visual.update(|window, cx| window.draw(cx).clear());
    let export = visual.debug_bounds("export-document").unwrap().center();
    visual.simulate_click(export, Default::default());
    assert!(visual.cx.did_prompt_for_new_path());
    visual.cx.simulate_new_path_selection(|_| None);
    visual.cx.run_until_parked();
    assert!(!output.exists());
    visual.simulate_keystrokes("ctrl-shift-e");
    visual
        .cx
        .simulate_new_path_selection(|_| Some(output.clone()));
    visual.cx.run_until_parked();
    assert_eq!(crate::document::load(&output).unwrap().json, json);
    let exported_bytes = std::fs::read(&output).unwrap();
    window
        .update(&mut visual.cx, |studio, window, cx| {
            assert_eq!(studio.tabs[0].file.path, internal);
            assert_eq!(
                studio.tabs[0].file.title,
                crate::i18n::message("untitled-name", &[("id", "1".into())])
            );
            set_rectangle(studio, 75., window, cx);
            studio.save_command(window, cx);
        })
        .unwrap();
    visual.cx.run_until_parked();
    assert_eq!(std::fs::read(&output).unwrap(), exported_bytes);
    assert_ne!(crate::document::load(&internal).unwrap().json, json);
    window
        .update(&mut visual.cx, |studio, window, cx| {
            studio.close_tab(studio.tabs[0].token, window, cx);
            studio.open_path(output.clone(), window, cx);
        })
        .unwrap();
    visual.cx.run_until_parked();
    let imported = window
        .update(&mut visual.cx, |studio, window, cx| {
            assert_eq!(studio.tabs.len(), 1);
            let tab = &studio.tabs[0];
            assert_ne!(tab.document_id, id);
            assert!(files::is_internal(workspace.path(), &tab.file.path));
            assert_eq!(tab.file.title, "Design");
            let path = tab.file.path.clone();
            set_rectangle(studio, 125., window, cx);
            studio.save_command(window, cx);
            path
        })
        .unwrap();
    visual.cx.run_until_parked();
    assert_eq!(std::fs::read(&output).unwrap(), exported_bytes);
    std::fs::remove_file(&output).unwrap();
    window
        .update(&mut visual.cx, |studio, window, cx| {
            studio.close_tab(studio.tabs[0].token, window, cx);
            studio.open_path(imported.clone(), window, cx);
        })
        .unwrap();
    visual.cx.run_until_parked();
    let loaded = crate::document::load(&imported).unwrap();
    assert_eq!(
        crate::document::Document::decode(&loaded.json)
            .unwrap()
            .pages[0]
            .shapes[0]
            .rect
            .x,
        125.
    );
}

#[gpui::test]
fn export_failure_keeps_the_document_and_close_waits_for_export(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let workspace = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let output = external.path().join("Design.rovar");
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(workspace.path().into(), window, cx)
    });
    window
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    cx.run_until_parked();
    let internal = window
        .update(cx, |studio, window, cx| {
            let internal = studio.tabs[0].file.path.clone();
            studio.export_to(1, internal.clone(), window, cx);
            assert!(studio.error.is_some());
            assert!(!studio.tabs[0].exporting);
            studio.error = None;
            studio.export_to(1, external.path().join("missing/output.rovar"), window, cx);
            internal
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |studio, window, cx| {
            assert!(studio.error.is_some());
            assert_eq!(studio.tabs[0].file.path, internal);
            assert!(!studio.tabs[0].exporting);
            set_rectangle(studio, 50., window, cx);
            studio.export_to(1, output.clone(), window, cx);
            studio.close_tab(1, window, cx);
            assert!(!studio.tabs.is_empty());
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |studio, _, _| assert!(studio.tabs.is_empty()))
        .unwrap();
    assert_eq!(
        crate::document::load(&output).unwrap().json,
        crate::document::load(&internal).unwrap().json
    );
}

#[gpui::test]
fn closing_a_tab_waits_for_component_export(cx: &mut TestAppContext) {
    fn draw(visual: &mut VisualTestContext) {
        visual.cx.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear());
        visual.cx.run_until_parked();
    }
    fn click(visual: &mut VisualTestContext, selector: &'static str) {
        draw(visual);
        let position = visual.debug_bounds(selector).unwrap().center();
        visual.simulate_click(position, Default::default());
        draw(visual);
    }
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    for succeeds in [false, true] {
        window
            .update(&mut visual.cx, |studio, window, cx| {
                studio.new_document(window, cx)
            })
            .unwrap();
        click(&mut visual, "add-rectangle");
        visual.simulate_mouse_down(
            gpui::point(px(450.), px(300.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        visual.simulate_mouse_move(
            gpui::point(px(600.), px(420.)),
            Some(gpui::MouseButton::Left),
            Default::default(),
        );
        visual.simulate_mouse_up(
            gpui::point(px(600.), px(420.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        draw(&mut visual);
        click(&mut visual, "export-submit");
        window
            .update(&mut visual.cx, |studio, window, cx| {
                assert!(
                    studio.tabs[0]
                        .editor
                        .as_ref()
                        .unwrap()
                        .read(cx)
                        .is_exporting()
                );
                studio.close_tab(studio.tabs[0].token, window, cx);
            })
            .unwrap();
        draw(&mut visual);
        window
            .update(&mut visual.cx, |studio, _, _| {
                assert_eq!(studio.tabs.len(), 1)
            })
            .unwrap();
        let path = output.path().join(if succeeds {
            "Rectangle.png"
        } else {
            "missing/Rectangle.png"
        });
        visual
            .cx
            .simulate_new_path_selection(|_| Some(path.clone()));
        draw(&mut visual);
        window
            .update(&mut visual.cx, |studio, window, cx| {
                studio.autosave(window, cx)
            })
            .unwrap();
        draw(&mut visual);
        window
            .update(&mut visual.cx, |studio, window, cx| {
                if succeeds {
                    assert!(studio.tabs.is_empty());
                    assert_eq!(image::open(&path).unwrap().width(), 150);
                } else {
                    assert_eq!(studio.tabs.len(), 1);
                    assert!(!studio.tabs[0].close_after_save);
                    assert!(studio.error.is_some());
                    assert!(!path.exists());
                    studio.error = None;
                    studio.close_tab(studio.tabs[0].token, window, cx);
                }
            })
            .unwrap();
    }
}

#[gpui::test]
fn closing_waits_for_component_storage_and_keeps_failed_saves_visible(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let root = tempfile::tempdir().unwrap();
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(root.path().into(), window, cx)
    });
    cx.run_until_parked();
    window
        .update(cx, |studio, window, cx| {
            studio.library.update(cx, |library, cx| {
                library.save("Broken".into(), b"invalid".to_vec(), vec![], [1., 1.], cx)
            });
            studio.begin_close(window, cx);
            assert!(studio.closing && studio.awaiting_library);
        })
        .unwrap();
    cx.run_until_parked();
    window
        .update(cx, |studio, window, cx| {
            assert!(!studio.closing && studio.error.is_some());
            let document = crate::document::Document::single(crate::document::Page {
                name: "Page 1".into(),
                id: uuid::Uuid::new_v4().to_string(),
                boards: vec![],
                texts: vec![],
                shapes: vec![crate::shape::Shape::new(
                    1,
                    None,
                    crate::shape::ShapeKind::Rectangle,
                    crate::artboard::Rect {
                        x: 0.,
                        y: 0.,
                        width: 80.,
                        height: 60.,
                    },
                )],
                hierarchy: Default::default(),
                next_id: 2,
                assets: vec![],
            });
            studio.library.update(cx, |library, cx| {
                library.save(
                    "Stored".into(),
                    serde_json::to_vec(&document).unwrap(),
                    vec![],
                    [80., 60.],
                    cx,
                )
            });
            studio.begin_close(window, cx);
            assert!(studio.closing && studio.awaiting_library);
        })
        .unwrap();
    cx.run_until_parked();
    assert!(window.update(cx, |_, _, _| ()).is_err());
    let path = std::fs::read_dir(root.path().join("components"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|s| s == "rovar"))
        .unwrap();
    assert_eq!(
        crate::document::load(&path)
            .unwrap()
            .into_document()
            .unwrap()
            .pages[0]
            .shapes
            .len(),
        1
    );
}
