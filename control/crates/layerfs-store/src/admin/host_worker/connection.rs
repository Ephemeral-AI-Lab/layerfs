//! One host worker connection on its Unix socket: the hello, the request,
//! its operation and the answer, within absolute deadlines. A PostgreSQL
//! request goes to the PostgreSQL queue and a streamed write to the write
//! queue; the guest worker's connections are `layerfs-api`'s.
use layerfs_home::reply::outcome::refused;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::time::{Duration, Instant};

/// Absolute deadlines apply to host protocol I/O. A Store producer is never
/// cancelled merely because the peer stops reading.
const INGRESS: Duration = Duration::from_secs(5);
const EGRESS: Duration = Duration::from_secs(5);

/// Serves one Unix connection on a reserved host worker thread: its hello
/// and request, unless `read` holds a request an ordinary thread already
/// read, then its operation and answer. A PostgreSQL read or single-change
/// PostgreSQL write read here whose request checks is returned for the
/// PostgreSQL lane instead (#819 P-B), and a streamed write read on the write endpoint for the
/// write lane (S2.2); a single-change SQLite write runs here. A request
/// that carries a login is overwritten once answered or held, on every
/// path.
pub(crate) fn serve_host_connection(
    server: &super::server::HostServer,
    socket: &std::os::unix::net::UnixStream,
    endpoint: super::server::Endpoint,
    read: Option<SdkValue>,
    admitted: &mut bool,
) -> super::server::Served {
    use super::server::{Endpoint, Served};
    let management = endpoint == Endpoint::Control;
    let writes = endpoint == Endpoint::Writes;
    let request = match read {
        Some(request) => request,
        None => match read_host_request(server, socket, management) {
            Some(request) if endpoint == Endpoint::Ordinary && is_postgres(&request) => {
                crate::admin::host_worker::postgresql::harden();
                // checked here, so the lane holds only small requests; one
                // the request alone refuses is answered on this thread
                match server.lane_request(request) {
                    Ok(held) => return Served::Postgres(held),
                    Err(request) => request,
                }
            }
            Some(request) => request,
            None => return Served::Answered(false),
        },
    };
    // a PostgreSQL call's password goes with its answer (#819 P-B)
    let secret = carries_login(&request);
    if secret {
        crate::admin::host_worker::postgresql::harden();
    }
    let request = layerfs_home::secret::wipe::Guard::new(request, secret);
    let request = request.get();
    let mut refusal = None;
    if !management {
        match held_write(server, socket, request, writes) {
            Some(Ok(held)) if writes => return Served::Write(held),
            Some(Ok(held)) => return Served::Answered(server.answer_write(&held, admitted)),
            Some(Err(error)) => refusal = Some(error),
            None => {}
        }
    }
    let answered = answer_host_request(server, socket, management, request, admitted, refusal);
    Served::Answered(answered)
}

/// Answers PostgreSQL `request`, read by an ordinary thread and never run,
/// with `refusal`: whether the answer was written.
pub(crate) fn refuse_host_request(
    server: &super::server::HostServer,
    socket: &std::os::unix::net::UnixStream,
    request: SdkValue,
    refusal: SdkRefusal,
) -> bool {
    let answered = answer_host_request(server, socket, false, &request, &mut false, Some(refusal));
    crate::admin::host_worker::postgresql::wipe_request(request);
    answered
}

/// Whether `request` is for the host worker's PostgreSQL lane: a
/// PostgreSQL read, or a single-change write that carries a login (a
/// streamed one comes in on the write endpoint).
pub(crate) fn is_postgres(request: &SdkValue) -> bool {
    match request.get_text("op") {
        Some(crate::admin::host_worker::postgresql::OP) => true,
        Some(crate::admin::host_worker::write::OP) => {
            carries_login(request)
                && request.get("args").and_then(|args| args.get_text("body")) == Some("none")
        }
        _ => false,
    }
}

/// Whether `request`'s arguments carry a PostgreSQL login (#819 P-B).
fn carries_login(request: &SdkValue) -> bool {
    (request.get("args")).is_some_and(|args| args.get("password").is_some())
}

/// The hello, then the one request frame, within their deadlines.
fn read_host_request(
    server: &super::server::HostServer,
    socket: &std::os::unix::net::UnixStream,
    management: bool,
) -> Option<SdkValue> {
    use layerfs_machine::route::{
        host_check_peer, read_host_frame, write_host_frame, HOST_MAX_REQUEST, MAX_HELLO,
    };

    if host_check_peer(socket).is_err()
        || write_host_frame(
            socket,
            &server.hello.to_frame(),
            MAX_HELLO,
            // From this thread's turn, not the accept: a connection that
            // waited in the queue behind long calls is still greeted (§16.1 C7).
            Instant::now() + layerfs_machine::route::HELLO_WAIT,
        )
        .is_err()
    {
        return None;
    }
    let max = if management {
        MAX_HELLO
    } else {
        HOST_MAX_REQUEST
    };
    read_host_frame(socket, max, Some(Instant::now() + INGRESS))
        .ok()
        .flatten()
}

