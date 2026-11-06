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
//! `project.init` and `project.remove` run one operation on this machine,
//! with `home` set to the worker's own root (the machine's data disk),
//! never a path the caller sends. Answers are
//! `{ value }` or `{ refused }`, after any `{ event }` frames.
//!
//! A request may carry `home_id`, the caller's home id: the root is not a
//! home and has no id of its own (O33), so host code that asks for one
//! gets the caller's, and the root is never converted or written as a home.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_machine::worker::{
    read_frame, read_frame_within, write_frame, Hello, HELLO_WAIT, MAX_HELLO,
};
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
    let _home = layerfs_home::home::guest::sent(request.get_text("home_id"));
    let outcome = run(
        request.get_text("op").unwrap_or(""),
        request.get("args").cloned().unwrap_or_default(),
        root,
        &mut |event| {
            let _ = write_frame(&mut stream, &SdkValue::map([("event", event)]));
        },
    );
    let frame = match outcome {
        Ok(value) => SdkValue::map([("value", value)]),
        Err(refusal) => refusal_frame(refusal),
    };
    write_frame(&mut stream, &frame).is_ok()
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

fn run(op: &str, args: SdkValue, root: &Path, events: &mut dyn FnMut(SdkValue)) -> Outcome {
    match op {
        "worker.ping" => Ok(SdkValue::map([
            ("kind", SdkValue::text("worker")),
            ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
            ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ])),
        // one Store call, on the Store file under this worker's root (2.0)
        "store.call" => {
            let name = args
                .get_text("op")
                .ok_or_else(|| refused("InvalidInput", "Store operation name"))?
                .to_owned();
            let inner = with_home(args.get("args").cloned().unwrap_or_default(), root);
            let store = inner
                .get_text("store")
                .ok_or_else(|| refused("InvalidInput", "a Store call names its Store"))?;
            layerfs_home::names::StoreName::parse(store)?;
            let file = layerfs_home::home::paths::store_file(root, store);
            // the worker holds the Store's lease under its root while it
            // keeps the Store open
            let lease = layerfs_store::lease::hold_locked(root, store)?;
            let inner = layerfs_home::home::with_field(
                inner,
                "storePath",
                SdkValue::text(file.display().to_string()),
            );
            layerfs_store::open::run(&name, inner, Some(lease))
        }
        // one step of a Store move or remove on this machine
        "store.file" => {
            let name = args
                .get_text("op")
                .ok_or_else(|| refused("InvalidInput", "Store step name"))?
                .to_owned();
            layerfs_store::sqlite::transfer::steps::run(
                root,
                &name,
                &args.get("args").cloned().unwrap_or_default(),
            )
        }
        // a Sandbox or Workspace operation on this machine's own native
        "sandbox.host" => crate::sandbox::route::in_worker(root, &args, events),
        // drops one project from the Store under this worker's root (2.0):
        // `{ store, project, store_record }`
        "project.remove" => layerfs_store::identity::remove_project(&with_home(args, root)),
        // writes or discards one part of a caller's folder copy under this
        // worker's root (#536 H11a)
        "project.stage" => layerfs_store::project::staged::run(root, &args),
        // adds a folder copy the caller sent as a new project of a Store
        // under this worker's root (#536 H11, H11a)
        "project.init" => layerfs_store::project::init::in_worker(&with_home(args, root)),
        _ => Err(refused(
            "InvalidInput",
            format!("the worker has no operation {op}"),
        )),
    }
}

/// `args` with `home` set to the worker's root, whatever the caller sent.
/// A call that names a valid `store` gets that Store's file under the root
/// as its `storePath`, whatever the caller sent.
fn with_home(args: SdkValue, root: &Path) -> SdkValue {
    let file = args
        .get_text("store")
        .filter(|store| layerfs_home::names::StoreName::parse(store).is_ok())
        .map(|store| layerfs_home::home::paths::store_file(root, store));
    let text = SdkValue::text(root.display().to_string());
    let args = layerfs_home::home::with_field(args, "home", text);
    match file {
        Some(file) => layerfs_home::home::with_field(
            args,
            "storePath",
            SdkValue::text(file.display().to_string()),
        ),
        None => args,
    }
}
