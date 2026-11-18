//! The Store doctor (call surface 2.0, `checks.store`): its records checks,
//! the shallow cut on this computer (reached, id and schema, heads against
//! each bucket link) in the frozen order, and the same cut through a real
//! worker's `store.diagnose`, which refuses a request without `cut`.
use layerfs_bridge::control::SdkValue;
use layerfs_control::doctor::{self, Kind, Options, Thing};
use layerfs_history::HistoryCatalogConfig;
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths, with_field, write_entity};
use layerfs_home::op::Op;
use layerfs_machine::worker::{diagnostic_connect, Endpoint};
use layerfs_persistence::{PersistenceConfig, SqliteHandles, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
const BINDING: &[u8] = b"doctor-store";

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("layerfs-store-doctor-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        home::private_dir(&root).unwrap();
        home::ensure(&root).unwrap();
        Self { root }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn fields(name: &str) -> SdkValue {
    SdkValue::map([
        ("name", SdkValue::text(name)),
        ("kind", SdkValue::text("sqlite")),
        ("machine", SdkValue::text("native")),
        ("binding", SdkValue::Bytes(BINDING.to_vec())),
        ("cursor", SdkValue::Bytes(vec![9; 32])),
    ])
}

/// A real sqlite Store file of the fixture's identity at `path`.
fn store_file(path: &Path) -> SqliteHandles {
    home::private_dir(path.parent().unwrap()).unwrap();
    SqliteHandles::create(
        PersistenceConfig::sqlite(path).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [9; 32],
            incarnation: 1,
        },
    )
    .unwrap()
}

fn report(root: &Path, name: &str) -> SdkValue {
    let mut sink = |_| {};
    let mut events = Events::new(&mut sink);
    let thing = Thing::new(Kind::Store, name);
    doctor::run(root, thing, &Options::default(), &mut events)
        .unwrap()
        .to_sdk()
}

fn list<'a>(value: &'a SdkValue, key: &str) -> &'a [SdkValue] {
    match value.get(key) {
        Some(SdkValue::List(items)) => items,
        _ => &[],
    }
}

/// `(check, outcome)` of each row of the report's Store node.
fn rows(report: &SdkValue) -> Vec<(String, String)> {
    list(report.get("tree").unwrap(), "checks")
        .iter()
        .map(|row| {
            let text = |key: &str| row.get_text(key).unwrap_or("").to_owned();
            (text("check"), text("outcome"))
        })
        .collect()
}

fn pairs(rows: &[(&str, &str)]) -> Vec<(String, String)> {
    rows.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

#[test]
fn a_native_store_is_reached_checked_to_its_schema_and_its_backups_compared() {
    let f = Fixture::new();
    let path = paths::store_file(&f.root, "one");
    let writer = store_file(&path);
    // one link whose heads are not the Store's now, one with none recorded
    let link = |heads: SdkValue| SdkValue::map([("file", SdkValue::text("one")), ("heads", heads)]);
    let record = with_field(
        fields("one"),
        "backups",
        SdkValue::map([
            ("r2-main", link(SdkValue::text("00".repeat(32)))),
            ("old", link(SdkValue::Null)),
        ]),
    );
    write_entity(&paths::store_record(&f.root, "one"), &record).unwrap();
    let before = std::fs::read(paths::store_record(&f.root, "one")).unwrap();
    let out = report(&f.root, "one");
    assert_eq!(
        rows(&out),
        pairs(&[
            ("store.record", "pass"),
            ("store.operation", "pass"),
            ("store.connection", "pass"),
            ("store.identity", "pass"),
            ("store.schema", "pass"),
            ("store.backups", "notice"),
        ]),
        "{out:?}"
    );
    let findings = list(&out, "findings");
    assert_eq!(findings.len(), 1, "{out:?}");
    assert_eq!(findings[0].get_text("check"), Some("store.backups"));
    assert_eq!(
        findings[0].get_text("message"),
        Some("its backup in r2-main is behind")
    );
    assert_eq!(
        findings[0].get_text("fix"),
        Some("layerfs backup create one r2-main")
    );
    // a doctor writes nothing
    assert_eq!(
        before,
        std::fs::read(paths::store_record(&f.root, "one")).unwrap()
    );
    drop(writer);
}

#[test]
fn records_checks_come_first_and_a_missing_file_stops_at_the_connection() {
    let f = Fixture::new();
    let record_path = paths::store_record(&f.root, "one");
    // a record naming another Store fails its record check and stops
    write_entity(&record_path, &fields("two")).unwrap();
    assert_eq!(
        rows(&report(&f.root, "one")),
        pairs(&[("store.record", "error")])
    );
    // a valid record with no file: an unresolved operation, then no file
    let op = Op::begin("store.remove", "delete", SdkValue::Null)
        .unwrap()
        .as_uncertain();
    let gone = SdkValue::map([
        ("pid", SdkValue::Int(0x7fff_fff0)),
        ("start", SdkValue::Int(1)),
    ]);
    let op = with_field(op.to_record().clone(), "owner", gone);
    write_entity(&record_path, &with_field(fields("one"), "op", op)).unwrap();
    let out = report(&f.root, "one");
    assert_eq!(
        rows(&out),
        pairs(&[
            ("store.record", "pass"),
            ("store.operation", "error"),
            ("store.connection", "error"),
        ]),
        "{out:?}"
    );
    let findings = list(&out, "findings");
    assert_eq!(
        findings[0].get_text("fix"),
        Some("layerfs store remove one")
    );
    assert_eq!(
        findings[1].get_text("message"),
        Some("machine native has no file where its record says")
    );
}

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore]
fn worker_child() {
    if let Ok(root) = std::env::var("LAYERFS_DOCTOR_WORKER_FIXTURE_ROOT") {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        std::fs::write(
            PathBuf::from(&root).join("endpoint"),
            listener.local_addr().unwrap().to_string(),
        )
        .unwrap();
        layerfs_control::worker_server::serve(
            listener,
            "fixture-secret".to_owned(),
            PathBuf::from(root),
        )
        .unwrap();
    }
}

