//! A panic and a stalled caller on a worker that holds a real Store
//! (layerfs-project#177): the Store and the Sandbox tables answer the next
//! call, and a stalled `project.export` gives up the Store's lease. And a
//! `project.init` whose answer is lost names its folder copy in the home's
//! uncertain log, and the `project delete` that settles that Init removes
//! the copy (spec v20). Empty without the `worker-fault` cargo feature:
//!
//! `cargo test -p layerfs-control --features worker-fault --test worker_fault_store`
#![cfg(feature = "worker-fault")]
use layerfs_protocol::provision::install::SdkValue;
use layerfs_api::worker::fault::{Faults, OpFault};
use layerfs_home::home::paths;
use layerfs_home::names::StoreName;
use layerfs_machine::worker;
use layerfs_store::project::frame::ExportFrame;
use layerfs_store::sqlite::transfer::step::{StepFrame, StoreStep};
use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "../support/worker_store.rs"]
mod worker_store;
use worker_store::{placed, sent, until, Placed, SECRET};

fn with(tag: &str, faults: Faults) -> (Placed, Arc<Faults>) {
    let faults = Arc::new(faults);
    let serving = faults.clone();
    let placed = placed(tag, |listener, root| {
        std::thread::spawn(move || {
            layerfs_api::worker::serve::serve_with(listener, SECRET.to_owned(), root, serving)
        });
    });
    (placed, faults)
}

#[test]
fn after_a_panic_in_a_store_call_the_next_calls_work() {
    let (placed, faults) = with("panic", Faults::default());
    let folder = worker_store::folder(&placed.scratch, 4096);
    worker_store::init(&placed.home, "web", &folder).0.unwrap();
    let before = worker_store::stats(&placed.home).unwrap();
    // a Workspace status of a Sandbox that is not there: the worker settles
    // its records and its Sandbox table refuses it
    let sandbox = worker_store::sandbox_call(&placed.endpoint).unwrap_err();
    assert_eq!(sandbox.code, "NoSuchSandbox", "{sandbox:?}");
    assert!(paths::project_record(&placed.root, "shop", "api").is_file());

    faults.on("store.call", OpFault::Panic);
    let lost = worker_store::stats(&placed.home).unwrap_err();
    assert_eq!(lost.code, "Unknown", "{lost:?}");
    assert!(lost.message.contains("may have run"), "{lost:?}");
    let lines = faults.lines();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].starts_with("store.call panicked: "), "{lines:?}");

    // the next Store call, and a Sandbox operation, answer as before
    let after = worker_store::stats(&placed.home).unwrap();
    assert_eq!(after.get_text("store"), before.get_text("store"));
    let again = worker_store::sandbox_call(&placed.endpoint).unwrap_err();
    assert_eq!((again.code, again.message), (sandbox.code, sandbox.message));
    let folder = worker_store::folder(&placed.scratch.join("more"), 4096);
    worker_store::init(&placed.home, "more", &folder).0.unwrap();
    placed.remove();
}

/// How many event frames `stream` carries, and the frame after them (the
/// answer) when the stream is not cut first.
fn frames(mut stream: std::net::TcpStream) -> (usize, Option<SdkValue>) {
    let mut events = 0;
    loop {
        match worker::read_frame(&mut stream) {
            Ok(Some(frame)) if frame.get("event").is_some() => events += 1,
            Ok(last) => return (events, last),
            Err(_) => return (events, None),
        }
    }
}

