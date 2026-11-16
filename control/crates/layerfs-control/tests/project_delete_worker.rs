//! `project.delete` on a worker-backed Store (T4.6): the caller writes its
//! sidecar naming the admission, then admits the project scope through the
//! worker and sends the marker. A live mount in the worker's Store refuses
//! the call with `MountsActive` (holders reported) and a live operation with
//! `Busy`, keeping the pending sidecar, writing no marker and leaking no
//! admission row; a pending retry then redos the marker and keeps the
//! caller and worker entries, and the next call finishes them. The
//! successful path releases the admission after the marker.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{HistoryName, LayerStackId, NamedSnapshot};
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths};
use layerfs_home::names::{ProjectRef, StoreName};
use layerfs_machine::worker::{self, read_frame, read_frame_within, write_frame, Hello, MAX_HELLO};
use layerfs_store::open_args::OpenArgs;
use layerfs_store::project::add::{add_project, ProjectAdd};
use layerfs_store::record::StoreRecord;
use layerfs_store::service::with_service;
use layerfs_store::sqlite::create::create;
use layerfs_store_service::history::lookup;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT: AtomicU64 = AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-delete-worker-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&directory);
    layerfs_home::home::layout::create(&directory).unwrap();
    directory
}

fn text(value: &str) -> SdkValue {
    SdkValue::text(value)
}

/// A folder to initialize a project from: no home layout inside it.
fn plaindir(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-delete-worker-folder-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    directory
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
    write_record(
        &record_path,
        &home::with_field(record, "machine", text(machine)),
    );
}

fn write_record(path: &Path, value: &SdkValue) {
    home::write_record(path, value).unwrap();
}