/// `request`'s operation and its answer: whether the answer was written.
fn answer_host_request(
    server: &super::server::HostServer,
    socket: &std::os::unix::net::UnixStream,
    management: bool,
    request: &SdkValue,
    admitted: &mut bool,
    refusal: Option<SdkRefusal>,
) -> bool {
    use layerfs_home::log::trace;

    let Some(request_id) = request
        .get_text("request_id")
        .filter(|id| !id.is_empty() && id.len() <= 128)
    else {
        return false;
    };
    let traced = request.get_bool("trace_step");
    if traced {
        trace::steps_begin();
    }
    let outcome = match refusal {
        Some(refusal) => Err(refusal),
        None => match (request.get_text("op"), request.get("args")) {
            _ if request.get_text("generation") != Some(server.hello.generation.as_str()) => {
                Err(refused(
                    "Unavailable",
                    "the request names another host worker generation",
                ))
            }
            (Some(op), Some(args @ SdkValue::Map(_))) => {
                server.dispatch(management, op, args, admitted)
            }
            _ => Err(refused(
                "InvalidInput",
                "the host request needs op and args",
            )),
        },
    };
    reply(socket, request_id, traced, outcome)
}

/// A `store.write` request with a valid id, this generation and a map of
/// arguments whose header checks (method, body, field names and scalar
/// values), held with its connection: on the write endpoint a streamed
/// write, for the write lane (§11); on the ordinary endpoint a
/// single-change write, run on its thread. A write of the other kind, any
/// other operation on the write endpoint, a header that does not check
/// and a connection that cannot be held are refused, nothing run. A
/// request without a valid id, generation or arguments is answered (or
/// not) as an ordinary request, which never reaches an operation.
fn held_write(
    server: &super::server::HostServer,
    socket: &std::os::unix::net::UnixStream,
    request: &SdkValue,
    writes: bool,
) -> Option<Result<super::server::Held, SdkRefusal>> {
    let unsupported = |why: &str| Some(Err(refused("Unsupported", why)));
    const ONLY_STREAMED: &str = "the host worker's write endpoint serves only streamed file writes";
    if request.get_text("op") != Some(crate::admin::host_worker::write::OP) {
        return if writes {
            unsupported(ONLY_STREAMED)
        } else {
            None
        };
    }
    let request_id = request
        .get_text("request_id")
        .filter(|id| !id.is_empty() && id.len() <= 128)?;
    if request.get_text("generation") != Some(server.hello.generation.as_str()) {
        return None;
    }
    let args @ SdkValue::Map(_) = request.get("args")? else {
        return None;
    };
    // checked before the write is held: a held write keeps only a header
    // within its bound (scalar fields of a 64 KiB request)
    let (store, body) = match crate::admin::host_worker::write::header(args) {
        Ok((_, store, _, body, _)) => (store, body),
        Err(refusal) => return Some(Err(refusal)),
    };
    // a PostgreSQL write waits for a session slot of its Store (#819 P-B);
    // an unusable name is refused when it runs, holding none
    let postgresql = (args.get("password").is_some())
        .then(|| layerfs_home::names::StoreName::parse(store).ok())
        .flatten()
        .map(|store| store.as_str().to_owned());
    match (body != "none", writes) {
        (true, false) => {
            return unsupported("a streamed file write goes to the host worker's write endpoint")
        }
        (false, true) => return unsupported(ONLY_STREAMED),
        _ => {}
    }
    Some(match socket.try_clone() {
        Ok(stream) => Ok(super::server::Held {
            stream,
            args: args.clone(),
            request_id: request_id.to_owned(),
            traced: request.get_bool("trace_step"),
            postgresql,
        }),
        Err(error) => Err(refused(
            "Failed",
            format!("cannot hold the write's connection: {error}; nothing was committed"),
        )),
    })
}

/// Writes one request's answer, with its trace steps when traced; whether
/// it was delivered.
pub(crate) fn reply(
    socket: &std::os::unix::net::UnixStream,
    request_id: &str,
    traced: bool,
    outcome: layerfs_home::reply::outcome::Outcome,
) -> bool {
    use layerfs_home::log::trace;
    use layerfs_machine::route::{write_host_frame, HOST_MAX_REPLY};

    let answer = match outcome {
        Ok(value) => ("ok", value),
        Err(refusal) => (
            "error",
            SdkValue::map([
                ("code", SdkValue::text(refusal.code)),
                ("message", SdkValue::text(refusal.message)),
                ("detail", refusal.detail),
            ]),
        ),
    };
    let mut fields = vec![
        ("request_id".to_owned(), SdkValue::text(request_id)),
        (answer.0.to_owned(), answer.1),
    ];
    if traced {
        trace::step("host_dispatch");
        fields.push(("steps".to_owned(), trace::steps_value(trace::steps_take())));
    }
    write_host_frame(
        socket,
        &SdkValue::Map(fields),
        HOST_MAX_REPLY,
        Instant::now() + EGRESS,
    )
    .is_ok()
}

/// Tells a held write's caller to send its input now (`{"request_id",
/// "go": true}`): sent when a write thread starts reading it, so the
/// caller never streams into a queue; whether it was delivered.
pub(crate) fn go(socket: &std::os::unix::net::UnixStream, request_id: &str) -> bool {
    use layerfs_machine::route::{write_host_frame, HOST_MAX_REPLY};
    let frame = SdkValue::map([
        ("request_id", SdkValue::text(request_id)),
        ("go", SdkValue::Bool(true)),
    ]);
    write_host_frame(socket, &frame, HOST_MAX_REPLY, Instant::now() + EGRESS).is_ok()
}
