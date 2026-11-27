//! LayerFS removes what interrupted operations left itself (spec v20): a
//! `sandbox create` on `native` sweeps its run root before the daemon
//! starts, the next `backup create` of a Store removes the snapshots a
//! killed create staged, and a `backup restore` that replaces the Store
//! does too. What something still owns stays: a run folder with no
//! `owner` marker or a fresh one, and staged snapshots while a create holds
//! the Store's create lock. Each removal is a line of the home's
//! `log/sweep.jsonl`. No daemon starts and no bucket answers: the calls are
//! refused after their sweep (the bucket record names a port nothing
//! listens on).
use layerfs_api::home::{self, paths, read_record, write_record};
use layerfs_protocol::provision::install::SdkValue;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("layerfs-sweep-{tag}-{}", std::process::id()));
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

fn file(path: &Path, bytes: usize) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, vec![b'x'; bytes]).unwrap();
}

/// The `removed` lines of the home's `log/sweep.jsonl`: `(kind, path)`.
fn swept(home: &Path) -> Vec<(String, String)> {
    let text =
        std::fs::read_to_string(paths::log_dir(home).join("sweep.jsonl")).unwrap_or_default();
    (text.lines())
        .map(|line| layerfs_home::json::parse(line).unwrap())
        .filter(|line| line.get_text("outcome") == Some("removed"))
        .map(|line| {
            let field = |name| line.get_text(name).unwrap().to_owned();
            (field("kind"), field("path"))
        })
        .collect()
}

/// A run folder in `home`'s run root whose `owner` marker names Sandbox
/// `sandbox`'s record folder, written `ago` before now.
fn run_folder(home: &Path, name: &str, sandbox: &str, ago: Duration) -> PathBuf {
    let folder = paths::run_root(home).join(name);
    file(&folder.join("daemon.log"), 10);
    let records = paths::sandbox_dir(home, "x").parent().unwrap().to_owned();
    std::fs::create_dir_all(&records).unwrap();
    let records = std::fs::canonicalize(records).unwrap();
    let owner = folder.join("owner");
    std::fs::write(&owner, records.join(sandbox).as_os_str().as_encoded_bytes()).unwrap();
    (std::fs::File::open(&owner).unwrap())
        .set_modified(SystemTime::now() - ago)
        .unwrap();
    folder
}

#[test]
fn a_sandbox_create_on_native_sweeps_its_run_root_first() {
    let home = tempdir("create");
    home::open(&home).unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    // the built-in machine's record, as a build with the machine calls
    // writes it
    let native = paths::machine_record(&home, layerfs_home::names::HOST_MACHINE);
    if !native.is_file() {
        let record = SdkValue::map([
            ("name", t(layerfs_home::names::HOST_MACHINE)),
            ("platform", t("host")),
        ]);
        home::write_entity(&native, &record).unwrap();
    }
    write_record(
        &paths::project_record(&home, "shop", "app"),
        &SdkValue::map([("name", t("app")), ("created", SdkValue::Int(1))]),
    )
    .unwrap();
    let hour = Duration::from_secs(3600);
    let orphan = run_folder(&home, "layerfs-0123abcd-gone", "gone", hour);
    let empty_root = paths::run_root(&home).join("layerfs-0123abcd-dead.root");
    std::fs::create_dir_all(&empty_root).unwrap();
    let fresh = run_folder(&home, "layerfs-0123abcd-fresh", "fresh", Duration::ZERO);
    let bare = paths::run_root(&home).join("layerfs-0123abcd-bare");
    file(&bare.join("daemon.log"), 3);

    // the create is refused or fails after its sweep: no daemon runs here
    let created = call(
        &home,
        "sandbox.create",
        &[
            ("project", t("shop/app")),
            ("name", t("tests")),
            ("machine", t(layerfs_home::names::HOST_MACHINE)),
        ],
    );
    if created.is_ok() {
        let _ = call(
            &home,
            "sandbox.remove",
            &[("sandbox", t("tests")), ("force", SdkValue::Bool(true))],
        );
    }
    assert!(!orphan.exists(), "{created:?}");
    assert!(!empty_root.exists(), "{created:?}");
    assert!(fresh.join("daemon.log").is_file(), "a fresh marker is kept");
    assert!(bare.join("daemon.log").is_file(), "no marker: kept");
    let mut lines = swept(&home);
    lines.sort();
    assert_eq!(
        lines,
        [
            ("empty_root".to_owned(), empty_root.display().to_string()),
            ("run_folder".to_owned(), orphan.display().to_string()),
        ]
    );
    let _ = std::fs::remove_dir_all(home);
}

