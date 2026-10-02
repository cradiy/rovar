use super::*;
use gpui::{MouseButton, TestAppContext, VisualTestContext, point, size};

fn draw(visual: &mut VisualTestContext) {
    visual.cx.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear());
    visual.cx.run_until_parked();
}
fn click(visual: &mut VisualTestContext, selector: &'static str) {
    draw(visual);
    let position = visual
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("Missing {selector}"))
        .center();
    visual.simulate_click(position, Default::default());
    draw(visual);
}
fn right(visual: &mut VisualTestContext, selector: &'static str) {
    draw(visual);
    let position = visual.debug_bounds(selector).unwrap().center();
    visual.simulate_mouse_down(position, MouseButton::Right, Default::default());
    visual.simulate_mouse_up(position, MouseButton::Right, Default::default());
    draw(visual);
}

#[gpui::test]
fn home_pagination_resets_filters_and_clamps_after_last_page_deletion(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    for _ in 0..7 {
        handle
            .update(cx, |studio, window, cx| studio.new_document(window, cx))
            .unwrap();
        cx.run_until_parked();
    }
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "home-tab");
    assert!(visual.debug_bounds("recent-file-5").is_some());
    assert!(visual.debug_bounds("recent-file-6").is_none());
    let footer = visual.debug_bounds("home-pagination").unwrap();
    assert!(footer.bottom() <= px(800.));
    assert!(footer.top() > px(0.));
    click(&mut visual, "home-page-prev");
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.home_page, 0)
        })
        .unwrap();
    click(&mut visual, "home-page-next");
    assert!(visual.debug_bounds("recent-file-0").is_some());
    assert!(visual.debug_bounds("recent-file-1").is_none());
    click(&mut visual, "home-page-next");
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.home_page, 1)
        })
        .unwrap();
    click(&mut visual, "home-all-files");
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.home_page, 0)
        })
        .unwrap();
    click(&mut visual, "home-page-2");
    click(&mut visual, "file-sort-menu");
    click(&mut visual, "sort-created-oldest");
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.home_page, 0)
        })
        .unwrap();
    click(&mut visual, "home-page-next");
    handle
        .update(&mut visual.cx, |studio, _, cx| {
            let name = studio.tabs[0].file.title.clone();
            studio
                .search
                .update(cx, |input, cx| input.set_value(name, cx));
        })
        .unwrap();
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.home_page, 0)
        })
        .unwrap();
    assert!(visual.debug_bounds("recent-file-1").is_none());
    handle
        .update(&mut visual.cx, |studio, _, cx| {
            studio
                .search
                .update(cx, |input, cx| input.set_value("no matching file", cx));
        })
        .unwrap();
    draw(&mut visual);
    assert!(visual.debug_bounds("home-pagination").is_none());
    handle
        .update(&mut visual.cx, |studio, _, cx| {
            studio
                .search
                .update(cx, |input, cx| input.set_value("", cx));
        })
        .unwrap();
    draw(&mut visual);
    click(&mut visual, "home-page-2");
    right(&mut visual, "recent-file-0");
    click(&mut visual, "file-menu-remove");
    click(&mut visual, "confirm-delete-document");
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.session.borrow().recent.len(), 6);
            assert_eq!(studio.home_page, 0);
        })
        .unwrap();
    assert!(visual.debug_bounds("recent-file-5").is_some());
    assert!(visual.debug_bounds("home-page-2").is_none());
}

