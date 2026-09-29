use super::{auth::Authenticated, mapping, response};
use crate::{application::Application, domain::error::Error};
use salvo::prelude::*;

#[handler]
pub async fn list(depot: &mut Depot, res: &mut Response) {
    let actor = &depot.get_typed::<Authenticated>().unwrap().identity.user_id;
    response::render(
        res,
        depot
            .get_typed::<Application>()
            .unwrap()
            .spaces
            .list(actor)
            .await
            .map(|items| items.into_iter().map(mapping::space).collect::<Vec<_>>()),
    );
}
#[handler]
pub async fn create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    req.set_secure_max_size(4096);
    let input: rovar_api::CreateTeam = match req.parse_json().await {
        Ok(value) => value,
        Err(_) => {
            response::failure(res, Error::Invalid("Invalid team name".into()));
            return;
        }
    };
    let actor = &depot.get_typed::<Authenticated>().unwrap().identity.user_id;
    response::render(
        res,
        depot
            .get_typed::<Application>()
            .unwrap()
            .spaces
            .create(actor, &input.name)
            .await
            .map(mapping::space),
    );
}
#[handler]
pub async fn join(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    req.set_secure_max_size(4096);
    let input: rovar_api::JoinTeam = match req.parse_json().await {
        Ok(value) => value,
        Err(_) => {
            response::failure(res, Error::Invalid("Invalid invitation".into()));
            return;
        }
    };
    let actor = &depot.get_typed::<Authenticated>().unwrap().identity.user_id;
    response::render(
        res,
        depot
            .get_typed::<Application>()
            .unwrap()
            .spaces
            .join(actor, &input.code)
            .await
            .map(mapping::space),
    );
}
#[handler]
pub async fn invite(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let actor = &depot.get_typed::<Authenticated>().unwrap().identity.user_id;
    let space = req.param::<String>("space").unwrap_or_default();
    response::render(
        res,
        depot
            .get_typed::<Application>()
            .unwrap()
            .spaces
            .invite(actor, &space)
            .await
            .map(|v| rovar_api::Invitation {
                code: v.code,
                expires_at: v.expires_at,
            }),
    );
}
#[handler]
pub async fn members(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let actor = &depot.get_typed::<Authenticated>().unwrap().identity.user_id;
    let space = req.param::<String>("space").unwrap_or_default();
    response::render(
        res,
        depot
            .get_typed::<Application>()
            .unwrap()
            .spaces
            .members(actor, &space)
            .await
            .map(|items| {
                items
                    .into_iter()
                    .map(|v| rovar_api::Member {
                        user_id: v.user_id,
                        username: v.username,
                        role: v.role,
                    })
                    .collect::<Vec<_>>()
            }),
    );
}
#[handler]
pub async fn remove(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let actor = &depot.get_typed::<Authenticated>().unwrap().identity.user_id;
    let space = req.param::<String>("space").unwrap_or_default();
    let user = req.param::<String>("user").unwrap_or_default();
    response::render(
        res,
        depot
            .get_typed::<Application>()
            .unwrap()
            .spaces
            .remove(actor, &space, &user)
            .await
            .map(|_| serde_json::json!({})),
    );
}
