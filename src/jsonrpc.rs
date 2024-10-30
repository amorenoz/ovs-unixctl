//! A simple JSON-RPC client compatible with OVS unixctl.

use std::{
    fmt, path,
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
    time,
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{error::*, unix, Result};

// JsonStreams are capable of sending and receiving JSON messages.
pub(crate) trait JsonStream {
    /// Send a message to the target.
    fn send<M: Serialize>(&mut self, msg: M) -> Result<()>;

    /// Receive a message from the target (blocking).
    fn recv<R>(&mut self) -> Result<R>
    where
        R: for<'a> Deserialize<'a>;
}

// Client streams can connect and disconnect from targets creating
// some JsonStream.
pub(crate) trait JsonStreamClient: fmt::Display {
    type Stream: JsonStream;
    /// Connect to the target.
    fn connect(&mut self) -> Result<Self::Stream>;
}

/// A JSON-RPC request.
#[derive(Debug, Serialize)]
pub struct Request<'a, P: Serialize + AsRef<str> = &'a str> {
    /// The name of the RPC call.
    pub method: &'a str,
    /// Parameters to the RPC call.
    pub params: &'a [P],
    /// Identifier for this request, which should appear in the response.
    pub id: usize,
}

/// A JSONRPC response object.
/// TODO make generic
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Response<R = String> {
    /// The result of the request.
    pub result: Option<R>,
    /// An error if it occurred.
    pub error: Option<String>,
    /// Identifier for this response. It should match that of the associated request.
    pub id: Option<usize>,
}

/// JSON-RPC client.
#[derive(Debug)]
pub(crate) struct Client<C: JsonStreamClient> {
    stream: C::Stream,
    last_id: AtomicUsize,
}

impl<C: JsonStreamClient> Client<C> {
    /// Creates a new client with the given transport.
    pub(crate) fn new(mut stream_client: C) -> Result<Client<C>> {
        let stream = stream_client.connect()?;
        Ok(Client {
            stream,
            last_id: AtomicUsize::new(1),
        })
    }

    /// Creates a new client with a Unix socket transport.
    pub(crate) fn unix<P: AsRef<path::Path>>(
        sock_path: P,
        timeout: Option<time::Duration>,
    ) -> Result<Client<unix::UnixJsonStreamClient>> {
        let mut stream_client = unix::UnixJsonStreamClient::new(sock_path);
        if let Some(timeout) = timeout {
            stream_client = stream_client.timeout(timeout);
        }
        Client::new(stream_client)
    }

    /// Builds a request with the given method and parameters.
    ///
    /// It internally deals with incrementing the id.
    fn build_request<'a, P: Serialize + AsRef<str>>(
        &self,
        method: &'a str,
        params: &'a [P],
    ) -> Request<'a, P> {
        Request {
            method,
            params,
            id: self.last_id.fetch_add(1, Relaxed),
        }
    }

    /// Sends a request and returns the response.
    pub fn send_request<R: DeserializeOwned, P: Serialize + AsRef<str>>(
        &mut self,
        request: Request<P>,
    ) -> Result<Response<R>> {
        let stream = &mut self.stream;
        let req_id = request.id;

        stream.send(request)?;
        let res: Response<R> = stream.recv()?;
        if res
            .id
            .ok_or_else(|| Error::Protocol("id not found in response".to_string()))?
            != req_id
        {
            return Err(Error::Protocol(
                "request and response ids do not match".to_string(),
            ));
        }

        Ok(res)
    }

    /// Calls a method with some arguments and returns the result.
    pub(crate) fn call_params<R: DeserializeOwned, P: Serialize + AsRef<str>>(
        &mut self,
        method: &str,
        params: &[P],
    ) -> Result<Response<R>> {
        let request = self.build_request(method, params);
        let response = self.send_request(request)?;
        if let Some(error) = response.error {
            return Err(Error::Command {
                cmd: String::from(method),
                params: params
                    .iter()
                    .map(|p| p.as_ref())
                    .collect::<Vec<&str>>()
                    .join(", "),
                error,
            });
        }
        Ok(response)
    }

    /// Calls a method without arguments and resturns the result.
    pub(crate) fn call<R: DeserializeOwned>(&mut self, method: &str) -> Result<Response<R>> {
        let request = self.build_request::<&str>(method, &[]);
        let response = self.send_request(request)?;
        if let Some(error) = response.error {
            return Err(Error::Command {
                cmd: String::from(method),
                params: String::default(),
                error,
            });
        }
        Ok(response)
    }
}
