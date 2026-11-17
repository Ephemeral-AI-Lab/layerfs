//! The worker's operations: what each request's `op` runs on this machine,
//! and which of them are read-only.
use super::Server;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store::project::discovery::home_of;
use layerfs_store::project::export::in_worker as export_in_worker;
use layerfs_store::project::frame::{ExportFrame, InitFrame, RemoveFrame, StageFrame};
use layerfs_store::project::{init::in_worker, remove::remove_project, staged};
use std::path::Path;

/// The operations that change nothing on the machine. Only these answer
/// `Failed` after a panic, and only when no event byte was written yet;
/// any other operation's panic closes the connection, so its caller
/// reports `Unknown` (layerfs-project#179 D7). A new read-only operation
/// is added here.
const READ_ONLY: [&str; 6] = [
    "worker.ping",
    layerfs_sandbox::inspect::OP,
    "machine.images",
    "store.reach",
    "doctor.sandbox",
    "store.diagnose",
];

/// Whether `op` is one of the read-only operations.
pub(super) fn read_only(op: &str) -> bool {
    READ_ONLY.contains(&op)
}

/// Who worker `server` is, as `machine.stats` shows it
/// (layerfs-project#164): `{ version, pid, started_at, protocol, bundle }`,
/// `started_at` taken once when it began to serve.
fn about(server: &Server) -> SdkValue {
    let said = |field: &str| server.ready.get(field).cloned().unwrap_or_default();
    SdkValue::map([
        ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
        ("pid", SdkValue::Int(i64::from(std::process::id()))),
        ("started_at", SdkValue::Int(server.started)),
        ("protocol", said("protocol")),
        ("bundle", said("bundle")),
    ])
}

/// One inspect request (layerfs-project#178): a read of this machine or
/// of one of its Sandboxes, under this process's inspect admission and
/// deadlines, on a helper thread. It sends no events and changes nothing.
/// The request carries who this worker is (`worker`, [`about`]), set here
/// whatever the caller sent.
#[cfg(not(feature = "worker-fault"))]
fn inspect(server: &Server, args: SdkValue) -> Outcome {
    let args = layerfs_home::home::with_field(args, "worker", about(server));
    layerfs_sandbox::inspect::in_worker(&server.root, args)
}

/// [`inspect`] with the fault switch of its helper thread: what
/// `inspect::in_worker` does, with the switch met first, under the permit.
#[cfg(feature = "worker-fault")]
fn inspect(server: &Server, args: SdkValue) -> Outcome {
    use layerfs_sandbox::inspect::{admit, serve, Side};
    let args = layerfs_home::home::with_field(args, "worker", about(server));
    let (root, faults) = (server.root.clone(), server.faults.clone());
    admit(move |budget| {
        faults.in_helper();
        serve(&Side::Worker(&root), &args, budget)
    })
}

/// Runs operation `op` once with `args`, under the root of worker
/// `server`. What it reports while it runs goes to `events`.
pub(super) fn run(
    op: &str,
    args: SdkValue,
    server: &Server,
    events: &mut dyn FnMut(SdkValue) -> std::result::Result<(), SdkRefusal>,
) -> Outcome {
    let root: &Path = &server.root;
    match op {
        "worker.ping" => {
            let mut fields = vec![
                ("kind", SdkValue::text("worker")),
                ("version", SdkValue::text(env!("CARGO_PKG_VERSION"))),
                ("pid", SdkValue::Int(i64::from(std::process::id()))),
            ];
            if args.get("capacity") == Some(&SdkValue::Bool(true)) {
                let available = layerfs_home::home::filesystem_available(root)
                    .ok()
                    .and_then(|bytes| i64::try_from(bytes).ok());
                fields.push(("free_disk", available.map_or(SdkValue::Null, SdkValue::Int)));
            }
            Ok(SdkValue::Map(
                fields
                    .into_iter()
                    .map(|(name, value)| (name.to_owned(), value))
                    .collect(),
            ))
        }
        layerfs_sandbox::inspect::OP => inspect(server, args),
        "machine.images" => layerfs_sandbox::images::in_worker(root, args),
        "machine.prune" => layerfs_sandbox::images::prune_in_worker(root, args),
        "doctor.sandbox" => crate::doctor::diagnose_worker_sandbox(root, args),
        // removes one leftover under this root, under its guard (#180)
        "doctor.fix" => crate::doctor::leftover_fix::fix_in_worker(root, &args),
        "store.diagnose" => crate::doctor::diagnose_store_in_worker(root, args, events),
        // `store.attach`'s check of this machine's connection to a
        // PostgreSQL Store: the caller sends the record and what it dials
        "store.reach" => crate::store::reach_in_worker(&args),
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
            // a hashing step (snapshot, verify) reports its bytes as events
            // the caller passes on (#536 §10b); once one cannot be written
            // the step still ends, and nothing more is written (#177)
            layerfs_store::sqlite::transfer::steps::run_observed(
                root,
                &frame,
                &mut |done, total| {
                    let _ = events(SdkValue::map([(
                        "progress",
                        SdkValue::map([
                            ("phase", SdkValue::text(frame.step.name())),
                            ("done", SdkValue::Int(done as i64)),
                            ("total", SdkValue::Int(total as i64)),
                        ]),
                    )]));
                },
            )
        }
        // a Sandbox image copied to this machine (`sandbox.create --image`,
        // `machine.load`), one step at a time
        "image.artifact" => layerfs_sandbox::image_artifact::in_worker(root, &args),
        // a Sandbox or Workspace operation on this machine's own native;
        // an event that cannot be written does not end it (#177)
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
