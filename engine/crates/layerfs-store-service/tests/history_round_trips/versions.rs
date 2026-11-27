//! #818 lane RH (H5): a history walk that looks its path up in version
//! windows answers exactly as the walk that looks each version up alone,
//! and refuses where it did. A window reads ahead of the walk through
//! per-object reads, so an object damaged in a version past the point where
//! the walk stops never surfaces, and one in a version the walk reaches
//! refuses there, as before.
//!
//! A window also reads ahead what the walk reads at its versions (#818 H6):
//! lengths, blame's texts, the revisions' metadata; `fs.diff_files` reads
//! both sides' files together. A damaged text past the point where the walk
//! stops never surfaces either, and one the call reaches refuses there.
//!
//! The product's windows ([`Batching::Provider`], on PostgreSQL with the
//! storage reader's per-object demands, #818 lane S) are held to the walk
//! that looks each version up alone ([`Batching::Alone`]). PostgreSQL only
//! (the row windows that name a window's versions are PostgreSQL's);
//! NOT_RUN without `LAYERFS_TEST_POSTGRES_URL`.
#[path = "../support/verify/round_trips.rs"]
mod rt;

use layerfs_history::HistoryName;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store_service::filesystem::{
    cache::Cached,
    compare::{diff::Selector, file_rows},
    paged::{query, revision},
    root,
    text::blame,
    tree::visibility::Visibility,
    versions::{windows_resolved, Batching},
};
use layerfs_store_service::history::rows::Rows;
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
    blame_from(service, name, file, 1, batching)
}

