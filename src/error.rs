use thiserror;

use serde_json;
use std::io;

/// A unified error type for anything returned by a method in the this crate.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Something in the jsonrpc protocol failed
    #[error("jsonrpc protocol error: {0}")]
    Protocol(String),
    /// Serialization or deserialization of data failed
    #[error("(de/)serialization error: {0}")]
    Serialize(serde_json::Error),
    /// An error occurred in the socket I/O handling
    #[error("input/output socket error: {0}")]
    Socket(#[from] io::Error),
    /// The connection timed-out waiting for a response
    #[error("connection timeout")]
    Timeout,
    /// The remote peer returned an error
    #[error("command {cmd}({params}) returns error: {error}")]
    Command {
        cmd: String,
        params: String,
        error: String,
    },
    /// An error occurred when trying to find the right unix socket
    #[error("socket not found: {0}")]
    SocketNotFound(String),
    /// OpenvSwitch is not running
    #[error("OpenvSwitch is not running")]
    OvsNotRunning,
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Error {
        // serde_json errors can encapsulate IO errors.
        use serde_json::error::Category::*;
        match error.classify() {
            Io => Error::Socket(error.into()),
            _ => Error::Serialize(error),
        }
    }
}
