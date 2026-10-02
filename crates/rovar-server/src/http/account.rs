use super::{auth::Authenticated, response};
use crate::{application::Application, domain::error::Error};
use salvo::prelude::*;

#[handler]
pub async fn password(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    req.set_secure_max_size(4096);
    let input: rovar_api::ChangePassword = match req.parse_json().await {
        Ok(input) => input,
        Err(_) => {
            response::failure(res, Error::Invalid("Invalid password request".into()));
            return;
        }
    };
    let token = &depot.get_typed::<Authenticated>().unwrap().token;
    let app = depot.get_typed::<Application>().unwrap();
    response::render(
        res,
        app.auth
            .change_password(token, input.current_password, input.new_password)
            .await
            .map(|_| serde_json::json!({})),
    );
}

#[handler]
pub async fn sessions(depot: &mut Depot, res: &mut Response) {
    let token = &depot.get_typed::<Authenticated>().unwrap().token;
    let app = depot.get_typed::<Application>().unwrap();
    response::render(
        res,
        app.auth.sessions(token).await.map(|items| {
            items
                .into_iter()
                .map(|s| rovar_api::AccountSession {
                    id: s.id,
                    created_at: s.created_at,
                    expires_at: s.expires_at,
                    current: s.current,
                    device: rovar_api::SessionDevice {
                        system: s.device.system,
                        name: s.device.name,
                        client: s.device.client,
                    },
                })
                .collect::<Vec<_>>()
        }),
    );
}

#[handler]
pub async fn revoke_others(depot: &mut Depot, res: &mut Response) {
    let token = &depot.get_typed::<Authenticated>().unwrap().token;
    let app = depot.get_typed::<Application>().unwrap();
    response::render(
        res,
        app.auth
            .revoke_sessions(token, None)
            .await
            .map(|_| serde_json::json!({})),
    );
}

#[handler]
pub async fn revoke(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(id) = req.param::<String>("id") else {
        response::failure(res, Error::NotFound);
        return;
    };
    let token = &depot.get_typed::<Authenticated>().unwrap().token;
    let app = depot.get_typed::<Application>().unwrap();
    response::render(
        res,
        app.auth
            .revoke_sessions(token, Some(&id))
            .await
            .map(|_| serde_json::json!({})),
    );
}