/// [`blame`] from line `from`.
fn blame_from(
    service: &StoreService,
    name: &str,
    file: &str,
    from: u64,
    batching: Batching,
) -> Result<SdkValue, SdkRefusal> {
    let stack = stack(service);
    let (reader, snap) = root::open(&service.store, &stack, name)?;
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
            from: Ok(from),
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
    let (reader, snap) = root::open(&service.store, &stack, name)?;
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

/// `fs.diff_files` of `file` between `left` and `right`, with `batching`.
fn diff(
    service: &StoreService,
    file: &str,
    left: &str,
    right: &str,
    batching: Batching,
) -> Result<SdkValue, SdkRefusal> {
    let side = |at| {
        Ok(Selector {
            path: file.to_owned(),
            at,
        })
    };
    file_rows::diff(
        &service.store,
        &stack(service),
        side(left),
        side(right),
        file_rows::Options {
            include_attribution: false,
            allow_missing: false,
            page: query::result_page(&SdkValue::Map(Vec::new()))?,
            extended: true,
            batching,
        },
    )
}

/// The answer or refusal, with both modes required to agree; returns it.
fn alike(
    label: &str,
    call: impl Fn(Batching) -> Result<SdkValue, SdkRefusal>,
) -> Result<SdkValue, SdkRefusal> {
    alike_in(label, true, call)
}

/// [`alike`], requiring a window to be read when `windowed_call`.
fn alike_in(
    label: &str,
    windowed_call: bool,
    call: impl Fn(Batching) -> Result<SdkValue, SdkRefusal>,
) -> Result<SdkValue, SdkRefusal> {
    let alone = call(Batching::Alone);
    let before = windows_resolved();
    let windowed = call(Batching::Provider);
    if windowed_call {
        assert!(windows_resolved() > before, "{label}: a window was read");
    }
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
    let node = layerfs_store_service::lookup::node(&id).unwrap();
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

/// How a test damages the Store: the location row of one object, the
/// bytes of the pack that holds it after its directory (a refused pack,
/// #818 M3 review), or that whole pack row.
#[derive(Clone, Copy)]
enum Damage {
    Location,
    PackBody,
    Pack,
}

/// `/hist/kept.txt` written once, then 20 Commits that change only
/// `/hist/a.txt` (one line, new each time); then the tree root of the 10th
/// Commit from the head is damaged as `damage` says. Blame of `a.txt` and a
/// one-item revision page stop at the head's parent, before it, and answer
/// as before; blame of `kept.txt` walks every version, reaches it and
/// refuses as before; each with the product's windows and walked alone.
fn damaged_version(label: &str, damage: Damage) {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql(label) else {
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
    write("/hist/kept.txt", "kept\n".to_owned());
    let mut roots = Vec::new();
    for n in 0..20 {
        roots.push(root_of(
            &opened.service,
            &write("/hist/a.txt", format!("x{n}\n")),
        ));
    }
    opened.stop();
    let damaged = roots[roots.len() - 10].clone();
    let mut admin = postgres::Client::connect(&store.url, postgres::NoTls).unwrap();
    admin
        .batch_execute(&format!("SET search_path TO {}", store.schema))
        .unwrap();
    match damage {
        Damage::Location => {
            let removed = admin
                .execute(
                    "DELETE FROM object_location WHERE object_id = $1",
                    &[&damaged],
                )
                .unwrap();
            assert_eq!(removed, 1, "the damaged root's location");
        }
        Damage::PackBody | Damage::Pack => {
            // A published pack is immutable and its locations reference it:
            // the test lifts both guards for its own session.
            let pack: i64 = admin
                .query_one(
                    "SELECT pack_id FROM object_location WHERE object_id = $1",
                    &[&damaged],
                )
                .unwrap()
                .get(0);
            admin
                .batch_execute("SET session_replication_role = replica")
                .unwrap();
            let changed = match damage {
                // Every byte after the pack's directory becomes 0xff; the
                // row keeps its length and directory.
                Damage::PackBody => admin.execute(
                    "UPDATE pack SET body = directory || decode(repeat('ff', \
                     (length - octet_length(directory))::int), 'hex') \
                     WHERE pack_id = $1",
                    &[&pack],
                ),
                _ => admin.execute("DELETE FROM pack WHERE pack_id = $1", &[&pack]),
            }
            .unwrap();
            assert_eq!(changed, 1, "the damaged root's pack");
        }
    }

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
    let listed = alike("revision reaches it", |b| {
        revisions(service, rt::BRANCH, "/hist/kept.txt", None, b)
    });
    assert!(listed.is_err(), "the list reaches the damage: {listed:?}");
    opened.stop();
}

#[test]
fn a_damaged_version_refuses_only_where_the_walk_reaches_it() {
    damaged_version("versions_damage_rh", Damage::Location);
}

#[test]
fn a_damaged_pack_refuses_only_where_the_walk_reaches_it() {
    damaged_version("versions_pack_body_rh", Damage::PackBody);
}

/// With the pack row gone (#818 M3 review), the per-object demand's locate
/// (`locate_chains_each`) gives each object located in it that object's own
/// `Io(Malformed)`, the refusal its lone locate meets, and the rest of the
/// window is read: the window refuses only where the lone walk does.
#[test]
fn a_missing_pack_refuses_only_where_the_walk_reaches_it() {
    damaged_version("versions_pack_rh", Damage::Pack);
}

/// The object id of `file`'s content in the tree of the Commit a `fs.write`
/// reply names.
fn content_of(service: &StoreService, reply: &SdkValue, file: &str) -> Vec<u8> {
    let root = layerfs_content::ObjectId::from_bytes(&root_of(service, reply)).unwrap();
    let stack = stack(service);
    let (reader, _) = root::open(&service.store, &stack, rt::BRANCH).unwrap();
    let cached = Cached::new(&reader).unwrap();
    let entry = cached.entry(root, file).unwrap().expect("the file");
    entry.value.content_root.as_bytes().to_vec()
}

#[test]
fn a_damaged_text_refuses_only_where_the_call_reaches_it() {
    let _turn = rt::postgresql_turn();
    let Some(mut store) = Store::postgresql("versions_text_rh") else {
        return;
    };
    let source = store.dir.join("source");
    rt::build_tree(&source);
    store.init(&source, &mut |_, _| {});
    let opened = store.open("setup");
    rt::fork_work(&opened);
    // 20 Commits of `/hist/b.txt`: its first line never changes, its second
    // is new each time.
    let (mut commits, mut texts) = (Vec::new(), Vec::new());
    for n in 0..20 {
        let reply = opened
            .call(
                "fs.write",
                vec![
                    ("branch", t(rt::BRANCH)),
                    path("/hist/b.txt"),
                    (
                        "bytes",
                        SdkValue::Bytes(format!("keep\nx{n}\n").into_bytes()),
                    ),
                ],
            )
            .unwrap();
        texts.push(content_of(&opened.service, &reply, "/hist/b.txt"));
        commits.push(rt::commit_id(&reply));
    }
    opened.stop();
    // The text of the 10th Commit from the head: blame of line 2, a
    // one-item revision page and the diff of the head with its parent stop
    // before it; blame of line 1, the whole revision list and a diff with
    // that Commit reach it.
    let at = texts.len() - 10;
    let mut admin = postgres::Client::connect(&store.url, postgres::NoTls).unwrap();
    admin
        .batch_execute(&format!("SET search_path TO {}", store.schema))
        .unwrap();
    let removed = admin
        .execute(
            "DELETE FROM object_location WHERE object_id = $1",
            &[&texts[at]],
        )
        .unwrap();
    assert_eq!(removed, 1, "the damaged text's location");

    let opened = store.open("damaged");
    let service = &opened.service;
    let b = "/hist/b.txt";
    let head = commits[commits.len() - 1].as_str();
    let parent = commits[commits.len() - 2].as_str();
    let damaged = commits[at].as_str();
    let stops = alike("blame line 2", |m| blame_from(service, rt::BRANCH, b, 2, m));
    assert!(stops.is_ok(), "line 2's walk stops before: {stops:?}");
    let page = alike("revision page", |m| {
        revisions(service, rt::BRANCH, b, Some(1), m)
    });
    assert!(page.is_ok(), "the page ends before the damage: {page:?}");
    let near = alike_in("diff near", false, |m| diff(service, b, head, parent, m));
    assert!(near.is_ok(), "the diff reads no damaged side: {near:?}");
    let reached = alike("blame line 1", |m| blame_from(service, rt::BRANCH, b, 1, m));
    assert!(reached.is_err(), "line 1's walk reaches it: {reached:?}");
    let listed = alike("revision list", |m| {
        revisions(service, rt::BRANCH, b, None, m)
    });
    assert!(listed.is_err(), "the list reaches it: {listed:?}");
    for (label, left, right) in [("diff left", damaged, head), ("diff right", head, damaged)] {
        let refused = alike_in(label, false, |m| diff(service, b, left, right, m));
        assert!(refused.is_err(), "{label}: a damaged side: {refused:?}");
    }
    opened.stop();
}
