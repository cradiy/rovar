use super::{auth::Authenticated, response};
use crate::{application::Application, domain::error::Error};
use futures_util::StreamExt;
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
    if req
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > rovar_api::MAX_MEDIA_BYTES as u64)
    {
        response::failure(res, Error::Invalid("Media exceeds the size limit".into()));
        return;
    }
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    let space = req.param::<String>("space").unwrap_or_default();
    let hash = req.param::<String>("hash").unwrap_or_default();
    let body = req.take_body().filter_map(|frame| {
        futures_util::future::ready(match frame {
            Ok(frame) => frame.into_data().ok().map(Ok),
            Err(error) => Some(Err(error)),
        })
    });
    response::render(
        res,
        app.documents
            .upload_media(&user.identity.user_id, &space, &hash, body)
            .await,
    );
}

#[handler]
pub async fn read(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    let space = req.param::<String>("space").unwrap_or_default();
    let hash = req.param::<String>("hash").unwrap_or_default();
    match app
        .documents
        .download_media(&user.identity.user_id, &space, &hash)
        .await
    {
        Ok(bytes) => {
            res.headers_mut()
                .insert("content-type", "application/octet-stream".parse().unwrap());
            res.headers_mut()
                .insert("content-length", bytes.len().into());
            res.headers_mut()
                .insert("cache-control", "private, no-store".parse().unwrap());
            if let Err(error) = res.write_body(bytes) {
                response::failure(res, Error::Internal(error.into()));
            }
        }
        Err(error) => response::failure(res, error),
    }
}
