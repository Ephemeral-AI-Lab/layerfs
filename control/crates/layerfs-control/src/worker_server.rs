//! The worker's side: `layerfs worker` serves one request per connection.
//! It runs every entity's operations, so it sits above them in
//! `layerfs-control` (layerfs-project#104); the protocol is
//! `layerfs-machine` `worker`.
//!
//! A connection starts with a small frame `{ secret }`: the worker reads at
//! most [`MAX_HELLO`] bytes, waiting at most [`HELLO_WAIT`], and answers
//! `{ ready: true, protocol, bundle }` ([`Hello`]), or refuses a wrong
//! secret and closes, before it reads anything larger. A caller of another
//! build sends nothing more and closes. Then a request `{ op, args }`, read with at most
//! [`REQUEST_WAIT`] between bytes. `worker.ping` answers who it is;
//! `store.call`, `store.file`, `sandbox.host`, `project.stage`,
//! `project.init`, `project.export` and `project.remove` run one operation on
//! this machine,
//! with `home` set to the worker's own root (the machine's data disk),
//! never a path the caller sends. Answers are
//! `{ value }` or `{ refused }`, after any `{ event }` frames.
//!
//! A request may carry `home_id`, the caller's home id: the root is not a
//! home and has no id of its own (O33), so host code that asks for one
//! gets the caller's, and the root is never converted or written as a home.
//!
//! A request with `trace_step: true` (a caller with `LAYERFS_TRACE_STEP`
//! set) has its steps timed from when it was read, and its answer carries
//! them as `steps: [{ step, us }]` beside `value` or `refused`.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::{
    read_frame, read_frame_within, write_frame, Hello, HELLO_WAIT, MAX_HELLO,
};
use layerfs_store::project::discovery::home_of;
use layerfs_store::project::export::in_worker as export_in_worker;
use layerfs_store::project::frame::{ExportFrame, InitFrame, RemoveFrame, StageFrame};
use layerfs_store::project::{init::in_worker, remove::remove_project, staged};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// The longest the worker waits for the next byte of a request.
pub const REQUEST_WAIT: Duration = Duration::from_secs(30);

/// What a worker served before it stopped.
#[derive(Debug, Default)]
pub struct Served {
    /// Requests answered.
    pub requests: u64,
}

/// Serves `listener` until it fails or the process is ended (`machine.stop`;
/// there is no idle stop, #535 D11). `root` is the worker's own state root.
pub fn serve(listener: TcpListener, secret: String, root: PathBuf) -> std::io::Result<Served> {
    // the root holds Stores and records: private, as a home is (a machine's
    // data disk mounts 0755), but passable by others (0701), since a
    // namespaces Sandbox's commands run as another identity below it
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o701)
            .create(&root)?;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o701))?;
    }
    layerfs_home::home::guest::serve_root(&root);
    layerfs_home::home::guest::retire_layout_2(&root);
    let secret = Arc::new(secret);
    let root = Arc::new(root);
    // what it answers every secret with, read once
    let ready = Arc::new(Hello::own().to_frame());
    let served = Arc::new(AtomicU64::new(0));
    for stream in listener.incoming() {
        let stream = stream?;
        let (secret, root, served) = (secret.clone(), root.clone(), served.clone());
        let ready = ready.clone();
        std::thread::spawn(move || {
            if answer(stream, &secret, &ready, &root) {
                served.fetch_add(1, Ordering::Relaxed);
            }
        });
    }
    Ok(Served {
        requests: served.load(Ordering::Relaxed),
    })
}

/// Equal secrets, compared in time independent of where they differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn answer(mut stream: TcpStream, secret: &str, ready: &SdkValue, root: &Path) -> bool {
    let _ = stream.set_nodelay(true);
    // the secret first, from a small frame: nothing larger is read, and
    // nothing is waited for long, before it matches
    if stream.set_read_timeout(Some(HELLO_WAIT)).is_err() {
        return false;
    }
    let hello = match read_frame_within(&mut stream, MAX_HELLO) {
        Ok(Some(hello)) => hello,
        _ => return false,
    };
    if !same(hello.get_text("secret").unwrap_or(""), secret) {
        let denied = refused("Denied", "the worker's secret does not match");
        let _ = write_frame(&mut stream, &refusal_frame(denied));
        return false;
    }
    if write_frame(&mut stream, ready).is_err()
        || stream.set_read_timeout(Some(REQUEST_WAIT)).is_err()
    {
        return false;
    }
    let request = match read_frame(&mut stream) {
        Ok(Some(request)) => request,
        _ => return false,
    };
    let _ = stream.set_read_timeout(None);
    let stepped = request.get("trace_step") == Some(&SdkValue::Bool(true));
    if stepped {
        layerfs_home::trace::steps_begin();
    }
    let _home = layerfs_home::home::guest::sent(request.get_text("home_id"));
    let (op, args) = op_and_args(request);
    let outcome = run(&op, args, root, &mut |event| {
        write_frame(&mut stream, &SdkValue::map([("event", event)]))
            .map_err(|e| refused("Failed", format!("worker event stream closed: {e}")))
    });
    let mut frame = match outcome {
        Ok(value) => SdkValue::map([("value", value)]),
        Err(refusal) => refusal_frame(refusal),
    };
    if stepped {
        layerfs_home::trace::step("result");
        let steps = layerfs_home::trace::steps_value(layerfs_home::trace::steps_take());
        frame = layerfs_home::home::with_field(frame, "steps", steps);
    }
    let sent = write_frame(&mut stream, &frame).is_ok();
    if sent {
        await_close(&mut stream);
    }
    sent
}

