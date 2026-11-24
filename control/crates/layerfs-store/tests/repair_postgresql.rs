//! The operator repair on a PostgreSQL Store: preview, settle, release and
//! `settle_legacy` go through the Store catalog, so the same call that a
//! Windows worker or a host reaches answers exactly as on SQLite. The lean
//! schema keeps no Store-side audit table; release retains the done row and
//! the bounded audit read reports it, beside the call log.
//!
//! Needs a writable PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` to a
//! libpq connection string or URL. The test creates and drops its own
//! database, because the Store uses the one `layerfs` schema. Without the
//! variable it reports NOT_RUN.
use layerfs_bridge::control::SdkValue;
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{HistoryCatalogConfig, PlaceAuthority};
use layerfs_home::home;
use layerfs_home::names::StoreName;
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::StoragePolicy;
use layerfs_store::open_args::OpenArgs;
use layerfs_store::project::add::{add_project, ProjectAdd};
use layerfs_store::record::StoreRecord;
use layerfs_store::service::{branch_named, repair_mount_holders_at, with_service};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const BINDING: &[u8] = b"repair-postgresql";

static NEXT: AtomicU64 = AtomicU64::new(0);

fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-repair-pg-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    dir
}

/// A folder to initialize the project from.
fn folder(tag: &str) -> PathBuf {
    let dir = tempdir(&format!("folder-{tag}"));
    std::fs::write(dir.join("a.txt"), b"a\n").unwrap();
    dir
}

/// The URL of `database` on the same server as `source`.
fn database_url(source: &str, database: &str) -> String {
    if source.starts_with("postgres://") || source.starts_with("postgresql://") {
        let (base, query) = match source.split_once('?') {
            Some((base, query)) => (base, format!("?{query}")),
            None => (source, String::new()),
        };
        let scheme = base.find("://").expect("a PostgreSQL URL") + 3;
        match base[scheme..].find('/').map(|at| scheme + at) {
            Some(at) => format!("{}/{database}{query}", &base[..at]),
            None => format!("{base}/{database}{query}"),
        }
    } else {
        format!("{source} dbname={database}")
    }
}

/// One disposable database: the Store uses the fixed `layerfs` schema, so a
/// test run needs a database nothing else uses.
struct Database {
    source: String,
    name: String,
}

impl Database {
    fn create(source: &str) -> Self {
        let name = format!(
            "lfs_repair_pg_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let mut admin = postgres::Client::connect(source, postgres::NoTls).unwrap();
        admin
            .batch_execute(&format!("DROP DATABASE IF EXISTS {name} WITH (FORCE)"))
            .unwrap();
        admin
            .batch_execute(&format!("CREATE DATABASE {name}"))
            .unwrap();
        Self {
            source: source.to_owned(),
            name,
        }
    }

    fn url(&self) -> String {
        database_url(&self.source, &self.name)
    }
}

impl Drop for Database {
    fn drop(&mut self) {
        if let Ok(mut admin) = postgres::Client::connect(&self.source, postgres::NoTls) {
            let _ = admin.batch_execute(&format!(
                "DROP DATABASE IF EXISTS {} WITH (FORCE)",
                self.name
            ));
        }
    }
}

fn future_ms() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("the clock is after the epoch")
        .as_millis();
    i64::try_from(now).expect("the clock fits an i64") + 60_000
}

fn mount(branch: &[u8], stack: &[u8], op: u8) -> GcIntent {
    GcIntent::shared(
        [op; 16],
        GcIntentKind::Mount,
        GcScopeKind::Branch,
        branch.to_vec(),
    )
    .in_project(stack.to_vec())
    .named(Some("app"), Some("main"))
}

fn holders(value: &SdkValue) -> &[SdkValue] {
    match value.get("holders") {
        Some(SdkValue::List(rows)) => rows,
        other => panic!("no holder list: {other:?}"),
    }
}