/// A worker's `OpenArgs` for Store `shop` under `root`, with the caller's
/// Store record (the same context every `project.delete*` frame carries).
fn worker_open(root: &Path, record: &StoreRecord) -> OpenArgs {
    OpenArgs {
        home: Some(root.to_path_buf()),
        store: Some("shop".to_owned()),
        project: Some("app".to_owned()),
        store_path: Some(paths::store_file(root, "shop").display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql: None,
    }
}

fn intents(open: &OpenArgs, stack: LayerStackId) -> Vec<GcIntent> {
    with_service(open, |service| {
        Ok(service
            .store
            .history()
            .gc_intents(GcScopeKind::Project, stack.as_slice())
            .unwrap())
    })
    .unwrap()
}

fn project_names(open: &OpenArgs) -> Vec<String> {
    with_service(open, |service| {
        let listed = service
            .store
            .published(layerfs_project::store::projects)
            .unwrap();
        Ok(listed
            .into_iter()
            .map(|stack| stack.name.as_str().to_owned())
            .collect())
    })
    .unwrap()
}

fn remove(home: &Path) -> Result<SdkValue, SdkRefusal> {
    let mut sink = |_| {};
    layerfs_control::project::delete(
        home,
        &mut Events::new(&mut sink),
        &ProjectRef::parse("shop/app").unwrap(),
    )
}

#[test]
fn the_worker_path_writes_the_sidecar_then_admits_and_releases_after_the_marker() {
    // a home with Store `shop` and project `app`, created locally. A
    // sandbox filesystem cannot hold a directly opened Store; the fake-home
    // cases in the `layerfs-store` tests cover the same home path, and this
    // case runs where the Store's filesystem carries SQLite locks.
    let home = tempdir("home");
    match create(&home, &StoreName::parse("shop").unwrap()) {
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
    std::fs::write(source.join("a.txt"), b"a\n").unwrap();
    add_project(&ProjectAdd {
        home: home.clone(),
        store: StoreName::parse("shop").unwrap(),
        name: "app".to_owned(),
        source,
        timeout_ms: Some(60_000),
        store_record: None,
    })
    .unwrap();
    let record = StoreRecord::read(&home, "shop").unwrap().unwrap();
    let caller_open = OpenArgs {
        home: Some(home.clone()),
        store: Some("shop".to_owned()),
        project: Some("app".to_owned()),
        store_path: Some(paths::store_file(&home, "shop").display().to_string()),
        store_record: Some(record.to_record().clone()),
        postgresql: None,
    };
    let (stack, branch) = with_service(&caller_open, |service| {
        let stack = lookup::project(&service.store, "app").unwrap();
        let project = HistoryName::new("app").unwrap();
        let main = HistoryName::new("main").unwrap();
        let NamedSnapshot::Found(snapshot) = service
            .store
            .published(|history| history.branch_snapshot_named(&project, &main))
            .unwrap()
        else {
            panic!("no main Branch");
        };
        Ok((stack.id, snapshot.branch.id))
    })
    .unwrap();

    // the Store belongs to machine `box`; its file is under the worker's root
    let root = tempdir("root");
    move_store_file(&home, &root, "shop");
    home::write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([("name", text("box")), ("platform", text("lima"))]),
    )
    .unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    home::write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", text(&address)), ("secret", text("k"))]),
    )
    .unwrap();
    place_store(&home, "shop", "box");
    let served = root.clone();
    std::thread::spawn(move || {
        layerfs_control::worker_server::serve(listener, "k".to_owned(), served)
    });

    let worker = worker_open(&root, &record);

    // a live mount in the worker's Store: `MountsActive`, a pending sidecar,
    // no marker
    let mount = GcIntent::shared(
        [9; 16],
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.as_slice(),
    )
    .in_project(stack.as_slice())
    .named(Some("app"), Some("main"))
    .placed(Some([9; 32]), Some("sandbox-9"), Some("machine-9"));
    with_service(&worker, |service| {
        service.store.history().gc_register(&mount).unwrap();
        Ok(())
    })
    .unwrap();
    let refusal = remove(&home).expect_err("a live mount holds the project");
    assert_eq!(refusal.code, "MountsActive", "{refusal:?}");
    assert!(
        intents(&worker, stack).is_empty(),
        "no project-scope row yet"
    );
    let SdkValue::List(holders) = refusal.detail.get("holders").expect("holders") else {
        panic!("{refusal:?}");
    };
    assert_eq!(holders[0].get_text("kind"), Some("mount"));
    assert_eq!(holders[0].get_text("sandbox"), Some("sandbox-9"));
    let pending = layerfs_store::project::delete::Intent::read(&home, "shop", "app")
        .unwrap()
        .expect("the pending sidecar precedes the admission");
    assert_eq!(
        pending.state,
        layerfs_store::project::delete::IntentState::Pending
    );
    let sidecar_op = pending.op.expect("the sidecar names its admission");
    assert!(paths::project_dir(&home, "shop", "app").is_dir());
    assert_eq!(project_names(&worker), vec!["app".to_owned()], "no marker");
    assert!(
        intents(&worker, stack).is_empty(),
        "the refused admission left no project-scope row"
    );

    // the mount ends; a live removal at project scope is `Busy`
    with_service(&worker, |service| {
        service.store.history().gc_release(&[9; 16]).unwrap();
        Ok(())
    })
    .unwrap();
    let holder = GcIntent::operation(
        [7; 16],
        GcIntentKind::Removal,
        GcScopeKind::Project,
        stack.as_slice(),
    )
    .named(Some("app"), None);
    with_service(&worker, |service| {
        service.store.history().gc_register(&holder).unwrap();
        Ok(())
    })
    .unwrap();
    let refusal = remove(&home).expect_err("another removal holds the project");
    assert_eq!(refusal.code, "Busy", "{refusal:?}");
    assert_eq!(
        layerfs_store::project::delete::Intent::read(&home, "shop", "app")
            .unwrap()
            .expect("the pending sidecar stays")
            .op,
        Some(sidecar_op),
        "the retry reuses the same admission identity"
    );
    assert_eq!(
        intents(&worker, stack).len(),
        1,
        "only the test's own holder"
    );
    assert!(paths::project_dir(&home, "shop", "app").is_dir());

    // the worker's own copy of the project entry: the deferred call must
    // keep it, and the finishing call removes it.
    home::write_record(
        &paths::project_record(&root, "shop", "app"),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", text("app")),
            ("stack", text(&hex(&stack.to_bytes()))),
            ("created", SdkValue::Int(1)),
        ]),
    )
    .unwrap();
    std::fs::create_dir_all(paths::project_dir(&root, "shop", "app")).unwrap();

    // the holder ends. The sidecar is still pending, so this call retries
    // without a confirmed marker: it writes the marker, settles the sidecar
    // and keeps the caller and worker entries.
    with_service(&worker, |service| {
        service.store.history().gc_release(&[7; 16]).unwrap();
        Ok(())
    })
    .unwrap();
    let outcome = remove(&home).unwrap();
    assert_eq!(outcome.get_text("kind"), Some("Removed"));
    assert!(
        !outcome.get_bool("home_entry_removed"),
        "a pending retry settles without finishing the entry"
    );
    assert_eq!(outcome.get_int("deleted_ms"), Some(pending.deleted_ms));
    assert!(paths::project_dir(&home, "shop", "app").is_dir());
    assert!(
        paths::project_dir(&root, "shop", "app").is_dir(),
        "the worker keeps its own entry"
    );
    let settled = layerfs_store::project::delete::Intent::read(&home, "shop", "app")
        .unwrap()
        .expect("the sidecar is marked");
    assert_eq!(
        settled.state,
        layerfs_store::project::delete::IntentState::Marked
    );
    assert_eq!(settled.deleted_ms, pending.deleted_ms);
    assert!(
        intents(&worker, stack).is_empty(),
        "the admission was released"
    );
    assert!(project_names(&worker).is_empty(), "the project is retired");

    // The marker is confirmed now: the next call removes both entries and
    // unlinks the sidecar.
    let outcome = remove(&home).unwrap();
    assert_eq!(outcome.get_text("kind"), Some("Removed"));
    assert!(outcome.get_bool("home_entry_removed"));
    assert!(!paths::project_dir(&home, "shop", "app").exists());
    assert!(!paths::project_dir(&root, "shop", "app").exists());
    assert_eq!(
        layerfs_store::project::delete::Intent::read(&home, "shop", "app").unwrap(),
        None
    );
    assert!(
        intents(&worker, stack).is_empty(),
        "the admission was released"
    );
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&root);
}

