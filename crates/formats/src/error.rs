//! Error type shared by all decoders.

use thiserror::Error;

/// Convenience alias: every decoder returns this.
pub type Result<T> = std::result::Result<T, DecodeError>;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
    /// Input length did not match what the format requires.
    #[error("expected {expected} bytes, got {actual}")]
    UnexpectedLength { expected: usize, actual: usize },
    /// Decoded pixel count did not match the requested dimensions.
    #[error("expected {expected} pixels, decoded {actual}")]
    SizeMismatch { expected: usize, actual: usize },
    /// A run-length opcode referenced bytes past the end of the input.
    #[error("run-length data ended unexpectedly")]
    TruncatedRun,
    /// The input is not the container this decoder expected (wrong file or
    /// wrong version).
    #[error("unrecognized input: {reason}")]
    Unrecognized { reason: &'static str },
    /// A compiled-sprite catalog or subroutine was structurally invalid.
    #[error("malformed sprite data: {reason}")]
    MalformedSprite { reason: &'static str },
}
