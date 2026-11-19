//! project.list and project.get on a worker-backed Store (T4.5c): the
//! listing is the Store's own filtered one, read in the machine's worker
//! (the same `layerfs_project::store::projects` a local Store uses), so a
//! project retired there is not listed and is `NoSuchProject` from
//! project.get; a live project still lists and gets. A Store whose worker
//! refuses the listing, or does not answer, blocks its projects (call
//! surface 2.0): they are listed `blocked`, naming the Store, and
//! project.get answers the same, never a refusal.
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::sdk_values::hex;
use layerfs_history::LayerStackId;
use layerfs_machine::worker::{self, read_frame, read_frame_within, write_frame, Hello, MAX_HELLO};
use layerfs_store::open_args::OpenArgs;
use layerfs_store::project::record::ProjectRecord;
use layerfs_store::record::StoreRecord;
use layerfs_store::service::with_service;
use layerfs_store_service::history::deletion::delete_stack;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lfs-list-worker-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn names(list: &SdkValue) -> Vec<String> {
    let SdkValue::List(items) = list.get("items").unwrap() else {
        panic!("{list:?}")
    };
    items
        .iter()
        .filter_map(|item| item.get_text("project"))
        .filter_map(|project| project.strip_prefix("shop/"))
        .map(str::to_owned)
        .collect()
}

/// Every row of `list` and the answer `get` are `blocked` by Store `shop`
/// for `reason`.
fn blocked_by_shop(list: &SdkValue, get: &SdkValue, reason: &str) {
    let by = SdkValue::map([("store", t("shop")), ("reason", t(reason))]);
    let SdkValue::List(items) = list.get("items").unwrap() else {
        panic!("{list:?}")
    };
    for item in items.iter().chain([get]) {
        assert_eq!(item.get_text("state"), Some("blocked"), "{item:?}");
        assert_eq!(item.get("blocked_by"), Some(&by), "{item:?}");
    }
}

/// Marks Store project `stack` retired (the `project.delete` write path,
/// T4.5b): the Store hides it, its home entry stays.
fn retire(home: &Path, store: &str, stack: &[u8]) {
    let record = StoreRecord::read(home, store).unwrap().unwrap();
    let open = OpenArgs {
        home: Some(home.to_path_buf()),
        store: Some(store.to_owned()),
        project: None,
        store_path: Some(paths::store_file(home, store).display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql: None,
    };
    let stack = LayerStackId::from_slice(stack).unwrap();
    with_service(&open, |service| {
        delete_stack(&service.store, stack, 1234).unwrap();
        Ok(SdkValue::Null)
    })
    .unwrap();
}

/// Moves Store `store`'s file from `from`'s Store folder to `to`'s.
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

/// The Store record of `store` places it on machine `machine`.
fn place_store(home: &Path, store: &str, machine: &str) {
    let record_path = paths::store_record(home, store);
    let record = home::read_record(&record_path).unwrap().unwrap();
    let record = home::with_field(record, "machine", t(machine));
    write_record(&record_path, &record).unwrap();
}

/// A home whose Store `shop` holds `live` and `retired`, the latter retired
/// in the Store, and whose machine `box` record names `listener` as its
/// worker; `shop`'s calls run on `box`. With `root`, the Store's file is
/// moved under the worker's root.
fn shop_on_box(tag: &str, listener: &TcpListener, root: Option<&Path>) -> PathBuf {
    let home = tempdir(tag);
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    let source = tempdir(&format!("{tag}-folder"));
    std::fs::write(source.join("README.md"), "live\n").unwrap();
    for name in ["live", "retired"] {
        ok(
            &home,
            "project.init",
            &[
                ("project", t(&format!("shop/{name}"))),
                ("folder", t(&source.display().to_string())),
            ],
        );
    }
    let retired = ProjectRecord::read(&home, "shop", "retired")
        .unwrap()
        .expect("the retired entry");
    retire(
        &home,
        "shop",
        &retired.stack().expect("the entry's manifest stack"),
    );
    write_record(
        &paths::machine_record(&home, "box"),
        // running, as its record says: a machine the record says is stopped
        // blocks its Stores' projects with no worker asked
        &SdkValue::map([
            ("name", t("box")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-test-vm")),
            ("state", t("running")),
        ]),
    )
    .unwrap();
    let address = listener.local_addr().unwrap().to_string();
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&address)), ("secret", t("k"))]),
    )
    .unwrap();
    if let Some(root) = root {
        move_store_file(&home, root, "shop");
    }
    place_store(&home, "shop", "box");
    home
}

