//! A push or pull stopped at a phase (#536 §7.1, §7.3, H4–H6; lane K1's
//! terminal-pass item 7). The binding calls the progress handler
//! synchronously before the phase is written, so a panic there stands in
//! for a kill at that point: the record keeps what a killed process would
//! leave, and its owner is then made gone. The calls that clear what is left
//! are the ones the refusals and the doctor name. A worker served in this
//! process stands in for the machine.
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-crash-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn map(args: &[(&str, SdkValue)]) -> SdkValue {
    SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    )
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    layerfs_api::call(Some(home), name, map(args), &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn call_ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|(code, message)| panic!("{name}: {code}: {message}"))
}

/// A home with machines `native` and `box` (a worker in this process whose
/// root is a directory of its own) and Store `shop` on native with project
/// `api` recorded.
fn home_with_shop() -> (PathBuf, PathBuf) {
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
    call_ok(&home, "store.create", &[("name", t("shop"))]);
    write_record(
        &paths::project_record(&home, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    (home, root)
}

fn record(home: &Path) -> SdkValue {
    read_record(&paths::store_record(home, "shop"))
        .unwrap()
        .unwrap()
}

/// Runs `name` until its progress reaches `phase`, and stops it there
/// (a panic: nothing after the event runs); answers the record's `op` as
/// the stop left it, its owner then made gone (another start time).
fn stopped_at(home: &Path, name: &str, args: &[(&str, SdkValue)], phase: &str) -> SdkValue {
    let stopped = std::panic::catch_unwind(AssertUnwindSafe(|| {
        layerfs_api::call(Some(home), name, map(args), &mut |event| {
            let progress = event.get("progress").unwrap();
            if progress.get_text("phase") == Some(phase) {
                panic!("stopped at {phase}");
            }
        })
    }));
    assert!(stopped.is_err(), "{name} ran past {phase}: {stopped:?}");
    let op = record(home).get("op").cloned().expect("an op is left");
    let owner = op.get("owner").cloned().unwrap();
    let start: u64 = owner.get_text("pid_start").unwrap().parse().unwrap();
    let gone = home::with_field(owner, "pid_start", t(&(start + 1).to_string()));
    let op = home::with_field(op, "owner", gone);
    let path = paths::store_record(home, "shop");
    write_record(&path, &home::with_field(record(home), "op", op.clone())).unwrap();
    op
}

fn read(home: &Path) -> Result<SdkValue, (String, String)> {
    call(
        home,
        "file.read",
        &[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/a")),
        ],
    )
}

/// The doctor's findings for Store `shop` as `(severity, message, fix)`.
fn doctor_on_shop(home: &Path) -> Vec<(String, String, String)> {
    let report = layerfs_api::doctor(home).unwrap();
    let Some(SdkValue::List(items)) = report.get("findings") else {
        panic!("{report:?}")
    };
    let text = |v: &SdkValue, k: &str| v.get_text(k).unwrap_or("").to_owned();
    items
        .iter()
        .filter(|f| f.get("entity").and_then(|e| e.get_text("name")) == Some("shop"))
        .map(|f| (text(f, "severity"), text(f, "message"), text(f, "fix")))
        .collect()
}

#[test]
fn a_push_stopped_between_install_and_record_is_unresolved_and_a_new_push_clears_it() {
    let (home, root) = home_with_shop();
    let bytes = std::fs::read(paths::store_file(&home, "shop")).unwrap();
    let push = [("name", t("shop")), ("machine", t("box"))];

    // stopped as install starts: nothing is in place yet, certain
    let op = stopped_at(&home, "store.push", &push, "install");
    assert_eq!(op.get_text("phase"), Some("verify"));
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(false)));
    assert!(!paths::store_file(&root, "shop").exists());
    // a gone process's certain intent holds nothing: the next push goes
    call_ok(&home, "store.push", &push);
    assert_eq!(record(&home).get_text("machine"), Some("box"));
    call_ok(&home, "store.pull", &[("name", t("shop"))]);

    // stopped after install, before record: the target holds the installed
    // copy and the intent was written uncertain before install began
    let op = stopped_at(&home, "store.push", &push, "record");
    assert_eq!(op.get_text("phase"), Some("install"));
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(true)));
    assert_eq!(
        std::fs::read(paths::store_file(&root, "shop")).unwrap(),
        bytes
    );
    assert!(paths::store_file(&home, "shop").exists());
    assert_eq!(record(&home).get_text("machine"), Some("native"));

    // unresolved: other calls are refused naming the push that clears it
    let (code, message) = read(&home).unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    for named in [
        "`layerfs store status shop`",
        "`layerfs store push shop box`",
    ] {
        assert!(message.contains(named), "{named} not in {message}");
    }
    let status = call_ok(&home, "store.status", &[("name", t("shop"))]);
    assert_eq!(status.get_text("state"), Some("unresolved"));
    let found = doctor_on_shop(&home);
    assert!(
        found
            .iter()
            .any(|(severity, message, fix)| severity == "error"
                && message.contains("unresolved store.push")
                && fix.contains("`layerfs store push shop box` clears it")),
        "{found:?}"
    );

    // the named call deletes the copy the record does not name, then pushes
    let moved = call_ok(&home, "store.push", &push);
    assert_eq!(moved.get("bytes"), Some(&SdkValue::Int(bytes.len() as i64)));
    let now = record(&home);
    assert_eq!(now.get_text("machine"), Some("box"));
    assert_eq!(now.get("op"), None);
    assert_eq!(
        std::fs::read(paths::store_file(&root, "shop")).unwrap(),
        bytes
    );
    assert!(!paths::store_file(&home, "shop").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_pull_stopped_between_install_and_record_is_cleared_by_a_new_pull() {
    let (home, root) = home_with_shop();
    let bytes = std::fs::read(paths::store_file(&home, "shop")).unwrap();
    call_ok(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("box"))],
    );
    let pull = [("name", t("shop"))];
    let op = stopped_at(&home, "store.pull", &pull, "record");
    assert_eq!(op.get_text("phase"), Some("install"));
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(true)));
    // native holds the installed copy the record does not name
    assert!(paths::store_file(&home, "shop").exists());
    assert_eq!(record(&home).get_text("machine"), Some("box"));
    let (code, message) = read(&home).unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    assert!(message.contains("`layerfs store pull shop`"), "{message}");

    let back = call_ok(&home, "store.pull", &pull);
    assert_eq!(back.get_text("to"), Some("native"));
    let now = record(&home);
    assert_eq!(now.get_text("machine"), Some("native"));
    assert_eq!(now.get("op"), None);
    assert_eq!(
        std::fs::read(paths::store_file(&home, "shop")).unwrap(),
        bytes
    );
    assert!(!paths::store_file(&root, "shop").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// The record's `last_transfer.retired`.
fn retired(home: &Path) -> Option<SdkValue> {
    record(home)
        .get("last_transfer")
        .and_then(|last| last.get("retired"))
        .cloned()
}

#[test]
fn a_transfer_stopped_at_retire_leaves_a_usable_store_and_the_next_one_deletes_the_old_copy() {
    let (home, root) = home_with_shop();
    let bytes = std::fs::read(paths::store_file(&home, "shop")).unwrap();
    let push = [("name", t("shop")), ("machine", t("box"))];
    let pull = [("name", t("shop"))];

    // a push stopped at retire: the record names `box`, native keeps its
    // copy, and the record says it is not retired
    let op = stopped_at(&home, "store.push", &push, "retire");
    assert_eq!(op.get_text("phase"), Some("retire"));
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(false)));
    assert_eq!(record(&home).get_text("machine"), Some("box"));
    assert_eq!(retired(&home), Some(SdkValue::Bool(false)));
    assert!(paths::store_file(&home, "shop").exists());
    assert!(paths::store_file(&root, "shop").exists());
    // the Store is in use on `box` as usual: the intent holds nothing
    let status = call_ok(&home, "store.status", &[("name", t("shop"))]);
    assert_eq!(status.get_text("state"), Some("idle"), "{status:?}");
    let found = doctor_on_shop(&home);
    assert!(
        found
            .iter()
            .any(|(severity, message, fix)| severity == "warning"
                && message.contains("did not retire the copy on native")
                && fix == "the next `layerfs store pull shop` deletes it first"),
        "{found:?}"
    );
    // the next pull deletes native's old copy first, then pulls
    call_ok(&home, "store.pull", &pull);
    let now = record(&home);
    assert_eq!(now.get_text("machine"), Some("native"));
    assert_eq!(now.get("op"), None);
    assert_eq!(retired(&home), Some(SdkValue::Bool(true)));
    assert_eq!(
        std::fs::read(paths::store_file(&home, "shop")).unwrap(),
        bytes
    );
    assert!(!paths::store_file(&root, "shop").exists());
    assert!(
        doctor_on_shop(&home).is_empty(),
        "{:?}",
        doctor_on_shop(&home)
    );

    // a pull stopped at retire: `box` keeps its copy; the next push deletes it
    call_ok(&home, "store.push", &push);
    stopped_at(&home, "store.pull", &pull, "retire");
    assert_eq!(record(&home).get_text("machine"), Some("native"));
    assert!(paths::store_file(&root, "shop").exists());
    let found = doctor_on_shop(&home);
    assert!(
        found
            .iter()
            .any(|(_, message, fix)| message.contains("the copy on box")
                && fix == "the next `layerfs store push shop <machine>` deletes it first"),
        "{found:?}"
    );
    call_ok(&home, "store.push", &push);
    assert_eq!(record(&home).get_text("machine"), Some("box"));
    assert_eq!(retired(&home), Some(SdkValue::Bool(true)));
    assert_eq!(
        std::fs::read(paths::store_file(&root, "shop")).unwrap(),
        bytes
    );
    assert!(!paths::store_file(&home, "shop").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_pull_leaves_no_store_folder_on_the_worker_and_follows_no_symlink_out_of_it() {
    let (home, root) = home_with_shop();
    call_ok(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("box"))],
    );
    // what a worker keeps beside the file: records and a project's entry,
    // and a symlink out of the folder
    let folder = paths::store_dir(&root, "shop");
    assert!(folder.join("store.sqlite").exists());
    write_record(
        &paths::project_record(&root, "shop", "api"),
        &SdkValue::map([("name", t("api"))]),
    )
    .unwrap();
    let outside = tempdir("outside");
    std::fs::write(outside.join("keep"), b"kept").unwrap();
    std::os::unix::fs::symlink(&outside, folder.join("link")).unwrap();

    call_ok(&home, "store.pull", &[("name", t("shop"))]);
    assert!(
        std::fs::symlink_metadata(&folder).is_err(),
        "the worker keeps nothing of the Store"
    );
    assert_eq!(std::fs::read(outside.join("keep")).unwrap(), b"kept");
    // the home's records stay with the Store on native
    assert!(paths::store_record(&home, "shop").exists());
    assert!(paths::project_record(&home, "shop", "api").exists());
    // a push finds the worker's root clean
    call_ok(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("box"))],
    );
    assert!(!paths::project_record(&root, "shop", "api").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

#[test]
fn wrong_direction_refuses_unresolved_without_deleting_leftover_and_remove_cleans_both_sides() {
    let (home, root) = home_with_shop();
    let push = [("name", t("shop")), ("machine", t("box"))];
    let pull = [("name", t("shop"))];

    // 1. A push stopped between install and record is `Unresolved` (the
    // record names native, `box` holds a copy). The opposite command must
    // refuse it without deleting that copy or clearing the intent.
    let op = stopped_at(&home, "store.push", &push, "record");
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(true)));
    assert!(paths::store_file(&root, "shop").exists());

    let (code, message) = call(&home, "store.pull", &pull).unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    assert!(
        message.contains("`layerfs store push shop box`"),
        "{message}"
    );
    assert!(
        paths::store_file(&root, "shop").exists(),
        "a wrong-direction pull must not delete the remote copy"
    );
    assert!(
        record(&home).get("op").is_some(),
        "a wrong-direction pull must not clear the unresolved intent"
    );

    // A push to another machine refuses it too, keeping `box`'s copy.
    write_record(
        &paths::machine_record(&home, "other"),
        &SdkValue::map([("name", t("other")), ("platform", t("lima"))]),
    )
    .unwrap();
    let (code, message) = call(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("other"))],
    )
    .unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    assert!(paths::store_file(&root, "shop").exists());
    assert!(record(&home).get("op").is_some());

    // 2. `store.remove` deletes both the record's machine's copy and the
    // leftover on `box`, so the name is free again.
    call_ok(&home, "store.remove", &[("name", t("shop"))]);
    assert!(!paths::store_dir(&home, "shop").exists());
    assert!(
        !paths::store_dir(&root, "shop").exists(),
        "store.remove must delete the leftover remote Store directory on box"
    );
    call_ok(&home, "store.create", &[("name", t("shop"))]);
    call_ok(&home, "store.push", &push);

    // 3. A pull stopped at retire names native and leaves the copy on `box`.
    // A `store.remove` whose cleanup of `box` loses its answer records
    // `leftover: "box"`, so a second call still cleans both sides.
    stopped_at(&home, "store.pull", &pull, "retire");
    assert_eq!(record(&home).get_text("machine"), Some("native"));
    assert_eq!(retired(&home), Some(SdkValue::Bool(false)));
    assert!(paths::store_file(&root, "shop").exists());

    // A proxy answers the worker's hello and drops the request: `Unknown`.
    let real_addr: std::net::SocketAddr = read_record(&paths::worker_record(&home, "box"))
        .unwrap()
        .unwrap()
        .get_text("address")
        .unwrap()
        .parse()
        .unwrap();
    let drop_listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let drop_addr = drop_listener.local_addr().unwrap().to_string();
    std::thread::spawn(move || {
        for client in drop_listener.incoming().flatten() {
            let mut client = client;
            let Ok(Some(hello)) =
                layerfs_api::worker::read_frame_within(&mut client, layerfs_api::worker::MAX_HELLO)
            else {
                continue;
            };
            let mut upstream = std::net::TcpStream::connect(real_addr).unwrap();
            layerfs_api::worker::write_frame(&mut upstream, &hello).unwrap();
            let ready = layerfs_api::worker::read_frame(&mut upstream)
                .unwrap()
                .unwrap();
            layerfs_api::worker::write_frame(&mut client, &ready).unwrap();
            let Some(request) = layerfs_api::worker::read_frame(&mut client).unwrap() else {
                continue;
            };
            if request.get_text("op") == Some("store.file") {
                // drop the connection during the cleanup: the answer is lost
                break;
            }
            layerfs_api::worker::write_frame(&mut upstream, &request).unwrap();
            if let Some(answer) = layerfs_api::worker::read_frame(&mut upstream).unwrap() {
                let _ = layerfs_api::worker::write_frame(&mut client, &answer);
            }
        }
    });
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&drop_addr)), ("secret", t("k"))]),
    )
    .unwrap();
    let (code, _) = call(&home, "store.remove", &[("name", t("shop"))]).unwrap_err();
    assert_eq!(code, "Unknown");
    let remove_op = record(&home)
        .get("op")
        .cloned()
        .expect("an unresolved remove intent");
    assert_eq!(remove_op.get_text("name"), Some("store.remove"));
    assert_eq!(
        remove_op
            .get("args")
            .and_then(|args| args.get_text("leftover")),
        Some("box")
    );
    // the intent's owner is made gone, as a killed process would leave it
    let owner = remove_op.get("owner").cloned().unwrap();
    let start: u64 = owner.get_text("pid_start").unwrap().parse().unwrap();
    let gone = home::with_field(owner, "pid_start", t(&(start + 1).to_string()));
    let remove_op = home::with_field(remove_op, "owner", gone);
    write_record(
        &paths::store_record(&home, "shop"),
        &home::with_field(record(&home), "op", remove_op),
    )
    .unwrap();
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&real_addr.to_string())), ("secret", t("k"))]),
    )
    .unwrap();

    call_ok(&home, "store.remove", &[("name", t("shop"))]);
    assert!(!paths::store_dir(&home, "shop").exists());
    assert!(
        !paths::store_dir(&root, "shop").exists(),
        "the second store.remove must still delete the leftover copy on box"
    );

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn retire_failure_reports_authority_switched_to_target_and_source_unretired() {
    let (home, root) = home_with_shop();
    let push = [("name", t("shop")), ("machine", t("box"))];
    call_ok(&home, "store.push", &push);

    // A pull that reaches `retire` has already written `machine: native` and
    // `last_transfer.retired: false`; make the source's folder undeletable so
    // retiring it fails.
    let refusal = layerfs_api::call(
        Some(&home),
        "store.pull",
        map(&[("name", t("shop"))]),
        &mut |event| {
            let progress = event.get("progress").unwrap();
            if progress.get_text("phase") == Some("retire") {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(
                    paths::store_dir(&root, "shop"),
                    std::fs::Permissions::from_mode(0o000),
                );
                let _ = std::fs::set_permissions(
                    root.join("store"),
                    std::fs::Permissions::from_mode(0o500),
                );
            }
        },
    )
    .unwrap_err();

    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(root.join("store"), std::fs::Permissions::from_mode(0o700));
    let _ = std::fs::set_permissions(
        paths::store_dir(&root, "shop"),
        std::fs::Permissions::from_mode(0o700),
    );

    assert_eq!(refusal.code, "Failed");
    assert_eq!(refusal.detail.get_text("phase"), Some("retire"));
    assert_eq!(refusal.detail.get_text("authority"), Some("native"));
    assert_eq!(refusal.detail.get_text("source"), Some("box"));
    assert_eq!(refusal.detail.get("retired"), Some(&SdkValue::Bool(false)));
    assert!(
        refusal.message.contains("authority switched to native"),
        "{}",
        refusal.message
    );
    assert!(
        refusal
            .message
            .contains("source copy on box was not retired"),
        "{}",
        refusal.message
    );
    assert_eq!(record(&home).get_text("machine"), Some("native"));
    assert_eq!(retired(&home), Some(SdkValue::Bool(false)));

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}
