#[derive(Debug)]
pub enum Error {
    Invalid(String),
    Unauthorized,
    Forbidden,
    AlreadyExists,
    NotFound,
    Conflict,
    DeltaBase,
    RateLimited,
    Internal(anyhow::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Unauthorized => f.write_str("Sign in to this server"),
            Self::NotFound => f.write_str("Resource not found"),
            Self::Forbidden => f.write_str("Access is not allowed"),
            Self::AlreadyExists => f.write_str("Username already exists"),
            Self::Conflict => f.write_str("The server has a newer version. Your local changes are retained; export them before reopening the server version."),
            Self::DeltaBase => f.write_str("The delta baseline is unavailable; retry with a complete snapshot"),
            Self::RateLimited => f.write_str("Too many requests. Try again in a minute"),
            Self::Internal(_) => f.write_str("Server operation failed"),
        }
    }
}
impl std::error::Error for Error {}
impl From<anyhow::Error> for Error {
    fn from(value: anyhow::Error) -> Self {
        Self::Internal(value)
    }
}
pub type Result<T> = std::result::Result<T, Error>;

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