/// A fake worker on `listener`: answers its secret, answers each request with
/// `reply`, and shares back the requests it read. A `None` answer drops the
/// connection without a reply, as a lost answer does.
fn fake_worker_with(
    listener: TcpListener,
    reply: impl Fn(&SdkValue) -> Option<SdkValue> + Send + 'static,
) -> Arc<Mutex<Vec<SdkValue>>> {
    let seen: Arc<Mutex<Vec<SdkValue>>> = Arc::default();
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
            if let Some(answer) = answer {
                let _ = write_frame(&mut stream, &answer);
            }
        }
    });
    seen
}

/// [`fake_worker_with`] of a worker that always answers.
fn fake_worker(
    listener: TcpListener,
    reply: impl Fn(&SdkValue) -> SdkValue + Send + 'static,
) -> Arc<Mutex<Vec<SdkValue>>> {
    fake_worker_with(listener, move |request| Some(reply(request)))
}

/// The `op` of one request the fake worker read.
fn op_of(request: &SdkValue) -> &str {
    request.get_text("op").unwrap_or("")
}

/// A frame's `args`.
fn args_of(request: &SdkValue) -> &SdkValue {
    request.get("args").expect("args")
}

/// The `op_id` a request's args carry.
fn sent_op(request: &SdkValue) -> String {
    args_of(request)
        .get_text("op_id")
        .expect("the frame names its admission")
        .to_owned()
}

fn admitted_for(stack: LayerStackId) -> SdkValue {
    SdkValue::map([(
        "value",
        SdkValue::map([
            ("kind", text("Admitted")),
            ("stack_id", text(&hex(&stack.to_bytes()))),
        ]),
    )])
}

fn removed_for(stack: LayerStackId, deleted_ms: i64, retired: bool) -> SdkValue {
    SdkValue::map([(
        "value",
        SdkValue::map([
            ("kind", text("Removed")),
            ("project", text("app")),
            ("stack_id", text(&hex(&stack.to_bytes()))),
            ("deleted_ms", SdkValue::Int(deleted_ms)),
            ("retired", SdkValue::Bool(retired)),
        ]),
    )])
}

