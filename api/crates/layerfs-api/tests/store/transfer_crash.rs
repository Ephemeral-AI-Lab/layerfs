//! A move either way stopped at a phase (#536 §7.1, §7.3, H4–H6; lane K1's
//! terminal-pass item 7). The binding calls the progress handler
//! synchronously before the phase is written, so a panic there stands in
//! for a kill at that point: the record keeps what a killed process would
//! leave, and its owner is then made gone. The calls that clear what is left
//! are the ones the refusals and the doctor name. A worker served in this
//! process stands in for the machine.
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_protocol::sdk::SdkValue;
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
    layerfs_api::call(
        Some(crate::root::root_of(home)),
        name,
        map(args),
        &mut |_| {},
    )
    .map_err(|r| (r.code, r.message))
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
    // `box` records its provider name, as every 2.0 create does
    for (name, platform, vm_name) in [
        (layerfs_home::names::HOST_MACHINE, "host", None),
        ("box", "lima", Some("layerfs-test-box")),
    ] {
        let mut record = SdkValue::map([
            ("name", t(name)),
            ("platform", t(platform)),
            ("state", t("running")),
        ]);
        if let Some(vm_name) = vm_name {
            record = home::with_field(record, "vm_name", t(vm_name));
        }
        write_record(&paths::machine_record(&home, name), &record).unwrap();
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
        layerfs_api::call(
            Some(crate::root::root_of(home)),
            name,
            map(args),
            &mut |event| {
                let progress = event.get("progress").unwrap();
                if progress.get_text("phase") == Some(phase) {
                    panic!("stopped at {phase}");
                }
            },
        )
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

/// The home doctor's records findings on Store `shop` as `(severity,
/// message, fix)`.
fn doctor_on_shop(home: &Path) -> Vec<(String, String, String)> {
    let report = call_ok(home, "home.doctor", &[("detail", SdkValue::Bool(true))]);
    let Some(SdkValue::List(items)) = report.get("findings") else {
        panic!("{report:?}")
    };
    let text = |v: &SdkValue, k: &str| v.get_text(k).unwrap_or("").to_owned();
    items
        .iter()
        .filter(|f| f.get_text("store") == Some("shop"))
        .filter(|f| f.get_text("check") == Some("home.records"))
        .map(|f| (text(f, "severity"), text(f, "message"), text(f, "fix")))
        .collect()
}

#[test]
fn a_move_stopped_between_install_and_record_is_unresolved_and_a_new_move_clears_it() {
    let (home, root) = home_with_shop();
    let bytes = std::fs::read(paths::store_file(&home, "shop")).unwrap();
    let push = [("store", t("shop")), ("machine", t("box"))];

    // stopped as install starts: nothing is in place yet, certain
    let op = stopped_at(&home, "store.move", &push, "install");
    assert_eq!(op.get_text("phase"), Some("verify"));
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(false)));
    assert!(!paths::store_file(&root, "shop").exists());
    // a gone process's certain intent holds nothing: the next push goes
    call_ok(&home, "store.move", &push);
    assert_eq!(record(&home).get_text("machine"), Some("box"));
    call_ok(
        &home,
        "store.move",
        &[
            ("store", t("shop")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ],
    );

    // stopped after install, before record: the target holds the installed
    // copy and the intent was written uncertain before install began
    let op = stopped_at(&home, "store.move", &push, "record");
    assert_eq!(op.get_text("phase"), Some("install"));
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(true)));
    assert_eq!(
        std::fs::read(paths::store_file(&root, "shop")).unwrap(),
        bytes
    );
    assert!(paths::store_file(&home, "shop").exists());
    assert_eq!(
        record(&home).get_text("machine"),
        Some(layerfs_home::names::HOST_MACHINE)
    );

    // unresolved: other calls are refused naming the move that clears it
    let (code, message) = read(&home).unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    for named in ["`layerfs store get shop`", "`layerfs store move shop box`"] {
        assert!(message.contains(named), "{named} not in {message}");
    }
    let status = call_ok(&home, "store.get", &[("store", t("shop"))]);
    assert_eq!(status.get_text("state"), Some("broken"));
    assert_eq!(status.get_text("reason"), Some("unresolved"));
    assert_eq!(status.get_text("fix"), Some("layerfs store move shop box"));
    let found = doctor_on_shop(&home);
    assert!(
        found
            .iter()
            .any(|(severity, message, fix)| severity == "error"
                && message.contains("unresolved store.move")
                && fix == "layerfs store move shop box"),
        "{found:?}"
    );

    // the named call deletes the copy the record does not name, then moves
    let moved = call_ok(&home, "store.move", &push);
    assert_eq!(
        moved.get("moved_bytes"),
        Some(&SdkValue::Int(bytes.len() as i64))
    );
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
fn a_move_here_stopped_between_install_and_record_is_cleared_by_a_new_move_here() {
    let (home, root) = home_with_shop();
    let bytes = std::fs::read(paths::store_file(&home, "shop")).unwrap();
    call_ok(
        &home,
        "store.move",
        &[("store", t("shop")), ("machine", t("box"))],
    );
    let pull = [
        ("store", t("shop")),
        ("machine", t(layerfs_home::names::HOST_MACHINE)),
    ];
    let op = stopped_at(&home, "store.move", &pull, "record");
    assert_eq!(op.get_text("phase"), Some("install"));
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(true)));
    // native holds the installed copy the record does not name
    assert!(paths::store_file(&home, "shop").exists());
    assert_eq!(record(&home).get_text("machine"), Some("box"));
    let (code, message) = read(&home).unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    assert!(
        message.contains(&format!(
            "`layerfs store move shop {}`",
            layerfs_home::names::HOST_MACHINE
        )),
        "{message}"
    );

    let mut long = pull.to_vec();
    long.push(("detail", SdkValue::Bool(true)));
    let back = call_ok(&home, "store.move", &long);
    assert_eq!(back.get_text("to"), Some(layerfs_home::names::HOST_MACHINE));
    let now = record(&home);
    assert_eq!(
        now.get_text("machine"),
        Some(layerfs_home::names::HOST_MACHINE)
    );
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
    let push = [("store", t("shop")), ("machine", t("box"))];
    let pull = [
        ("store", t("shop")),
        ("machine", t(layerfs_home::names::HOST_MACHINE)),
    ];

    // a push stopped at retire: the record names `box`, native keeps its
    // copy, and the record says it is not retired
    let op = stopped_at(&home, "store.move", &push, "retire");
    assert_eq!(op.get_text("phase"), Some("retire"));
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(false)));
    assert_eq!(record(&home).get_text("machine"), Some("box"));
    assert_eq!(retired(&home), Some(SdkValue::Bool(false)));
    assert!(paths::store_file(&home, "shop").exists());
    assert!(paths::store_file(&root, "shop").exists());
    // the Store is in use on `box` as usual: the intent holds nothing
    let status = call_ok(&home, "store.get", &[("store", t("shop"))]);
    assert_eq!(status.get_text("state"), Some("ready"), "{status:?}");
    let found = doctor_on_shop(&home);
    assert!(
        found
            .iter()
            .any(|(severity, message, fix)| severity == "warning"
                && message.contains(&format!(
                    "did not retire the copy on {}",
                    layerfs_home::names::HOST_MACHINE
                ))
                && message.contains("the next `layerfs store move shop` deletes it first")
                && fix.is_empty()),
        "{found:?}"
    );
    // the next move here deletes native's old copy first, then moves
    call_ok(&home, "store.move", &pull);
    let now = record(&home);
    assert_eq!(
        now.get_text("machine"),
        Some(layerfs_home::names::HOST_MACHINE)
    );
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

    // a move here stopped at retire: `box` keeps its copy; the next move
    // there deletes it
    call_ok(&home, "store.move", &push);
    stopped_at(&home, "store.move", &pull, "retire");
    assert_eq!(
        record(&home).get_text("machine"),
        Some(layerfs_home::names::HOST_MACHINE)
    );
    assert!(paths::store_file(&root, "shop").exists());
    let found = doctor_on_shop(&home);
    assert!(
        found
            .iter()
            .any(|(_, message, fix)| message.contains("the copy on box")
                && message.contains("the next `layerfs store move shop` deletes it first")
                && fix.is_empty()),
        "{found:?}"
    );
    call_ok(&home, "store.move", &push);
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
fn a_move_here_leaves_no_store_folder_on_the_worker_and_follows_no_symlink_out_of_it() {
    let (home, root) = home_with_shop();
    call_ok(
        &home,
        "store.move",
        &[("store", t("shop")), ("machine", t("box"))],
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

    call_ok(
        &home,
        "store.move",
        &[
            ("store", t("shop")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ],
    );
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
        "store.move",
        &[("store", t("shop")), ("machine", t("box"))],
    );
    assert!(!paths::project_record(&root, "shop", "api").exists());
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(outside);
}

#[test]
fn wrong_direction_refuses_unresolved_without_deleting_leftover_and_remove_cleans_both_sides() {
    let (home, root) = home_with_shop();
    let push = [("store", t("shop")), ("machine", t("box"))];
    let pull = [
        ("store", t("shop")),
        ("machine", t(layerfs_home::names::HOST_MACHINE)),
    ];

    // 1. A move to `box` stopped between install and record is `Unresolved`
    // (the record names native, `box` holds a copy). A move the other way
    // must refuse it without deleting that copy or clearing the intent.
    let op = stopped_at(&home, "store.move", &push, "record");
    assert_eq!(op.get("uncertain"), Some(&SdkValue::Bool(true)));
    assert!(paths::store_file(&root, "shop").exists());

    let (code, message) = call(&home, "store.move", &pull).unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    assert!(
        message.contains("`layerfs store move shop box`"),
        "{message}"
    );
    assert!(
        paths::store_file(&root, "shop").exists(),
        "a wrong-direction move must not delete the remote copy"
    );
    assert!(
        record(&home).get("op").is_some(),
        "a wrong-direction move must not clear the unresolved intent"
    );

    // A move to another machine refuses it too, keeping `box`'s copy.
    write_record(
        &paths::machine_record(&home, "other"),
        &SdkValue::map([
            ("name", t("other")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-test-other")),
        ]),
    )
    .unwrap();
    let (code, message) = call(
        &home,
        "store.move",
        &[("store", t("shop")), ("machine", t("other"))],
    )
    .unwrap_err();
    assert_eq!(code, "Unresolved", "{message}");
    assert!(paths::store_file(&root, "shop").exists());
    assert!(record(&home).get("op").is_some());

    // 2. `store.remove` deletes both the record's machine's copy and the
    // leftover on `box`, so the name is free again.
    call_ok(&home, "store.remove", &[("store", t("shop"))]);
    assert!(!paths::store_dir(&home, "shop").exists());
    assert!(
        !paths::store_dir(&root, "shop").exists(),
        "store.remove must delete the leftover remote Store directory on box"
    );
    call_ok(&home, "store.create", &[("name", t("shop"))]);
    call_ok(&home, "store.move", &push);

    // 3. A move here stopped at retire names native and leaves the copy on `box`.
    // A `store.remove` whose cleanup of `box` loses its answer records
    // `leftover: "box"`, so a second call still cleans both sides.
    stopped_at(&home, "store.move", &pull, "retire");
    assert_eq!(
        record(&home).get_text("machine"),
        Some(layerfs_home::names::HOST_MACHINE)
    );
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
    let (code, _) = call(&home, "store.remove", &[("store", t("shop"))]).unwrap_err();
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

    call_ok(&home, "store.remove", &[("store", t("shop"))]);
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
    let push = [("store", t("shop")), ("machine", t("box"))];
    call_ok(&home, "store.move", &push);

    // A move here that reaches `retire` has already written `machine: native` and
    // `last_transfer.retired: false`; make the source's folder undeletable so
    // retiring it fails.
    let refusal = layerfs_api::call(
        Some(crate::root::root_of(&home)),
        "store.move",
        map(&[
            ("store", t("shop")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ]),
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
    assert_eq!(
        refusal.detail.get_text("authority"),
        Some(layerfs_home::names::HOST_MACHINE)
    );
    assert_eq!(refusal.detail.get_text("source"), Some("box"));
    assert_eq!(refusal.detail.get("retired"), Some(&SdkValue::Bool(false)));
    assert!(
        refusal.message.contains(&format!(
            "authority switched to {}",
            layerfs_home::names::HOST_MACHINE
        )),
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
    assert_eq!(
        record(&home).get_text("machine"),
        Some(layerfs_home::names::HOST_MACHINE)
    );
    assert_eq!(retired(&home), Some(SdkValue::Bool(false)));

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}
