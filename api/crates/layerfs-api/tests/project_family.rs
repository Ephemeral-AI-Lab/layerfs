//! Projects and Init (#533 step 9), end to end on this host: a Store made on
//! native, a folder Inited into it as a live add, file and Branch calls on
//! it, the Store pushed to a worker machine and read there, then the project
//! removed.
use layerfs_api::home::{self, paths, write_record};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
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
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
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
    // project.init on an existing Store refuses up front if its directory was
    // made non-private (e.g. chmod 0755), reporting path, 0755, 0700 and chmod 700
    {
        use std::os::unix::fs::PermissionsExt;
        let shop_dir = paths::store_dir(&home, "shop");
        std::fs::set_permissions(&shop_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (code, message) = call(
            &home,
            "project.init",
            &[
                ("project", t("shop/api")),
                ("folder", t(&folder.display().to_string())),
            ],
        )
        .unwrap_err();
        assert_eq!(code, "Unsupported", "{message}");
        assert!(
            message.contains(&shop_dir.display().to_string()),
            "{message}"
        );
        assert!(
            message.contains("0755") && message.contains("0700"),
            "{message}"
        );
        assert!(
            message.contains(&format!("chmod 700 {}", shop_dir.display())),
            "{message}"
        );
        std::fs::set_permissions(&shop_dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let mut events = Vec::new();
    let args = SdkValue::map([
        ("project", t("shop/api")),
        ("folder", t(&folder.display().to_string())),
    ]);
    let made = layerfs_api::call(Some(&home), "project.init", args, &mut |e| events.push(e))
        .unwrap_or_else(|e| panic!("init: {} {}", e.code, e.message));
    assert_eq!(made.get_text("project"), Some("shop/api"));
    assert_eq!(made.get_text("machine"), Some("native"));
    // in process on native, progress is a push's (#536 §10b): no `send`,
    // null counts, nothing but `phase`, `done` and `total`
    let expected: Vec<SdkValue> = ["check", "install", "record"]
        .iter()
        .map(|phase| {
            let progress = SdkValue::map([
                ("phase", t(phase)),
                ("done", SdkValue::Null),
                ("total", SdkValue::Null),
            ]);
            SdkValue::map([("progress", progress)])
        })
        .collect();
    assert_eq!(events, expected);

    let list = ok(&home, "project.list", &[]);
    let SdkValue::List(items) = list.get("items").unwrap() else {
        panic!()
    };
    assert_eq!(items.len(), 1);
    ok(&home, "project.get", &[("project", t("shop/api"))]);

    // project.list pages with `has_more` (#611)
    let web = tempdir("web");
    std::fs::write(web.join("index.html"), "<h1>shop</h1>\n").unwrap();
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/web")),
            ("folder", t(&web.display().to_string())),
        ],
    );
    let page = ok(&home, "project.list", &[("page_size", SdkValue::Int(1))]);
    let SdkValue::List(one) = page.get("items").unwrap() else {
        panic!("{page:?}")
    };
    assert_eq!(one.len(), 1);
    assert_eq!(page.get("has_more"), Some(&SdkValue::Bool(true)));
    assert_eq!(page.get("next_page_pos"), Some(&SdkValue::Int(1)));
    let page = ok(
        &home,
        "project.list",
        &[
            ("page_size", SdkValue::Int(1)),
            ("page_pos", SdkValue::Int(1)),
        ],
    );
    let SdkValue::List(two) = page.get("items").unwrap() else {
        panic!("{page:?}")
    };
    assert_eq!(two.len(), 1);
    assert_eq!(page.get("has_more"), Some(&SdkValue::Bool(false)));
    assert_eq!(page.get("next_page_pos"), Some(&SdkValue::Null));
    assert_eq!(
        [one[0].get_text("name"), two[0].get_text("name")],
        [Some("api"), Some("web")]
    );
    let _ = std::fs::remove_dir_all(web);

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
        layerfs_api::json::print(&diff).contains("NEW.md"),
        "{diff:?}"
    );

    // main lists its published Layers, where a Layer id stands for a Commit
    // (S §3.3); a short identity is an identity fault, not a Branch fault
    // (#611). The publish below advances main to fix's head before fix is
    // edited again.
    let main = ok(&home, "commit.list", &[p(), ("branch", t("main"))]);
    let SdkValue::List(layers) = main.get("items").unwrap() else {
        panic!("{main:?}")
    };
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].get("parent"), Some(&SdkValue::Null));
    let base_layer = layers[0].get_text("id").unwrap().to_owned();
    ok(&home, "branch.publish", &[p(), ("branch", t("fix"))]);
    let main = ok(&home, "commit.list", &[p(), ("branch", t("main"))]);
    let SdkValue::List(layers) = main.get("items").unwrap() else {
        panic!("{main:?}")
    };
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].get_text("source_commit"), Some(id.as_str()));
    assert_eq!(layers[0].get_text("source_branch"), Some("fix"));
    let short: String = id.chars().take(12).collect();
    assert_eq!(
        call(
            &home,
            "commit.diff",
            &[
                p(),
                ("from_commit", t(&short)),
                ("to_commit", t(&base_layer))
            ]
        )
        .unwrap_err()
        .0,
        "InvalidInput"
    );
    ok(
        &home,
        "commit.diff",
        &[
            p(),
            ("from_commit", t(&base_layer)),
            ("to_commit", t(&base_layer)),
        ],
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
    assert!(layerfs_api::json::print(&branches).contains("\"fix\""));
    ok(&home, "branch.get", &[p(), ("branch", t("fix"))]);

    let export_dir = tempdir("export").join("out");
    let exported = ok(
        &home,
        "project.export",
        &[
            p(),
            ("destination", t(&export_dir.display().to_string())),
            ("branch", t("fix")),
        ],
    );
    assert_eq!(exported.get_text("project"), Some("shop/api"));
    assert_eq!(exported.get_text("branch"), Some("fix"));
    assert_eq!(exported.get_int("files"), Some(3));
    assert_eq!(exported.get_int("directories"), Some(1));
    assert_eq!(
        std::fs::read_to_string(export_dir.join("NEW.md")).unwrap(),
        "newer\n"
    );
    let _ = std::fs::remove_dir_all(export_dir.parent().unwrap());

    // the Store's open service is closed before a push: a lease of this
    // process would not block it, but the open file must be closed first
    std::thread::sleep(std::time::Duration::from_millis(10));
    ok(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("box"))],
    );
    let read = ok(
        &home,
        "file.read",
        &[p(), ("branch", t("fix")), ("path", t("/NEW.md"))],
    );
    assert_eq!(content(&read), "newer\n", "read through the worker");

    // `project.delete` retires the project in the Store under `box` and
    // removes both entries; its history stays until a compaction.
    let deleted = ok(&home, "project.delete", &[p()]);
    assert_eq!(deleted.get_text("kind"), Some("Removed"));
    assert_eq!(deleted.get_text("project"), Some("shop/api"));
    assert_eq!(
        deleted.get("home_entry_removed"),
        Some(&SdkValue::Bool(true))
    );
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

