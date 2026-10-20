//! Errors with stable codes from `docs/errors.md`.

use std::fmt;
use t_rustable_engine::EngineError;

pub mod code {
    pub const INVALID_ARGUMENT: &str = "invalid-argument";
    pub const NOT_FOUND: &str = "not-found";
    pub const COMMAND_FAILED: &str = "command-failed";
    pub const UNSUPPORTED_FORMAT: &str = "unsupported-format";
    pub const UNSUPPORTED_CONTENT: &str = "unsupported-content";
    pub const MISSING_GLYPH: &str = "missing-glyph";
    pub const SIGNED_DOCUMENT: &str = "signed-document";
    pub const FILE_IN_USE: &str = "file-in-use";
    pub const PASSWORD_REQUIRED: &str = "password-required";
    pub const DAMAGED_FILE: &str = "damaged-file";
    pub const IO: &str = "io-error";
    pub const NOTHING_TO_UNDO: &str = "nothing-to-undo";
    pub const NOTHING_TO_REDO: &str = "nothing-to-redo";
    pub const REPLAY_FAILED: &str = "replay-failed";
    pub const ENGINE_RESTARTED: &str = "engine-restarted";
    pub const CANCELLED: &str = "cancelled";
    pub const INTERNAL: &str = "internal";
}

#[derive(Clone, Debug, PartialEq)]
pub struct CoreError {
    pub code: &'static str,
    pub message: String,
    pub hint: Option<String>,
    pub details: Option<String>,
    /// Index of the failing command in a transaction.
    pub command_index: Option<usize>,
}

pub type Result<T, E = CoreError> = std::result::Result<T, E>;

impl CoreError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            hint: None,
            details: None,
            command_index: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    pub fn at_command(mut self, index: usize) -> Self {
        self.command_index = Some(index);
        self
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(code::INVALID_ARGUMENT, message)
    }

    pub fn not_found(what: impl fmt::Display) -> Self {
        Self::new(
            code::NOT_FOUND,
            format!("{what} does not exist in this document."),
        )
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(code::INTERNAL, message)
    }

    pub fn io(what: &str, e: std::io::Error) -> Self {
        Self::new(code::IO, format!("{what}: {e}"))
    }
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for CoreError {}

impl From<EngineError> for CoreError {
    fn from(e: EngineError) -> Self {
        match e {
            EngineError::Cancelled => Self::new(code::CANCELLED, "The request was cancelled."),
            EngineError::Password => {
                Self::new(code::PASSWORD_REQUIRED, "The document needs a password.")
                    .with_hint("Open the document again and type its password.")
            }
            EngineError::BadFile(m) => Self::new(
                code::DAMAGED_FILE,
                "The file is not a PDF document, or it is damaged.",
            )
            .with_details(m),
            EngineError::InvalidArgument(m) => Self::invalid(m),
            EngineError::UnsupportedContent(m) => Self::new(code::UNSUPPORTED_CONTENT, m)
                .with_hint("This page has content that the editor cannot write again."),
            EngineError::UnsupportedImage(m) => Self::new(
                code::UNSUPPORTED_FORMAT,
                "The image is not a valid PNG or JPEG file.",
            )
            .with_details(m),
            EngineError::OpFailed { error, .. } => Self::from(*error),
            other => Self::internal(other.to_string()),
        }
    }
}
