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
    let Some(item) = upload_item(req, res) else {
        return;
    };
    let Some(offset) = req.query::<u64>("offset") else {
        response::failure(res, Error::Invalid("Missing upload offset".into()));
        return;
    };
    if req
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > rovar_api::MEDIA_UPLOAD_CHUNK_BYTES as u64)
    {
        response::failure(res, Error::Invalid("Media exceeds the size limit".into()));
        return;
    }
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    let space = req.param::<String>("space").unwrap_or_default();
    let body = req.take_body().filter_map(|frame| {
        futures_util::future::ready(match frame {
            Ok(frame) => frame.into_data().ok().map(Ok),
            Err(error) => Some(Err(error)),
        })
    });
    response::render(
        res,
        app.documents
            .upload_media_chunk(&user.identity.user_id, &space, item, offset, body)
            .await,
    );
}

fn upload_item(req: &Request, res: &mut Response) -> Option<crate::domain::document::Media> {
    res.headers_mut()
        .insert("cache-control", "private, no-store".parse().unwrap());
    let Some(length) = req.query::<u64>("length") else {
        response::failure(res, Error::Invalid("Missing media length".into()));
        return None;
    };
    Some(crate::domain::document::Media {
        hash: req.param::<String>("hash").unwrap_or_default(),
        length,
    })
}

#[handler]
pub async fn upload_status(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(item) = upload_item(req, res) else {
        return;
    };
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    let space = req.param::<String>("space").unwrap_or_default();
    response::render(
        res,
        app.documents
            .media_upload_status(&user.identity.user_id, &space, item)
            .await,
    );
}

#[handler]
pub async fn finish_upload(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(item) = upload_item(req, res) else {
        return;
    };
    let app = depot.get_typed::<Application>().unwrap();
    let user = depot.get_typed::<Authenticated>().unwrap();
    let space = req.param::<String>("space").unwrap_or_default();
    response::render(
        res,
        app.documents
            .finish_media_upload(&user.identity.user_id, &space, item)
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
        Ok(download) => {
            let etag = format!("\"{hash}\"");
            let range = req.headers().get("range");
            let range = if req
                .headers()
                .get("if-range")
                .is_none_or(|value| value.to_str().ok() == Some(etag.as_str()))
            {
                range.map(|value| {
                    value
                        .to_str()
                        .ok()
                        .and_then(|value| byte_range(value, download.length))
                })
            } else {
                None
            };
            let (start, length) = match range {
                None => (0, download.length),
                Some(Some((start, end))) => {
                    res.status_code(StatusCode::PARTIAL_CONTENT);
                    res.headers_mut().insert(
                        "content-range",
                        format!("bytes {start}-{end}/{}", download.length)
                            .parse()
                            .unwrap(),
                    );
                    (start, end - start + 1)
                }
                Some(None) => {
                    res.status_code(StatusCode::RANGE_NOT_SATISFIABLE);
                    res.headers_mut().insert(
                        "content-range",
                        format!("bytes */{}", download.length).parse().unwrap(),
                    );
                    return;
                }
            };
            res.headers_mut()
                .insert("content-type", "application/octet-stream".parse().unwrap());
            res.headers_mut().insert("content-length", length.into());
            res.headers_mut().insert("etag", etag.parse().unwrap());
            res.headers_mut()
                .insert("accept-ranges", "bytes".parse().unwrap());
            res.headers_mut()
                .insert("cache-control", "private, no-store".parse().unwrap());
            res.stream(ranged_body(download.body, start, length));
        }
        Err(error) => response::failure(res, error),
    }
}

fn byte_range(value: &str, length: u64) -> Option<(u64, u64)> {
    let (start, end) = value.strip_prefix("bytes=")?.split_once('-')?;
    let last = length.checked_sub(1)?;
    if start.is_empty() {
        let suffix = end.parse::<u64>().ok()?;
        return (suffix > 0).then_some((length.saturating_sub(suffix), last));
    }
    let start = start.parse::<u64>().ok()?;
    let end = if end.is_empty() {
        last
    } else {
        end.parse::<u64>().ok()?.min(last)
    };
    (start <= end).then_some((start, end))
}

fn ranged_body(
    body: crate::application::ports::ContentStream,
    skip: u64,
    length: u64,
) -> crate::application::ports::ContentStream {
    Box::pin(futures_util::stream::try_unfold(
        (body, skip, length),
        |(mut body, mut skip, mut remaining)| async move {
            while remaining != 0 {
                let chunk = body
                    .next()
                    .await
                    .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::UnexpectedEof))??;
                if skip >= chunk.len() as u64 {
                    skip -= chunk.len() as u64;
                    continue;
                }
                let start = skip as usize;
                let count = remaining.min((chunk.len() - start) as u64) as usize;
                remaining -= count as u64;
                let bytes = if start == 0 && count == chunk.len() {
                    chunk
                } else {
                    chunk[start..start + count].to_vec()
                };
                return Ok(Some((bytes, (body, 0, remaining))));
            }
            Ok(None)
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn ranges_handle_boundaries_and_propagate_corrupt_or_short_streams() {
        assert_eq!(byte_range("bytes=3-", 6), Some((3, 5)));
        assert_eq!(byte_range("bytes=1-99", 6), Some((1, 5)));
        assert_eq!(byte_range("bytes=-2", 6), Some((4, 5)));
        for value in [
            "bytes=6-",
            "bytes=2-1",
            "bytes=-0",
            "bytes=0-1,3-4",
            "garbage",
        ] {
            assert_eq!(byte_range(value, 6), None);
        }
        assert_eq!(byte_range("bytes=0-", 0), None);
        let body = || {
            Box::pin(futures_util::stream::iter([
                Ok(b"abc".to_vec()),
                Ok(b"def".to_vec()),
            ])) as crate::application::ports::ContentStream
        };
        let mut range = ranged_body(body(), 2, 3);
        let mut bytes = Vec::new();
        while let Some(chunk) = range.next().await {
            bytes.extend_from_slice(&chunk.unwrap());
        }
        assert_eq!(bytes, b"cde");
        let mut short = ranged_body(body(), 5, 2);
        assert_eq!(short.next().await.unwrap().unwrap(), b"f");
        assert!(short.next().await.unwrap().is_err());
        let corrupt = Box::pin(futures_util::stream::iter([Err(
            std::io::ErrorKind::InvalidData.into(),
        )]));
        assert!(ranged_body(corrupt, 3, 3).next().await.unwrap().is_err());
    }
}
