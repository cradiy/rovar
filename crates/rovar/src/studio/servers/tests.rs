use super::*;
use gpui::{TestAppContext, VisualTestContext, size};

struct LoginPage(Entity<Studio>);

#[gpui::test]
fn account_settings_validate_passwords_and_reauthentication_preserves_tabs(
    cx: &mut TestAppContext,
) {
    cx.update(uic::init);
    let root = tempfile::tempdir().unwrap();
    let window = cx.open_window(size(px(360.), px(740.)), |window, cx| {
        Studio::new(root.path().into(), window, cx)
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let id = window
        .update(&mut visual.cx, |studio, window, cx| {
            let id = studio.remote.update(cx, |r, cx| {
                r.connect(
                    "https://example.test".into(),
                    rovar_api::Identity {
                        server_id: "test".into(),
                        user_id: "alice".into(),
                        username: "Alice".into(),
                        api_version: rovar_api::VERSION,
                        registration: rovar_api::RegistrationPolicy {
                            personal: true,
                            teams: true,
                        },
                        spaces: vec![rovar_api::Space {
                            id: "personal".into(),
                            name: "Personal".into(),
                            kind: "personal".into(),
                            role: "owner".into(),
                        }],
                    },
                    "token".into(),
                    cx,
                )
                .unwrap()
            });
            studio.new_document(window, cx);
            studio.open_servers(None, window, cx);
            let panel = studio.servers.as_mut().unwrap();
            panel.view = View::Settings;
            panel.account = Some(id.clone());
            panel.mode = "password";
            panel
                .password
                .update(cx, |input, cx| input.set_value("current", cx));
            panel
                .new_password
                .update(cx, |input, cx| input.set_value("一二三四五", cx));
            studio.save_account_password(window, cx);
            assert_eq!(
                studio.servers.as_ref().unwrap().error.as_deref(),
                Some(t("server-password-short"))
            );
            let panel = studio.servers.as_mut().unwrap();
            panel
                .new_password
                .update(cx, |input, cx| input.set_value("一二三四五六", cx));
            panel
                .confirm_password
                .update(cx, |input, cx| input.set_value("different", cx));
            studio.save_account_password(window, cx);
            let panel = studio.servers.as_ref().unwrap();
            assert_eq!(panel.error.as_deref(), Some(t("account-password-mismatch")));
            assert!(!panel.busy);
            assert_eq!(panel.password.read(cx).value().as_ref(), "current");
            id
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    let dialog = visual.debug_bounds("server-dialog").unwrap();
    for field in [
        "account-current-password",
        "account-new-password",
        "account-confirm-password",
    ] {
        let bounds = visual.debug_bounds(field).unwrap();
        assert!(bounds.left() >= dialog.left() && bounds.right() <= dialog.right());
    }
    window
        .update(&mut visual.cx, |studio, window, cx| {
            let active = studio.active;
            let count = studio.tabs.len();
            studio.reauthenticate(id.clone(), window, cx);
            assert_eq!(studio.active, active);
            assert_eq!(studio.tabs.len(), count);
            let panel = studio.servers.as_ref().unwrap();
            assert_eq!(panel.resume.as_ref(), Some(&id));
            assert_eq!(panel.username.read(cx).value().as_ref(), "Alice");
        })
        .unwrap();
}

impl Render for LoginPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |studio, cx| {
            studio.web_login_page(cx).into_any_element()
        })
    }
}

#[gpui::test]
fn web_login_shares_registration_and_language_without_server_controls(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let root = tempfile::tempdir().unwrap();
    let window = cx.open_window(size(px(840.), px(720.)), |window, cx| {
        let studio = cx.new(|cx| Studio::new(root.path().into(), window, cx));
        studio.update(cx, |studio, cx| {
            studio.open_servers(None, window, cx);
            studio.server_view(View::Login, window, cx);
            studio.servers.as_mut().unwrap().registration = Some(rovar_api::RegistrationPolicy {
                personal: true,
                teams: true,
            });
        });
        LoginPage(studio)
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("web-login-page").is_some());
    assert!(visual.debug_bounds("server-user").is_some());
    assert!(visual.debug_bounds("language-menu").is_some());
    assert!(visual.debug_bounds("server-dialog").is_none());
    assert!(visual.debug_bounds("server-url").is_none());
    let login_height = visual.debug_bounds("login-card").unwrap().size.height;
    window
        .update(&mut visual.cx, |page, window, cx| {
            page.0.update(cx, |studio, cx| {
                studio.registration_options(window, cx);
                let panel = studio.servers.as_mut().unwrap();
                panel.mode = "team";
                panel
                    .username
                    .update(cx, |input, cx| input.set_value("Alice", cx));
                panel
                    .team_name
                    .update(cx, |input, cx| input.set_value("Studio", cx));
                studio.change_language(crate::i18n::Language::Chinese, window, cx);
                let panel = studio.servers.as_ref().unwrap();
                assert_eq!(panel.username.read(cx).value().as_ref(), "Alice");
                assert_eq!(panel.team_name.read(cx).value().as_ref(), "Studio");
                assert_eq!(panel.mode, "team");
            })
        })
        .unwrap();
    for _ in 0..3 {
        visual.update(|window, cx| window.draw(cx).clear());
    }
    let card = visual.debug_bounds("login-card").unwrap();
    let team = visual.debug_bounds("server-team").unwrap();
    assert!(card.size.height > login_height);
    assert!(card.contains(&team.bottom_right()));
    window
        .update(&mut visual.cx, |page, window, cx| {
            page.0.update(cx, |studio, cx| {
                let panel = studio.servers.as_ref().unwrap();
                panel
                    .password
                    .update(cx, |input, cx| input.set_value("一二三四五", cx));
                studio.server_login(window, cx);
                let panel = studio.servers.as_ref().unwrap();
                assert_eq!(panel.error.as_deref(), Some(t("server-password-short")));
                assert!(!panel.busy);
                assert_eq!(panel.password.read(cx).value().as_ref(), "一二三四五");
                panel
                    .password
                    .update(cx, |input, cx| input.set_value("一二三四五六", cx));
                panel
                    .team_name
                    .update(cx, |input, cx| input.set_value("  ", cx));
                studio.server_login(window, cx);
                let panel = studio.servers.as_ref().unwrap();
                assert_eq!(panel.error.as_deref(), Some(t("server-team-required")));
                assert!(!panel.busy);
            });
        })
        .unwrap();
    crate::i18n::set_language(crate::i18n::Language::English).unwrap();
}

#[gpui::test]
fn address_first_dialog_renders_and_groups_accounts_across_spaces(cx: &mut TestAppContext) {
    cx.update(uic::init);
    let root = tempfile::tempdir().unwrap();
    let window = cx.open_window(size(px(840.), px(600.)), |window, cx| {
        Studio::new(root.path().into(), window, cx)
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.open_servers(None, window, cx);
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("server-url").is_some());
    assert!(visual.debug_bounds("server-user").is_none());

    let id = window
        .update(&mut visual.cx, |this, window, cx| {
            let id = this.remote.update(cx, |remote, cx| {
                remote
                    .save_server("https://second.example".into(), "Second studio".into(), cx)
                    .unwrap();
                remote
                    .save_server("https://second.example".into(), "Second studio".into(), cx)
                    .unwrap();
                remote
                    .connect(
                        "https://first.example".into(),
                        rovar_api::Identity {
                            server_id: "first".into(),
                            user_id: "alice".into(),
                            username: "Alice".into(),
                            api_version: rovar_api::VERSION,
                            registration: rovar_api::RegistrationPolicy {
                                personal: true,
                                teams: true,
                            },
                            spaces: ["personal", "team"]
                                .into_iter()
                                .map(|kind| rovar_api::Space {
                                    id: kind.into(),
                                    name: kind.into(),
                                    kind: kind.into(),
                                    role: "owner".into(),
                                })
                                .collect(),
                        },
                        "test-token-not-for-disk".into(),
                        cx,
                    )
                    .unwrap()
            });
            assert_eq!(this.remote.read(cx).servers().len(), 2);
            assert_eq!(
                this.remote.read(cx).accounts("https://first.example").len(),
                1
            );
            assert!(
                this.remote
                    .read(cx)
                    .accounts("https://second.example")
                    .is_empty()
            );
            let panel = this.servers.as_mut().unwrap();
            panel.selected = Some("https://first.example".into());
            panel.registration = Some(rovar_api::RegistrationPolicy {
                personal: true,
                teams: true,
            });
            this.server_view(View::Accounts, window, cx);
            id
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("server-account-0").is_some());
    assert!(visual.debug_bounds("server-account-1").is_none());
    assert!(visual.debug_bounds("server-user").is_none());
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.server_view(View::Login, window, cx)
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("server-user").is_some());
    assert!(visual.debug_bounds("server-url").is_none());
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.registration_options(window, cx);
            this.servers.as_mut().unwrap().mode = "team";
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("server-team").is_some());
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.remote
                .update(cx, |remote, cx| remote.sign_out(&id, cx));
            assert_eq!(this.remote.read(cx).servers().len(), 2);
            assert!(!this.remote.read(cx).accounts("https://first.example")[0].authenticated);
            this.server_view(View::Servers, window, cx);
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("saved-server-0").is_some());
    assert!(visual.debug_bounds("saved-server-1").is_some());
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.rename_server("https://first.example".into(), window, cx);
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("server-name").is_some());
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.servers
                .as_ref()
                .unwrap()
                .name
                .update(cx, |input, cx| input.set_value("My studio", cx));
            this.save_server_name(window, cx);
            assert_eq!(
                this.remote.read(cx).server_name("https://first.example"),
                "My studio"
            );
            assert_eq!(
                this.remote.read(cx).accounts("https://first.example")[0].id,
                id
            );
        })
        .unwrap();
    let saved = std::fs::read_to_string(root.path().join("servers.json")).unwrap();
    let catalog: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(catalog["servers"].as_object().unwrap().len(), 2);
    assert_eq!(catalog["servers"]["https://first.example"], "My studio");
    assert!(!saved.contains("test-token-not-for-disk"));
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.servers = None;
            this.new_document(window, cx);
            let tab = &this.tabs[0];
            let path = tab.file.path.clone();
            let title = tab.file.title.clone();
            this.remote.update(cx, |remote, cx| {
                remote.track(path, id, title, rovar_api::Kind::Document, cx)
            });
            assert!(
                this.remote_status(cx).is_none(),
                "Pending uploads must not add a full-width status row"
            );
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    let badge = visual.debug_bounds("server-badge").unwrap();
    let tabs = visual.debug_bounds("document-tabs").unwrap();
    assert!(
        badge.top() >= tabs.top()
            && badge.bottom() <= tabs.bottom()
            && badge.left() >= tabs.right(),
        "Server control belongs in the titlebar, clear of document tabs"
    );
    assert!(visual.debug_bounds("server-info-panel").is_none());
    visual.simulate_mouse_move(badge.center(), None, Default::default());
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("server-info-panel").is_none());
    visual.simulate_mouse_down(badge.center(), gpui::MouseButton::Left, Default::default());
    visual.simulate_mouse_up(badge.center(), gpui::MouseButton::Left, Default::default());
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("server-info-panel").is_some());
    visual.simulate_keystrokes("escape");
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("server-info-panel").is_none());
    let toggle = visual.debug_bounds("toggle-layers").unwrap().center();
    visual.simulate_mouse_down(toggle, gpui::MouseButton::Left, Default::default());
    visual.simulate_mouse_up(toggle, gpui::MouseButton::Left, Default::default());
    visual.update(|window, cx| window.draw(cx).clear());
    let collapsed_badge = visual.debug_bounds("server-badge").unwrap();
    assert_eq!(collapsed_badge, badge);
    visual.simulate_resize(size(px(1280.), px(800.)));
    visual.update(|window, cx| window.draw(cx).clear());
    let wide_badge = visual.debug_bounds("server-badge").unwrap();
    assert!(wide_badge.left() >= visual.debug_bounds("document-tabs").unwrap().right());
    window
        .update(&mut visual.cx, |this, _, cx| {
            this.active_editor().unwrap().update(cx, |editor, cx| {
                editor.restore_view([100., -200., 2.5]);
                cx.notify();
            });
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    assert_eq!(wide_badge, visual.debug_bounds("server-badge").unwrap());
    window
        .update(&mut visual.cx, |this, window, cx| {
            this.new_document(window, cx);
            this.strip.hover_card = Some(this.tabs[0].token);
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(
        visual.debug_bounds("tab-preview-account").is_some(),
        "Hovering a server document must show its account even while a local tab is active"
    );
    window
        .update(&mut visual.cx, |this, _, _| {
            this.strip.hover_card = this.active;
        })
        .unwrap();
    visual.update(|window, cx| window.draw(cx).clear());
    assert!(visual.debug_bounds("tab-preview-account").is_none());
}
