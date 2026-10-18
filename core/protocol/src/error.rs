//! The one error the protocol crate returns.
//!
//! The server sends a code and the client owns the sentence (the web client's
//! rule, and the apps keep it): `Refused` carries the server's code untouched,
//! and the apps turn it into words in the person's own language.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
//
// A variant's words are in `detail` rather than `message`: Kotlin's bindings
// make each variant an exception, and a field called `message` there
// redeclares Throwable's own, which does not compile.
pub enum ProtocolError {
    /// The deployment answered and said no; `code` is its `{"error": ...}`.
    #[error("refused with {code} ({status})")]
    Refused { status: u16, code: String },
    /// Nothing usable came back: no connection, a timeout, a broken stream.
    #[error("network: {detail}")]
    Network { detail: String },
    /// Something came back that does not have the shape it should.
    #[error("unexpected answer: {detail}")]
    Unexpected { detail: String },
    /// The caller asked for something that cannot mean anything.
    #[error("invalid: {detail}")]
    Invalid { detail: String },
}