/// A connected bucket whose endpoint answers nothing.
fn dead_bucket(home: &Path) {
    layerfs_store::backup::secret::write(home, "dead", "secret").unwrap();
    write_record(
        &paths::bucket_record(home, "dead"),
        &SdkValue::map([
            ("name", t("dead")),
            ("provider", t("minio")),
            ("endpoint", t("http://127.0.0.1:9")),
            ("bucket", t("nothing")),
            ("prefix", t("")),
            ("region", t("us-east-1")),
            ("access_key", t("user")),
        ]),
    )
    .unwrap();
}

/// The two snapshots a killed `backup create` of `shop` leaves in the home.
fn staged(home: &Path) -> [PathBuf; 2] {
    let dir = paths::store_dir(home, "shop");
    let fetched = dir.join("backup.staging");
    let mut beside = paths::store_file(home, "shop").into_os_string();
    beside.push(".backup");
    let beside = PathBuf::from(beside);
    file(&fetched, 7);
    file(&beside, 7);
    [fetched, beside]
}

#[test]
fn the_next_backup_create_or_restore_removes_stale_staging() {
    let home = tempdir("backup");
    home::open(&home).unwrap();
    ok(&home, "store.create", &[("name", t("shop"))]);
    dead_bucket(&home);
    let backup = [("store", t("shop")), ("bucket", t("dead"))];

    // a create that runs holds the Store's create lock: its snapshots stay
    let left = staged(&home);
    {
        let _running = layerfs_store::backup::create::Creating::take(&home, "shop").unwrap();
        let (code, _) = call(&home, "backup.create", &backup).unwrap_err();
        assert_eq!(code, "Busy");
        assert!(left.iter().all(|path| path.is_file()));
    }
    // the next create removes them, then fails at the bucket
    let (code, message) = call(&home, "backup.create", &backup).unwrap_err();
    assert_ne!(code, "Busy", "{message}");
    assert!(left.iter().all(|path| !path.exists()));
    let mut lines = swept(&home);
    lines.sort();
    let mut want: Vec<_> = (left.iter())
        .map(|path| ("backup_staging".to_owned(), path.display().to_string()))
        .collect();
    want.sort();
    assert_eq!(lines, want);

    // a restore that replaces the Store: linked at its current heads, so
    // only the bucket stands in the way
    let heads = layerfs_store::backup::copy::heads(&paths::store_file(&home, "shop")).unwrap();
    let path = paths::store_record(&home, "shop");
    let record = read_record(&path).unwrap().unwrap();
    let linked = SdkValue::map([(
        "dead",
        SdkValue::map([
            ("file", t("shop")),
            ("heads", SdkValue::Bytes(heads.to_vec())),
        ]),
    )]);
    write_record(&path, &home::with_field(record, "backups", linked)).unwrap();
    let restore = [
        ("bucket", t("dead")),
        ("file", t("shop")),
        ("store", t("shop")),
    ];
    let left = staged(&home);
    {
        let _running = layerfs_store::backup::create::Creating::take(&home, "shop").unwrap();
        let _ = call(&home, "backup.restore", &restore).unwrap_err();
        assert!(
            left.iter().all(|path| path.is_file()),
            "a running create keeps them"
        );
    }
    let _ = call(&home, "backup.restore", &restore).unwrap_err();
    assert!(left.iter().all(|path| !path.exists()));
    assert_eq!(swept(&home).len(), 4);
    let _ = std::fs::remove_dir_all(home);
}
