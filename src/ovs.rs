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

    /// Runs the common "list-commands" command and returns the list of commands and their
    /// arguments.
    pub fn list_commands(&mut self) -> Result<Vec<(String, String)>> {
        let response: jsonrpc::Response<String> = self.client.call("list-commands")?;
        Ok(response
            .result
            .ok_or(Error::OvsInvalidResponse {
                cmd: "list-commands".to_string(),
                response: String::default(),
                error: "should not be empty".to_string(),
            })?
            .lines()
            .skip(1)
            .map(|l| {
                let (cmd, args) = l.trim().split_once(char::is_whitespace).unwrap_or((l, ""));
                (cmd.trim().into(), args.trim().into())
            })
            .collect())
    }

    /// Retrieve the version of the running daemon.
    pub fn version(&mut self) -> Result<(u32, u32, u32, String)> {
        let response: jsonrpc::Response<String> = self.client.call("version")?;
        let invalid = InvalidResponse(
            "version".to_string(),
            response.result.clone().unwrap_or_default(),
        );

        match response
            .result
            .ok_or(invalid.error("should not be empty".to_string()))?
            .trim()
            .strip_prefix("ovs-vswitchd (Open vSwitch) ")
            .ok_or(invalid.error("invalid prefix".to_string()))?
            .splitn(4, &['.', '-'])
            .collect::<Vec<&str>>()[..]
        {
            [x, y, z] => Ok((
                x.to_string()
                    .parse()
                    .map_err(|e| invalid.error(format!("can't parse {x}: {e}")))?,
                y.to_string()
                    .parse()
                    .map_err(|e| invalid.error(format!("can't parse {y}: {e}")))?,
                z.to_string()
                    .parse()
                    .map_err(|e| invalid.error(format!("can't parse {z}: {e}")))?,
                String::default(),
            )),
            [x, y, z, patch] => Ok((
                x.to_string()
                    .parse()
                    .map_err(|e| invalid.error(format!("can't parse {x}: {e}")))?,
                y.to_string()
                    .parse()
                    .map_err(|e| invalid.error(format!("can't parse {y}: {e}")))?,
                z.to_string()
                    .parse()
                    .map_err(|e| invalid.error(format!("can't parse {z}: {e}")))?,
                String::from(patch),
            )),
            _ => Err(invalid.error("parse error".to_string())),
        }
    }

    /// Run an arbitrary command.
    pub fn run(&mut self, cmd: &str, params: Option<&[&str]>) -> Result<Option<String>> {
        let response: jsonrpc::Response<String> = match params {
            Some(params) => self.client.call_params(cmd, params)?,
            None => self.client.call(cmd)?,
        };
        Ok(response.result)
    }
}

/// Convenient struct to make it easy to build OvsInvalidResponse errors during parsing.
struct InvalidResponse(String, String);
impl InvalidResponse {
    pub(crate) fn error(&self, error: String) -> Error {
        Error::OvsInvalidResponse {
            cmd: self.0.clone(),
            response: self.1.clone(),
            error,
        }
    }
}

#[cfg(test)]
mod tests {

    use std::{
        path::{Path, PathBuf},
        process::{id, Command, Stdio},
    };

    use super::*;