fn released_for(request: &SdkValue) -> SdkValue {
    SdkValue::map([(
        "value",
        SdkValue::map([
            ("kind", text("Released")),
            ("op_id", text(&sent_op(request))),
        ]),
    )])
}

fn refused(code: &str, detail: SdkValue) -> SdkValue {
    SdkValue::map([(
        "refused",
        SdkValue::map([
            ("code", text(code)),
            ("message", text("the fake worker refused")),
            ("detail", detail),
        ]),
    )])
}

/// A caller's home naming Store `shop` on machine `box`, whose project `app`
/// records `stack`, with the fake worker's address.
fn caller_home(tag: &str, address: &str, stack: LayerStackId) -> PathBuf {
    let home = tempdir(tag);
    home::write_record(
        &paths::store_record(&home, "shop"),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("kind", text("sqlite")),
            ("name", text("shop")),
            ("machine", text("box")),
        ]),
    )
    .unwrap();
    home::write_record(
        &paths::project_record(&home, "shop", "app"),
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", text("app")),
            ("stack", text(&hex(&stack.to_bytes()))),
            ("created", SdkValue::Int(1)),
        ]),
    )
    .unwrap();
    home::write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", text(address)), ("secret", text("k"))]),
    )
    .unwrap();
    home
}

#[test]
fn a_fake_workers_mounts_active_refusal_keeps_a_pending_sidecar_and_writes_no_marker() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let stack = LayerStackId::from_authority([3; 16]);
    let home = caller_home("fake-mount", &address, stack);
    let holders = SdkValue::map([(
        "holders",
        SdkValue::List(vec![SdkValue::map([
            ("kind", text("mount")),
            ("entity", text("main")),
        ])]),
    )]);
    let seen = fake_worker(listener, move |request| match op_of(request) {
        "project.delete.admit" => refused("MountsActive", holders.clone()),
        "project.delete.release" => released_for(request),
        other => panic!("unexpected op {other}"),
    });

    let refusal = remove(&home).expect_err("the fake worker's Store has a live mount");
    assert_eq!(refusal.code, "MountsActive", "{refusal:?}");
    // The caller wrote its sidecar before admitting and sent no marker; the
    // refused admission registered nothing, so the caller settles the
    // identity it wrote and keeps the pending sidecar for a retry.
    let pending = layerfs_store::project::delete::Intent::read(&home, "shop", "app")
        .unwrap()
        .expect("the pending sidecar precedes the admission");
    assert_eq!(
        pending.state,
        layerfs_store::project::delete::IntentState::Pending
    );
    assert!(pending.op.is_some(), "the sidecar names its admission");
    assert!(paths::project_dir(&home, "shop", "app").is_dir());
    let requests = seen.lock().unwrap();
    let ops: Vec<&str> = requests.iter().map(op_of).collect();
    assert_eq!(
        ops,
        ["project.delete.admit", "project.delete.release"],
        "{requests:?}"
    );
    assert_eq!(sent_op(&requests[0]), sent_op(&requests[1]));
    drop(requests);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_fake_workers_marker_refusal_releases_the_admission_it_took() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let stack = LayerStackId::from_authority([4; 16]);
    let home = caller_home("fake-busy", &address, stack);
    let seen = fake_worker(listener, move |request| match op_of(request) {
        "project.delete.admit" => admitted_for(stack),
        "project.delete" => refused("Busy", SdkValue::Null),
        "project.delete.release" => released_for(request),
        other => panic!("unexpected op {other}"),
    });

    let refusal = remove(&home).expect_err("a live operation holds the project");
    assert_eq!(refusal.code, "Busy", "{refusal:?}");
    // the admission was taken, named in the sidecar, used by the marker and
    // released after the marker's refusal
    let requests = seen.lock().unwrap();
    let ops: Vec<&str> = requests.iter().map(op_of).collect();
    assert_eq!(
        ops,
        [
            "project.delete.admit",
            "project.delete",
            "project.delete.release"
        ],
        "{requests:?}"
    );
    let op = sent_op(&requests[0]);
    assert_eq!(sent_op(&requests[1]), op);
    assert_eq!(sent_op(&requests[2]), op);
    drop(requests);
    let body: [u8; 16] = hex_decode(&op).try_into().unwrap();
    assert_eq!(
        layerfs_store::project::delete::Intent::read(&home, "shop", "app")
            .unwrap()
            .expect("the pending sidecar stays for a retry")
            .op,
        Some(body)
    );
    assert!(paths::project_dir(&home, "shop", "app").is_dir());
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_fake_workers_success_removes_the_entry_and_releases_the_admission() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let stack = LayerStackId::from_authority([5; 16]);
    let home = caller_home("fake-ok", &address, stack);
    let seen = fake_worker(listener, move |request| match op_of(request) {
        "project.delete.admit" => admitted_for(stack),
        "project.delete" => removed_for(stack, 42, false),
        "project.delete.release" => released_for(request),
        other => panic!("unexpected op {other}"),
    });

    let outcome = remove(&home).unwrap();
    assert_eq!(outcome.get_text("kind"), Some("Removed"));
    assert_eq!(outcome.get_int("deleted_ms"), Some(42));
    assert!(outcome.get_bool("home_entry_removed"));
    assert!(!paths::project_dir(&home, "shop", "app").exists());
    assert_eq!(
        layerfs_store::project::delete::Intent::read(&home, "shop", "app").unwrap(),
        None
    );
    let requests = seen.lock().unwrap();
    let ops: Vec<&str> = requests.iter().map(op_of).collect();
    assert_eq!(
        ops,
        [
            "project.delete.admit",
            "project.delete",
            "project.delete.release"
        ],
        "{requests:?}"
    );
    let op = sent_op(&requests[0]);
    assert_eq!(sent_op(&requests[1]), op);
    assert_eq!(sent_op(&requests[2]), op);
    assert!(
        !args_of(&requests[1]).get_bool("pending_retry"),
        "a fresh delete is no retry"
    );
    drop(requests);
    let _ = std::fs::remove_dir_all(home);
}

