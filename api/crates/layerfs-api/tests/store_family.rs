//! sqlite Stores (#533 step 8, #536 §7.3): create on native, list/get/status,
//! a push to a machine behind a worker and a pull back with history heads
//! equal, the refusals of push and pull, and the InUse refusals of push and
//! remove.
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-stores-{tag}-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
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

/// A home with machines `native` and `box`; `box` is a worker in this test
/// process whose root is a directory of its own.
fn home_with_box() -> (PathBuf, PathBuf) {
    let home = tempdir("home");
    home::open(&home).unwrap();
    for (name, platform) in [("native", "native"), ("box", "lima")] {
        write_record(
            &paths::machine_record(&home, name),
            &SdkValue::map([("name", t(name)), ("platform", t(platform))]),
        )
        .unwrap();
    }
    let root = tempdir("guest");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let guest = root.clone();
    std::thread::spawn(move || layerfs_api::worker::serve(listener, "k".to_owned(), guest));
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&address)), ("secret", t("k"))]),
    )
    .unwrap();
    (home, root)
}

#[test]
fn create_list_get_and_status() {
    use std::os::unix::fs::PermissionsExt;
    let (home, root) = home_with_box();
    // a pre-existing store directory with group/other access is refused up
    // front without changing the user's directory mode
    let bad_dir = paths::store_dir(&home, "open-dir");
    home::private_dir(bad_dir.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&bad_dir).unwrap();
    std::fs::set_permissions(&bad_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (code, message) = call(&home, "store.create", &[("name", t("open-dir"))]).unwrap_err();
    assert_eq!(code, "Unsupported", "{message}");
    assert!(
        message.contains(&bad_dir.display().to_string()),
        "{message}"
    );
    assert!(
        message.contains("0755") && message.contains("0700"),
        "{message}"
    );
    assert!(
        message.contains(&format!("chmod 700 {}", bad_dir.display())),
        "{message}"
    );
    assert!(bad_dir.exists(), "user's existing directory is not deleted");
    assert_eq!(
        std::fs::metadata(&bad_dir).unwrap().permissions().mode() & 0o7777,
        0o755,
        "user's existing directory permissions are not modified"
    );
    std::fs::remove_dir_all(&bad_dir).unwrap();

    let made = call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    assert_eq!(
        std::fs::metadata(paths::store_dir(&home, "shop"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o700,
        "a Store's own folder is 0700"
    );
    assert_eq!(made.get_text("machine"), Some("native"));
    assert!(
        made.get("cursor").is_none(),
        "the cursor key never leaves the home"
    );
    assert!(paths::store_file(&home, "shop").is_file());
    assert_eq!(
        call(&home, "store.create", &[("name", t("shop"))])
            .unwrap_err()
            .0,
        "Exists"
    );

    let list = call(&home, "store.list", &[]).unwrap();
    let SdkValue::List(items) = list.get("items").unwrap() else {
        panic!()
    };
    assert_eq!(items.len(), 1);
    // each item carries the state `store.status` shows (#536 §10b)
    assert_eq!(items[0].get_text("state"), Some("idle"), "{items:?}");
    assert_eq!(
        call(&home, "store.get", &[("name", t("nope"))])
            .unwrap_err()
            .0,
        "NotFound"
    );

    let status = call(&home, "store.status", &[("name", t("shop"))]).unwrap();
    assert_eq!(status.get_text("state"), Some("idle"));
    assert_eq!(
        status.get("file").and_then(|f| f.get("exists")),
        Some(&SdkValue::Bool(true))
    );
    assert_eq!(
        status.get("file").and_then(|f| f.get("incoming_bytes")),
        Some(&SdkValue::Int(0))
    );
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_store_is_pushed_to_a_worker_machine_and_pulled_back_with_equal_heads() {
    let (home, root) = home_with_box();
    call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    let bytes = std::fs::read(paths::store_file(&home, "shop")).unwrap();

    let mut phases = Vec::new();
    let mut snap = Vec::new();
    let mut sent = Vec::new();
    let mut verified = Vec::new();
    let args = SdkValue::map([("name", t("shop")), ("machine", t("box"))]);
    let moved = layerfs_api::call(Some(&home), "store.push", args, &mut |event| {
        let progress = event.get("progress").unwrap();
        let phase = progress.get_text("phase").unwrap();
        if phases.last().map(String::as_str) != Some(phase) {
            phases.push(phase.to_owned());
        }
        // bytes while hashing (`snapshot`, `verify`) or sending (`send`),
        // null in every other phase (#536 §10b)
        let (done, total) = (progress.get("done"), progress.get("total"));
        match phase {
            "snapshot" => snap.push((done.cloned(), total.cloned())),
            "send" => sent.push((done.cloned(), total.cloned())),
            "verify" => verified.push((done.cloned(), total.cloned())),
            _ => assert_eq!(
                (done, total),
                (Some(&SdkValue::Null), Some(&SdkValue::Null))
            ),
        }
    })
    .unwrap();
    assert_eq!(
        phases,
        ["check", "snapshot", "send", "verify", "install", "record", "retire"]
    );
    let all = SdkValue::Int(bytes.len() as i64);
    assert_eq!(
        snap,
        vec![
            (Some(SdkValue::Null), Some(SdkValue::Null)),
            (Some(SdkValue::Int(0)), Some(all.clone())),
            (Some(all.clone()), Some(all.clone())),
        ]
    );
    assert_eq!(
        sent.first(),
        Some(&(Some(SdkValue::Int(0)), Some(all.clone())))
    );
    assert_eq!(sent.last(), Some(&(Some(all.clone()), Some(all.clone()))));
    assert_eq!(
        verified,
        vec![
            (Some(SdkValue::Null), Some(SdkValue::Null)),
            (Some(SdkValue::Int(0)), Some(all.clone())),
            (Some(all.clone()), Some(all.clone())),
        ]
    );
    let heads = moved.get("heads").cloned().unwrap();
    assert!(
        !paths::store_file(&home, "shop").exists(),
        "the source copy is retired"
    );
    assert_eq!(
        std::fs::read(paths::store_file(&root, "shop")).unwrap(),
        bytes
    );
    let record = read_record(&paths::store_record(&home, "shop"))
        .unwrap()
        .unwrap();
    assert_eq!(record.get_text("machine"), Some("box"));
    assert_eq!(record.get("op"), None, "a finished push drops `op`");
    assert_eq!(
        record
            .get("last_transfer")
            .and_then(|last| last.get("retired")),
        Some(&SdkValue::Bool(true))
    );
    // its machine's worker answers: the list finds it idle there
    let listed = call(&home, "store.list", &[]).unwrap();
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}")
    };
    assert_eq!(items[0].get_text("machine"), Some("box"));
    assert_eq!(items[0].get_text("state"), Some("idle"), "{items:?}");

    // a second push is refused before any effect: the Store is not on native
    let (code, message) = call(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("box"))],
    )
    .unwrap_err();
    assert_eq!(code, "InvalidInput", "{message}");
    assert!(message.contains("store pull shop"), "{message}");

    let mut pull_snap = Vec::new();
    let mut pull_verified = Vec::new();
    let back = layerfs_api::call(
        Some(&home),
        "store.pull",
        SdkValue::map([("name", t("shop"))]),
        &mut |event| {
            let progress = event.get("progress").unwrap();
            let phase = progress.get_text("phase").unwrap();
            let (done, total) = (
                progress.get("done").cloned(),
                progress.get("total").cloned(),
            );
            match phase {
                "snapshot" => pull_snap.push((done, total)),
                "verify" => pull_verified.push((done, total)),
                _ => {}
            }
        },
    )
    .unwrap();
    assert_eq!(
        pull_snap,
        vec![
            (Some(SdkValue::Null), Some(SdkValue::Null)),
            (Some(SdkValue::Int(0)), Some(all.clone())),
            (Some(all.clone()), Some(all.clone())),
        ]
    );
    assert_eq!(
        pull_verified,
        vec![
            (Some(SdkValue::Null), Some(SdkValue::Null)),
            (Some(SdkValue::Int(0)), Some(all.clone())),
            (Some(all.clone()), Some(all)),
        ]
    );
    assert_eq!(back.get_text("from"), Some("box"));
    assert_eq!(back.get_text("to"), Some("native"));
    assert_eq!(back.get("heads"), Some(&heads));
    assert_eq!(
        std::fs::read(paths::store_file(&home, "shop")).unwrap(),
        bytes
    );
    assert!(!paths::store_file(&root, "shop").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn push_and_remove_refuse_a_store_in_use_and_remove_deletes_it() {
    let (home, root) = home_with_box();
    call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    // A live OTHER process holds the lease (its own pid is skipped);
    // unix names init, Windows begets a sleeper child for the role.
    #[cfg(unix)]
    let holder = 1u32;
    #[cfg(windows)]
    let holder = {
        let child = std::process::Command::new("cmd")
            .args(["/C", "pause"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .expect("sleeper");
        child.id()
    };
    let lease = paths::lease_dir(&home, "shop").join(holder.to_string());
    std::fs::create_dir_all(lease.parent().unwrap()).unwrap();
    std::fs::write(&lease, format!("{holder}\n")).unwrap();
    let moved = call(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("box"))],
    );
    assert_eq!(moved.unwrap_err().0, "InUse");
    assert_eq!(
        call(&home, "store.remove", &[("name", t("shop"))])
            .unwrap_err()
            .0,
        "InUse"
    );
    let status = call(&home, "store.status", &[("name", t("shop"))]).unwrap();
    assert_eq!(status.get_text("state"), Some("in_use"));
    let listed = call(&home, "store.list", &[]).unwrap();
    assert!(
        layerfs_api::json::print(&listed).contains(r#""state":"in_use""#),
        "{listed:?}"
    );
    std::fs::remove_file(&lease).unwrap();

    // a failed phase keeps what exists: the target already has a file
    std::fs::create_dir_all(paths::store_dir(&root, "shop")).unwrap();
    std::fs::write(paths::store_file(&root, "shop"), b"x").unwrap();
    let (code, message) = call(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("box"))],
    )
    .unwrap_err();
    assert_eq!(code, "Exists", "{message}");
    assert!(message.contains("check"), "{message}");
    assert!(paths::store_file(&home, "shop").is_file());
    let record = read_record(&paths::store_record(&home, "shop"))
        .unwrap()
        .unwrap();
    assert_eq!(record.get_text("machine"), Some("native"));

    call(&home, "store.remove", &[("name", t("shop"))]).unwrap();
    assert!(!paths::store_dir(&home, "shop").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// The frames a killed opener leaves in `file`'s write-ahead log (#12): one
/// writable session saves an object and the process ends without closing it
/// (`process::exit` runs no destructor), which is what a `SIGKILL`ed opener
/// leaves. It runs as a child of the test below, never on its own.
#[test]
#[ignore = "child of snapshot_checkpoints_an_unheld_leftover_wal_before_pushing_and_pulling"]
fn a_killed_opener_leaves_its_log_frames() {
    let Ok(home) = std::env::var("LAYERFS_TEST_WAL_HOME") else {
        return;
    };
    let Ok(file) = std::env::var("LAYERFS_TEST_WAL_FILE") else {
        return;
    };
    let home = PathBuf::from(home);
    let record = read_record(&paths::store_record(&home, "shop"))
        .unwrap()
        .unwrap();
    let identity = layerfs_store::identity::identity_of(&record).unwrap();
    let handles = layerfs_persistence::SqliteHandles::open_writable(
        layerfs_persistence::PersistenceConfig::sqlite(&file)
            .with_sqlite_profile(layerfs_persistence::SqlitePersistenceProfile::Disposable),
        &identity.binding,
        identity.cursor_key,
    )
    .unwrap();
    save_one_object(&handles);
    std::process::exit(70);
}

/// One committed save, whose frames sit in the log until a checkpoint.
fn save_one_object(handles: &layerfs_persistence::SqliteHandles) {
    let storage = layerfs_storage::Storage::new(handles.storage.clone()).unwrap();
    let save = storage.begin_save().unwrap();
    let policy = storage.policy();
    let root = layerfs_telemetry::timer::Timing::disabled("construct", |scope| {
        layerfs_content::construct_bytes(
            policy.construction(),
            &policy.construction().capacities(),
            b"leftover write-ahead log frames",
            &mut save.sink(),
            scope.child("content"),
        )
    })
    .0
    .unwrap()
    .root;
    save.finish().unwrap();
    let _ = root;
}

/// Runs the child above on Store file `file`, which leaves a killed opener's
/// non-empty write-ahead log beside it.
fn kill_an_opener(home: &Path, file: &Path) {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "a_killed_opener_leaves_its_log_frames",
            "--ignored",
            "--nocapture",
        ])
        .env("LAYERFS_TEST_WAL_HOME", home)
        .env("LAYERFS_TEST_WAL_FILE", file)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert_eq!(
        child.code(),
        Some(70),
        "the child did not leave its log unclosed"
    );
}

fn wal_bytes(file: &Path) -> u64 {
    std::fs::metadata(file.with_file_name("store.sqlite-wal")).map_or(0, |meta| meta.len())
}

#[test]
fn snapshot_checkpoints_an_unheld_leftover_wal_before_pushing_and_pulling() {
    use std::os::unix::fs::PermissionsExt;
    let (home, root) = home_with_box();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), b"hello\n").unwrap();
    call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    call(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    )
    .unwrap();
    call(
        &home,
        "branch.fork",
        &[
            ("project", t("shop/api")),
            ("name", t("dev")),
            ("from_branch", t("main")),
        ],
    )
    .unwrap();
    call(
        &home,
        "file.write",
        &[
            ("project", t("shop/api")),
            ("branch", t("dev")),
            ("path", t("/README.md")),
            ("content", t("from-wal\n")),
            ("message", t("wal commit")),
        ],
    )
    .unwrap();
    layerfs_api::close_idle(std::time::Duration::ZERO);
    let store_path = paths::store_file(&home, "shop");
    assert_eq!(wal_bytes(&store_path), 0, "a closed Store keeps no log");

    // a killed opener's log on native: `store.push` folds it in and sends
    kill_an_opener(&home, &store_path);
    assert!(wal_bytes(&store_path) > 0, "the killed opener kept its log");
    let pushed = call(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("box"))],
    )
    .unwrap();
    assert_eq!(pushed.get_text("to"), Some("box"));
    assert_eq!(wal_bytes(&store_path), 0, "the push folded the log in");

    // and on the machine: `store.pull` folds it in there before reading
    let remote = paths::store_file(&root, "shop");
    kill_an_opener(&home, &remote);
    assert!(wal_bytes(&remote) > 0, "the killed opener kept its log");
    let pulled = call(&home, "store.pull", &[("name", t("shop"))]).unwrap();
    assert_eq!(pulled.get_text("to"), Some("native"));
    let read = call(
        &home,
        "file.read",
        &[
            ("project", t("shop/api")),
            ("branch", t("dev")),
            ("path", t("/README.md")),
        ],
    )
    .unwrap();
    assert_eq!(read.get_text("text"), Some("from-wal\n"));

    layerfs_api::close_idle(std::time::Duration::ZERO);
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(folder);
}
