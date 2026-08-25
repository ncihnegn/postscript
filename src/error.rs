use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum PsError {
    #[error("Stack underflow")]
    StackUnderflow,
    #[error("Type check error: expected {expected}, got {got}")]
    TypeCheck {
        expected: &'static str,
        got: String,
    },
    #[error("Undefined name: /{0}")]
    Undefined(String),
    #[error("Undefined result / division by zero")]
    UndefinedResult,
    #[error("Range check error: {0}")]
    RangeCheck(String),
    #[error("Limit check error: {0}")]
    LimitCheck(String),
    #[error("Syntax error: {0}")]
    SyntaxError(String),
    #[error("IO error: {0}")]
    IoError(String),
    #[error("Invalid font format: {0}")]
    InvalidFont(String),
    #[error("Unsupported feature: {0}")]
    Unsupported(String),
    #[error("Interrupted / exit")]
    Exit,
    #[error("Stop encountered")]
    Stop,
}

pub type PsResult<T> = Result<T, PsError>;