/// The bytes of a lowercase hex string.
fn hex_decode(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

#[test]
fn a_fake_workers_pending_retry_settles_the_marker_and_keeps_the_entry() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let stack = LayerStackId::from_authority([8; 16]);
    let home = caller_home("fake-settle", &address, stack);
    // The sidecar written by the crashed attempt, before its admission.
    let op = [0x3c; 16];
    let pending = layerfs_store::project::delete::Intent {
        store: "shop".to_owned(),
        project: "app".to_owned(),
        stack,
        deleted_ms: 77,
        attempt_ms: 1,
        state: layerfs_store::project::delete::IntentState::Pending,
        op: Some(op),
    };
    layerfs_store::project::delete::Intent::write(&home, &pending).unwrap();

    let attempt = Arc::new(AtomicU32::new(0));
    let count = attempt.clone();
    let seen = fake_worker_with(listener, move |request| match op_of(request) {
        "project.delete.admit" => Some(admitted_for(stack)),
        "project.delete" if count.fetch_add(1, Ordering::SeqCst) == 0 => {
            Some(removed_for(stack, 77, false))
        }
        "project.delete" => Some(removed_for(stack, 77, true)),
        "project.delete.release" => Some(released_for(request)),
        other => panic!("unexpected op {other}"),
    });

    // The first call is the pending retry: it redos the marker with the
    // sidecar's deleted_ms, keeps the entry and leaves the sidecar marked.
    let outcome = remove(&home).unwrap();
    assert_eq!(outcome.get_text("kind"), Some("Removed"));
    assert!(!outcome.get_bool("home_entry_removed"));
    assert_eq!(outcome.get_int("deleted_ms"), Some(77));
    assert!(paths::project_dir(&home, "shop", "app").is_dir());
    let settled = layerfs_store::project::delete::Intent::read(&home, "shop", "app")
        .unwrap()
        .expect("the sidecar is marked");
    assert_eq!(
        settled.state,
        layerfs_store::project::delete::IntentState::Marked
    );
    {
        let requests = seen.lock().unwrap();
        let marker = requests
            .iter()
            .find(|request| op_of(request) == "project.delete")
            .expect("the marker frame");
        assert_eq!(sent_op(marker), hex(&op));
        assert!(args_of(marker).get_bool("pending_retry"));
    }

    // The marker is confirmed: the next call finishes the entry.
    let outcome = remove(&home).unwrap();
    assert!(outcome.get_bool("home_entry_removed"));
    assert!(!paths::project_dir(&home, "shop", "app").exists());
    assert_eq!(
        layerfs_store::project::delete::Intent::read(&home, "shop", "app").unwrap(),
        None
    );
    let requests = seen.lock().unwrap();
    let markers: Vec<&SdkValue> = requests
        .iter()
        .filter(|request| op_of(request) == "project.delete")
        .collect();
    assert!(!args_of(markers[1]).get_bool("pending_retry"));
    drop(requests);
    let _ = std::fs::remove_dir_all(home);
}

