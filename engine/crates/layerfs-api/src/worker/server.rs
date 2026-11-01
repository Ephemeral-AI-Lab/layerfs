//! The worker's side: `layerfs worker` serves one request per connection.
//!
//! A request is `{ secret, op, args }`. `worker.ping` answers who it is;
//! `host.call` and `host.backend` run one host operation (`args.op` with
//! `args.args`) on this machine, with `home` set to the worker's own root
//! (the machine's data disk), never a path the caller sends. A wrong secret
//! is refused before anything runs. Answers are `{ value }` or
//! `{ refused }`, after any `{ event }` frames.
use super::frame::{read_frame, write_frame};
use crate::host::outcome::{refused, Outcome};
use layerfs_bridge::control::SdkValue;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// What a worker served before it stopped.
#[derive(Debug, Default)]
pub struct Served {
    /// Requests answered.
    pub requests: u64,
}

/// Serves `listener` until it fails. `root` is the worker's own state root.
pub fn serve(listener: TcpListener, secret: String, root: PathBuf) -> std::io::Result<Served> {
    // the root holds Stores and records: private to the worker's user, as a
    // home is (a machine's data disk mounts 0755)
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&root)?;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700))?;
    }
    let secret = Arc::new(secret);
    let root = Arc::new(root);
    let served = Arc::new(AtomicU64::new(0));
    for stream in listener.incoming() {
        let stream = stream?;
        let (secret, root, served) = (secret.clone(), root.clone(), served.clone());
        std::thread::spawn(move || {
            if answer(stream, &secret, &root) {
                served.fetch_add(1, Ordering::Relaxed);
            }
            super::server::touch();
        });
    }
    Ok(Served {
        requests: served.load(Ordering::Relaxed),
    })
}

static LAST: AtomicU64 = AtomicU64::new(0);

/// Marks this worker active now (the idle clock restarts).
pub(crate) fn touch() {
    LAST.store(crate::home::now_ms() as u64, Ordering::Relaxed);
}

/// When this worker last answered, in ms since the epoch; 0 before any.
pub fn last_active() -> u64 {
    LAST.load(Ordering::Relaxed)
}

/// Equal secrets, compared in time independent of where they differ.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn answer(mut stream: TcpStream, secret: &str, root: &Path) -> bool {
    let _ = stream.set_nodelay(true);
    let request = match read_frame(&mut stream) {
        Ok(Some(request)) => request,
        _ => return false,
    };
    let outcome = if !same(request.get_text("secret").unwrap_or(""), secret) {
        Err(refused("Denied", "the worker's secret does not match"))
    } else {
        touch();
        run(
            request.get_text("op").unwrap_or(""),
            request.get("args").cloned().unwrap_or_default(),
            root,
            &mut |event| {
                let _ = write_frame(&mut stream, &SdkValue::map([("event", event)]));
            },
        )
    };
    let frame = match outcome {
        Ok(value) => SdkValue::map([("value", value)]),
        Err(refusal) => SdkValue::map([(
            "refused",
            SdkValue::map([
                ("code", SdkValue::Text(refusal.code)),
                ("message", SdkValue::Text(refusal.message)),
                ("detail", refusal.detail),
            ]),
        )]),
    };
    write_frame(&mut stream, &frame).is_ok()
}

fn run(op: &str, args: SdkValue, root: &Path, _events: &mut dyn FnMut(SdkValue)) -> Outcome {
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
            crate::names::name("store", store).map_err(|why| refused("InvalidInput", why))?;
            let file = crate::home::paths::store_file(root, store);
            let inner = crate::home::with_field(
                inner,
                "storePath",
                SdkValue::text(file.display().to_string()),
            );
            crate::store::open::run(&name, inner, None)
        }
        // one step of a Store move or remove on this machine
        "store.file" => {
            let name = args
                .get_text("op")
                .ok_or_else(|| refused("InvalidInput", "Store step name"))?
                .to_owned();
            crate::store::files::run(root, &name, &args.get("args").cloned().unwrap_or_default())
        }
        // a Sandbox or Workspace operation on this machine's own native
        "sandbox.host" => crate::family::sandboxes::in_worker(root, &args),
        "host.call" | "host.backend" => {
            let name = args
                .get_text("op")
                .ok_or_else(|| refused("InvalidInput", "host operation name"))?
                .to_owned();
            let inner = with_home(args.get("args").cloned().unwrap_or_default(), root);
            if op == "host.call" {
                crate::host::dispatch::call(&name, inner)
            } else {
                crate::host::dispatch::backend(name, inner)
            }
        }
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
        .filter(|store| crate::names::name("store", store).is_ok())
        .map(|store| crate::home::paths::store_file(root, store));
    let text = SdkValue::text(root.display().to_string());
    let args = crate::home::with_field(args, "home", text);
    match file {
        Some(file) => crate::home::with_field(
            args,
            "storePath",
            SdkValue::text(file.display().to_string()),
        ),
        None => args,
    }
}