type Seen = Arc<Mutex<Vec<SdkValue>>>;

/// A fake worker that completes the greeting and reads the request, then
/// drops the connection without an answer: the request may have run and the
/// caller cannot know the outcome.
fn fake_lost_worker(listener: TcpListener) -> Seen {
    let seen: Seen = Arc::default();
    let requests = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                return;
            };
            let Some(_hello) = read_frame_within(&mut stream, MAX_HELLO).ok().flatten() else {
                continue;
            };
            let ready = Hello {
                protocol: Some(worker::PROTOCOL),
                bundle: None,
            };
            if write_frame(&mut stream, &ready.to_frame()).is_err() {
                continue;
            }
            if let Some(request) = read_frame(&mut stream).ok().flatten() {
                requests.lock().unwrap().push(request);
            }
            // No answer: the caller's request may have run.
        }
    });
    seen
}

/// A fake worker: answers its secret, reads each request and answers
/// `reply`'s frame; the requests it read are shared back.
fn fake_worker(
    listener: TcpListener,
    reply: impl Fn(&SdkValue) -> SdkValue + Send + 'static,
) -> Seen {
    let seen: Seen = Arc::default();
    let requests = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                return;
            };
            let Some(_hello) = read_frame_within(&mut stream, MAX_HELLO).ok().flatten() else {
                continue;
            };
            let ready = Hello {
                protocol: Some(worker::PROTOCOL),
                bundle: None,
            };
            if write_frame(&mut stream, &ready.to_frame()).is_err() {
                continue;
            }
            let Some(request) = read_frame(&mut stream).ok().flatten() else {
                continue;
            };
            let answer = reply(&request);
            requests.lock().unwrap().push(request);
            let _ = write_frame(&mut stream, &answer);
        }
    });
    seen
}