/// Waits for the client to close first, once the answer is written. The
/// client sends nothing after its request and closes when it has read the
/// answer. Apple Container's published port drops what it still holds for
/// the host when the guest side closes first, which cut the tail off large
/// answers (an export's last parts). Reads only that close, at most one
/// [`REQUEST_WAIT`]; a client that never closes then loses nothing worse
/// than before.
fn await_close(stream: &mut TcpStream) {
    use std::io::Read;
    if stream.set_read_timeout(Some(REQUEST_WAIT)).is_ok() {
        let _ = stream.read(&mut [0u8; 1]);
    }
}

/// A request's `op` and its `args` (moved out, not copied: they may hold a
/// large part of a file).
fn op_and_args(request: SdkValue) -> (String, SdkValue) {
    let (mut op, mut args) = (String::new(), SdkValue::Null);
    if let SdkValue::Map(fields) = request {
        for (name, value) in fields {
            match (name.as_str(), value) {
                ("op", SdkValue::Text(text)) => op = text,
                ("args", value) => args = value,
                _ => {}
            }
        }
    }
    (op, args)
}

fn refusal_frame(refusal: SdkRefusal) -> SdkValue {
    SdkValue::map([(
        "refused",
        SdkValue::map([
            ("code", SdkValue::Text(refusal.code)),
            ("message", SdkValue::Text(refusal.message)),
            ("detail", refusal.detail),
        ]),
    )])
}

fn run(
    op: &str,
    args: SdkValue,
    root: &Path,
    events: &mut dyn FnMut(SdkValue) -> std::result::Result<(), SdkRefusal>,
) -> Outcome {
    match op {
        "worker.ping" => Ok(SdkValue::map([
            ("kind", SdkValue::text("worker")),
            ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
            ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ])),
        // one Store call, on the Store file under this worker's root (2.0)
        "store.call" => {
            let call = layerfs_store::call::CallFrame::decode(&args)?;
            let store = call.store.as_str();
            layerfs_home::names::StoreName::parse(store)?;
            let file = layerfs_home::home::paths::store_file(root, store);
            // the worker holds the Store's lease under its root while it
            // keeps the Store open
            let lease = layerfs_store::lease::hold_locked(root, store)?;
            let open = layerfs_store::open_args::OpenArgs {
                home: Some(root.to_path_buf()),
                store_path: Some(file.display().to_string()),
                ..call.open()
            };
            layerfs_store::open::run(&call.method, &open, call.fields, Some(lease))
        }
        // one step of a Store move or remove on this machine
        "store.file" => {
            let frame = layerfs_store::sqlite::transfer::step::StepFrame::decode(&args)?;
            layerfs_store::sqlite::transfer::steps::run(root, &frame)
        }
        // a Sandbox or Workspace operation on this machine's own native
        "sandbox.host" => layerfs_sandbox::route::in_worker(
            root,
            layerfs_sandbox::host::decode(&args)?,
            &mut |event| {
                let _ = events(event);
            },
            layerfs_sandbox::workspace::dispatch::call,
        ),
        // drops one project from the Store under this worker's root (2.0):
        // `{ store, project, store_record }`
        "project.remove" => {
            let root = home_of(Some(root))?;
            remove_project(&root, &RemoveFrame::decode(&args)?)
        }
        // writes or discards one part of a caller's folder copy under this
        // worker's root (#536 H11a)
        "project.stage" => staged::run(root, &StageFrame::decode(args)?),
        // adds a folder copy the caller sent as a new project of a Store
        // under this worker's root (#536 H11, H11a)
        "project.init" => {
            let root = home_of(Some(root))?;
            in_worker(&root, &InitFrame::decode(&args)?)
        }
        // streams one project's snapshot from a Store under this worker's
        // root as folder-copy parts (#53)
        "project.export" => export_in_worker(root, &ExportFrame::decode(&args)?, events),
        _ => Err(refused(
            "InvalidInput",
            format!("the worker has no operation {op}"),
        )),
    }
}
