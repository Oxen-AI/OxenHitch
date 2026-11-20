#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    /// Something the user can act on; the message is shown as-is.
    #[error("{0}")]
    Invalid(String),
    #[error("Keychain: {0}")]
    Keychain(String),
    #[error("{0}")]
    Process(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn invalid(msg: impl Into<String>) -> Self {
        Error::Invalid(msg.into())
    }
}