fn refused(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkRefusal {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(home), name, args, &mut |_| {}).expect_err("the call is refused")
}

fn private_home(tag: &str) -> PathBuf {
    let home = tempdir(tag);
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    home
}

/// A `project.init` name collision is the typed `NameTaken`: the home record
/// first, and the Store's own holder when the home has no record for it.
#[test]
fn project_init_name_collision_reports_holder_and_source() {
    let home = private_home("name-taken");
    let folder = tempdir("name-taken-folder");
    std::fs::write(folder.join("README"), b"held\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );

    let home_taken = refused(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    assert_eq!(home_taken.code, "NameTaken", "{}", home_taken.message);
    let holder = home_taken.detail.get("holder").expect("holder detail");
    assert_eq!(holder.get_text("kind"), Some("project"));
    assert_eq!(holder.get_text("name"), Some("api"));
    assert_eq!(holder.get_text("state"), Some("live"));
    assert_eq!(home_taken.detail.get_text("source"), Some("home"));

    // A home that no longer records the project meets the Store's own holder.
    std::fs::remove_dir_all(paths::project_dir(&home, "shop", "api")).unwrap();
    let store_taken = refused(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    assert_eq!(store_taken.code, "NameTaken", "{}", store_taken.message);
    let holder = store_taken.detail.get("holder").expect("holder detail");
    assert_eq!(holder.get_text("kind"), Some("project"));
    assert_eq!(holder.get_text("name"), Some("api"));
    assert_eq!(holder.get_text("state"), Some("live"));
    assert_eq!(store_taken.detail.get_text("source"), Some("store"));

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

/// A Layer keeps the name of the Branch it was published from after that
/// Branch is retired: provenance rendering is not a caller-facing lookup.
#[test]
fn a_retired_source_branch_still_renders_on_its_layers() {
    let home = private_home("retired-source");
    let folder = tempdir("retired-source-folder");
    std::fs::write(folder.join("base.txt"), b"base\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    ok(
        &home,
        "branch.fork",
        &[
            ("project", t("shop/api")),
            ("name", t("work")),
            ("from_branch", t("main")),
        ],
    );
    ok(
        &home,
        "file.write",
        &[
            ("project", t("shop/api")),
            ("branch", t("work")),
            ("path", t("/new.txt")),
            ("content", t("new\n")),
            ("message", t("new")),
        ],
    );
    ok(
        &home,
        "branch.publish",
        &[("project", t("shop/api")), ("branch", t("work"))],
    );

    let main = ok(
        &home,
        "commit.list",
        &[("project", t("shop/api")), ("branch", t("main"))],
    );
    let SdkValue::List(layers) = main.get("items").expect("items") else {
        panic!("{main:?}")
    };
    let id = layers[0].get_text("id").expect("layer id").to_owned();
    assert_eq!(layers[0].get_text("source_branch"), Some("work"), "{main:?}");

    ok(
        &home,
        "branch.remove",
        &[("project", t("shop/api")), ("branch", t("work"))],
    );

    // `layer.list` still renders the provenance ...
    let main = ok(
        &home,
        "commit.list",
        &[("project", t("shop/api")), ("branch", t("main"))],
    );
    let SdkValue::List(layers) = main.get("items").expect("items") else {
        panic!("{main:?}")
    };
    assert_eq!(
        layers[0].get_text("source_branch"),
        Some("work"),
        "layer.list: {main:?}"
    );
    // ... and so does `layer.get` ...
    let got = ok(
        &home,
        "commit.get",
        &[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("commit", t(&id)),
        ],
    );
    assert_eq!(
        got.get_text("source_branch"),
        Some("work"),
        "layer.get: {got:?}"
    );
    // ... while a caller-facing lookup of the retired Branch stays refused.
    let (code, message) = call(
        &home,
        "branch.get",
        &[("project", t("shop/api")), ("branch", t("work"))],
    )
    .unwrap_err();
    assert_eq!(code, "NoSuchBranch", "{message}");

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}
