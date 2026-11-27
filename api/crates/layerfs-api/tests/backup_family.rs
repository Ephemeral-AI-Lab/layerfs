//! Backups (#533 step 11, call surface 2.0) against a real MinIO: connect
//! with the key's secret kept in the home, create, the list facts
//! (`behind`, `changed`, `missing`), the restore Conflict, a restore as a
//! new Store, a backup of a Store behind a worker (whose heads `list`
//! reads through the worker), a key rotation keeping the links (spec v21)
//! and disconnect keeping the objects. Needs the
//! LAYERFS_REMOTE_* variables tests/minio_service.sh of layerfs-bucket
//! prints; without them the test says NOT_RUN and passes.
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_protocol::provision::install::SdkValue;
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

/// A prefix of this run's own in the bucket: a container's process ids
/// repeat from one run to the next, and the objects stay.
fn run_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
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

/// The key's two options, from the variables minio_service.sh prints.
fn key() -> [(&'static str, SdkValue); 2] {
    let read = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} unset"));
    [
        ("access_key", t(&read("LAYERFS_REMOTE_ACCESS_KEY"))),
        ("secret_key", t(&read("LAYERFS_REMOTE_SECRET_KEY"))),
    ]
}

/// `backup.connect(name, url)` with the key.
fn connect(home: &Path, name: &str, url: &str) -> SdkValue {
    let [access, secret] = key();
    ok(
        home,
        "backup.connect",
        &[("name", t(name)), ("url", t(url)), access, secret],
    )
}

/// One listed row's facts as one word, as this test compares them:
/// `missing`, `unlinked` (no Store), `unknown` (a fact that could not be
/// read), else from `behind` and `changed`.
fn word(item: &SdkValue) -> String {
    let fact = |key: &str| match item.get(key) {
        Some(SdkValue::Bool(yes)) => Some(*yes),
        _ => None,
    };
    let word = if fact("missing") == Some(true) {
        "missing"
    } else if item.get_text("store").is_none() {
        "unlinked"
    } else {
        match (fact("behind"), fact("changed")) {
            (Some(false), Some(false)) => "current",
            (Some(true), Some(false)) => "store_changed",
            (Some(false), Some(true)) => "bucket_changed",
            (Some(true), Some(true)) => "both_changed",
            _ => "unknown",
        }
    };
    word.to_owned()
}

/// The long form of `backup.list`.
fn listed(home: &Path) -> Vec<SdkValue> {
    let args = SdkValue::map([("detail", SdkValue::Bool(true))]);
    let listed = layerfs_api::call(Some(home), "backup.list", args, &mut |_| {})
        .unwrap_or_else(|r| panic!("backup.list: {r:?}"));
    assert_eq!(listed.get("not_listed"), Some(&SdkValue::List(vec![])));
    let Some(SdkValue::List(items)) = listed.get("items") else {
        panic!("{listed:?}")
    };
    items.clone()
}

