mod view;
use super::*;
use crate::remote::Connection;

#[derive(Clone, Copy, PartialEq)]
enum View {
    Overview,
    Create,
    Join,
}

pub(super) struct Panel {
    view: View,
    connection: Connection,
    name: Entity<TextInput>,
    code: Entity<TextInput>,
    members: Vec<rovar_api::Member>,
    invitation: Option<String>,
    busy: bool,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

enum Action {
    Load,
    Create(String),
    Join(String),
    Invite,
    Remove(String),
}

impl Studio {
    pub(super) fn open_spaces(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(connection) = self
            .source
            .as_ref()
            .and_then(|id| self.remote.read(cx).connection(id))
            .cloned()
        else {
            return;
        };
        let mut panel = Panel {
            view: View::Overview,
            connection,
            name: cx.new(|cx| TextInput::new(cx).placeholder(t("space-team-name"))),
            code: cx.new(|cx| TextInput::new(cx).placeholder(t("space-invitation"))),
            members: Vec::new(),
            invitation: None,
            busy: false,
            error: None,
            _subscriptions: Vec::new(),
        };
        for (input, create) in [(&panel.name, true), (&panel.code, false)] {
            panel._subscriptions.push(cx.subscribe_in(
                input,
                window,
                move |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::Submit(_))
                        && let Some(panel) = &this.spaces
                    {
                        let action = if create {
                            Action::Create(panel.name.read(cx).value().to_string())
                        } else {
                            Action::Join(panel.code.read(cx).value().to_string())
                        };
                        this.space_action(action, window, cx);
                    }
                },
            ));
        }
        self.spaces = Some(panel);
        self.space_action(Action::Load, window, cx);
    }

    fn space_action(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = &mut self.spaces else {
            return;
        };
        if panel.busy {
            return;
        }
        panel.busy = true;
        panel.error = None;
        let connection = panel.connection.clone();
        let client = connection.client();
        cx.spawn_in(window, async move |this, cx| {
            let result = async {
                let mut selected = connection.space.id.clone();
                let mut invitation = None;
                match action {
                    Action::Load => {}
                    Action::Create(name) => {
                        let space: rovar_api::Space = client
                            .json("POST", "teams", Some(serde_json::json!({ "name": name })))
                            .await?;
                        selected = space.id;
                    }
                    Action::Join(code) => {
                        let space: rovar_api::Space = client
                            .json(
                                "POST",
                                "teams/join",
                                Some(serde_json::json!({ "code": code })),
                            )
                            .await?;
                        selected = space.id;
                    }
                    Action::Invite => {
                        let value: rovar_api::Invitation = client
                            .json("POST", &format!("teams/{selected}/invites"), None)
                            .await?;
                        invitation = Some(value.code);
                    }
                    Action::Remove(user) => {
                        client
                            .json::<serde_json::Value>(
                                "DELETE",
                                &format!("teams/{selected}/members/{user}"),
                                None,
                            )
                            .await?;
                    }
                }
                let identity: rovar_api::Identity = client.json("GET", "session", None).await?;
                let space = identity
                    .spaces
                    .iter()
                    .find(|s| s.id == selected)
                    .or_else(|| identity.spaces.iter().find(|s| s.kind == "personal"))
                    .ok_or_else(|| anyhow::anyhow!("Account has no workspace"))?;
                selected = space.id.clone();
                let members = if space.kind == "team" {
                    client
                        .json::<Vec<rovar_api::Member>>(
                            "GET",
                            &format!("teams/{selected}/members"),
                            None,
                        )
                        .await?
                } else {
                    Vec::new()
                };
                Ok::<_, anyhow::Error>((identity, selected, members, invitation))
            }
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok((identity, selected, members, invitation)) => {
                        let connected = this.remote.update(cx, |remote, cx| {
                            remote.connect(client.url.clone(), identity, client.token.clone(), cx)
                        });
                        match connected {
                            Ok(_) => {
                                let updated = this
                                    .remote
                                    .read(cx)
                                    .connections()
                                    .iter()
                                    .find(|c| {
                                        c.url == client.url
                                            && c.identity.user_id == connection.identity.user_id
                                            && c.space.id == selected
                                    })
                                    .cloned();
                                if let Some(updated) = updated {
                                    if this.source.as_ref() != Some(&updated.id) {
                                        this.select_source(Some(updated.id.clone()), window, cx);
                                    }
                                    if let Some(panel) = &mut this.spaces {
                                        if panel.connection.id != updated.id {
                                            panel.invitation = None;
                                            panel.view = View::Overview;
                                        }
                                        panel.connection = updated;
                                        panel.members = members;
                                        panel.busy = false;
                                        if invitation.is_some() {
                                            panel.invitation = invitation;
                                        }
                                        panel.code.update(cx, |input, cx| input.set_value("", cx));
                                        panel.name.update(cx, |input, cx| input.set_value("", cx));
                                    }
                                }
                            }
                            Err(error) => {
                                if let Some(panel) = &mut this.spaces {
                                    panel.busy = false;
                                    panel.error = Some(error.to_string());
                                }
                            }
                        }
                    }
                    Err(error) => {
                        if let Some(panel) = &mut this.spaces {
                            panel.busy = false;
                            panel.error = Some(error.to_string());
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
