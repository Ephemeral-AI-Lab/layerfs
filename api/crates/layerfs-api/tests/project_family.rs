//! Projects and Init (#533 step 9), end to end on this host: a Store made on
//! native, a folder Inited into it as a live add, file and Branch calls on
//! it, the Store moved to a worker machine and read there, then the project
//! removed.
use layerfs_api::calls;
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-projects-{tag}-{}-{}",
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

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    calls::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn content(read: &SdkValue) -> String {
    match read.get("text") {
        Some(SdkValue::Text(text)) => text.clone(),
        Some(SdkValue::Bytes(bytes)) => String::from_utf8_lossy(bytes).into_owned(),
        other => panic!("no content: {other:?} in {read:?}"),
    }
}

#[test]
fn init_files_branches_move_and_remove() {
    let home = tempdir("home");
    // a home is private to its user (Init checks it)
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([("name", t("box")), ("platform", t("lima"))]),
    )
    .unwrap();
    // a guest root as a data disk mounts it; the worker makes it private
    let root = tempdir("guest");
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap().to_string();
    let guest = root.clone();
    std::thread::spawn(move || layerfs_api::worker::serve(listener, "k".to_owned(), guest));
    write_record(
        &paths::worker_record(&home, "box"),
        &SdkValue::map([("address", t(&address)), ("secret", t("k"))]),
    )
    .unwrap();

    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "hello\nworld\n").unwrap();
    std::fs::create_dir(folder.join("src")).unwrap();
    std::fs::write(folder.join("src/main.rs"), "fn main() {}\n").unwrap();

    // Init needs an existing Store
    let refused = call(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    assert_eq!(refused.unwrap_err().0, "NotFound");
    ok(&home, "store.create", &[("name", t("shop"))]);
    let mut events = 0;
    let args = SdkValue::map([
        ("project", t("shop/api")),
        ("folder", t(&folder.display().to_string())),
    ]);
    let made = calls::call(Some(&home), "project.init", args, &mut |_| events += 1)
        .unwrap_or_else(|e| panic!("init: {} {}", e.code, e.message));
    assert_eq!(made.get_text("project"), Some("shop/api"));
    assert_eq!(made.get_text("machine"), Some("native"));

    let list = ok(&home, "project.list", &[]);
    let SdkValue::List(items) = list.get("items").unwrap() else {
        panic!()
    };
    assert_eq!(items.len(), 1);
    ok(&home, "project.get", &[("project", t("shop/api"))]);

    let p = || ("project", t("shop/api"));
    let read = ok(
        &home,
        "file.read",
        &[p(), ("branch", t("main")), ("path", t("/README.md"))],
    );
    assert_eq!(content(&read), "hello\nworld\n");
    ok(
        &home,
        "branch.fork",
        &[p(), ("name", t("fix")), ("from_branch", t("main"))],
    );
    ok(
        &home,
        "file.write",
        &[
            p(),
            ("branch", t("fix")),
            ("path", t("/NEW.md")),
            ("content", t("new\n")),
            ("message", t("add NEW")),
        ],
    );
    let commits = ok(&home, "commit.list", &[p(), ("branch", t("fix"))]);
    let SdkValue::List(commits) = commits.get("items").unwrap() else {
        panic!("{commits:?}")
    };
    assert_eq!(commits.len(), 1);
    let id = commits[0].get_text("id").unwrap().to_owned();
    let got = ok(
        &home,
        "commit.get",
        &[p(), ("branch", t("fix")), ("commit", t(&id))],
    );
    assert_eq!(got.get_text("id"), Some(id.as_str()));
    let diff = ok(
        &home,
        "branch.diff",
        &[
            p(),
            ("from_branch", t("main")),
            ("to_branch", t("fix")),
            ("paths_only", SdkValue::Bool(true)),
        ],
    );
    assert!(
        layerfs_api::calls::json::print(&diff).contains("NEW.md"),
        "{diff:?}"
    );
    ok(
        &home,
        "file.edit",
        &[
            p(),
            ("branch", t("fix")),
            ("path", t("/NEW.md")),
            ("old", t("new")),
            ("new", t("newer")),
        ],
    );
    let read = ok(
        &home,
        "file.read",
        &[p(), ("branch", t("fix")), ("path", t("/NEW.md"))],
    );
    assert_eq!(content(&read), "newer\n");
    let branches = ok(&home, "branch.list", &[p()]);
    assert!(layerfs_api::calls::json::print(&branches).contains("\"fix\""));
    ok(&home, "branch.get", &[p(), ("branch", t("fix"))]);

    // the Store's open service is closed before a move: a lease of this
    // process would not block it, but the open file must be closed first
    std::thread::sleep(std::time::Duration::from_millis(10));
    ok(
        &home,
        "store.move",
        &[("name", t("shop")), ("machine", t("box"))],
    );
    let read = ok(
        &home,
        "file.read",
        &[p(), ("branch", t("fix")), ("path", t("/NEW.md"))],
    );
    assert_eq!(content(&read), "newer\n", "read through the worker");

    ok(&home, "project.remove", &[p()]);
    assert_eq!(
        call(&home, "project.get", &[p()]).unwrap_err().0,
        "NoSuchProject"
    );

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(folder);
}

/// The host's private-home check (moved from layerfs-control's
/// local_root_modes, whose route `runtime.store.create` is gone, #536 step
/// 2): Init into a Store refuses a home others may read or write, and
/// accepts one others may only pass through (o+x).
#[test]
fn a_private_home_may_be_passable_and_nothing_more() {
    use std::os::unix::fs::PermissionsExt;
    let set = |path: &Path, mode: u32| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap()
    };
    let home = tempdir("private");
    set(&home, 0o700);
    home::open(&home).unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    let folder = tempdir("private-folder");
    std::fs::write(folder.join("README.md"), "hello\n").unwrap();
    let init = |name: &str| {
        call(
            &home,
            "project.init",
            &[
                ("project", t(&format!("shop/{name}"))),
                ("folder", t(&folder.display().to_string())),
            ],
        )
    };
    for (name, open) in [("a", 0o750), ("b", 0o711), ("c", 0o705), ("d", 0o702)] {
        set(&home, open);
        let (code, message) = init(name).unwrap_err();
        assert_eq!(code, "Unauthorized", "{open:o}: {message}");
        assert!(
            message.contains("must be owned by this user and private"),
            "{message}"
        );
    }
    for (name, passable) in [("e", 0o700), ("f", 0o701)] {
        set(&home, passable);
        init(name).unwrap_or_else(|e| panic!("{passable:o}: {e:?}"));
    }
    set(&home, 0o700);
    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}
