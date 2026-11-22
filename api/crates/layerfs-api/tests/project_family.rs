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
        &SdkValue::map([
            ("name", t("box")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-00000000-box")),
        ]),
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
        ("detail", SdkValue::Bool(true)),
    ]);
    let made = layerfs_api::call(Some(&home), "project.init", args, &mut |e| events.push(e))
        .unwrap_or_else(|e| panic!("init: {} {}", e.code, e.message));
    assert_eq!(made.get_text("project"), Some("shop/api"));
    assert_eq!(made.get_text("machine"), Some("native"));
    assert_eq!(made.get_text("outcome"), Some("created"));
    // `commit` is main's first Layer, its full id
    assert_eq!(made.get_text("commit").map(str::len), Some(66), "{made:?}");
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

    // A named Store the home does not record is `NoSuchStore`, the same code
    // every other project call reports (spec §4.2).
    let refused = call(&home, "project.list", &[("store", t("nope"))]).unwrap_err();
    assert_eq!(refused.0, "NotFound", "{refused:?}");
    // project.list pages by cursor (call surface 2.0)
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
    let page = ok(&home, "project.list", &[("limit", SdkValue::Int(1))]);
    let SdkValue::List(one) = page.get("items").unwrap() else {
        panic!("{page:?}")
    };
    assert_eq!(one.len(), 1);
    let next = page.get("next_cursor").cloned().expect("a next page");
    let page = ok(
        &home,
        "project.list",
        &[("limit", SdkValue::Int(1)), ("cursor", next)],
    );
    let SdkValue::List(two) = page.get("items").unwrap() else {
        panic!("{page:?}")
    };
    assert_eq!(two.len(), 1);
    assert_eq!(page.get("next_cursor"), None, "the last page");
    assert_eq!(
        [one[0].get_text("project"), two[0].get_text("project")],
        [Some("shop/api"), Some("shop/web")]
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
        &[p(), ("name", t("fix")), ("from", t("main"))],
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
    let id = commits[0].get_text("commit").unwrap().to_owned();
    assert_eq!(commits[0].get_text("message"), Some("add NEW"));
    let detail = || ("detail", SdkValue::Bool(true));
    let got = ok(&home, "commit.get", &[p(), ("commit", t(&id)), detail()]);
    assert_eq!(got.get_text("commit"), Some(id.as_str()));
    assert_eq!(got.get_text("branch"), Some("fix"));
    // fix compared from its fork point by default
    let diff = ok(&home, "commit.diff", &[p(), ("ref", t("fix"))]);
    assert!(
        layerfs_api::json::print(&diff).contains("/NEW.md"),
        "{diff:?}"
    );

    // main lists its published Layers, where a Layer id stands for a Commit
    // (S §3.3); a too-short identity is no Commit (#611). The publish
    // below advances main to fix's head before fix is edited again.
    let main = ok(
        &home,
        "commit.list",
        &[p(), ("branch", t("main")), detail()],
    );
    let SdkValue::List(layers) = main.get("items").unwrap() else {
        panic!("{main:?}")
    };
    assert_eq!(layers.len(), 1);
    assert_eq!(layers[0].get("parent"), Some(&SdkValue::Null));
    assert_eq!(layers[0].get_text("via"), Some("init"));
    let base_layer = layers[0].get_text("commit").unwrap().to_owned();
    ok(&home, "branch.publish", &[p(), ("branch", t("fix"))]);
    let main = ok(
        &home,
        "commit.list",
        &[p(), ("branch", t("main")), detail()],
    );
    let SdkValue::List(layers) = main.get("items").unwrap() else {
        panic!("{main:?}")
    };
    assert_eq!(layers.len(), 2);
    assert_eq!(layers[0].get_text("source_commit"), Some(id.as_str()));
    assert_eq!(layers[0].get_text("source_branch"), Some("fix"));
    assert_eq!(layers[0].get_text("via"), Some("publish"));
    // seven characters are too few for a Commit: `commit.get` refuses them
    // as input, and `commit.diff` reads them as a Branch name, which is absent
    let short: String = id.chars().take(7).collect();
    assert_eq!(
        call(&home, "commit.get", &[p(), ("commit", t(&short))])
            .unwrap_err()
            .0,
        "InvalidInput"
    );
    assert_eq!(
        call(&home, "commit.diff", &[p(), ("ref", t(&short))])
            .unwrap_err()
            .0,
        "NotFound"
    );
    // a Commit by a prefix, from its parent (here main's first publication)
    let diff = ok(
        &home,
        "commit.diff",
        &[p(), ("ref", t(&id[..12])), detail()],
    );
    assert_eq!(diff.get_text("from"), Some(base_layer.as_str()), "{diff:?}");
    // main's first publication, from the empty tree
    let first = ok(
        &home,
        "commit.diff",
        &[p(), ("ref", t(&base_layer)), detail()],
    );
    assert_eq!(first.get("from"), Some(&SdkValue::Null), "{first:?}");
    assert!(layerfs_api::json::print(&first).contains("/README.md"));

    ok(
        &home,
        "file.edit",
        &[
            p(),
            ("branch", t("fix")),
            ("path", t("/NEW.md")),
            ("old_string", t("new")),
            ("new_string", t("newer")),
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
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(exported.get_int("file_count"), Some(3));
    assert_eq!(exported.get_int("dir_count"), Some(1));
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
        "store.move",
        &[("store", t("shop")), ("machine", t("box"))],
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
    assert_eq!(deleted, SdkValue::map([("ok", SdkValue::Bool(true))]));
    assert_eq!(
        call(&home, "project.get", &[p()]).unwrap_err().0,
        "NotFound"
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
        assert_eq!(code, "Unavailable", "{open:o}: {message}");
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
    assert_eq!(home_taken.code, "Exists", "{}", home_taken.message);
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
    assert_eq!(store_taken.code, "Exists", "{}", store_taken.message);
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
            ("from", t("main")),
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
    let id = layers[0].get_text("commit").expect("layer id").to_owned();
    assert_eq!(
        layers[0].get_text("source_branch"),
        Some("work"),
        "{main:?}"
    );

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
            ("detail", SdkValue::Bool(true)),
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
    assert_eq!(code, "NotFound", "{message}");

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(folder);
}

fn worker_box_home(tag: &str) -> (PathBuf, PathBuf) {
    let home = tempdir(tag);
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    ok(&home, "home.init", &[]);
    write_record(
        &paths::machine_record(&home, "native"),
        &SdkValue::map([("name", t("native")), ("platform", t("native"))]),
    )
    .unwrap();
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([
            ("name", t("box")),
            ("platform", t("lima")),
            ("vm_name", t("layerfs-00000000-box")),
        ]),
    )
    .unwrap();
    let root = tempdir(&format!("{tag}-guest"));
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
    (home, root)
}

#[test]
fn worker_inited_project_keeps_manifest_across_move_to_native_and_back() {
    let (home, root) = worker_box_home("worker-manifest");
    let folder = tempdir("worker-manifest-folder");
    std::fs::write(folder.join("README.md"), "hello\n").unwrap();

    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "store.move",
        &[("store", t("shop")), ("machine", t("box"))],
    );
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );

    let host_record = layerfs_api::home::read_record(&paths::project_record(&home, "shop", "api"))
        .unwrap()
        .unwrap();
    assert!(
        host_record.get("manifest").is_some() && host_record.get("installed").is_some(),
        "host record after worker init must keep manifest and installed: {host_record:?}"
    );
    layerfs_store::project::discovery::project(Some(&home), Some("shop"), Some("api"))
        .expect("host project discovery succeeds right after worker init");

    ok(
        &home,
        "store.move",
        &[("store", t("shop")), ("machine", t("native"))],
    );
    layerfs_store::project::discovery::project(Some(&home), Some("shop"), Some("api"))
        .expect("host project discovery succeeds after store move to native");
    let doc = ok(
        &home,
        "project.doctor",
        &[
            ("project", t("shop/api")),
            ("full", SdkValue::Bool(true)),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(doc.get("ok"), Some(&SdkValue::Bool(true)), "{doc:?}");

    ok(
        &home,
        "store.move",
        &[("store", t("shop")), ("machine", t("box"))],
    );
    layerfs_store::project::discovery::project(Some(&home), Some("shop"), Some("api"))
        .expect("host project discovery succeeds after moving back to worker");

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn move_and_sandbox_repair_an_existing_incomplete_project_record() {
    let (home, root) = worker_box_home("repair-incomplete");
    let folder = tempdir("repair-incomplete-folder");
    std::fs::write(folder.join("README.md"), "legacy\n").unwrap();

    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "store.move",
        &[("store", t("shop")), ("machine", t("box"))],
    );
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );

    let record_path = paths::project_record(&home, "shop", "api");
    let full = layerfs_api::home::read_record(&record_path)
        .unwrap()
        .unwrap();
    let stack = full
        .get_text("stack")
        .map(str::to_owned)
        .unwrap_or_else(|| {
            let SdkValue::Bytes(bytes) = full.get("manifest").unwrap() else {
                panic!()
            };
            let m = layerfs_bridge::provision::StoreManifest::decode(bytes).unwrap();
            layerfs_bridge::sdk_values::hex(&m.stack)
        });
    let write_stub = |created: i64| {
        write_record(
            &record_path,
            &SdkValue::map([
                ("v", SdkValue::Int(1)),
                ("name", t("api")),
                ("stack", t(&stack)),
                ("created", SdkValue::Int(created)),
            ]),
        )
        .unwrap();
    };

    // Case 1: Store is on worker with a pre-fix 88-byte host record; move to native repairs it.
    write_stub(12345);
    assert_eq!(
        layerfs_store::project::discovery::project(Some(&home), Some("shop"), Some("api"))
            .err()
            .map(|r| r.code),
        Some("NoSuchProject".to_owned())
    );
    let doc_before = ok(
        &home,
        "project.doctor",
        &[
            ("project", t("shop/api")),
            ("full", SdkValue::Bool(true)),
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(
        doc_before.get("ok"),
        Some(&SdkValue::Bool(false)),
        "{doc_before:?}"
    );

    ok(
        &home,
        "store.move",
        &[("store", t("shop")), ("machine", t("native"))],
    );
    layerfs_store::project::discovery::project(Some(&home), Some("shop"), Some("api"))
        .expect("store move to native repairs incomplete host project record");
    let repaired = layerfs_api::home::read_record(&record_path)
        .unwrap()
        .unwrap();
    assert_eq!(
        repaired.get_int("created"),
        Some(12345),
        "created timestamp preserved"
    );

    // Case 2: Incomplete host record on native when moving back to worker is also repaired.
    write_stub(24680);
    assert_eq!(
        layerfs_store::project::discovery::project(Some(&home), Some("shop"), Some("api"))
            .err()
            .map(|r| r.code),
        Some("NoSuchProject".to_owned())
    );
    ok(
        &home,
        "store.move",
        &[("store", t("shop")), ("machine", t("box"))],
    );
    layerfs_store::project::discovery::project(Some(&home), Some("shop"), Some("api"))
        .expect("store move from native repairs incomplete host project record");
    let repaired2 = layerfs_api::home::read_record(&record_path)
        .unwrap()
        .unwrap();
    assert_eq!(repaired2.get_int("created"), Some(24680));

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn move_skips_unreadable_project_record_and_still_repairs_incomplete_ones() {
    let (home, root) = worker_box_home("unreadable-record");
    let folder = tempdir("unreadable-record-folder");
    std::fs::write(folder.join("hello.txt"), "hello\n").unwrap();

    ok(&home, "store.create", &[("name", t("shop"))]);
    for name in ["good", "damaged", "legacy"] {
        ok(
            &home,
            "project.init",
            &[
                ("project", t(&format!("shop/{name}"))),
                ("folder", t(&folder.display().to_string())),
            ],
        );
    }

    let legacy_path = paths::project_record(&home, "shop", "legacy");
    let legacy_full = layerfs_api::home::read_record(&legacy_path)
        .unwrap()
        .unwrap();
    let SdkValue::Bytes(bytes) = legacy_full.get("manifest").unwrap() else {
        panic!()
    };
    let stack = layerfs_bridge::sdk_values::hex(
        &layerfs_bridge::provision::StoreManifest::decode(bytes)
            .unwrap()
            .stack,
    );
    let write_legacy_stub = |created: i64| {
        write_record(
            &legacy_path,
            &SdkValue::map([
                ("v", SdkValue::Int(1)),
                ("name", t("legacy")),
                ("stack", t(&stack)),
                ("created", SdkValue::Int(created)),
            ]),
        )
        .unwrap();
    };

    let damaged_path = paths::project_record(&home, "shop", "damaged");
    std::fs::write(&damaged_path, b"{").unwrap();
    write_legacy_stub(11111);
    // a record of a stack the Store does not hold cannot be rebuilt: the
    // move still ends, and says so in log/repair.jsonl
    let orphan_path = paths::project_record(&home, "shop", "orphan");
    home::private_dir(orphan_path.parent().unwrap()).unwrap();
    write_record(
        &orphan_path,
        &SdkValue::map([
            ("v", SdkValue::Int(1)),
            ("name", t("orphan")),
            ("stack", t("0101010101010101010101010101010101")),
            ("created", SdkValue::Int(1)),
        ]),
    )
    .unwrap();

    ok(
        &home,
        "store.move",
        &[("store", t("shop")), ("machine", t("box"))],
    );
    let repair_log = std::fs::read_to_string(paths::log_dir(&home).join("repair.jsonl")).unwrap();
    assert!(
        repair_log
            .lines()
            .any(|line| line.contains("\"project\":\"orphan\"")
                && line.contains("\"call\":\"store.move\"")),
        "{repair_log}"
    );
    assert!(!repair_log.contains("\"legacy\""), "{repair_log}");
    let store_rec = layerfs_api::home::read_record(&paths::store_record(&home, "shop"))
        .unwrap()
        .unwrap();
    assert_eq!(store_rec.get_text("machine"), Some("box"));
    assert_eq!(store_rec.get("op"), None);
    assert_eq!(std::fs::read(&damaged_path).unwrap(), b"{");
    let read = ok(
        &home,
        "file.read",
        &[
            ("project", t("shop/good")),
            ("branch", t("main")),
            ("path", t("/hello.txt")),
        ],
    );
    assert_eq!(content(&read), "hello\n");
    layerfs_store::project::discovery::project(Some(&home), Some("shop"), Some("legacy"))
        .expect("legacy project repaired on move to worker despite damaged sibling");
    assert_eq!(
        layerfs_api::home::read_record(&legacy_path)
            .unwrap()
            .unwrap()
            .get_int("created"),
        Some(11111)
    );

    // Moving back to native with damaged sibling still unreadable also succeeds and repairs legacy.
    write_legacy_stub(22222);
    ok(
        &home,
        "store.move",
        &[("store", t("shop")), ("machine", t("native"))],
    );
    let store_rec = layerfs_api::home::read_record(&paths::store_record(&home, "shop"))
        .unwrap()
        .unwrap();
    assert_eq!(store_rec.get_text("machine"), Some("native"));
    assert_eq!(store_rec.get("op"), None);
    assert_eq!(std::fs::read(&damaged_path).unwrap(), b"{");
    let read = ok(
        &home,
        "file.read",
        &[
            ("project", t("shop/good")),
            ("branch", t("main")),
            ("path", t("/hello.txt")),
        ],
    );
    assert_eq!(content(&read), "hello\n");
    layerfs_store::project::discovery::project(Some(&home), Some("shop"), Some("legacy"))
        .expect("legacy project repaired on move to native despite damaged sibling");
    assert_eq!(
        layerfs_api::home::read_record(&legacy_path)
            .unwrap()
            .unwrap()
            .get_int("created"),
        Some(22222)
    );

    std::fs::remove_dir_all(home).unwrap();
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(folder);
}
