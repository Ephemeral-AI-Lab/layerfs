//! `store.compact` of a sqlite Store on another machine (spec S6, #42 D5):
//! the caller keeps the Store's lease and sends `{ store, store_record }` to
//! that machine's worker, which compacts the file under its own root and
//! sends each phase back. Nothing is opened, rebuilt or left under the
//! caller's home.
use layerfs_home::disk;
use layerfs_home::disk::paths;
use layerfs_home::names::{ProjectRef, StoreName};
use layerfs_home::reply::events::Events;
use layerfs_machine::route::{self, Endpoint};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_store::admin::open::args::OpenArgs;
use layerfs_store::admin::open::service::with_service;
use layerfs_store::admin::record::store::StoreRecord;
use layerfs_store::backend::sqlite::create::create;
use layerfs_store::data::project::init::sqlite::{add_project, ProjectAdd};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-compact-worker-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&directory);
    disk::layout::create(&directory).unwrap();
    directory
}

fn plaindir(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-compact-worker-folder-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
}

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

/// Moves Store `store`'s files from `from`'s Store folder to `to`'s.
fn move_store_file(from: &Path, to: &Path, store: &str) {
    let source = paths::store_dir(from, store);
    let target = paths::store_dir(to, store);
    disk::private_dir(&target).unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("store.sqlite") {
            std::fs::rename(entry.path(), target.join(&name)).unwrap();
        }
    }
}

/// The names in Store `store`'s folder under `root` that start with
/// `store.sqlite`: the file and anything a compaction left beside it.
fn beside(root: &Path, store: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(paths::store_dir(root, store))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("store.sqlite"))
        .collect();
    names.sort();
    names
}

/// A worker serving `root` on loopback, and its endpoint.
fn serve(root: &Path) -> Endpoint {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint {
        address: listener.local_addr().unwrap(),
        secret: "k".to_owned(),
    };
    let served = root.to_path_buf();
    std::thread::spawn(move || layerfs_api::worker::serve::serve(listener, "k".to_owned(), served));
    for _ in 0..200 {
        if route::ping(&endpoint).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    endpoint
}

fn compact(home: &Path) -> (Result<SdkValue, SdkRefusal>, Vec<SdkValue>) {
    let mut seen = Vec::new();
    let mut sink = |event| seen.push(event);
    let outcome = layerfs_store::admin::maintenance::compact::compact(
        home,
        &mut Events::new(&mut sink),
        "shop",
    );
    (outcome, seen)
}

/// The phases of `events`' progress frames, in order.
fn phases(events: &[SdkValue]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| event.get("progress"))
        .filter_map(|progress| progress.get_text("phase"))
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_store_on_another_machine_is_compacted_by_its_worker() {
    // Store `shop` with a project whose content is then deleted: most of the
    // file is dead, so the compaction rebuilds.
    let home = tempdir("home");
    match create(&home, "shop", None) {
        Ok(_) => {}
        Err(refusal)
            if refusal.code == "Unsupported"
                && refusal.message.contains("does not carry SQLite locks") =>
        {
            eprintln!("NOT RUN: this filesystem cannot hold a directly opened Store: {refusal:?}");
            let _ = std::fs::remove_dir_all(&home);
            return;
        }
        Err(refusal) => panic!("store.create: {refusal:?}"),
    }
    let source = plaindir("folder");
    // incompressible bytes, so the deleted project's packs dominate the file
    let mut state = 0x9e37_79b9_7f4a_7c15_u64;
    let bytes: Vec<u8> = (0..2 * 1024 * 1024)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect();
    std::fs::write(source.join("big.bin"), &bytes).unwrap();
    for project in ["gone", "kept"] {
        add_project(&ProjectAdd {
            home: home.clone(),
            store: StoreName::parse("shop").unwrap(),
            name: project.to_owned(),
            source: source.clone(),
            timeout_ms: Some(60_000),
            store_record: None,
        })
        .unwrap();
        // the kept project shares nothing with the deleted one
        std::fs::write(source.join("big.bin"), b"small\n").unwrap();
    }
    let mut sink = |_| {};
    layerfs_store::data::project::with_users::delete(
        &home,
        &mut Events::new(&mut sink),
        &ProjectRef::parse("shop/gone").unwrap(),
        &layerfs_sandbox::users::STORE,
    )
    .unwrap();

    // the Store belongs to machine `box`; its file is under the worker's root
    let root = tempdir("root");
    move_store_file(&home, &root, "shop");
    let before = std::fs::metadata(paths::store_file(&root, "shop"))
        .unwrap()
        .len();
    disk::write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([
            ("name", text("box")),
            ("platform", text("lima")),
            ("vm_name", text("layerfs-00000000-box")),
        ]),
    )
    .unwrap();
    let endpoint = serve(&root);
    disk::write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([
            ("address", text(&endpoint.address.to_string())),
            ("secret", text("k")),
        ]),
    )
    .unwrap();
    let record_path = paths::store_record(&home, "shop");
    let record = disk::read_record(&record_path).unwrap().unwrap();
    disk::write_record(
        &record_path,
        &disk::with_field(record, "machine", text("box")),
    )
    .unwrap();

    let (outcome, events) = compact(&home);
    let report = outcome.expect("the worker compacts the Store");
    assert_eq!(report.get_text("outcome"), Some("compacted"), "{report:?}");
    assert_eq!(
        phases(&events),
        ["mark", "build", "verify", "install"],
        "the worker's phases come back in order"
    );

    // the file under the worker's root shrank and nothing is left beside it;
    // nothing was opened or rebuilt under the caller's home
    let after = std::fs::metadata(paths::store_file(&root, "shop"))
        .unwrap()
        .len();
    assert!(after < before, "{after} >= {before}");
    assert_eq!(beside(&root, "shop"), ["store.sqlite"]);
    assert!(
        beside(&home, "shop").is_empty(),
        "{:?}",
        beside(&home, "shop")
    );
    assert!(
        std::fs::read_dir(paths::lease_dir(&home, "shop"))
            .map(|entries| entries.count() == 0)
            .unwrap_or(true),
        "the caller's lease ended with the call"
    );

    // the kept project still reads on the rebuilt file
    let open = OpenArgs {
        home: Some(root.clone()),
        store: Some("shop".to_owned()),
        project: Some("kept".to_owned()),
        store_path: Some(paths::store_file(&root, "shop").display().to_string()),
        store_record: Some(
            StoreRecord::read(&home, "shop")
                .unwrap()
                .unwrap()
                .to_record()
                .clone(),
        ),
        postgresql: None,
    };
    let names = with_service(&open, |service| {
        let listed = service
            .store
            .published(layerfs_project::store::projects)
            .unwrap();
        Ok(listed
            .into_iter()
            .map(|stack| stack.name.as_str().to_owned())
            .collect::<Vec<_>>())
    })
    .unwrap();
    assert_eq!(names, ["kept"]);

    // a second compaction through the worker finds nothing to rebuild
    let (outcome, events) = compact(&home);
    let report = outcome.expect("the second compaction");
    assert_eq!(report.get_text("outcome"), Some("unchanged"), "{report:?}");
    assert_eq!(phases(&events), ["mark"]);
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&source);
}