#[test]
fn a_stalled_export_ends_and_gives_up_the_store() {
    let wait = Duration::from_millis(1500);
    let (placed, faults) = with("export", Faults::default().write_wait(wait));
    // more than a socket holds: the export's parts cannot all be written
    let folder = worker_store::folder(&placed.scratch, 24 << 20);
    worker_store::init(&placed.home, "web", &folder).0.unwrap();
    let export = || {
        ExportFrame {
            store: StoreName::parse("shop").unwrap(),
            project: "web".to_owned(),
            branch: None,
            at: None,
            store_record: Some(worker_store::store_record(&placed.home)),
        }
        .encode()
    };
    let lease = paths::lease_dir(&placed.root, "shop").join(std::process::id().to_string());
    until(Duration::from_secs(5), "the Init's connections", || {
        faults.serving_now() == 0
    });

    let started = Instant::now();
    let mut stalled = sent(&placed.endpoint, "project.export", export());
    // the export is under way: its first frame is here, and it holds the lease
    let mut first = [0u8; 4];
    stalled.read_exact(&mut first).unwrap();
    assert!(lease.is_file(), "the export holds the Store's lease");
    until(wait * 6, "the stalled export's thread", || {
        faults.serving_now() == 0
    });
    assert!(started.elapsed() >= wait);
    // the lease went with it, and a push's first step on this machine runs
    assert!(!lease.exists(), "the lease outlived the export");
    assert!(!layerfs_store::lease::held_here(&placed.root, "shop"));
    let snapshot = StepFrame {
        store: "shop".to_owned(),
        store_record: Some(worker_store::store_record(&placed.home)),
        step: StoreStep::Snapshot,
    };
    let taken = worker::call(
        &placed.endpoint,
        "store.file",
        snapshot.encode(),
        &mut |_| {},
    );
    let taken = taken.unwrap_or_else(|refusal| panic!("store.file snapshot: {refusal:?}"));
    assert!(
        taken.get_int("bytes").is_some_and(|bytes| bytes > 0),
        "{taken:?}"
    );
    assert!(taken.get_text("sha256").is_some(), "{taken:?}");
    drop(stalled);

    // a caller that reads gets the whole export under the same wait
    let (count, last) = frames(sent(&placed.endpoint, "project.export", export()));
    let value = last.as_ref().and_then(|frame| frame.get("value"));
    let value = value.unwrap_or_else(|| panic!("no answer after {count} frames: {last:?}"));
    assert_eq!(value.get_int("bytes"), Some((24 << 20) + 14), "{value:?}");
    assert!(count >= 3, "a prepare event and the parts: {count}");
    placed.remove();
}

#[test]
fn a_lost_init_names_its_copy_and_its_delete_removes_it() {
    let (placed, faults) = with("lostinit", Faults::default());
    let folder = worker_store::folder(&placed.scratch, 4096);
    // the copy is staged, then the worker's Init dies before it answers
    faults.on("project.init", OpFault::Panic);
    let before = layerfs_home::home::now_ms();
    let lost = worker_store::init(&placed.home, "web", &folder)
        .0
        .unwrap_err();
    assert_eq!(lost.code, "Unknown", "{lost:?}");

    // one line of the uncertain log names the machine, the copy and the
    // deadline: when the request went out plus the Init's timeout
    let log = paths::log_dir(&placed.home).join("uncertain.jsonl");
    let text = std::fs::read_to_string(&log).unwrap();
    let lines: Vec<SdkValue> = (text.lines())
        .map(|line| layerfs_home::json::parse(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 1, "{text}");
    let line = &lines[0];
    assert_eq!(line.get_text("call"), Some("project.init"));
    assert_eq!(line.get_text("store"), Some("shop"));
    assert_eq!(line.get_text("project"), Some("web"));
    assert_eq!(line.get_text("machine"), Some("box"));
    assert_eq!(line.get_text("phase"), Some("install"));
    let copy = line.get_text("copy").unwrap();
    assert!(lost.message.contains(&format!("init-{copy}")), "{lost:?}");
    let deadline = line.get_int("deadline").unwrap();
    assert!(deadline > before, "{line:?}");
    let staged = placed.root.join("incoming").join(format!("init-{copy}"));
    assert!(staged.join("README.md").is_file(), "the copy is there");

    // the project is `broken` `unresolved` until `project delete` settles
    // its Init, and settling it removes the copy (spec v20), logged in the
    // home's `log/sweep.jsonl`; the uncertain line stays as a record
    let project = layerfs_home::names::ProjectRef::parse("shop/web").unwrap();
    let mut sink = |_| {};
    let deleted = layerfs_store::data::project::with_users::delete(
        &placed.home,
        &mut layerfs_home::events::Events::new(&mut sink),
        &project,
    )
    .unwrap_or_else(|refusal| panic!("project delete: {refusal:?}"));
    assert_eq!(deleted.get_text("outcome"), Some("deleted"), "{deleted:?}");
    assert!(!staged.exists(), "the settled Init's copy is removed");
    let swept = std::fs::read_to_string(paths::log_dir(&placed.home).join("sweep.jsonl")).unwrap();
    assert!(
        swept.contains("\"init_copy\"") && swept.contains(&format!("init-{copy}")),
        "{swept}"
    );
    assert_eq!(std::fs::read_to_string(&log).unwrap(), text);
    placed.remove();
}
