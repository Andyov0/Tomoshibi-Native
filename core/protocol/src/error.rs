//! The one error the protocol crate returns.
//!
//! The server sends a code and the client owns the sentence (the web client's
//! rule, and the apps keep it): `Refused` carries the server's code untouched,
//! and the apps turn it into words in the person's own language.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum ProtocolError {
    /// The deployment answered and said no; `code` is its `{"error": ...}`.
    #[error("refused with {code} ({status})")]
    Refused { status: u16, code: String },
    /// Nothing usable came back: no connection, a timeout, a broken stream.
    #[error("network: {message}")]
    Network { message: String },
    /// Something came back that does not have the shape it should.
    #[error("unexpected answer: {message}")]
    Unexpected { message: String },
    /// The caller asked for something that cannot mean anything.
    #[error("invalid: {message}")]
    Invalid { message: String },
}
