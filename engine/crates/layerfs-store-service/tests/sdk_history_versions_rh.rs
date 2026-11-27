//! #818 lane RH (H5): a history walk that looks its path up in version
//! windows answers exactly as the walk that looks each version up alone,
//! and refuses where it did. A window reads ahead of the walk through
//! per-object reads, so an object damaged in a version past the point where
//! the walk stops never surfaces, and one in a version the walk reaches
//! refuses there, as before.
//!
//! The windows are forced ([`Batching::Forced`]): the Store's reader reads
//! per-object demands one object at a time until the storage reader serves
//! them at once, which is when the product turns them on. PostgreSQL only
//! (the row windows that name a window's versions are PostgreSQL's); NOT_RUN
//! without `LAYERFS_TEST_POSTGRES_URL`.
#[path = "support/round_trips.rs"]
mod rt;

use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_history::HistoryName;
use layerfs_store_service::history::rows::Rows;
use layerfs_store_service::read::{
    blame,
    cached::Cached,
    query, revision, snapshot,
    versions::{windows_resolved, Batching},
    visibility::Visibility,
};
use layerfs_store_service::StoreService;
use rt::{path_arg as path, t, Store};

fn stack(service: &StoreService) -> layerfs_history::LayerStackRecord {
    service
        .store
        .published(|history| history.layer_stack_named(&HistoryName::new(rt::PROJECT).unwrap()))
        .unwrap()
        .expect("the project")
}

/// `fs.blame` of `file` at `name`, as the method runs it, with `batching`.
fn blame(
    service: &StoreService,
    name: &str,
    file: &str,
    batching: Batching,
) -> Result<SdkValue, SdkRefusal> {
    let stack = stack(service);
    let (reader, snap) = snapshot::open(&service.store, &stack, name)?;
    let cached = Cached::new(&reader)?;
    let seen = Visibility::for_snap(&service.store, &snap)?;
    let mut rows = Rows::new(&service.store);
    blame::blame(
        &mut rows,
        &cached,
        &snap,
        file,
        &seen,
        blame::Window {
            from: Ok(1),
            count: Ok(2000),
        },
        &mut blame::Limits {
            versions: query::MAXIMUM_QUERY_WORK,
            bytes: 64 * 1024 * 1024,
        },
        batching,
    )
}

/// `fs.revision` of `file` at `name` on Branch `work`, with `batching`.
fn revisions(
    service: &StoreService,
    name: &str,
    file: &str,
    size: Option<i64>,
    batching: Batching,
) -> Result<SdkValue, SdkRefusal> {
    let stack = stack(service);
    let (reader, snap) = snapshot::open(&service.store, &stack, name)?;
    let args = SdkValue::Map(
        size.map(|size| ("pageSize".to_owned(), SdkValue::Int(size)))
            .into_iter()
            .collect(),
    );
    revision::list(
        &service.store,
        &reader,
        snap.node,
        revision::Request {
            path: file,
            traversal: "provenance",
            page: query::result_page(&args)?,
            branch: rt::BRANCH,
            continuation: None,
            stack: stack.id,
            batching,
        },
    )
}

/// The answer or refusal, with both modes required to agree; returns it.
fn alike(
    label: &str,
    call: impl Fn(Batching) -> Result<SdkValue, SdkRefusal>,
) -> Result<SdkValue, SdkRefusal> {
    let alone = call(Batching::Provider);
    let before = windows_resolved();
    let windowed = call(Batching::Forced);
    assert!(windows_resolved() > before, "{label}: a window was read");
    assert_eq!(windowed, alone, "{label}: windows answer as the lone walk");
    alone
}

#[test]
fn windows_answer_as_the_lone_walk() {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("versions_rh") else {
        return;
    };
    let ids = rt::probe_store(&mut store);
    let opened = store.open("versions");
    let service = &opened.service;
    let a = "/hist/a.txt";
    for (label, name) in [("h100", rt::BRANCH), ("h10", ids[9].as_str())] {
        alike(&format!("blame {label}"), |b| blame(service, name, a, b)).unwrap();
        alike(&format!("revision {label}"), |b| {
            revisions(service, name, a, None, b)
        })
        .unwrap();
    }
    alike("blame other", |b| {
        blame(service, rt::BRANCH, "/hist/other.txt", b)
    })
    .unwrap();
    alike("revision page 7", |b| {
        revisions(service, rt::BRANCH, a, Some(7), b)
    })
    .unwrap();
    opened.stop();
}

/// The root of the Commit a `fs.write` reply names.
fn root_of(service: &StoreService, reply: &SdkValue) -> Vec<u8> {
    let id = rt::commit_id(reply);
    let node = layerfs_store_service::history::lookup::node(&id).unwrap();
    let layerfs_history::NodeId::Commit(commit) = node else {
        panic!("a Commit")
    };
    service
        .store
        .published(|history| history.commit(commit))
        .unwrap()
        .expect("the Commit")
        .root
        .as_bytes()
        .to_vec()
}

#[test]
fn a_damaged_version_refuses_only_where_the_walk_reaches_it() {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("versions_damage_rh") else {
        return;
    };
    let source = store.dir.join("source");
    rt::build_tree(&source);
    store.init(&source, &mut |_, _| {});
    let opened = store.open("setup");
    rt::fork_work(&opened);
    let write = |file: &str, text: String| {
        opened
            .call(
                "fs.write",
                vec![
                    ("branch", t(rt::BRANCH)),
                    path(file),
                    ("bytes", SdkValue::Bytes(text.into_bytes())),
                ],
            )
            .unwrap()
    };
    // `/hist/kept.txt` is written once, then 20 Commits change only
    // `/hist/a.txt` (one line, new each time).
    write("/hist/kept.txt", "kept\n".to_owned());
    let mut roots = Vec::new();
    for n in 0..20 {
        roots.push(root_of(
            &opened.service,
            &write("/hist/a.txt", format!("x{n}\n")),
        ));
    }
    opened.stop();
    // The tree root of the 10th Commit from the head: blame of `a.txt` and
    // a one-item revision page stop at the head's parent, before it; blame
    // of `kept.txt` walks every version and reaches it.
    let damaged = roots[roots.len() - 10].clone();
    let mut admin = postgres::Client::connect(&store.url, postgres::NoTls).unwrap();
    admin
        .batch_execute(&format!("SET search_path TO {}", store.schema))
        .unwrap();
    let removed = admin
        .execute(
            "DELETE FROM object_location WHERE object_id = $1",
            &[&damaged],
        )
        .unwrap();
    assert_eq!(removed, 1, "the damaged root's location");

    let opened = store.open("damaged");
    let service = &opened.service;
    let a = "/hist/a.txt";
    let answer = alike("blame stops before", |b| blame(service, rt::BRANCH, a, b));
    assert!(
        answer.is_ok(),
        "the walk stops before the damage: {answer:?}"
    );
    let page = alike("revision stops before", |b| {
        revisions(service, rt::BRANCH, a, Some(1), b)
    });
    assert!(page.is_ok(), "the page ends before the damage: {page:?}");
    let reached = alike("blame reaches it", |b| {
        blame(service, rt::BRANCH, "/hist/kept.txt", b)
    });
    assert!(reached.is_err(), "the walk reaches the damage: {reached:?}");
    opened.stop();
}
