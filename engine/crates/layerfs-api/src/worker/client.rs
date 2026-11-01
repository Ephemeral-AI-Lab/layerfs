//! The caller's side: one request per connection, sent once.
use super::frame::{read_frame, write_frame};
use crate::host::outcome::{refused, Outcome};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

/// How the caller reaches a machine's worker (from `machine/<name>/worker.json`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// The loopback address on the caller's host.
    pub address: SocketAddr,
    /// The worker's secret.
    pub secret: String,
}

const CONNECT: Duration = Duration::from_secs(10);

/// Runs one host operation `op` in the worker. Events the worker sends
/// (progress, command output) go to `events`. A connection that cannot be
/// made is `Unavailable` (nothing was sent); one lost after the request was
/// sent is `Unknown`: the operation may have run, and nothing is resent.
pub fn call(
    endpoint: &Endpoint,
    op: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
) -> Outcome {
    let mut stream = TcpStream::connect_timeout(&endpoint.address, CONNECT).map_err(|error| {
        refused(
            "Unavailable",
            format!(
                "the worker at {} does not answer: {error}",
                endpoint.address
            ),
        )
    })?;
    let _ = stream.set_nodelay(true);
    let request = SdkValue::map([
        ("secret", SdkValue::text(&endpoint.secret)),
        ("op", SdkValue::text(op)),
        ("args", args),
    ]);
    write_frame(&mut stream, &request).map_err(|error| {
        refused(
            "Unknown",
            format!("sending to the worker failed part way: {error}"),
        )
    })?;
    loop {
        let frame = match read_frame(&mut stream) {
            Ok(Some(frame)) => frame,
            Ok(None) => return Err(lost(op, "the worker closed the connection")),
            Err(refusal) => return Err(lost(op, &refusal.message)),
        };
        if let Some(event) = frame.get("event") {
            events(event.clone());
        } else if let Some(value) = frame.get("value") {
            return Ok(value.clone());
        } else if let Some(refusal) = frame.get("refused") {
            return Err(SdkRefusal {
                code: refusal.get_text("code").unwrap_or("Failed").to_owned(),
                message: refusal.get_text("message").unwrap_or("").to_owned(),
                detail: refusal.get("detail").cloned().unwrap_or_default(),
            });
        } else {
            return Err(refused(
                "Failed",
                "the worker sent a frame of no known kind",
            ));
        }
    }
}

fn lost(op: &str, why: &str) -> SdkRefusal {
    refused(
        "Unknown",
        format!("{op}: the worker's answer was lost ({why}); it may have run, and was not resent"),
    )
}

/// Whether the worker answers its secret: `{ kind: "worker", version, pid }`.
pub fn ping(endpoint: &Endpoint) -> Outcome {
    call(endpoint, "worker.ping", SdkValue::Null, &mut |_| {})
}