#[test]
fn a_lost_worker_reply_is_unknown_and_keeps_the_sidecar_for_the_retry() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let stack = LayerStackId::from_authority([6; 16]);
    let home = caller_home("fake-lost", &address, stack);
    // The first marker attempt loses its answer; the second (the retry)
    // retires the Stack with the sidecar's `deleted_ms`.
    let attempts = Arc::new(AtomicU32::new(0));
    let count = attempts.clone();
    let seen = fake_worker_with(listener, move |request| match op_of(request) {
        "project.delete.admit" => Some(admitted_for(stack)),
        "project.delete" => {
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                None
            } else {
                Some(removed_for(stack, 42, true))
            }
        }
        "project.delete.release" => Some(released_for(request)),
        other => panic!("unexpected op {other}"),
    });

    let refusal = remove(&home).expect_err("the marker answer was lost");
    assert_eq!(refusal.code, "Unknown", "{refusal:?}");
    // The sidecar stays `pending`, naming the admission, and the entry stays
    // for the retry: an unknown outcome is never reported clean.
    let intent = layerfs_store::project::delete::Intent::read(&home, "shop", "app")
        .unwrap()
        .expect("the pending sidecar stays");
    assert_eq!(
        intent.state,
        layerfs_store::project::delete::IntentState::Pending
    );
    let op = intent.op.expect("the sidecar names its admission");
    assert!(paths::project_dir(&home, "shop", "app").is_dir());
    let requests = seen.lock().unwrap();
    let ops: Vec<&str> = requests.iter().map(op_of).collect();
    assert_eq!(
        ops,
        [
            "project.delete.admit",
            "project.delete",
            "project.delete.release"
        ],
        "{requests:?}"
    );
    let first = sent_op(&requests[0]);
    assert_eq!(sent_op(&requests[1]), first);
    assert_eq!(sent_op(&requests[2]), first);
    assert_eq!(hex_decode(&first), op.to_vec());
    drop(requests);

    // The retry re-registers the same identity and finishes the entry.
    let outcome = remove(&home).expect("the retry settles the unknown outcome");
    assert_eq!(outcome.get_text("kind"), Some("Removed"));
    assert_eq!(outcome.get_int("deleted_ms"), Some(42));
    assert!(outcome.get_bool("home_entry_removed"));
    assert!(!paths::project_dir(&home, "shop", "app").exists());
    assert_eq!(
        layerfs_store::project::delete::Intent::read(&home, "shop", "app").unwrap(),
        None,
        "the retry unlinked the sidecar"
    );
    let requests = seen.lock().unwrap();
    let ops: Vec<&str> = requests.iter().map(op_of).collect();
    assert_eq!(
        ops,
        [
            "project.delete.admit",
            "project.delete",
            "project.delete.release",
            "project.delete.admit",
            "project.delete",
            "project.delete.release"
        ],
        "{requests:?}"
    );
    let same: Vec<&str> = requests[3..].iter().map(sent_op_ref).collect();
    assert_eq!(same, [first.as_str(), first.as_str(), first.as_str()]);
    drop(requests);
    let _ = std::fs::remove_dir_all(home);
}

/// The `op_id` a request's args carry, borrowed.
fn sent_op_ref(request: &SdkValue) -> &str {
    args_of(request)
        .get_text("op_id")
        .expect("the frame names its admission")
}