#[test]
fn the_worker_operation_needs_a_sqlite_store_record() {
    let root = tempdir("frames");
    let endpoint = serve(&root);
    let call = |args: SdkValue| route::call(&endpoint, "store.compact", args, &mut |_| {});
    let refusal = call(SdkValue::map([("name", text("shop"))])).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{}", refusal.message);
    assert_eq!(refusal.message, "store.compact names its Store");
    let refusal = call(SdkValue::map([("store", text("shop"))])).unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{}", refusal.message);
    assert_eq!(refusal.message, "store.compact sends the Store's record");
    let postgresql = SdkValue::map([
        ("name", text("shop")),
        ("kind", text("postgresql")),
        ("url", text("postgresql://127.0.0.1:1/x")),
    ]);
    let refusal = call(SdkValue::map([
        ("store", text("shop")),
        ("store_record", postgresql),
    ]))
    .unwrap_err();
    assert_eq!(refusal.code, "InvalidInput", "{}", refusal.message);
    assert!(
        refusal.message.contains("not a sqlite Store"),
        "{}",
        refusal.message
    );
    assert!(
        !paths::store_dir(&root, "shop").exists(),
        "nothing was made"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Plan Part 3 fix 4: a worker asked to compact a Store whose file it does
/// not hold answers `NotFound`, and makes no lease folder for it first.
#[test]
fn a_worker_without_the_store_file_answers_not_found_and_makes_nothing() {
    let root = tempdir("missing");
    let endpoint = serve(&root);
    let sqlite = SdkValue::map([
        ("name", text("shop")),
        ("kind", text("sqlite")),
        ("machine", text(layerfs_home::names::HOST_MACHINE)),
    ]);
    let refusal = route::call(
        &endpoint,
        "store.compact",
        SdkValue::map([("store", text("shop")), ("store_record", sqlite)]),
        &mut |_| {},
    )
    .unwrap_err();
    assert_eq!(refusal.code, "NotFound", "{}", refusal.message);
    assert!(
        !paths::store_dir(&root, "shop").exists(),
        "no Store folder (and no open/ lease folder) was made"
    );
    let _ = std::fs::remove_dir_all(&root);
}