#[test]
fn a_worker_makes_the_shallow_cut_and_refuses_a_request_without_one() {
    let f = Fixture::new();
    let writer = store_file(&paths::store_file(&f.root, "one"));
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "worker_child"])
        .env("LAYERFS_DOCTOR_WORKER_FIXTURE_ROOT", &f.root)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    let endpoint = loop {
        if let Ok(text) = std::fs::read_to_string(f.root.join("endpoint")) {
            if let Ok(address) = text.parse() {
                break Endpoint {
                    address,
                    secret: "fixture-secret".to_owned(),
                };
            }
        }
        assert!(Instant::now() < until, "the worker did not start");
        assert!(child.try_wait().unwrap().is_none());
        std::thread::sleep(Duration::from_millis(5));
    };
    let _server = Server(child);
    let ask = |args: SdkValue| {
        diagnostic_connect(&endpoint, Instant::now() + Duration::from_secs(2))
            .unwrap()
            .diagnose_store(
                args,
                Some(Instant::now() + Duration::from_secs(5)),
                &mut |_| Ok(()),
            )
    };
    let args = |store: &str, binding: &[u8]| {
        SdkValue::map([
            ("store", SdkValue::text(store)),
            ("binding", SdkValue::Bytes(binding.to_vec())),
            ("cursor", SdkValue::Bytes(vec![9; 32])),
            ("cut", SdkValue::text("schema")),
            ("heads", SdkValue::Bool(true)),
        ])
    };
    let found = ask(args("one", BINDING)).unwrap();
    assert_eq!(found.get("found"), Some(&SdkValue::Bool(true)), "{found:?}");
    assert!(found.get("schema_error").is_none(), "{found:?}");
    assert_eq!(found.get_text("heads").map(str::len), Some(64), "{found:?}");
    // another Store's id: the cut fails at the schema check
    let other = ask(args("one", b"another-store")).unwrap();
    assert!(other.get_text("schema_error").is_some(), "{other:?}");
    // no file
    let none = ask(args("two", BINDING)).unwrap();
    assert_eq!(none.get("found"), Some(&SdkValue::Bool(false)));
    // a request without the cut (today's whole-Store scan) is refused
    let full = with_field(args("one", BINDING), "cut", SdkValue::Null);
    assert_eq!(ask(full).unwrap_err().code, "InvalidInput");
    drop(writer);
}

/// A `<store>.compact-op` record beside `file` naming this process as its
/// owner (`live`), or a process that is gone.
fn compact_record(file: &Path, live: bool) {
    let (start, boot) = match (live, layerfs_home::process::me()) {
        (true, Some((start, boot))) => (start.to_string(), format!("{boot:032x}")),
        (true, None) => (String::new(), String::new()),
        (false, _) => ("1".to_owned(), "0".repeat(32)),
    };
    let zeros = |n: usize| "0".repeat(n);
    let text = format!(
        "version=1\nop_id={}\ngeneration=4\nphase=build\nstarted_ms=1794993419000\n\
owner_pid={}\nowner_pid_start={start}\nowner_pid_boot={boot}\nowner_pid_namespace=\n\
catalog_id={}\nnext_history_serial=1\nnext_pack_id=1\nnext_ordinal=1\n\
mark_serial=0\nmark_pack_id=1\nmark_digest={}\nmark_totals=0,0,0,0\nmark_stamp=0\nhistory_rows=0\n\
live_heads_digest={}\nreturned=0\n",
        zeros(32),
        std::process::id(),
        zeros(32),
        zeros(64),
        zeros(64),
    );
    let mut path = file.as_os_str().to_owned();
    path.push(".compact-op");
    std::fs::write(path, text).unwrap();
}

/// v19: `store.operation` also reads the compaction record beside a sqlite
/// Store's file through its machine: a run live there passes, naming its
/// phase; a run whose process is gone fails with `store compact` as the fix.
#[test]
fn store_operation_reads_the_compaction_record_beside_the_file() {
    let f = Fixture::new();
    let path = paths::store_file(&f.root, "one");
    let writer = store_file(&path);
    write_entity(&paths::store_record(&f.root, "one"), &fields("one")).unwrap();
    compact_record(&path, true);
    let out = report(&f.root, "one");
    assert_eq!(
        rows(&out)[1],
        ("store.operation".to_owned(), "pass".to_owned())
    );
    assert!(
        !list(&out, "findings")
            .iter()
            .any(|f| f.get_text("check") == Some("store.operation")),
        "{out:?}"
    );
    compact_record(&path, false);
    let out = report(&f.root, "one");
    let finding = list(&out, "findings")
        .iter()
        .find(|f| f.get_text("check") == Some("store.operation"))
        .unwrap_or_else(|| panic!("an operation finding: {out:?}"))
        .clone();
    assert_eq!(finding.get_text("severity"), Some("error"), "{finding:?}");
    assert!(
        finding
            .get_text("message")
            .is_some_and(|m| m.contains("compaction") && m.contains("build")),
        "{finding:?}"
    );
    assert_eq!(finding.get_text("fix"), Some("layerfs store compact one"));
    drop(writer);
}
