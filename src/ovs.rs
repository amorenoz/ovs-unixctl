//! OVS unixctl interface

use std::{
    env,
    path::{Path, PathBuf},
};

use std::fs;
use std::time;

use anyhow::{anyhow, bail, Result};

use crate::{jsonrpc, unix};

/// OVS Unix control interface.
///
/// It allows the execution of well control commands against ovs-vswitchd.
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
    pub fn new() -> Result<OvsUnixCtl> {
        let sockpath = Self::find_socket("ovs-vswitchd".into())?;
        Self::unix(sockpath, Some(time::Duration::from_secs(5)))
    }

    /// Creates a new OvsUnixCtl against the provided target, e.g.: ovs-vswitchd, ovsdb-server,
    /// northd, etc.
    ///
    /// Tries to find the pidfile and socket in the default path or in the one specified in the
    /// OVS_RUNDIR env variable.
    pub fn with_target(target: String) -> Result<OvsUnixCtl> {
        let sockpath = Self::find_socket(target)?;
        Self::unix(sockpath, Some(time::Duration::from_secs(5)))
    }

    /// Creates a new OvsUnixCtl by specifing a concrete unix socket path.
    ///
    /// Tries to find the socket in the default paths.
    pub fn unix<P: AsRef<Path>>(path: P, timeout: Option<time::Duration>) -> Result<OvsUnixCtl> {
        Ok(Self {
            client: jsonrpc::Client::<unix::UnixJsonStreamClient>::unix(path, timeout),
        })
    }

    fn find_socket_at<P: AsRef<Path>>(target: &str, rundir: P) -> Result<PathBuf> {
        // Find $OVS_RUNDIR/{target}.pid
        let pidfile_path = rundir.as_ref().join(format!("{}.pid", &target));
        println!("pidfile {:?}", pidfile_path);
        let pid_str = fs::read_to_string(pidfile_path.clone())?;
        let pid_str = pid_str.trim();

        if pid_str.is_empty() {
            bail!("pidfile is empty: {:?}", &pidfile_path);
        }

        // Find $OVS_RUNDIR/{target}.{pid}.ctl
        let sock_path = rundir.as_ref().join(format!("{}.{}.ctl", &target, pid_str));
        if !fs::exists(&sock_path)? {
            bail!("failed to find control socket for target {}", &target);
        }
        Ok(sock_path)
    }

    fn find_socket(target: String) -> Result<PathBuf> {
        let rundir: String = match env::var_os("OVS_RUNDIR") {
            Some(rundir) => rundir
                .into_string()
                .map_err(|_| anyhow!("OVS_RUNDIR non-unicode content"))?,
            None => "/var/run/openvswitch".into(),
        };
        Self::find_socket_at(target.as_str(), PathBuf::from(rundir))
    }
}
