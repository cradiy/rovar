use anyhow::{Result, ensure};
use serde::de::DeserializeOwned;

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
        #[cfg(not(target_family = "wasm"))]
        let bytes = {
            static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> =
                std::sync::OnceLock::new();
            let runtime = RUNTIME.get_or_init(|| {
                tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .unwrap()
            });
            let (sender, receiver) = futures_channel::oneshot::channel();
            runtime.spawn(async move {
                let _ = sender.send(client.request(method, path, body).await);
            });
            receiver.await??
        };
        #[cfg(target_family = "wasm")]
        let bytes = client.request(method, path, body).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
    async fn request(
        self,
        method: String,
        path: String,
        body: Option<serde_json::Value>,
    ) -> Result<Vec<u8>> {
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
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await?;
        let status = response.status();
        let bytes = response.bytes().await?.to_vec();
        if !status.is_success() {
            return Err(HttpError::from_response(status, &bytes).into());
        }
        Ok(bytes)
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests;
