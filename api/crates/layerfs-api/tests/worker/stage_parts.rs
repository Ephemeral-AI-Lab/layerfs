//! The worker has no connection cap (layerfs-project#177): `project.init`
//! sends its folder copy several parts at once, each on its own connection,
//! and under the one-attempt rule a part refused `Busy` would fail the
//! Init. Checked against the worker itself; the caller's side, with its
//! parts held in flight, is `layerfs-api` `project_init_worker`.
use layerfs_home::home::paths;
use layerfs_machine::worker;
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::project::frame::{StageFrame, StageStep};

#[path = "../support/worker_store.rs"]
mod worker_store;

fn serve(listener: std::net::TcpListener, root: std::path::PathBuf) {
    std::thread::spawn(move || {
        layerfs_api::worker::serve::serve(listener, worker_store::SECRET.to_owned(), root)
    });
}

#[test]
fn init_with_parts_sent_at_once_succeeds_with_no_busy() {
    let placed = worker_store::placed("parts", serve);
    // more 8 MiB parts than the caller sends at once
    let big = (36 << 20) + 123;
    let folder = worker_store::folder(&placed.scratch, big);
    let (added, events) = worker_store::init(&placed.home, "web", &folder);
    let added = added.unwrap_or_else(|refusal| panic!("project.init: {refusal:?}"));
    assert_eq!(added.get_text("project"), Some("shop/web"));
    let sends = (events.iter())
        .filter_map(|event| event.get("progress"))
        .filter(|progress| progress.get_text("phase") == Some("send"))
        .count();
    assert!(sends >= 3, "the copy went in several parts: {sends}");
    assert!(paths::project_dir(&placed.root, "shop", "web").is_dir());
    assert_eq!(
        std::fs::read_dir(placed.root.join("incoming"))
            .unwrap()
            .count(),
        0
    );
    // the Store answers through the same worker afterwards
    worker_store::stats(&placed.home)
        .unwrap_or_else(|refusal| panic!("store.get --with stats: {refusal:?}"));

    // more connections at once than an Init opens: each is served, none
    // refused or made to fail
    let part = |id: &str, step| StageFrame {
        store: "shop".to_owned(),
        name: "many".to_owned(),
        id: id.to_owned(),
        step,
    };
    let callers: Vec<_> = (0..16)
        .map(|at| {
            let endpoint = placed.endpoint.clone();
            let id = format!("{at:016x}");
            std::thread::spawn(move || {
                let write = part(&id, StageStep::Write(Vec::new())).encode();
                let wrote = worker::call(&endpoint, "project.stage", write, &mut |_| {});
                let discard = part(&id, StageStep::Discard).encode();
                let dropped = worker::call(&endpoint, "project.stage", discard, &mut |_| {});
                (wrote, dropped)
            })
        })
        .collect();
    for caller in callers {
        let (wrote, dropped) = caller.join().unwrap();
        assert_eq!(wrote.unwrap().get("records"), Some(&SdkValue::Int(0)));
        assert_eq!(
            dropped.unwrap().get("discarded"),
            Some(&SdkValue::Bool(true))
        );
    }
    placed.remove();
}
