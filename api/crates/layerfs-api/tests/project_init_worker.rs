//! `project.init` into a sqlite Store on another machine runs in that
//! machine's worker (#536 H11): the worker adds the folder it finds on its
//! machine and the caller records the project.
use layerfs_api::calls;
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("layerfs-init-worker-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn call(
    home: &Path,
    name: &str,
    args: SdkValue,
    events: &mut dyn FnMut(SdkValue),
) -> Result<SdkValue, (String, String)> {
    calls::call(Some(home), name, args, events).map_err(|r| (r.code, r.message))
}

#[test]
fn init_into_a_worker_s_store_runs_in_the_worker() {
    let home = tempdir("home");
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
    home::open(&home).unwrap();
    write_record(
        &paths::machine_record(&home, "box"),
        &SdkValue::map([("name", t("box")), ("platform", t("lima"))]),
    )
    .unwrap();
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

    // a Store on box: made on native, its file placed under the worker's root
    let made = call(
        &home,
        "store.create",
        SdkValue::map([("name", t("shop"))]),
        &mut |_| {},
    );
    made.unwrap_or_else(|e| panic!("store.create: {e:?}"));
    let native = paths::store_dir(&home, "shop");
    let placed = paths::store_dir(&root, "shop");
    std::fs::create_dir_all(&placed).unwrap();
    for entry in std::fs::read_dir(&native).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("store.sqlite") {
            std::fs::rename(entry.path(), placed.join(&name)).unwrap();
        }
    }
    let record_path = paths::store_record(&home, "shop");
    let record = read_record(&record_path).unwrap().unwrap();
    let record = home::with_field(record, "machine", t("box"));
    write_record(&record_path, &record).unwrap();

    // the folder as the worker's machine sees it
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "on box\n").unwrap();
    let args = || {
        SdkValue::map([
            ("project", t("shop/web")),
            ("folder", t(&folder.display().to_string())),
        ])
    };
    // a folder this small ends before the first progress poll
    let added = call(&home, "project.init", args(), &mut |_| {})
        .unwrap_or_else(|e| panic!("project.init: {e:?}"));
    assert_eq!(added.get_text("project"), Some("shop/web"));
    assert_eq!(added.get_text("machine"), Some("box"));
    assert!(added.get_text("root").is_some(), "{added:?}");
    // the caller records it; the worker's root holds the Store's project
    let record = read_record(&paths::project_record(&home, "shop", "web")).unwrap();
    assert!(record.unwrap().get_text("stack").is_some());
    assert!(paths::project_dir(&root, "shop", "web").is_dir());
    assert!(!paths::store_file(&home, "shop").exists());

    let read = call(
        &home,
        "file.read",
        SdkValue::map([
            ("project", t("shop/web")),
            ("branch", t("main")),
            ("path", t("/README.md")),
        ]),
        &mut |_| {},
    )
    .unwrap_or_else(|e| panic!("file.read: {e:?}"));
    assert_eq!(read.get_text("text"), Some("on box\n"));

    let again = call(&home, "project.init", args(), &mut |_| {}).unwrap_err();
    assert_eq!(again.0, "Exists", "{}", again.1);

    let _ = std::fs::remove_dir_all(home);
    let _ = std::fs::remove_dir_all(root);
    let _ = std::fs::remove_dir_all(folder);
}
