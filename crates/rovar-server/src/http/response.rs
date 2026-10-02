use crate::domain::error::{Error, Result};
use salvo::prelude::*;
use serde::Serialize;

pub fn render<T: Serialize + Send>(res: &mut Response, result: Result<T>) {
    match result {
        Ok(value) => res.render(Json(value)),
        Err(error) => failure(res, error),
    }
}

pub fn failure(res: &mut Response, error: Error) {
    let (status, code) = match &error {
        Error::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
        Error::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
        Error::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
        Error::AlreadyExists => (StatusCode::CONFLICT, "already_exists"),
        Error::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        Error::Conflict => (StatusCode::CONFLICT, "revision_conflict"),
        Error::DeltaBase => (StatusCode::CONFLICT, "delta_base_mismatch"),
        Error::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
        Error::Internal(source) => {
            eprintln!("Server operation failed: {source:#}");
            (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
        }
    };
    res.status_code(status);
    res.render(Json(rovar_api::ApiError {
        code: code.into(),
        message: error.to_string(),
    }));
}