#[gpui::test]
fn all_files_sort_menu_orders_cards_without_changing_recent(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    for _ in 0..3 {
        handle
            .update(cx, |studio, window, cx| studio.new_document(window, cx))
            .unwrap();
        cx.run_until_parked();
    }
    let paths = handle
        .update(cx, |studio, _, _| {
            let paths: Vec<_> = studio
                .tabs
                .iter()
                .map(|tab| tab.file.path.clone())
                .collect();
            let mut session = studio.session.borrow_mut();
            for (index, tab) in studio.tabs.iter_mut().enumerate() {
                tab.file.created = [100, 200, 300][index];
                tab.file.modified = [30, 10, 20][index];
                *session
                    .recent
                    .iter_mut()
                    .find(|file| file.path == tab.file.path)
                    .unwrap() = tab.file.clone();
            }
            paths
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "home-tab");
    assert!(visual.debug_bounds("file-sort-menu").is_none());
    click(&mut visual, "home-all-files");
    for (option, expected) in [
        (None, 2),
        (Some("sort-created-oldest"), 0),
        (Some("sort-modified-newest"), 0),
        (Some("sort-modified-oldest"), 1),
        (Some("sort-created-newest"), 2),
    ] {
        if let Some(option) = option {
            let before = visual.debug_bounds("recent-file-0").unwrap();
            click(&mut visual, "file-sort-menu");
            assert_eq!(visual.debug_bounds("recent-file-0").unwrap(), before);
            click(&mut visual, option);
        }
        click(&mut visual, "recent-file-0");
        handle
            .update(&mut visual.cx, |studio, _, _| {
                let tab = studio
                    .tabs
                    .iter()
                    .find(|tab| Some(tab.token) == studio.active)
                    .unwrap();
                assert_eq!(tab.file.path, paths[expected]);
            })
            .unwrap();
        click(&mut visual, "home-tab");
    }
    click(&mut visual, "home-recent");
    assert!(visual.debug_bounds("file-sort-menu").is_none());
    click(&mut visual, "recent-file-0");
    handle
        .update(&mut visual.cx, |studio, window, cx| {
            let tab = studio
                .tabs
                .iter()
                .find(|tab| Some(tab.token) == studio.active)
                .unwrap();
            assert_eq!(tab.file.path, paths[0]);
            studio.save_command(window, cx);
        })
        .unwrap();
    visual.cx.run_until_parked();
    let session: Session =
        serde_json::from_slice(&std::fs::read(directory.path().join("session.json")).unwrap())
            .unwrap();
    for (index, path) in paths.iter().enumerate() {
        assert_eq!(
            session
                .recent
                .iter()
                .find(|file| &file.path == path)
                .unwrap()
                .created,
            [100, 200, 300][index]
        );
    }
}

#[gpui::test]
fn home_and_tab_rename_commit_cancel_and_persist_without_changing_document_path(
    cx: &mut TestAppContext,
) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    handle
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    cx.run_until_parked();
    let path = handle
        .update(cx, |studio, _, _| studio.tabs[0].file.path.clone())
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "home-tab");
    right(&mut visual, "recent-file-0");
    click(&mut visual, "file-menu-rename");
    assert!(visual.debug_bounds("document-name-input").is_some());
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("  项目 Alpha  ");
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    assert!(visual.debug_bounds("document-name-input").is_none());
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.tabs[0].file.title, "项目 Alpha");
            assert_eq!(studio.session.borrow().recent[0].title, "项目 Alpha");
            assert_eq!(studio.tabs[0].file.path, path);
        })
        .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let saved: Session =
        serde_json::from_slice(&std::fs::read(directory.path().join("session.json")).unwrap())
            .unwrap();
    assert_eq!(saved.recent[0].title, "项目 Alpha");
    click(&mut visual, "document-tab-1");
    right(&mut visual, "document-tab-1");
    click(&mut visual, "file-menu-rename");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("Discard");
    visual.simulate_keystrokes("escape");
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.tabs[0].file.title, "项目 Alpha")
        })
        .unwrap();
    draw(&mut visual);
    let position = visual.debug_bounds("document-tab-1").unwrap().center();
    visual.simulate_event(gpui::MouseDownEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    visual.simulate_event(gpui::MouseUpEvent {
        position,
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    draw(&mut visual);
    assert!(visual.debug_bounds("document-name-input").is_some());
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input(" ");
    visual.simulate_keystrokes("enter");
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.tabs[0].file.title, "项目 Alpha")
        })
        .unwrap();
}

#[gpui::test]
fn removing_recent_keeps_document_in_all_files_until_explicit_reopen(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    handle
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    cx.run_until_parked();
    let path = handle
        .update(cx, |studio, _, _| studio.tabs[0].file.path.clone())
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    click(&mut visual, "home-tab");
    right(&mut visual, "recent-file-0");
    click(&mut visual, "file-menu-remove");
    assert!(visual.debug_bounds("recent-file-0").is_none());
    assert!(path.exists());
    visual.cx.executor().advance_clock(Duration::from_secs(2));
    draw(&mut visual);
    assert!(visual.debug_bounds("recent-file-0").is_none());
    assert!(
        records::removed_recent(&std::fs::canonicalize(directory.path()).unwrap()).contains(&path)
    );
    // Recovered files must not undo a removal after restarting the home.
    let restored = visual
        .cx
        .open_window(size(px(1280.), px(800.)), |window, cx| {
            Studio::new(directory.path().into(), window, cx)
        });
    let mut restored_visual = VisualTestContext::from_window(restored.into(), &visual.cx);
    draw(&mut restored_visual);
    assert!(restored_visual.debug_bounds("recent-file-0").is_none());
    click(&mut restored_visual, "home-all-files");
    assert!(restored_visual.debug_bounds("recent-file-0").is_some());
    restored
        .update(&mut restored_visual.cx, |_, window, _| {
            window.remove_window()
        })
        .unwrap();
    click(&mut visual, "home-all-files");
    assert!(visual.debug_bounds("recent-file-0").is_some());
    click(&mut visual, "recent-file-0");
    assert!(!records::removed_recent(directory.path()).contains(&path));
    click(&mut visual, "home-tab");
    click(&mut visual, "home-recent");
    assert!(visual.debug_bounds("recent-file-0").is_some());
}

