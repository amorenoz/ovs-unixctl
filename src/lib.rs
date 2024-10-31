//! OpenvSwitch application control (appctl) library.

//FIXME
#![allow(dead_code)]

mod jsonrpc;
pub mod ovs;
mod unix;
pub use ovs::*;

pub mod error;
pub use error::Error;

/// An alias for [`std::result::Result`] with a generic error from this crate.
pub type Result<T> = std::result::Result<T, Error>;
