use std::path::{Path, PathBuf};

/// Every fallible operation in `beacon-core` returns this error.
///
/// Variants deliberately carry the offending path: most failures here are
/// filesystem or user-configuration problems, and a message without the path
/// is close to useless when diagnosing them.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("failed to access {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("failed to serialize {path}: {source}")]
    Serialize {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "{path} was written by a newer version of Beacon (schema {found}, this build supports {supported})"
    )]
    UnsupportedSchema {
        path: PathBuf,
        found: u32,
        supported: u32,
    },

    #[error("no workspace with id {0}")]
    WorkspaceNotFound(String),

    #[error("no project with id {0}")]
    ProjectNotFound(String),

    #[error("{0}")]
    Invalid(String),

    #[error("no session with id {0}")]
    SessionNotFound(String),

    /// Wraps failures from the PTY layer, which reports `anyhow` errors.
    #[error("{context}: {message}")]
    Session {
        context: &'static str,
        message: String,
    },
}

impl CoreError {
    pub fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    pub fn session(context: &'static str, error: impl std::fmt::Display) -> Self {
        Self::Session {
            context,
            message: error.to_string(),
        }
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