#[gpui::test]
fn document_delete_closes_its_open_tab_in_another_window_and_cannot_be_autosaved_back(
    cx: &mut TestAppContext,
) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let source = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    let (path, session) = source
        .update(cx, |studio, window, cx| {
            studio.new_document(window, cx);
            (studio.tabs[0].file.path.clone(), studio.session.clone())
        })
        .unwrap();
    cx.run_until_parked();
    source
        .update(cx, |studio, window, cx| studio.save_command(window, cx))
        .unwrap();
    cx.run_until_parked();
    let home = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        let mut studio = Studio::new(directory.path().into(), window, cx);
        studio.tabs.clear();
        studio.session = session;
        studio
    });
    let mut visual = VisualTestContext::from_window(home.into(), cx);
    click(&mut visual, "home-all-files");
    right(&mut visual, "recent-file-0");
    click(&mut visual, "file-menu-rename");
    visual.simulate_keystrokes("ctrl-a");
    visual.simulate_input("Shared name");
    visual.simulate_keystrokes("enter");
    source
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.tabs[0].file.title, "Shared name")
        })
        .unwrap();
    right(&mut visual, "recent-file-0");
    click(&mut visual, "file-menu-remove");
    assert!(path.exists());
    assert!(visual.debug_bounds("delete-document-dialog").is_some());
    click(&mut visual, "cancel-delete-document");
    assert!(path.exists());
    assert!(visual.debug_bounds("delete-document-dialog").is_none());
    right(&mut visual, "recent-file-0");
    click(&mut visual, "file-menu-remove");
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    assert!(path.exists());
    assert!(visual.debug_bounds("delete-document-dialog").is_none());
    right(&mut visual, "recent-file-0");
    click(&mut visual, "file-menu-remove");
    visual.simulate_keystrokes("enter");
    assert!(path.exists());
    source
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.tabs.len(), 1)
        })
        .unwrap();
    source
        .update(&mut visual.cx, |studio, _, _| studio.tabs[0].saving = true)
        .unwrap();
    click(&mut visual, "confirm-delete-document");
    assert!(path.exists());
    source
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.tabs.len(), 1);
            studio.tabs[0].saving = false;
        })
        .unwrap();
    right(&mut visual, "recent-file-0");
    click(&mut visual, "file-menu-remove");
    click(&mut visual, "confirm-delete-document");
    assert!(!path.exists());
    source
        .update(&mut visual.cx, |studio, _, _| {
            assert!(studio.tabs.is_empty());
            assert!(studio.active.is_none());
        })
        .unwrap();
    visual.cx.executor().advance_clock(Duration::from_secs(2));
    draw(&mut visual);
    assert!(!path.exists());
    assert!(visual.debug_bounds("recent-file-0").is_none());
    let saved: Session =
        serde_json::from_slice(&std::fs::read(directory.path().join("session.json")).unwrap())
            .unwrap();
    assert!(!saved.open.contains(&path));
    assert!(saved.recent.is_empty());
}

#[gpui::test]
fn history_buttons_follow_real_edits_undo_redo_and_tab_switches(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    handle
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    click(&mut visual, "document-undo-disabled");
    assert!(visual.debug_bounds("document-redo-disabled").is_some());
    click(&mut visual, "add-rectangle");
    visual.simulate_mouse_down(
        point(px(430.), px(220.)),
        MouseButton::Left,
        Default::default(),
    );
    visual.simulate_mouse_move(
        point(px(600.), px(350.)),
        Some(MouseButton::Left),
        Default::default(),
    );
    visual.simulate_mouse_up(
        point(px(600.), px(350.)),
        MouseButton::Left,
        Default::default(),
    );
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |studio, _, cx| {
            let editor = studio.active_editor().unwrap();
            let json = editor
                .read(cx)
                .snapshot_document(&studio.tabs[0].document_id, cx)
                .unwrap()
                .0;
            assert_eq!(
                crate::document::Document::decode(&json).unwrap().pages[0]
                    .shapes
                    .len(),
                1,
                "drawing must create a shape"
            );
            assert!(
                editor.read(cx).can_undo_redo(false),
                "shape creation must enable undo"
            );
        })
        .unwrap();
    click(&mut visual, "document-undo-enabled");
    assert!(visual.debug_bounds("document-undo-disabled").is_some());
    click(&mut visual, "document-redo-enabled");
    assert!(visual.debug_bounds("document-undo-enabled").is_some());
    assert!(visual.debug_bounds("document-redo-disabled").is_some());
    visual.simulate_keystrokes("ctrl-n");
    draw(&mut visual);
    assert!(visual.debug_bounds("document-undo-disabled").is_some());
    click(&mut visual, "document-tab-1");
    assert!(visual.debug_bounds("document-undo-enabled").is_some());
}

