use std::path::PathBuf;

/// Errors raised by the conversion core.
///
/// Mirrors `libresvip.core.exceptions` plus the generic failures that the
/// Python implementation reports through ordinary exceptions.
#[derive(Debug, thiserror::Error)]
pub enum LibreSvipError {
    #[error("invalid file type")]
    InvalidFileType,

    #[error("unsupported project version: {0}")]
    UnsupportedProjectVersion(String),

    #[error("no track found")]
    NoTrack,

    #[error("notes overlapped")]
    NotesOverlapped,

    #[error("params error: {0}")]
    Params(String),

    #[error("not implemented")]
    NotImplemented,

    #[error("no projects to merge")]
    NoProjectsToMerge,

    #[error("all projects must have the same time signatures and tempos")]
    IncompatibleProjects,

    #[error("no singing tracks found")]
    NoSingingTracks,

    #[error("{0}")]
    Message(String),

    #[error("{0}")]
    Parse(String),

    #[error("{0}")]
    Validation(String),

    #[error("io error at {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    IoRaw(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),

    #[error(transparent)]
    Regex(#[from] regex::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T, E = LibreSvipError> = std::result::Result<T, E>;

impl LibreSvipError {
    pub fn msg(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

/// Convenience helpers for attaching a path to I/O failures.
pub trait IoContext<T> {
    fn at_path(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for std::io::Result<T> {
    fn at_path(self, path: impl Into<PathBuf>) -> Result<T> {
        let path = path.into();
        self.map_err(|source| LibreSvipError::Io { path, source })
    }
}