#[test]
fn operator_repair_round_trips_on_postgresql() {
    let Some(source) = url() else {
        return;
    };
    let database = Database::create(&source);
    let url = database.url();
    // The Store schema the PostgreSQL engine writes, with this test's own
    // catalog identity.
    let setup = PostgresHandles::create(
        &PostgresSelection::new(&url, "layerfs"),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: BINDING.to_vec(),
            cursor_key: [71; 32],
            incarnation: 7,
        },
    )
    .unwrap();
    drop(setup);

    let home = tempdir("home");
    home::ensure(&home).unwrap();
    let record = StoreRecord::connected("pgs", &url, "layerfs", "local", None)
        .with("binding", SdkValue::Bytes(BINDING.to_vec()))
        .with("cursor", SdkValue::Bytes(vec![71; 32]));
    record.write(&home).unwrap();
    add_project(&ProjectAdd {
        home: home.clone(),
        store: StoreName::parse("pgs").unwrap(),
        name: "app".to_owned(),
        source: folder("app"),
        timeout_ms: Some(60_000),
        store_record: None,
    })
    .unwrap();
    let record = StoreRecord::read(&home, "pgs").unwrap().unwrap();
    let open = OpenArgs::of_record(&home, "pgs", &record).unwrap();
    let branch = branch_named(&open, "app", "main").unwrap();
    let stack = with_service(&open, |service| {
        Ok(layerfs_store_service::history::lookup::project(&service.store, "app")?.id)
    })
    .unwrap();

    let live = mount(branch.as_slice(), stack.as_slice(), 0x21)
        .placed(Some([0x22; 32]), Some("sandbox"), Some("box"))
        .owned(Some("pid:1@0"), Some("boot:1"))
        .authorized(PlaceAuthority {
            machine_incarnation: Some("worker:1".to_owned()),
            sandbox_generation: Some("generation:1".to_owned()),
            lease_expires_ms: Some(future_ms()),
            fence_token: Some("fence:1".to_owned()),
        });
    let legacy = mount(branch.as_slice(), stack.as_slice(), 0x31);
    with_service(&open, |service| {
        service.store.history().gc_register(&live).unwrap();
        service.store.history().gc_register(&legacy).unwrap();
        Ok(())
    })
    .unwrap();

    let preview = repair_mount_holders_at(
        &open, "app", "main", "preview", None, false, "alice", 0, 100,
    )
    .expect("preview");
    let rows = holders(&preview);
    assert_eq!(rows.len(), 2, "{rows:?}");
    let row = rows
        .iter()
        .find(|row| row.get_text("op") == Some("21".repeat(16).as_str()))
        .expect("the live holder");
    assert_eq!(row.get_text("state"), Some("live"));
    assert_eq!(row.get_text("machine_incarnation"), Some("worker:1"));
    assert_eq!(row.get_text("sandbox_generation"), Some("generation:1"));
    assert_eq!(row.get("legacy"), Some(&SdkValue::Bool(false)));

    let refused = repair_mount_holders_at(
        &open,
        "app",
        "main",
        "release",
        Some([0x21; 16]),
        false,
        "alice",
        0,
        100,
    )
    .unwrap_err();
    assert_eq!(refused.code, "Conflict", "{refused:?}");
    repair_mount_holders_at(
        &open,
        "app",
        "main",
        "settle",
        Some([0x21; 16]),
        false,
        "alice",
        0,
        100,
    )
    .expect("settle");
    repair_mount_holders_at(
        &open,
        "app",
        "main",
        "release",
        Some([0x21; 16]),
        false,
        "alice",
        0,
        100,
    )
    .expect("release");
    let settled = repair_mount_holders_at(
        &open,
        "app",
        "main",
        "settle_legacy",
        None,
        false,
        "alice",
        0,
        100,
    )
    .expect("settle_legacy");
    assert_eq!(settled.get_int("settled"), Some(1));
    repair_mount_holders_at(
        &open,
        "app",
        "main",
        "release",
        Some([0x31; 16]),
        false,
        "alice",
        0,
        100,
    )
    .expect("release the legacy holder");
    let preview = repair_mount_holders_at(
        &open, "app", "main", "preview", None, false, "alice", 0, 100,
    )
    .expect("preview");
    assert!(holders(&preview).is_empty());
    // The two releases retained their rows as the PostgreSQL Store's durable
    // operator audit; the audit read does not resolve a Branch first.
    let audit =
        repair_mount_holders_at(&open, "app", "main", "audit", None, false, "alice", 0, 100)
            .expect("audit");
    let Some(SdkValue::List(items)) = audit.get("items") else {
        panic!("audit items: {audit:?}");
    };
    let releases: Vec<_> = items
        .iter()
        .filter(|item| {
            item.get_text("evidence")
                .is_some_and(|evidence| evidence.contains("action=release;operator=alice;op="))
        })
        .collect();
    assert_eq!(releases.len(), 2, "{audit:?}");
    let previews: Vec<_> = items
        .iter()
        .filter(|item| item.get_text("evidence") == Some("action=preview;operator=alice"))
        .collect();
    assert_eq!(previews.len(), 2, "the durable preview rows: {audit:?}");
    for item in previews {
        assert_eq!(item.get_text("project"), Some("app"));
        assert_eq!(item.get_text("branch"), Some("main"));
        assert_eq!(item.get_text("op").map(str::len), Some(32));
    }

    let _ = std::fs::remove_dir_all(&home);
}