#[gpui::test]
fn all_files_keeps_new_saved_and_imported_documents(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let window = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(root.path().into(), window, cx)
    });
    window
        .update(cx, |studio, window, cx| studio.new_document(window, cx))
        .unwrap();
    cx.run_until_parked();
    let original = window
        .update(cx, |studio, _, _| studio.tabs[0].file.path.clone())
        .unwrap();
    let imported = external.path().join("Imported.rovar");
    std::fs::copy(&original, &imported).unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    click(&mut visual, "home-tab");
    click(&mut visual, "home-all-files");
    assert!(visual.debug_bounds("recent-file-0").is_some());
    click(&mut visual, "recent-file-0");
    visual.simulate_keystrokes("ctrl-s");
    draw(&mut visual);
    click(&mut visual, "home-tab");
    assert!(visual.debug_bounds("recent-file-0").is_some());
    assert!(visual.debug_bounds("recent-file-1").is_none());
    window
        .update(&mut visual.cx, |studio, window, cx| {
            studio.open_path(imported.clone(), window, cx)
        })
        .unwrap();
    draw(&mut visual);
    click(&mut visual, "home-tab");
    assert!(visual.debug_bounds("recent-file-0").is_some());
    assert!(visual.debug_bounds("recent-file-1").is_some());
    assert!(visual.debug_bounds("recent-file-2").is_none());
    // Imported documents can be deleted through the same confirmation flow.
    right(&mut visual, "recent-file-0");
    click(&mut visual, "file-menu-remove");
    click(&mut visual, "confirm-delete-document");
    assert!(original.exists());
    assert!(imported.exists());
    assert!(visual.debug_bounds("recent-file-0").is_some());
    assert!(visual.debug_bounds("recent-file-1").is_none());
}

#[gpui::test]
fn failed_open_shows_dismissible_dialog_and_preserves_other_documents(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let invalid = directory.path().join("Unsupported.rovar");
    let mut writer = rovar_format::Writer::create(&invalid).unwrap();
    writer.commit().unwrap();
    drop(writer);
    let mut bytes = std::fs::read(&invalid).unwrap();
    bytes[12..20].fill(0);
    std::fs::write(&invalid, &bytes).unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    handle
        .update(cx, |studio, window, cx| {
            studio.open_path(invalid.clone(), window, cx);
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    assert!(visual.debug_bounds("open-error-dialog").is_some());
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert!(studio.tabs.is_empty());
            assert!(studio.active.is_none());
        })
        .unwrap();
    visual.simulate_keystrokes("ctrl-n");
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert!(studio.tabs.is_empty())
        })
        .unwrap();
    click(&mut visual, "dismiss-open-error");
    assert!(visual.debug_bounds("open-error-dialog").is_none());
    visual.simulate_keystrokes("ctrl-n");
    draw(&mut visual);
    let (token, editor) = handle
        .update(&mut visual.cx, |studio, window, cx| {
            let token = studio.active.unwrap();
            let editor = studio.active_editor().unwrap();
            studio.open_path(invalid.clone(), window, cx);
            studio.select_tab(Some(token), window, cx);
            (token, editor)
        })
        .unwrap();
    draw(&mut visual);
    assert!(visual.debug_bounds("open-error-dialog").is_some());
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.active, Some(token));
            assert_eq!(studio.active_editor(), Some(editor));
            assert_eq!(studio.tabs.len(), 1);
        })
        .unwrap();
    visual.simulate_keystrokes("escape");
    draw(&mut visual);
    assert!(visual.debug_bounds("open-error-dialog").is_none());
    assert_eq!(std::fs::read(&invalid).unwrap(), bytes);
}

#[gpui::test]
fn simultaneous_open_failures_are_queued(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let directory = tempfile::tempdir().unwrap();
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        Studio::new(directory.path().into(), window, cx)
    });
    handle
        .update(cx, |studio, window, cx| {
            for name in ["First.rovar", "Second.rovar"] {
                let path = directory.path().join(name);
                std::fs::write(&path, b"invalid").unwrap();
                studio.open_path(path, window, cx);
            }
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert_eq!(studio.open_errors.len(), 2);
            assert!(studio.tabs.is_empty());
        })
        .unwrap();
    click(&mut visual, "dismiss-open-error");
    assert!(visual.debug_bounds("open-error-dialog").is_some());
    visual.simulate_keystrokes("enter");
    draw(&mut visual);
    assert!(visual.debug_bounds("open-error-dialog").is_none());
    visual.simulate_keystrokes("ctrl-n");
    draw(&mut visual);
    handle
        .update(&mut visual.cx, |studio, _, _| {
            assert!(studio.active_editor().is_some())
        })
        .unwrap();
}
