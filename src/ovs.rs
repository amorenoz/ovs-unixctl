//! OVS unixctl interface

use std::{
    env,
    path::{Path, PathBuf},
};

use std::fs;
use std::time::Duration;

use crate::{error::Error, jsonrpc, unix, Result};

const DEFAULT_RUNDIR: &str = "/var/run/openvswitch";

/// OVS Unix control interface.
///
/// It allows the execution of control commands against ovs-vswitchd.
#[derive(Debug)]
pub struct OvsUnixCtl {
    // JSON-RPC client. For now, only Unix is supported. If more are supported in the future, this
    // would have to be a generic type.
    client: jsonrpc::Client<unix::UnixJsonStreamClient>,
}

impl OvsUnixCtl {
    /// Creates a new OvsUnixCtl against ovs-vswitchd.
    ///
    /// Tries to find the pidfile and socket in the default path or in the one specified in the
    /// OVS_RUNDIR env variable.
    pub fn new(timeout: Option<Duration>) -> Result<OvsUnixCtl> {
        let sockpath = Self::find_socket("ovs-vswitchd".into())?;
        Self::unix(sockpath, timeout)
    }

    /// Creates a new OvsUnixCtl against the provided target, e.g.: ovs-vswitchd, ovsdb-server,
    /// northd, etc.
    ///
    /// Tries to find the pidfile and socket in the default path or in the one specified in the
    /// OVS_RUNDIR env variable.
    pub fn with_target(target: String, timeout: Option<Duration>) -> Result<OvsUnixCtl> {
        let sockpath = Self::find_socket(target)?;
        Self::unix(sockpath, timeout)
    }

    /// Creates a new OvsUnixCtl by specifing a concrete unix socket path.
    pub fn unix<P: AsRef<Path>>(path: P, timeout: Option<Duration>) -> Result<OvsUnixCtl> {
        if !path.as_ref().exists() {
            return Err(Error::SocketNotFound(format!(
                "{}",
                path.as_ref().display()
            )));
        }

        Ok(Self {
            client: jsonrpc::Client::<unix::UnixJsonStreamClient>::unix(
                path,
                timeout.or(Some(Duration::from_secs(1))),
            )?,
        })
    }

    fn find_socket_at<P: AsRef<Path>>(target: &str, rundir: P) -> Result<PathBuf> {
        // Find $OVS_RUNDIR/{target}.pid
        let pidfile_path = rundir.as_ref().join(format!("{}.pid", &target));
        let pid_str = fs::read_to_string(pidfile_path.clone()).map_err(|_| Error::OvsNotRunning)?;
        let pid_str = pid_str.trim();

        if pid_str.is_empty() {
            return Err(Error::OvsNotRunning);
        }

        // Find $OVS_RUNDIR/{target}.{pid}.ctl
        let sock_path = rundir.as_ref().join(format!("{}.{}.ctl", &target, pid_str));
        if !sock_path.exists() {
            return Err(Error::SocketNotFound(format!("{}", sock_path.display())));
        }
        Ok(sock_path)
    }

    fn find_socket(target: String) -> Result<PathBuf> {
        let rundir: String = match env::var_os("OVS_RUNDIR") {
            Some(rundir) => rundir.into_string().unwrap_or(DEFAULT_RUNDIR.to_string()),
            None => DEFAULT_RUNDIR.to_string(),
        };
        Self::find_socket_at(target.as_str(), PathBuf::from(rundir))
    }
}
