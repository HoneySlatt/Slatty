pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error while {context}: {source}")]
    Network {
        context: &'static str,
        #[source]
        source: reqwest::Error,
    },
    #[error("{context} failed with HTTP {status}")]
    Http { context: &'static str, status: u16 },
    #[error("not logged in")]
    NotLoggedIn,
    #[error("the GOG session was rejected (expired or revoked); log in again")]
    SessionRejected,
    #[error("secret storage unavailable: {0}")]
    Keyring(String),
    #[error("invalid login input: {0}")]
    InvalidLoginInput(&'static str),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("unexpected data from {context}: {detail}")]
    Parse {
        context: &'static str,
        detail: String,
    },
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Refused(String),
    #[error("cancelled")]
    Cancelled,
}

impl Error {
    pub fn network(context: &'static str, source: reqwest::Error) -> Self {
        Self::Network {
            context,
            source: source.without_url(),
        }
    }

    pub fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    pub fn parse(context: &'static str, detail: impl ToString) -> Self {
        Self::Parse {
            context,
            detail: detail.to_string(),
        }
    }
}
