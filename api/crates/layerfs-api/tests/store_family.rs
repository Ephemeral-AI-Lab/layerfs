//! sqlite Stores (#533 step 8): create on native, list/get/status, a move to
//! a machine behind a worker and back with history heads equal, and the
//! InUse refusals of move and remove.
use layerfs_api::calls;
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
    calls::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
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
    let (home, root) = home_with_box();
    let made = call(&home, "store.create", &[("name", t("shop"))]).unwrap();
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
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_store_moves_to_a_worker_machine_and_back_with_equal_heads() {
    let (home, root) = home_with_box();
    call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    let bytes = std::fs::read(paths::store_file(&home, "shop")).unwrap();

    let mut phases = Vec::new();
    let args = SdkValue::map([("name", t("shop")), ("machine", t("box"))]);
    let moved = calls::call(Some(&home), "store.move", args, &mut |event| {
        if let Some(phase) = event.get("progress").and_then(|p| p.get_text("phase")) {
            if phases.last().map(String::as_str) != Some(phase) {
                phases.push(phase.to_owned());
            }
        }
    })
    .unwrap();
    assert_eq!(
        phases,
        ["check", "snapshot", "send", "verify", "install", "record", "retire"]
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
    assert_eq!(record.get("op"), None, "the record phase drops `op`");
    // its machine's worker answers: the list finds it idle there
    let listed = call(&home, "store.list", &[]).unwrap();
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}")
    };
    assert_eq!(items[0].get_text("machine"), Some("box"));
    assert_eq!(items[0].get_text("state"), Some("idle"), "{items:?}");

    // a second move to the same machine is refused before any effect
    let again = call(
        &home,
        "store.move",
        &[("name", t("shop")), ("machine", t("box"))],
    );
    assert_eq!(again.unwrap_err().0, "InvalidInput");

    let back = call(
        &home,
        "store.move",
        &[("name", t("shop")), ("machine", t("native"))],
    )
    .unwrap();
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
fn move_and_remove_refuse_a_store_in_use_and_remove_deletes_it() {
    let (home, root) = home_with_box();
    call(&home, "store.create", &[("name", t("shop"))]).unwrap();
    // process 1 is always alive: its lease holds the Store
    let lease = paths::lease_dir(&home, "shop").join("1");
    std::fs::create_dir_all(lease.parent().unwrap()).unwrap();
    std::fs::write(&lease, "1\n").unwrap();
    let moved = call(
        &home,
        "store.move",
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
        calls::json::print(&listed).contains(r#""state":"in_use""#),
        "{listed:?}"
    );
    std::fs::remove_file(&lease).unwrap();

    // a failed phase keeps what exists: the target already has a file
    std::fs::create_dir_all(paths::store_dir(&root, "shop")).unwrap();
    std::fs::write(paths::store_file(&root, "shop"), b"x").unwrap();
    let (code, message) = call(
        &home,
        "store.move",
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