#[test]
fn a_worker_s_filtered_listing_hides_a_retired_project() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let home = shop_on_box("fake-list", &listener, None);
    let live = ProjectRecord::read(&home, "shop", "live")
        .unwrap()
        .expect("the live entry");
    let live_stack = hex(&live.stack().expect("the live entry's stack"));
    let seen = fake_worker(listener, move |_| {
        // The worker's answer envelope is `{ value }`; its value is the
        // `project.list` reply.
        SdkValue::map([(
            "value",
            SdkValue::map([
                ("kind", t("Listed")),
                (
                    "projects",
                    SdkValue::List(vec![SdkValue::map([
                        ("name", t("live")),
                        ("stack_id", t(&live_stack)),
                    ])]),
                ),
            ]),
        )])
    });

    // The worker's listing names only `live`: the retired entry is hidden.
    assert_eq!(names(&ok(&home, "project.list", &[])), vec!["live"]);
    assert_eq!(
        names(&ok(&home, "project.list", &[("store", t("shop"))])),
        vec!["live"]
    );
    // project.get refuses the retired entry and gets the live one.
    let (code, message) =
        call(&home, "project.get", &[("project", t("shop/retired"))]).unwrap_err();
    assert_eq!(code, "NotFound", "{message}");
    let live = ok(&home, "project.get", &[("project", t("shop/live"))]);
    // the short form: its Store and its state
    assert_eq!(live.get_text("store"), Some("shop"), "{live:?}");
    assert_eq!(live.get_text("state"), Some("ready"), "{live:?}");
    // Both home entries are still there; the Store, not the home, hides one.
    for name in ["live", "retired"] {
        assert!(
            paths::project_record(&home, "shop", name).exists(),
            "{name}"
        );
    }

    // Every read went to the worker, as the Store listing's own frame.
    let requests = seen.lock().unwrap();
    assert_eq!(requests.len(), 4, "{requests:?}");
    for request in requests.iter() {
        assert_eq!(request.get_text("op"), Some("project.list"));
        let args = request.get("args").expect("args");
        assert_eq!(args.get_text("store"), Some("shop"));
        assert!(args.get("store_record").is_some());
    }
    drop(requests);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_worker_refusal_blocks_the_stores_projects() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let home = shop_on_box("fake-refusal", &listener, None);
    let _seen = fake_worker(listener, |_| {
        SdkValue::map([(
            "refused",
            SdkValue::map([
                ("code", t("Busy")),
                ("message", t("the Store is held")),
                ("detail", SdkValue::Null),
            ]),
        )])
    });

    // project.get answers the project blocked by its Store, never the
    // refusal; the listings keep every recorded project of the Store, the
    // Store's list being unread
    let get = ok(&home, "project.get", &[("project", t("shop/live"))]);
    for list in [
        ok(&home, "project.list", &[("store", t("shop"))]),
        ok(&home, "project.list", &[]),
    ] {
        assert_eq!(names(&list), vec!["live", "retired"], "{list:?}");
        blocked_by_shop(&list, &get, "busy");
    }
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_worker_store_lists_only_its_live_projects() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let root = tempdir("worker-root");
    let home = shop_on_box("real-list", &listener, Some(&root));
    let guest = root.clone();
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, "k".to_owned(), guest)
    });

    // The Store under the worker's root hides its retired project; the
    // caller's home entry for it stays but is not listed.
    assert_eq!(names(&ok(&home, "project.list", &[])), vec!["live"]);
    let (code, message) =
        call(&home, "project.get", &[("project", t("shop/retired"))]).unwrap_err();
    assert_eq!(code, "NotFound", "{message}");
    let live = ok(&home, "project.get", &[("project", t("shop/live"))]);
    // the short form: its Store and its state
    assert_eq!(live.get_text("store"), Some("shop"), "{live:?}");
    assert_eq!(live.get_text("state"), Some("ready"), "{live:?}");
    assert!(paths::project_record(&home, "shop", "retired").exists());
    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(root);
}

/// A stale worker's lost listing answer blocks the Store's projects, the
/// Store `not_answering` (it was `Unknown` naming the Store before call
/// surface 2.0, F43R3-1).
#[test]
fn a_lost_worker_answer_blocks_the_stores_projects() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let home = shop_on_box("fake-lost", &listener, None);
    let seen = fake_lost_worker(listener);

    let get = ok(&home, "project.get", &[("project", t("shop/live"))]);
    let list = ok(&home, "project.list", &[("store", t("shop"))]);
    blocked_by_shop(&list, &get, "not_answering");
    let list = ok(&home, "project.list", &[]);
    blocked_by_shop(&list, &get, "not_answering");

    // Every read went to the worker as the Store listing's own frame.
    let requests = seen.lock().unwrap();
    assert_eq!(requests.len(), 3, "{requests:?}");
    for request in requests.iter() {
        assert_eq!(request.get_text("op"), Some("project.list"));
    }
    drop(requests);
    let _ = std::fs::remove_dir_all(home);
}

/// A worker that refuses with the bare empty-map `Unknown` detail (a Store
/// listing that could not be read) blocks the Store's projects exactly as
/// a lost answer does (F43R4-1 named the Store in an `Unknown` refusal).
#[test]
fn a_worker_unknown_with_the_empty_map_detail_blocks_the_stores_projects() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let home = shop_on_box("fake-empty-unknown", &listener, None);
    let _seen = fake_worker(listener, |_| {
        SdkValue::map([(
            "refused",
            SdkValue::map([
                ("code", t("Unknown")),
                ("message", t("the Store's listing did not answer")),
                ("detail", SdkValue::map::<0>([])),
            ]),
        )])
    });

    let get = ok(&home, "project.get", &[("project", t("shop/live"))]);
    let list = ok(&home, "project.list", &[("store", t("shop"))]);
    blocked_by_shop(&list, &get, "not_answering");
    let _ = std::fs::remove_dir_all(home);
}
