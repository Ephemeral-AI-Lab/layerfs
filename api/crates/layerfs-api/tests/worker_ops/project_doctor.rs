//! `project doctor --full` of a project whose sqlite Store is on another
//! machine: the Store's list is read in that machine's worker, and the data
//! walk runs there too (`store.diagnose` with `cut: project`), its `data`
//! progress events passed back. The caller's home holds no Store file.
use layerfs_protocol::provision::install::SdkValue;
use layerfs_inspect::{self, Kind, Options, Thing};
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths};
use layerfs_home::names::StoreName;
use layerfs_machine::worker::{self, Endpoint};
use layerfs_store::project::add::{add_project, ProjectAdd};
use layerfs_store::sqlite::create::create;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn tempdir(tag: &str, layout: bool) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-pdoctor-worker-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&directory);
    match layout {
        true => drop(home::layout::create(&directory).unwrap()),
        false => std::fs::create_dir_all(&directory).unwrap(),
    }
    directory
}

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

/// Moves Store `store`'s files from `from`'s Store folder to `to`'s.
fn move_store_file(from: &Path, to: &Path, store: &str) {
    let source = paths::store_dir(from, store);
    let target = paths::store_dir(to, store);
    home::private_dir(&target).unwrap();
    for entry in std::fs::read_dir(&source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("store.sqlite") {
            std::fs::rename(entry.path(), target.join(&name)).unwrap();
        }
    }
}

/// A worker serving `root` on loopback, and its endpoint.
fn serve(root: &Path) -> Endpoint {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Endpoint {
        address: listener.local_addr().unwrap(),
        secret: "k".to_owned(),
    };
    let served = root.to_path_buf();
    std::thread::spawn(move || {
        layerfs_api::worker::serve::serve(listener, "k".to_owned(), served)
    });
    for _ in 0..200 {
        if worker::ping(&endpoint).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    endpoint
}

#[test]
fn the_data_walk_of_a_store_on_another_machine_runs_in_its_worker() {
    let home = tempdir("home", true);
    match create(&home, "shop", None) {
        Ok(_) => {}
        Err(refusal) if refusal.code == "Unsupported" => {
            eprintln!("NOT_RUN: this filesystem cannot hold a directly opened Store: {refusal:?}");
            let _ = std::fs::remove_dir_all(&home);
            return;
        }
        Err(refusal) => panic!("store.create: {refusal:?}"),
    }
    let source = tempdir("folder", false);
    std::fs::write(source.join("a.txt"), "hello\n").unwrap();
    std::fs::create_dir(source.join("src")).unwrap();
    std::fs::write(source.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    add_project(&ProjectAdd {
        home: home.clone(),
        store: StoreName::parse("shop").unwrap(),
        name: "app".to_owned(),
        source: source.clone(),
        timeout_ms: Some(60_000),
        store_record: None,
    })
    .unwrap();
    layerfs_store::open::close_idle(std::time::Duration::ZERO);

    // the Store belongs to machine `box`; its file is under the worker's root
    let root = tempdir("root", true);
    move_store_file(&home, &root, "shop");
    let record_path = paths::store_record(&home, "shop");
    let record = home::read_record(&record_path).unwrap().unwrap();
    home::write_record(
        &record_path,
        &home::with_field(record, "machine", text("box")),
    )
    .unwrap();
    home::write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([
            ("name", text("box")),
            ("platform", text("lima")),
            ("vm_name", text("layerfs-test-box")),
            ("state", text("running")),
        ]),
    )
    .unwrap();
    let endpoint = serve(&root);
    home::write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([
            ("address", text(&endpoint.address.to_string())),
            ("secret", text("k")),
        ]),
    )
    .unwrap();

    let mut seen = Vec::new();
    let mut sink = |event| seen.push(event);
    let options = Options {
        full: true,
        ..Options::default()
    };
    let report = layerfs_inspect::run(
        &home,
        Thing::new(Kind::Project, "shop/app"),
        &options,
        &mut Events::new(&mut sink),
    )
    .unwrap();
    let node = report.tree.to_sdk(false);
    let checks: Vec<(String, String)> = match node.get("checks") {
        Some(SdkValue::List(rows)) => rows
            .iter()
            .map(|row| {
                (
                    row.get_text("check").unwrap().to_owned(),
                    row.get_text("outcome").unwrap().to_owned(),
                )
            })
            .collect(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        checks,
        [
            ("project.record".to_owned(), "pass".to_owned()),
            ("project.listed".to_owned(), "pass".to_owned()),
            ("project.data".to_owned(), "pass".to_owned())
        ],
        "{node:?}"
    );
    assert_eq!(node.get_text("coverage"), Some("full"), "{node:?}");
    let data: Vec<(Option<i64>, Option<i64>)> = seen
        .iter()
        .filter_map(|event| event.get("progress"))
        .filter(|progress| progress.get_text("phase") == Some("data"))
        .map(|progress| (progress.get_int("done"), progress.get_int("total")))
        .collect();
    assert_eq!(data, [(Some(0), Some(1)), (Some(1), Some(1))], "{seen:?}");
    assert!(
        !paths::store_file(&home, "shop").exists(),
        "the caller opened no Store file of its own"
    );
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&source);
}
