//! Parse error types and handling.

use super::location::SourceRange;
use std::fmt;

/// The kind of parse error encountered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// A syntax error in the source code.
    SyntaxError,
    /// The parser encountered an unexpected token.
    UnexpectedToken,
    /// The source file could not be read.
    IoError,
    /// The language is not supported.
    UnsupportedLanguage,
    /// A query pattern is invalid.
    InvalidQuery,
    /// Generic parsing failure.
    ParseFailed,
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SyntaxError => write!(f, "syntax error"),
            Self::UnexpectedToken => write!(f, "unexpected token"),
            Self::IoError => write!(f, "I/O error"),
            Self::UnsupportedLanguage => write!(f, "unsupported language"),
            Self::InvalidQuery => write!(f, "invalid query"),
            Self::ParseFailed => write!(f, "parse failed"),
        }
    }
}

/// A parse error with location information and context.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// The kind of error.
    pub kind: ParseErrorKind,
    /// Human-readable error message.
    pub message: String,
    /// Source location where the error occurred, if available.
    pub location: Option<SourceRange>,
    /// The source file path, if available.
    pub file_path: Option<String>,
}

impl ParseError {
    /// Creates a new parse error.
    pub fn new(kind: ParseErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            location: None,
            file_path: None,
        }
    }

    /// Sets the source location for this error.
    pub fn with_location(mut self, location: SourceRange) -> Self {
        self.location = Some(location);
        self
    }

    /// Sets the file path for this error.
    pub fn with_file_path(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    /// Creates a syntax error.
    pub fn syntax_error(message: impl Into<String>) -> Self {
        Self::new(ParseErrorKind::SyntaxError, message)
    }

    /// Creates an I/O error.
    pub fn io_error(message: impl Into<String>) -> Self {
        Self::new(ParseErrorKind::IoError, message)
    }

    /// Creates an unsupported language error.
    pub fn unsupported_language(language: impl Into<String>) -> Self {
        Self::new(
            ParseErrorKind::UnsupportedLanguage,
            format!("Unsupported language: {}", language.into()),
        )
    }

    /// Creates an invalid query error.
    pub fn invalid_query(message: impl Into<String>) -> Self {
        Self::new(ParseErrorKind::InvalidQuery, message)
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref path) = self.file_path {
            write!(f, "{}: ", path)?;
        }
        if let Some(ref loc) = self.location {
            write!(f, "{}:{}: ", loc.start.line, loc.start.column)?;
        }
        write!(f, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ParseError {}

impl From<std::io::Error> for ParseError {
    fn from(err: std::io::Error) -> Self {
        Self::io_error(err.to_string())
    }
}
