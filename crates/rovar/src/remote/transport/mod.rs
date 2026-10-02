use anyhow::{Result, ensure};
use futures_util::StreamExt;
use serde::de::DeserializeOwned;
mod download;
mod media;

#[derive(Debug)]
pub(crate) struct HttpError {
    pub status: u16,
    code: String,
    message: String,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for HttpError {}

impl HttpError {
    pub fn account_changed() -> Self {
        Self {
            status: 401,
            code: "account_changed".into(),
            message: crate::i18n::t("server-account-mismatch").into(),
        }
    }
    fn from_response(status: reqwest::StatusCode, bytes: &[u8]) -> Self {
        let error = serde_json::from_slice::<rovar_api::ApiError>(bytes).unwrap_or_else(|_| {
            rovar_api::ApiError {
                code: "http_error".into(),
                message: format!("Server returned {status}"),
            }
        });
        Self {
            status: status.as_u16(),
            code: error.code,
            message: error.message,
        }
    }

    pub fn is_conflict(&self) -> bool {
        self.status == 409 && self.code == "revision_conflict"
    }

    pub fn is_delta_base_mismatch(&self) -> bool {
        self.status == 409 && self.code == "delta_base_mismatch"
    }
}

#[derive(Clone)]
pub(crate) struct Client {
    pub url: String,
    pub token: String,
}
impl Client {
    pub async fn logout(&self) -> Result<()> {
        match self.json::<serde_json::Value>("POST", "logout", None).await {
            Ok(_) => Ok(()),
            // An expired/revoked session is already signed out on the server.
            Err(error)
                if error
                    .downcast_ref::<HttpError>()
                    .is_some_and(|e| e.status == 401) =>
            {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }
    pub fn new(url: &str) -> Result<Self> {
        let url = reqwest::Url::parse(url.trim())?;
        ensure!(
            url.scheme() == "https"
                || (url.scheme() == "http"
                    && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))),
            "Use HTTPS, or HTTP on localhost"
        );
        ensure!(
            url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none()
                && url.path() == "/",
            "Enter the server origin, without a path or credentials"
        );
        Ok(Self {
            url: url.as_str().trim_end_matches('/').into(),
            token: String::new(),
        })
    }
    pub async fn json<T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<T> {
        let client = self.clone();
        let method = method.to_owned();
        let path = path.to_owned();
        let bytes = execute(async move {
            let mut request = client.request(&method, &path)?;
            if let Some(body) = body {
                request = request.json(&body);
            }
            let response = successful(request.send().await?).await?;
            bounded_body(
                response,
                rovar_api::MAX_DOCUMENT_REQUEST_BYTES.max(rovar_api::MAX_DELTA_REQUEST_BYTES),
            )
            .await
        })
        .await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
    fn request(&self, method: &str, path: &str) -> Result<reqwest::RequestBuilder> {
        let builder = reqwest::Client::builder();
        #[cfg(not(target_family = "wasm"))]
        let builder = builder
            .timeout(std::time::Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::none());
        let client = builder.build()?;
        let timeout = if path.starts_with("spaces/") { 120 } else { 30 };
        let mut request = client
            .request(method.parse()?, format!("{}/api/v1/{path}", self.url))
            .timeout(std::time::Duration::from_secs(timeout));
        if !self.token.is_empty() {
            request = request.bearer_auth(&self.token);
        }
        #[cfg(target_family = "wasm")]
        {
            request = request.fetch_credentials_same_origin();
        }
        Ok(request)
    }
}

async fn successful(response: reqwest::Response) -> Result<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    // Preserve authentication/conflict status even for oversized proxy errors.
    let bytes = bounded_body(response, 64 * 1024).await.unwrap_or_default();
    Err(HttpError::from_response(status, &bytes).into())
}

async fn bounded_body(response: reqwest::Response, maximum: usize) -> Result<Vec<u8>> {
    ensure!(
        response
            .content_length()
            .is_none_or(|n| n <= maximum as u64),
        "Response exceeds the size limit"
    );
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        ensure!(
            chunk.len() <= maximum.saturating_sub(bytes.len()),
            "Response exceeds the size limit"
        );
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(not(target_family = "wasm"))]
async fn execute<T: Send + 'static>(
    future: impl std::future::Future<Output = Result<T>> + Send + 'static,
) -> Result<T> {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap()
    });
    let (sender, receiver) = futures_channel::oneshot::channel();
    runtime.spawn(async move {
        let _ = sender.send(future.await);
    });
    receiver.await?
}

#[cfg(target_family = "wasm")]
async fn execute<T>(future: impl std::future::Future<Output = Result<T>>) -> Result<T> {
    future.await
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests;
