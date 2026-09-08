use std::fmt;

/// Error classification shared by snapshot queries and their host boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryErrorKind {
    InvalidArgument,
    Internal,
}

#[derive(Debug)]
pub struct QueryError {
    pub kind: QueryErrorKind,
    message: String,
}

impl QueryError {
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            kind: QueryErrorKind::InvalidArgument,
            message: message.into(),
        }
    }
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            kind: QueryErrorKind::Internal,
            message: message.into(),
        }
    }
}
impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for QueryError {}
