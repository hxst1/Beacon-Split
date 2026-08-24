use serde::{Serialize, Serializer};

/// The error every Tauri command returns.
///
/// Core errors carry paths and context that are useful in a log but that we do
/// not want to hand to the UI verbatim, so the message is what the user sees
/// and the full chain is what we log.
#[derive(Debug)]
pub struct CommandError {
    message: String,
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<beacon_core::CoreError> for CommandError {
    fn from(error: beacon_core::CoreError) -> Self {
        tracing::error!(error = %error, "command failed");
        Self {
            message: error.to_string(),
        }
    }
}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        tracing::error!(error = %message, "command failed");
        Self { message }
    }
}

impl Serialize for CommandError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.message)
    }
}

pub type CommandResult<T> = Result<T, CommandError>;
