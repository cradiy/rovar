use super::{auth::Authenticated, response};
use crate::{application::Application, domain::error::Error};
use salvo::prelude::*;

#[handler]
pub async fn changes(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let space = req.param::<String>("space").unwrap_or_default();
    let Some(after) = req.query::<i64>("after") else {
        response::failure(res, Error::Invalid("Missing or invalid sync cursor".into()));
        return;
    };
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    response::render(
        res,
        app.documents
            .changes(&user.identity.user_id, &space, after)
            .await
            .map(|page| rovar_api::Changes {
                objects: page
                    .documents
                    .into_iter()
                    .map(super::mapping::document)
                    .collect(),
                cursor: page.cursor,
                has_more: page.has_more,
            }),
    );
}

#[handler]
pub async fn metadata(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let space = req.param::<String>("space").unwrap_or_default();
    let id = req.param::<String>("id").unwrap_or_default();
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    response::render(
        res,
        app.documents
            .metadata(&user.identity.user_id, &space, &id)
            .await
            .map(super::mapping::document),
    );
}

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
pub async fn transfer(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let space = req.param::<String>("space").unwrap_or_default();
    let id = req.param::<String>("id").unwrap_or_default();
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    response::render(
        res,
        app.documents
            .transfer(&user.identity.user_id, &space, &id)
            .await
            .map(super::mapping::snapshot),
    );
}

#[handler]
pub async fn download(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    req.set_secure_max_size(4096);
    let input: rovar_api::DownloadBase = match req.parse_json().await {
        Ok(value) => value,
        Err(_) => {
            response::failure(res, Error::Invalid("Invalid download baseline".into()));
            return;
        }
    };
    let space = req.param::<String>("space").unwrap_or_default();
    let id = req.param::<String>("id").unwrap_or_default();
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    response::render(
        res,
        app.documents
            .download(
                &user.identity.user_id,
                &space,
                &id,
                input.revision,
                input.hash,
            )
            .await
            .map(|value| rovar_api::Transfer {
                snapshot: super::mapping::snapshot(value.snapshot),
                encoding: if value.delta {
                    rovar_api::TransferEncoding::Delta
                } else {
                    rovar_api::TransferEncoding::Full
                },
            }),
    );
}

#[handler]
pub async fn save(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    save_request(req, depot, res, false).await;
}

#[handler]
pub async fn save_delta(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    save_request(req, depot, res, true).await;
}

async fn save_request(req: &mut Request, depot: &mut Depot, res: &mut Response, delta: bool) {
    req.set_secure_max_size(if delta {
        rovar_api::MAX_DELTA_REQUEST_BYTES
    } else {
        rovar_api::MAX_DOCUMENT_REQUEST_BYTES
    });
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
        Ok(command) => {
            let result = if delta {
                app.documents
                    .save_delta(&user.identity.user_id, &space, command)
                    .await
            } else {
                app.documents
                    .save(&user.identity.user_id, &space, command)
                    .await
            };
            response::render(res, result.map(super::mapping::document));
        }
        Err(error) => response::failure(res, error),
    }
}
