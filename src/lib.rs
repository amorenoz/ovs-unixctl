//! OpenvSwitch application control (appctl) library.
//!
//! Example:
//! ```no_run
//! use ovs_unixctl::OvsUnixCtl;
//!
//! let mut unixctl = OvsUnixCtl::new(None).unwrap();
//! let commands = unixctl.list_commands().unwrap();
//! println!("Available commands");
//! for (command, args) in commands.iter() {
//!     println!("{command}: {args}");
//! }
//!
//! let bonds = unixctl.run("bond/list", None).unwrap();
//! println!("{}", bonds.unwrap());
//! let bond0 = unixctl.run("bond/show", Some(&["bond0"])).unwrap();
//! println!("{}", bond0.unwrap());
//! ```

mod jsonrpc;
pub mod ovs;
mod unix;
pub use ovs::*;

pub mod error;
pub use error::Error;

/// An alias for [`std::result::Result`] with a generic error from this crate.
pub type Result<T> = std::result::Result<T, Error>;
