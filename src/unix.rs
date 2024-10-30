//! Synchronous jsonrpc transport over Unix sockets.

use std::{
    fmt,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Deserializer;

use crate::{
    error::*,
    jsonrpc::{JsonStream, JsonStreamClient},
    Result,
};

/// Unix socket transport.
#[derive(Debug)]
pub(crate) struct UnixJsonStream {
    sock: UnixStream,
}

impl JsonStream for UnixJsonStream {
    fn send<M: Serialize>(&mut self, msg: M) -> Result<()> {
        Ok(serde_json::to_writer(&self.sock, &msg)?)
    }

    fn recv<R>(&mut self) -> Result<R>
    where
        R: for<'a> Deserialize<'a>,
    {
        let resp: R = Deserializer::from_reader(&mut self.sock)
            .into_iter()
            .next()
            .ok_or_else(|| Error::Timeout)??;
        Ok(resp)
    }
}

#[derive(Debug)]
pub(crate) struct UnixJsonStreamClient {
    /// The path to the Unix Domain Socket.
    path: PathBuf,
    /// The read and write timeout to use.
    timeout: Option<Duration>,
}

impl UnixJsonStreamClient {
    /// Creates a new [`UnixJsonStreamClient`] without timeouts to use.
    pub(crate) fn new<P: AsRef<Path>>(path: P) -> UnixJsonStreamClient {
        UnixJsonStreamClient {
            path: path.as_ref().to_path_buf(),
            timeout: None,
        }
    }

    /// Sets the timeout.
    pub(crate) fn timeout(mut self, timeout: Duration) -> UnixJsonStreamClient {
        self.timeout = Some(timeout);
        self
    }
}

impl JsonStreamClient for UnixJsonStreamClient {
    type Stream = UnixJsonStream;

    fn connect(&mut self) -> Result<UnixJsonStream> {
        let sock = UnixStream::connect(&self.path).map_err(Error::Socket)?;
        sock.set_read_timeout(self.timeout).map_err(Error::Socket)?;
        sock.set_write_timeout(self.timeout)
            .map_err(Error::Socket)?;
        Ok(UnixJsonStream { sock })
    }
}

impl fmt::Display for UnixJsonStreamClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        write!(f, "unix://{}", self.path.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, os::unix::net::UnixListener, path, process, thread};

    use super::*;
    use crate::jsonrpc;

    #[test]
    fn ping_pong() {
        #[derive(Clone, serde::Deserialize, serde::Serialize)]
        struct Result {
            val: String,
            extra: u32,
        }

        let socket_path: path::PathBuf = format!("unix_test-{}.socket", process::id()).into();
        let server = UnixListener::bind(&socket_path).unwrap();

        // Client thread
        let cli_socket_path = socket_path.clone();
        let client_thread = thread::spawn(move || {
            let stream_client =
                UnixJsonStreamClient::new(cli_socket_path).timeout(Duration::from_secs(2));
            assert_eq!(
                format!("{}", stream_client),
                format!("unix://unix_test-{}.socket", process::id())
            );

            let mut client = jsonrpc::Client::new(stream_client).expect("client creation failed");

            for _n in 1..5 {
                let response: jsonrpc::Response<Result> = client
                    .call_params("ping", &["hello world".to_string()])
                    .unwrap();
                assert!(response.error.is_none());
                assert!(response.result.is_some());
                assert_eq!(response.result.as_ref().unwrap().val, "pong");
                assert_eq!(response.result.as_ref().unwrap().extra, 42);
            }
        });

        // Response and Request are optimized for used by the client, not the server.
        #[derive(Debug, Clone, Deserialize)]
        struct ReceiveRequest {
            method: String,
            params: Option<serde_json::Value>,
            id: usize,
        }

        #[derive(Debug, Clone, Serialize)]
        struct SendResponse<R> {
            result: Option<R>,
            error: Option<String>,
            id: Option<usize>,
        }

        // Fake server
        let (sock, _) = server.accept().unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut stream = UnixJsonStream { sock };
        for _n in 1..5 {
            let request: ReceiveRequest = stream.recv().unwrap();
            if request.method == "ping" {
                let params: Vec<String> =
                    serde_json::from_value(request.params.expect("params should exist"))
                        .expect("params should be Vector of Strings");
                assert_eq!(params.first().unwrap(), "hello world");

                let response = SendResponse {
                    result: Some(Result {
                        val: "pong".into(),
                        extra: 42,
                    }),
                    error: None,
                    id: Some(request.id),
                };
                stream.send(response).unwrap();
            } else {
                let response = SendResponse::<()> {
                    result: None,
                    error: Some("method not found".into()),
                    id: Some(request.id),
                };
                stream.send(response).unwrap();
            }
        }

        client_thread.join().unwrap();

        // Clean up
        fs::remove_file(&socket_path).unwrap();
    }
}
