use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("journal root does not exist: {0}")]
    RootMissing(PathBuf),
    #[error("entry not found: {0}")]
    NotFound(String),
    #[error("ambiguous id prefix {0}: {1} matches")]
    Ambiguous(String, usize),
    #[error("invalid frontmatter in {path}: {msg}")]
    Frontmatter { path: PathBuf, msg: String },
    #[error("invalid timezone: {0}")]
    Timezone(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Yaml(#[from] serde_norway::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