/// `(file, state)` of every listed copy, sorted.
fn states(home: &Path) -> Vec<(String, String)> {
    let mut out: Vec<_> = listed(home)
        .iter()
        .map(|i| (i.get_text("file").unwrap().to_owned(), word(i)))
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

    let prefix = format!("lfs533-{}", run_id());
    let url = format!("{endpoint}/{bucket}/{prefix}");
    // a key the bucket refuses is Unavailable before anything is recorded
    let [access, _] = key();
    let bad = call(
        &home,
        "backup.connect",
        &[
            ("name", t("b1")),
            ("url", t(&url)),
            access,
            ("secret_key", t("not-the-secret")),
        ],
    );
    assert_eq!(bad.unwrap_err().0, "Unavailable");
    assert!(!paths::bucket_record(&home, "b1").exists());
    assert!(!home.join("secret/bucket/b1.json").exists());
    let connected = connect(&home, "b1", &url);
    assert_eq!(connected, SdkValue::map([("ok", SdkValue::Bool(true))]));
    // the secret is in the home's secret folder, never in the record
    let secret = home.join("secret/bucket/b1.json");
    let mode = std::fs::metadata(&secret).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let record = std::fs::read_to_string(paths::bucket_record(&home, "b1")).unwrap();
    let secret_key = std::env::var("LAYERFS_REMOTE_SECRET_KEY").unwrap();
    assert!(!record.contains(&secret_key), "{record}");
    // a name that is taken at another URL is Exists (spec v21)
    let [access, secret_key] = key();
    let taken = call(
        &home,
        "backup.connect",
        &[
            ("name", t("b1")),
            ("url", t(&format!("{url}-other"))),
            access,
            secret_key,
        ],
    );
    assert_eq!(taken.unwrap_err().0, "Exists");
    assert_eq!(states(&home), vec![]);

    let made = ok(
        &home,
        "backup.create",
        &[("store", t("shop")), ("bucket", t("b1"))],
    );
    assert_eq!(made.get_text("file"), Some("shop"));
    assert_eq!(states(&home), vec![pair("shop", "current")]);
    // connected again at the same URL, the key is replaced and the Store's
    // link stays (spec v21)
    let store_record = std::fs::read(paths::store_record(&home, "shop")).unwrap();
    let [access, secret_key] = key();
    let rotated = ok(
        &home,
        "backup.connect",
        &[
            ("name", t("b1")),
            ("url", t(&url)),
            access,
            secret_key,
            ("detail", SdkValue::Bool(true)),
        ],
    );
    assert_eq!(rotated.get_text("outcome"), Some("rotated"), "{rotated:?}");
    assert_eq!(
        std::fs::read(paths::store_record(&home, "shop")).unwrap(),
        store_record
    );
    assert_eq!(states(&home), vec![pair("shop", "current")]);

    // the Store changes: list says so, and a restore over it is refused
    ok(
        &home,
        "branch.fork",
        &[
            ("project", t("shop/api")),
            ("name", t("dev")),
            ("from", t("main")),
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
        &[
            ("bucket", t("b1")),
            ("file", t("shop")),
            ("store", t("shop")),
        ],
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
    let replaced = layerfs_api::call(
        Some(&home),
        "backup.restore",
        SdkValue::map([
            ("bucket", t("b1")),
            ("file", t("shop")),
            ("store", t("copy")),
            ("detail", SdkValue::Bool(true)),
        ]),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(replaced.get_text("outcome"), Some("replaced"));
    // its `op` was dropped as it ended
    let copy = read_record(&paths::store_record(&home, "copy"))
        .unwrap()
        .unwrap();
    assert!(copy.get("op").is_none(), "{copy:?}");

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
        "store.move",
        &[("store", t("shop")), ("machine", t("box"))],
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
    // a link whose last transfer moved a file the bucket no longer holds (a
    // link that never moved one, a failed first attempt, names nothing)
    let moved = links.get("b1").cloned().unwrap();
    let gone = home::with_field(moved, "file", t("gone"));
    links = home::with_field(links, "b1", gone);
    write_record(&path, &home::with_field(record.clone(), "backups", links)).unwrap();
    // shop is on box now: its heads are read through box's worker, so it
    // is not behind its backup
    assert_eq!(
        states(&home),
        vec![
            pair("from-box", "current"),
            pair("gone", "missing"),
            pair("shop", "unlinked")
        ]
    );
    let items = listed(&home);
    let boxed = items
        .iter()
        .find(|i| i.get_text("file") == Some("from-box"))
        .unwrap();
    assert_eq!(
        boxed.get("behind"),
        Some(&SdkValue::Bool(false)),
        "{boxed:?}"
    );
    assert_eq!(boxed.get("message"), Some(&SdkValue::Null), "{boxed:?}");
    write_record(&path, &record).unwrap();
    assert!(states(&home).contains(&pair("shop", "store_changed")));

    // disconnect forgets the bucket and the links; the objects stay
    let gone = ok(&home, "backup.disconnect", &[("bucket", t("b1"))]);
    let mut unlinked = match gone.get("unlinked") {
        Some(SdkValue::List(names)) => names.clone(),
        other => panic!("{other:?}"),
    };
    unlinked.sort_by_key(|name| format!("{name:?}"));
    assert_eq!(unlinked, vec![t("copy"), t("shop")]);
    assert!(
        !home.join("secret/bucket/b1.json").exists(),
        "its key went too"
    );
    let shop = read_record(&paths::store_record(&home, "shop"))
        .unwrap()
        .unwrap();
    assert_eq!(shop.get("backups"), Some(&SdkValue::Map(vec![])));
    connect(&home, "b1", &url);
    assert_eq!(states(&home).len(), 2, "objects stayed");

    // a Store folder already there at the daemon's own mode is restored into
    // as before (#570 P1): its mode stays, and the Store it takes opens
    let private = paths::store_dir(&home, "kept");
    std::fs::create_dir_all(&private).unwrap();
    std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
    let kept = ok(
        &home,
        "backup.restore",
        &[
            ("store", t("kept")),
            ("bucket", t("b1")),
            ("file", t("shop")),
        ],
    );
    assert_eq!(kept.get("projects"), Some(&SdkValue::List(vec![t("api")])));
    assert!(paths::store_file(&home, "kept").is_file());
    assert_eq!(
        std::fs::metadata(&private).unwrap().permissions().mode() & 0o7777,
        0o700,
        "a Store's own folder stays 0700"
    );
    let read = ok(
        &home,
        "file.read",
        &[
            ("project", t("kept/api")),
            ("branch", t("dev")),
            ("path", t("/NEW")),
        ],
    );
    assert_eq!(read.get_text("text"), Some("x"));

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
    let url = format!("{endpoint}/{bucket}/lfs533-{tag}-{}", run_id());
    connect(&home, "b1", &url);
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
    connect(&other, "b1", &url);
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
    let mut out: Vec<_> = listed(home)
        .iter()
        .map(|i| {
            (
                i.get_text("file").unwrap().to_owned(),
                i.get_text("store").unwrap_or("-").to_owned(),
                word(i),
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

/// The list facts (#533 review P1-14): the Store's heads (`behind`) and the
/// bucket copy's sha256 (`changed`) each against the link's last transfer,
/// one item per linked Store; a fact that cannot be read is null with a
/// `message`.
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
            ("from", t("main")),
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
    let items = listed(&home);
    let twin = items
        .iter()
        .find(|i| i.get_text("store") == Some("twin"))
        .unwrap();
    assert_eq!(word(twin), "unknown");
    assert_eq!(twin.get("changed"), Some(&SdkValue::Null));
    assert!(
        twin.get_text("message").unwrap().contains("sha256"),
        "{twin:?}"
    );
    for dir in [home, folder] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The project a call names, as `sandbox.create` reads it
/// (`layerfs-sandbox`'s `lifecycle::installed`): its entry's manifest and
/// installed manifest.
fn entry(home: &Path, store: &str, project: &str) -> layerfs_store::project::discovery::Project {
    layerfs_store::project::discovery::project(Some(home), Some(store), Some(project))
        .unwrap_or_else(|refusal| panic!("{store}/{project}: {refusal:?}"))
}

/// The project `store` restored into `home`, which must describe the same
/// project as `original`: the manifest a restore rebuilds names the
/// restored Store's own history and authority, and the entry carries the
/// installed manifest `sandbox.create` requires.
fn same_project(
    home: &Path,
    store: &str,
    original: &layerfs_store::project::discovery::Project,
) -> layerfs_store::project::discovery::Project {
    let restored = entry(home, store, "api");
    // the restored Store's own record: the file's binding, and the cursor
    // authority a load gives the new Store
    let record = read_record(&paths::store_record(home, store))
        .unwrap()
        .expect("the restored Store's record");
    let field = |name: &str| match record.get(name) {
        Some(SdkValue::Bytes(bytes)) => bytes.clone(),
        other => panic!("{name}: {other:?}"),
    };
    let (want, got) = (&original.manifest, &restored.manifest);
    assert_eq!(got.binding, want.binding, "binding");
    assert_eq!(got.binding, field("binding"), "the Store's own binding");
    assert_eq!(
        got.cursor_key.to_vec(),
        field("cursor"),
        "the Store's own cursor key"
    );
    assert_eq!(got.stack, want.stack, "stack");
    assert_eq!(got.branch, want.branch, "branch");
    assert_eq!(got.root, want.root, "root");
    assert_eq!(got.profile, want.profile, "profile");
    let installed = restored
        .installed
        .as_ref()
        .expect("a restored project's entry records its installed manifest");
    assert_eq!(installed.stack, want.stack, "installed stack");
    assert_eq!(installed.branch, want.branch, "installed branch");
    assert_eq!(installed.root, want.root, "installed root");
    // the daemon's own checks of the manifest it is handed
    for manifest in [&restored.manifest, installed] {
        manifest.check().expect("a manifest a daemon accepts");
    }
    restored
}

/// #668: every project of a restored Store gets the record `project.init`
/// writes, so `sandbox.create` finds its Store: a restore under a new Store
/// name in the same home, a restore into a new home, and a restore over an
/// existing Store, which keeps the records it has.
#[test]
fn restored_records_are_complete_for_a_sandbox() {
    let Some((home, folder, url)) = backed_up("records") else {
        return;
    };
    let original = entry(&home, "shop", "api");
    // the same home, the backup as a new Store
    ok(
        &home,
        "backup.restore",
        &[
            ("store", t("copy")),
            ("bucket", t("b1")),
            ("file", t("shop")),
        ],
    );
    let copy = same_project(&home, "copy", &original);
    let file = paths::store_file(&home, "copy");
    assert_eq!(
        copy.manifest.bytes,
        std::fs::metadata(&file).unwrap().len(),
        "the restored manifest names its own Store file"
    );
    // a new home, the backup as a new Store
    let other = tempdir("records-home");
    home::open(&other).unwrap();
    connect(&other, "b1", &url);
    ok(
        &other,
        "backup.restore",
        &[
            ("bucket", t("b1")),
            ("file", t("shop")),
            ("store", t("shop")),
        ],
    );
    same_project(&other, "shop", &original);
    let read = ok(
        &other,
        "file.read",
        &[
            ("project", t("shop/api")),
            ("branch", t("main")),
            ("path", t("/README.md")),
        ],
    );
    assert_eq!(read.get_text("text"), Some("backed up\n"));
    // the unchanged Store restores over itself and keeps its record
    let record = paths::project_record(&other, "shop", "api");
    let before = std::fs::read(&record).unwrap();
    ok(
        &other,
        "backup.restore",
        &[
            ("bucket", t("b1")),
            ("file", t("shop")),
            ("store", t("shop")),
        ],
    );
    assert_eq!(std::fs::read(&record).unwrap(), before, "record rewritten");
    // a record an earlier restore left without its manifests is completed
    let stack = layerfs_protocol::sdk::hex(&original.manifest.stack);
    let listing = SdkValue::map([("name", t("api")), ("stack", t(&stack))]);
    write_record(&record, &listing).unwrap();
    ok(
        &other,
        "backup.restore",
        &[
            ("bucket", t("b1")),
            ("file", t("shop")),
            ("store", t("shop")),
        ],
    );
    same_project(&other, "shop", &original);
    for dir in [home, folder, other] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
