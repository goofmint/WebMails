//! Application error types.
//!
//! `AppError` is the single error type surfaced to commands and, from
//! there, to the frontend. Every `AppError` that reaches a command is
//! serialized as `{ kind, message }` and shown in the settings UI
//! (design.md §5.2).

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};
use std::io;

/// The application's unified error type.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// A filesystem path could not be resolved, e.g. Tauri's path
    /// resolver failed to produce the app config or data directory.
    #[error("path error: {0}")]
    Path(String),

    /// An I/O operation failed.
    #[error("io error: {0}")]
    Io(#[from] io::Error),

    /// The configuration file is missing, malformed, or fails validation.
    #[error("config error: {0}")]
    Config(String),

    /// Application state (`state.json`) could not be loaded or is invalid.
    #[error("state error: {0}")]
    State(String),
}

impl AppError {
    /// A stable, machine-readable identifier for this error's variant.
    ///
    /// This is the `kind` field of the `{ kind, message }` shape the
    /// frontend expects (design.md §5.2).
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Path(_) => "path",
            AppError::Io(_) => "io",
            AppError::Config(_) => "config",
            AppError::State(_) => "state",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AppError", 2)?;
        state.serialize_field("kind", self.kind())?;
        state.serialize_field("message", &self.to_string())?;
        state.end()
    }
}

/// Convenience alias for results that fail with [`AppError`].
pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn path_error_serializes_kind_and_message() {
        let err = AppError::Path("could not resolve app config dir".to_string());
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(err.kind(), "path");
        assert_eq!(value, json!({ "kind": "path", "message": err.to_string() }));
    }

    #[test]
    fn config_error_serializes_kind_and_message() {
        let err = AppError::Config("missing key `services`".to_string());
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(err.kind(), "config");
        assert_eq!(
            value,
            json!({ "kind": "config", "message": err.to_string() })
        );
    }

    #[test]
    fn state_error_serializes_kind_and_message() {
        let err = AppError::State("corrupt state.json".to_string());
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(err.kind(), "state");
        assert_eq!(
            value,
            json!({ "kind": "state", "message": err.to_string() })
        );
    }

    #[test]
    fn io_error_serializes_kind_and_message() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let err: AppError = io_err.into();
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(err.kind(), "io");
        assert_eq!(value, json!({ "kind": "io", "message": err.to_string() }));
    }

    #[test]
    fn serialized_value_has_only_kind_and_message_keys() {
        let err = AppError::Path("x".to_string());
        let value = serde_json::to_value(&err).expect("serialize");
        let obj = value.as_object().expect("object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["kind", "message"]);
    }

    #[test]
    fn from_io_error_via_question_mark_operator() {
        fn fails() -> AppResult<()> {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "nope"))?;
            Ok(())
        }

        let err = fails().expect_err("should fail");
        assert_eq!(err.kind(), "io");
    }
}
