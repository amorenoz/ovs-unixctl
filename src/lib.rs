//! OpenvSwitch application control (appctl) library.
//!
//! Example:
//! ```no_run
//! use ovs_unixctl::OvsUnixCtl;
//!
//! let mut unixctl = OvsUnixCtl::new().unwrap();
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

//FIXME
#[allow(dead_code)]
pub mod jsonrpc;
pub mod ovs;
pub use ovs::*;
pub mod unix;
