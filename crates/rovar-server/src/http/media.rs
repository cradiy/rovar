use super::{auth::Authenticated, response};
use crate::{application::Application, domain::error::Error};
use base64::{Engine, engine::general_purpose::STANDARD};
use salvo::prelude::*;

#[handler]
pub async fn missing(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    req.set_secure_max_size(512 * 1024);
    let Ok(items) = req.parse_json::<Vec<rovar_api::Media>>().await else {
        response::failure(res, Error::Invalid("Invalid media manifest".into()));
        return;
    };
    let media = items
        .into_iter()
        .map(|m| crate::domain::document::Media {
            hash: m.hash,
            length: m.length,
        })
        .collect::<Vec<_>>();
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    let space = req.param::<String>("space").unwrap_or_default();
    response::render(
        res,
        app.documents
            .missing_media(&user.identity.user_id, &space, &media)
            .await,
    );
}

#[handler]
pub async fn upload(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    req.set_secure_max_size(rovar_api::MAX_CONTENT_BYTES * 4 / 3 + 1024);
    let bytes = match req.parse_json::<rovar_api::MediaContent>().await {
        Ok(input) => STANDARD.decode(input.content).ok(),
        Err(_) => None,
    };
    let Some(bytes) = bytes else {
        response::failure(res, Error::Invalid("Invalid media content".into()));
        return;
    };
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    let space = req.param::<String>("space").unwrap_or_default();
    let hash = req.param::<String>("hash").unwrap_or_default();
    response::render(
        res,
        app.documents
            .upload_media(&user.identity.user_id, &space, &hash, bytes)
            .await,
    );
}

#[handler]
pub async fn read(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    let space = req.param::<String>("space").unwrap_or_default();
    let hash = req.param::<String>("hash").unwrap_or_default();
    response::render(
        res,
        app.documents
            .download_media(&user.identity.user_id, &space, &hash)
            .await
            .map(|bytes| rovar_api::MediaContent {
                content: STANDARD.encode(bytes),
            }),
    );
}
