use super::{auth::Authenticated, response};
use crate::{application::Application, domain::error::Error};
use salvo::prelude::*;

#[handler]
pub async fn list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let space = req.param::<String>("space").unwrap_or_default();
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    response::render(
        res,
        app.documents
            .list(&user.identity.user_id, &space)
            .await
            .map(|documents| {
                documents
                    .into_iter()
                    .map(super::mapping::document)
                    .collect::<Vec<_>>()
            }),
    );
}

#[handler]
pub async fn read(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let space = req.param::<String>("space").unwrap_or_default();
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    response::render(
        res,
        app.documents
            .read(&user.identity.user_id, &space, &id)
            .await
            .map(super::mapping::snapshot),
    );
}

#[handler]
pub async fn save(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    req.set_secure_max_size(rovar_api::MAX_CONTENT_BYTES * 4 / 3 + 8192);
    let input = match req.parse_json().await {
        Ok(value) => value,
        Err(_) => {
            response::failure(res, Error::Invalid("Invalid or oversized document".into()));
            return;
        }
    };
    let space = req.param::<String>("space").unwrap_or_default();
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    let id = req.param::<String>("id").unwrap_or_default();
    match super::mapping::save(id, input) {
        Ok(command) => response::render(
            res,
            app.documents
                .save(&user.identity.user_id, &space, command)
                .await
                .map(super::mapping::document),
        ),
        Err(error) => response::failure(res, error),
    }
}