    fn ovs_setup(test: &str) -> PathBuf {
        let tmpdir = format!("/tmp/ovs-unixctl-test-{}-{}", id(), test);
        let ovsdb_path = PathBuf::from(&tmpdir).join("conf.db");

        let schema: PathBuf = match env::var_os("OVS_DATADIR") {
            Some(datadir) => datadir
                .into_string()
                .expect("OVS_DATADIR has non-unicode content")
                .into(),
            None => "/usr/share/openvswitch/vswitch.ovsschema".into(),
        };

        fs::create_dir_all(&tmpdir).expect("cannot create tmp dir");

        Command::new("ovsdb-tool")
            .arg("create")
            .arg(&ovsdb_path)
            .arg(&schema)
            .status()
            .expect("Failed to create OVS database");

        let ovsdb_logfile = Path::new(&tmpdir).join("ovsdb-server.log");
        Command::new("ovsdb-server")
            .env("OVS_RUNDIR", &tmpdir)
            .arg(&ovsdb_path)
            .arg("--detach")
            .arg("--no-chdir")
            .arg("--pidfile")
            .arg(format!(
                "--remote=punix:{}",
                Path::new(&tmpdir).join("db.sock").to_str().unwrap()
            ))
            .arg(format!("--log-file={}", ovsdb_logfile.to_str().unwrap()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("Failed to start ovsdb-server");

        let ovs_logfile = Path::new(&tmpdir).join("ovs-vswitchd.log");
        Command::new("ovs-vswitchd")
            .env("OVS_RUNDIR", &tmpdir)
            .arg("--detach")
            .arg("--no-chdir")
            .arg("--pidfile")
            .arg(format!("--log-file={}", ovs_logfile.to_str().unwrap()))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("Failed to start ovs-vswitchd");
        std::thread::sleep(Duration::from_secs(1));
        PathBuf::from(tmpdir)
    }

    fn ovs_cleanup(tmpdir: &Path) {
        // Find and kill the processes based on PID files
        for daemon in &["ovsdb-server", "ovs-vswitchd"] {
            let log_file = tmpdir.join(format!("{}.log", daemon));
            if let Ok(log) = fs::read_to_string(&log_file) {
                println!("{}.log: \n{}", daemon, log);
            }
            let pid_file = tmpdir.join(format!("{}.pid", daemon));

            if pid_file.exists() {
                if let Ok(pid) = fs::read_to_string(&pid_file) {
                    if let Ok(pid) = pid.trim().parse::<i32>() {
                        Command::new("kill")
                            .arg("-9")
                            .arg(pid.to_string())
                            .status()
                            .expect("Failed to kill daemon process");
                    }
                }
            }
        }
        if let Err(err) = fs::remove_dir_all(tmpdir) {
            println!("{}", err);
        }
    }

    fn ovs_test<T>(name: &str, test: T)
    where
        T: Fn(OvsUnixCtl),
    {
        let tmp = ovs_setup(name);
        let tmp_copy = tmp.clone();

        std::panic::set_hook(Box::new(move |info| {
            ovs_cleanup(&tmp_copy);
            println!("panic: {}", info);
        }));
        let ovs = OvsUnixCtl::unix(
            OvsUnixCtl::find_socket_at("ovs-vswitchd", &tmp).expect("Failed to find socket"),
            None,
        );
        let ovs = ovs.unwrap();

        test(ovs);

        ovs_cleanup(&tmp);
    }

    #[test]
    #[cfg_attr(not(feature = "test_integration"), ignore)]
    fn list_commands() {
        ovs_test("list_commands", |mut ovs| {
            let cmds = ovs.list_commands().unwrap();
            assert!(cmds.iter().any(|(cmd, _args)| cmd == "list-commands"));

            assert!(cmds.iter().any(|(cmd, args)| (cmd, args)
                == (&"dpif-netdev/bond-show".to_string(), &"[dp]".to_string())));
        })
    }

    #[test]
    #[cfg_attr(not(feature = "test_integration"), ignore)]
    fn version() {
        ovs_test("version", |mut ovs| {
            let (x, y, z, _) = ovs.version().unwrap();
            // We don't know what version is running, let's check at least it's not 0.0.0.
            assert!(x + y + z > 0);
        })
    }

    #[test]
    #[cfg_attr(not(feature = "test_integration"), ignore)]
    fn vlog() {
        ovs_test("vlog", |mut ovs| {
            fn get_vlog_level(vlog: String, name: &str) -> String {
                let levels: Vec<(&str, &str)> = vlog
                    .lines()
                    .skip(2)
                    .map(|l| {
                        let parts = l.split_whitespace().collect::<Vec<&str>>();
                        assert_eq!(parts.len(), 4);
                        (parts[0], parts[3])
                    })
                    .collect();
                let (_, level) = levels.iter().find(|(module, _)| *module == name).unwrap();
                level.to_string()
            }

            let vlog = ovs.run("vlog/list", None).unwrap().unwrap();
            assert_eq!(get_vlog_level(vlog, "unixctl"), "INFO");

            ovs.run("vlog/set", Some(&["unixctl:dbg"])).unwrap();

            let vlog = ovs.run("vlog/list", None).unwrap().unwrap();
            assert_eq!(get_vlog_level(vlog, "unixctl"), "DBG");
        })
    }
}
