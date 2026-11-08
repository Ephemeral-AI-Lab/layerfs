//! Backups (#533 step 11) against a real MinIO: connect, create, the list
//! states, the restore Conflict, a restore as a new Store, a backup of
//! a Store behind a worker, and disconnect keeping the objects. Needs the
//! LAYERFS_REMOTE_* variables tests/minio_service.sh of layerfs-bucket
//! prints; without them the test says NOT_RUN and passes.
#![cfg(unix)] // unix stat/lock semantics; the Windows face is its own port

use layerfs_api::home::{self, paths, read_record, write_record};

use layerfs_bridge::control::SdkValue;
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-backup-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
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

/// `(file, state)` of every listed copy, sorted.
fn states(home: &Path) -> Vec<(String, String)> {
    let listed = ok(home, "backup.list", &[]);
    let SdkValue::List(items) = listed.get("items").unwrap() else {
        panic!()
    };
    let mut out: Vec<_> = items
        .iter()
        .map(|i| {
            (
                i.get_text("file").unwrap().to_owned(),
                i.get_text("state").unwrap().to_owned(),
            )
        })
        .collect();
    out.sort();
    out
}

fn pair(file: &str, state: &str) -> (String, String) {
    (file.to_owned(), state.to_owned())
}

#[test]
fn backups_against_minio() {
    let (Ok(endpoint), Ok(bucket)) = (
        std::env::var("LAYERFS_REMOTE_ENDPOINT"),
        std::env::var("LAYERFS_REMOTE_BUCKET"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_REMOTE_ENDPOINT / LAYERFS_REMOTE_BUCKET unset");
        return;
    };
    let home = tempdir("home");
    home::open(&home).unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("README.md"), "backed up\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );

    let prefix = format!("lfs533-{}", std::process::id());
    let url = format!("{endpoint}/{bucket}/{prefix}");
    let key = ("key", t("env:LAYERFS_REMOTE"));
    // a key that does not resolve is refused before anything is recorded
    let bad = call(
        &home,
        "backup.connect",
        &[
            ("name", t("b1")),
            ("url", t(&url)),
            ("key", t("env:LAYERFS_NOPE")),
        ],
    );
    assert!(bad.is_err());
    assert!(!paths::bucket_record(&home, "b1").exists());
    ok(
        &home,
        "backup.connect",
        &[("name", t("b1")), ("url", t(&url)), key.clone()],
    );
    assert_eq!(states(&home), vec![]);

    let made = ok(
        &home,
        "backup.create",
        &[("store", t("shop")), ("bucket", t("b1"))],
    );
    assert_eq!(made.get_text("file"), Some("shop"));
    assert_eq!(states(&home), vec![pair("shop", "current")]);

    // the Store changes: list says so, and a restore over it is refused
    ok(
        &home,
        "branch.fork",
        &[
            ("project", t("shop/api")),
            ("name", t("dev")),
            ("from_branch", t("main")),
        ],
    );
    ok(
        &home,
        "file.write",
        &[
            ("project", t("shop/api")),
            ("branch", t("dev")),
            ("path", t("/NEW")),
            ("content", t("x")),
        ],
    );
    assert_eq!(states(&home), vec![pair("shop", "store_changed")]);
    let refused = call(
        &home,
        "backup.restore",
        &[("store", t("shop")), ("bucket", t("b1"))],
    );
    assert_eq!(refused.unwrap_err().0, "Conflict");
    ok(
        &home,
        "backup.create",
        &[("store", t("shop")), ("bucket", t("b1"))],
    );
    assert_eq!(states(&home), vec![pair("shop", "current")]);

    // a restore as a new Store on native, with its project records
    let restored = ok(
        &home,
        "backup.restore",
        &[
            ("store", t("copy")),
            ("bucket", t("b1")),
            ("file", t("shop")),
        ],
    );
    assert_eq!(
        restored.get("projects"),
        Some(&SdkValue::List(vec![t("api")]))
    );
    let read = ok(
        &home,
        "file.read",
        &[
            ("project", t("copy/api")),
            ("branch", t("dev")),
            ("path", t("/NEW")),
        ],
    );
    assert_eq!(read.get_text("text"), Some("x"));
    // the unchanged Store restores over itself
    ok(
        &home,
        "backup.restore",
        &[("store", t("copy")), ("bucket", t("b1"))],
    );

    // a Store behind a worker backs up: the bytes come through this process
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
    ok(
        &home,
        "store.push",
        &[("name", t("shop")), ("machine", t("box"))],
    );
    ok(
        &home,
        "backup.create",
        &[
            ("store", t("shop")),
            ("bucket", t("b1")),
            ("file", t("from-box")),
        ],
    );

    // the four states: current, store_changed (copy), unlinked (shop's old
    // file, now that shop links from-box), missing (a link to a gone file)
    ok(
        &home,
        "file.write",
        &[
            ("project", t("copy/api")),
            ("branch", t("dev")),
            ("path", t("/NEW2")),
            ("content", t("y")),
        ],
    );
    let path = paths::store_record(&home, "copy");
    let record = read_record(&path).unwrap().unwrap();
    let mut links = record.get("backups").cloned().unwrap();
    links = home::with_field(links, "b1", SdkValue::map([("file", t("gone"))]));
    write_record(&path, &home::with_field(record.clone(), "backups", links)).unwrap();
    // shop is on box now: its heads are not read here, so its state is
    // unknown, with the reason, never current
    assert_eq!(
        states(&home),
        vec![
            pair("from-box", "unknown"),
            pair("gone", "missing"),
            pair("shop", "unlinked")
        ]
    );
    let listed = ok(&home, "backup.list", &[]);
    let SdkValue::List(items) = listed.get("items").unwrap() else {
        panic!()
    };
    let boxed = items
        .iter()
        .find(|i| i.get_text("file") == Some("from-box"))
        .unwrap();
    assert!(
        boxed.get_text("reason").unwrap().contains("on box"),
        "{boxed:?}"
    );
    write_record(&path, &record).unwrap();
    assert!(states(&home).contains(&pair("shop", "store_changed")));

    // disconnect forgets the bucket and the links; the objects stay
    ok(&home, "backup.disconnect", &[("name", t("b1"))]);
    let shop = read_record(&paths::store_record(&home, "shop"))
        .unwrap()
        .unwrap();
    assert_eq!(shop.get("backups"), Some(&SdkValue::Map(vec![])));
    ok(
        &home,
        "backup.connect",
        &[("name", t("b1")), ("url", t(&url)), key],
    );
    assert_eq!(states(&home).len(), 2, "objects stayed");

    for dir in [home, folder, root] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// A home with Store `shop` (project `api`) backed up to bucket `b1` under a
/// prefix of its own, or `None` when no MinIO is configured.
fn backed_up(tag: &str) -> Option<(PathBuf, PathBuf, String)> {
    let (Ok(endpoint), Ok(bucket)) = (
        std::env::var("LAYERFS_REMOTE_ENDPOINT"),
        std::env::var("LAYERFS_REMOTE_BUCKET"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_REMOTE_ENDPOINT / LAYERFS_REMOTE_BUCKET unset");
        return None;
    };
    let home = tempdir(&format!("{tag}-home"));
    home::open(&home).unwrap();
    let folder = tempdir(&format!("{tag}-folder"));
    std::fs::write(folder.join("README.md"), "backed up\n").unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/api")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    let url = format!("{endpoint}/{bucket}/lfs533-{tag}-{}", std::process::id());
    ok(
        &home,
        "backup.connect",
        &[
            ("name", t("b1")),
            ("url", t(&url)),
            ("key", t("env:LAYERFS_REMOTE")),
        ],
    );
    ok(
        &home,
        "backup.create",
        &[("store", t("shop")), ("bucket", t("b1"))],
    );
    Some((home, folder, url))
}

/// Two restores under the same new name at once (#533 review P0-8): both
/// download, the first to install wins and the other is `Exists`, re-checked
/// under the home lock; neither overwrites the other or leaves its download.
#[test]
fn concurrent_restores_under_a_new_name() {
    let Some((home, folder, _)) = backed_up("twice") else {
        return;
    };
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let home = home.clone();
            std::thread::spawn(move || {
                call(
                    &home,
                    "backup.restore",
                    &[
                        ("store", t("dup")),
                        ("bucket", t("b1")),
                        ("file", t("shop")),
                    ],
                )
            })
        })
        .collect();
    let mut outcomes: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    outcomes.sort_by_key(|o| o.is_err());
    assert!(outcomes[0].is_ok(), "{outcomes:?}");
    assert_eq!(
        outcomes[1].as_ref().unwrap_err().0,
        "Exists",
        "{outcomes:?}"
    );
    let read = ok(
        &home,
        "file.read",
        &[
            ("project", t("dup/api")),
            ("branch", t("main")),
            ("path", t("/README.md")),
        ],
    );
    assert_eq!(read.get_text("text"), Some("backed up\n"));
    let hidden: Vec<_> = std::fs::read_dir(home.join("store"))
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert!(hidden.is_empty(), "{hidden:?}");
    for dir in [home, folder] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// `layerfs-exported-from` (#533 review P1-14): a create names this home and
/// the Store's machine on the object; another home's create to the same
/// file is `Exists`, and nothing is uploaded. A link that records no sha256
/// of its last copy expects the file absent, so it never overwrites one.
#[test]
fn foreign_copies_and_unrecorded_links() {
    let Some((home, folder, url)) = backed_up("foreign") else {
        return;
    };
    let id = home::ensure(&home).unwrap().id;
    let other = tempdir("foreign-other");
    home::open(&other).unwrap();
    ok(&other, "store.create", &[("name", t("shop"))]);
    ok(
        &other,
        "backup.connect",
        &[
            ("name", t("b1")),
            ("url", t(&url)),
            ("key", t("env:LAYERFS_REMOTE")),
        ],
    );
    let refused = call(
        &other,
        "backup.create",
        &[("store", t("shop")), ("bucket", t("b1"))],
    )
    .unwrap_err();
    assert_eq!(refused.0, "Exists", "{refused:?}");
    assert!(refused.1.contains(&format!("{id}/native")), "{refused:?}");
    // the object is still this home's copy
    assert_eq!(states(&home), vec![pair("shop", "current")]);
    // another file is the other home's own
    ok(
        &other,
        "backup.create",
        &[
            ("store", t("shop")),
            ("bucket", t("b1")),
            ("file", t("other-shop")),
        ],
    );

    // a link without its last copy's sha256 does not overwrite the file
    let path = paths::store_record(&home, "shop");
    let record = read_record(&path).unwrap().unwrap();
    let bare = SdkValue::map([("b1", SdkValue::map([("file", t("shop"))]))]);
    write_record(&path, &home::with_field(record, "backups", bare)).unwrap();
    let refused = call(
        &home,
        "backup.create",
        &[("store", t("shop")), ("bucket", t("b1"))],
    )
    .unwrap_err();
    assert_eq!(refused.0, "Conflict", "{refused:?}");

    for dir in [home, folder, other] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// `(file, store, state)` of every listed item, sorted.
fn linked_states(home: &Path) -> Vec<(String, String, String)> {
    let listed = ok(home, "backup.list", &[]);
    let SdkValue::List(items) = listed.get("items").unwrap() else {
        panic!()
    };
    let mut out: Vec<_> = items
        .iter()
        .map(|i| {
            (
                i.get_text("file").unwrap().to_owned(),
                i.get_text("store").unwrap_or("-").to_owned(),
                i.get_text("state").unwrap().to_owned(),
            )
        })
        .collect();
    out.sort();
    out
}

fn triple(file: &str, store: &str, state: &str) -> (String, String, String) {
    (file.to_owned(), store.to_owned(), state.to_owned())
}

fn write(home: &Path, project: &str, path: &str) {
    ok(
        home,
        "file.write",
        &[
            ("project", t(project)),
            ("branch", t("dev")),
            ("path", t(path)),
            ("content", t(path)),
        ],
    );
}

/// The list states (#533 review P1-14): the Store's heads and the bucket
/// copy's sha256 each against the link's last transfer, one item per
/// linked Store; a link that cannot be compared is `unknown` with a reason.
#[test]
fn list_states_compare_the_store_and_the_copy() {
    let Some((home, folder, _)) = backed_up("states") else {
        return;
    };
    // a writable Branch, backed up
    ok(
        &home,
        "branch.fork",
        &[
            ("project", t("shop/api")),
            ("name", t("dev")),
            ("from_branch", t("main")),
        ],
    );
    ok(
        &home,
        "backup.create",
        &[("store", t("shop")), ("bucket", t("b1"))],
    );
    ok(
        &home,
        "backup.restore",
        &[
            ("store", t("twin")),
            ("bucket", t("b1")),
            ("file", t("shop")),
        ],
    );
    assert_eq!(
        linked_states(&home),
        vec![
            triple("shop", "shop", "current"),
            triple("shop", "twin", "current")
        ]
    );
    // twin writes the shared file: the copy shop linked changed under it
    write(&home, "twin/api", "/T1");
    ok(
        &home,
        "backup.create",
        &[
            ("store", t("twin")),
            ("bucket", t("b1")),
            ("file", t("shop")),
        ],
    );
    assert_eq!(
        linked_states(&home),
        vec![
            triple("shop", "shop", "bucket_changed"),
            triple("shop", "twin", "current")
        ]
    );
    write(&home, "shop/api", "/S1");
    write(&home, "twin/api", "/T2");
    assert_eq!(
        linked_states(&home),
        vec![
            triple("shop", "shop", "both_changed"),
            triple("shop", "twin", "store_changed")
        ]
    );
    // a link without its last copy's sha256 cannot say
    let path = paths::store_record(&home, "twin");
    let record = read_record(&path).unwrap().unwrap();
    let link = record.get("backups").unwrap().get("b1").unwrap().clone();
    let heads = link.get("heads").unwrap().clone();
    let bare = SdkValue::map([("b1", SdkValue::map([("file", t("shop")), ("heads", heads)]))]);
    write_record(&path, &home::with_field(record, "backups", bare)).unwrap();
    let listed = ok(&home, "backup.list", &[]);
    let SdkValue::List(items) = listed.get("items").unwrap() else {
        panic!()
    };
    let twin = items
        .iter()
        .find(|i| i.get_text("store") == Some("twin"))
        .unwrap();
    assert_eq!(twin.get_text("state"), Some("unknown"));
    assert!(
        twin.get_text("reason").unwrap().contains("sha256"),
        "{twin:?}"
    );
    for dir in [home, folder] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
