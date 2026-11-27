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
use layerfs_history::records::{GcIntent, GcIntentKind, GcScopeKind};
use layerfs_history::{HistoryCatalogConfig, PlaceAuthority};
use layerfs_home::home;
use layerfs_home::names::ProjectRef;
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_protocol::sdk::SdkValue;
use layerfs_storage::StoragePolicy;
use layerfs_store::open_args::OpenArgs;
use layerfs_store::project::family::{init, ProjectInit};
use layerfs_store::record::StoreRecord;
use layerfs_store::repair::{repair_at, Repair};
use layerfs_store::service::{branch_named, with_service};
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
    match value.get("items") {
        Some(SdkValue::List(rows)) => rows,
        other => panic!("no holder list: {other:?}"),
    }
}

/// A PostgreSQL Store `pgs` in a fresh database and home, with project
/// `app` initialized on it, and its `main` Branch and project ids.
fn initialized(
    source: &str,
) -> (
    Database,
    PathBuf,
    OpenArgs,
    layerfs_history::BranchId,
    layerfs_history::LayerStackId,
) {
    let database = Database::create(source);
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
    // `store.connect`'s record shape, which `project.init` dials with: the
    // address alone and the user in the record (the URL after its user
    // part), the password saved under the Store's name (a trust-auth server
    // takes any password)
    let read: postgres::Config = url.parse().expect("LAYERFS_TEST_POSTGRES_URL is a URL");
    let user = read.get_user().unwrap_or("postgres");
    let password = read.get_password().map_or("unused".to_owned(), |password| {
        String::from_utf8(password.to_vec()).expect("a UTF-8 password")
    });
    layerfs_store::postgresql::login::save(&home, "pgs", &password).unwrap();
    let address = match layerfs_store::postgresql::connect::holds_login(&url) {
        false => url.clone(),
        true => {
            let (scheme, after) = url.split_once("://").expect("a postgres:// URL");
            format!(
                "{scheme}://{}",
                after.split_once('@').map_or(after, |(_, at)| at)
            )
        }
    };
    let record = StoreRecord::connected("pgs", &address, user, "layerfs", "local")
        .with("binding", SdkValue::Bytes(BINDING.to_vec()))
        .with("cursor", SdkValue::Bytes(vec![71; 32]));
    record.write(&home).unwrap();
    let mut sink = |_| {};
    let mut events = layerfs_home::events::Events::new(&mut sink);
    init(
        &home,
        &mut events,
        &ProjectInit {
            project: ProjectRef::parse("pgs/app").unwrap(),
            folder: folder("app").display().to_string(),
            timeout: Some(60_000),
        },
    )
    .expect("project.init on the PostgreSQL Store");
    let record = StoreRecord::read(&home, "pgs").unwrap().unwrap();
    let open = OpenArgs::of_record(&home, "pgs", &record).unwrap();
    let branch = branch_named(&open, "app", "main").unwrap();
    let stack = with_service(&open, |service| {
        Ok(layerfs_store_service::history::lookup::project(&service.store, "app")?.id)
    })
    .unwrap();

    (database, home, open, branch, stack)
}

#[test]
fn operator_repair_round_trips_on_postgresql() {
    let Some(source) = url() else {
        return;
    };
    let (_database, home, open, branch, stack) = initialized(&source);

    let live = mount(branch.as_slice(), stack.as_slice(), 0x21)
        .placed(Some([0x22; 32]), Some("sandbox"), Some("box"))
        .owned(Some("pid:1@0"), Some("boot:1"))
        .authorized(PlaceAuthority {
            machine_incarnation: Some("worker:1".to_owned()),
            sandbox_generation: Some("generation:1".to_owned()),
        });
    let legacy = mount(branch.as_slice(), stack.as_slice(), 0x31);
    with_service(&open, |service| {
        service.store.history().gc_register(&live).unwrap();
        service.store.history().gc_register(&legacy).unwrap();
        Ok(())
    })
    .unwrap();

    let repair = |action: &str, op: Option<u8>, force: bool| {
        repair_at(
            &open,
            &Repair {
                project: "app".to_owned(),
                branch: "main".to_owned(),
                action: action.to_owned(),
                op: op.map(|op| [op; 16]),
                force,
                operator: "alice".to_owned(),
                limit: 100,
                ..Repair::default()
            },
        )
    };
    let preview = repair("preview", None, false).expect("preview");
    let rows = holders(&preview);
    assert_eq!(rows.len(), 2, "{rows:?}");
    let row = rows
        .iter()
        .find(|row| row.get_text("op") == Some("21".repeat(16).as_str()))
        .expect("the live holder");
    assert_eq!(row.get_text("state"), Some("live"));
    // a shared Store proves no owner gone from here: it still looks live
    assert_eq!(row.get_text("liveness"), Some("live"));
    assert_eq!(row.get("safe"), Some(&SdkValue::Bool(false)));
    assert_eq!(row.get_text("machine_incarnation"), Some("worker:1"));
    assert_eq!(row.get_text("sandbox_generation"), Some("generation:1"));
    assert_eq!(row.get("legacy"), Some(&SdkValue::Bool(false)));

    let refused = repair("release", Some(0x21), false).unwrap_err();
    assert_eq!(refused.code, "Conflict", "{refused:?}");
    let refused = repair("settle", Some(0x21), false).unwrap_err();
    assert_eq!(refused.code, "InUse", "a live-looking holder: {refused:?}");
    repair("settle", Some(0x21), true).expect("a forced settle");
    repair("release", Some(0x21), false).expect("release");
    // a second release answers NotFound, as on SQLite (#783 gap 5)
    let again = repair("release", Some(0x21), false).unwrap_err();
    assert_eq!(again.code, "NotFound", "{again:?}");
    let settled = repair("settle_legacy", None, false).expect("settle_legacy");
    assert_eq!(settled.get_int("settled"), Some(1));
    repair("release", Some(0x31), false).expect("release the legacy holder");
    let preview = repair("preview", None, false).expect("preview");
    assert!(holders(&preview).is_empty());
    // The two releases retained their rows as the PostgreSQL Store's durable
    // operator audit; the audit read does not resolve a Branch first.
    let audit = repair("audit", None, false).expect("audit");
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
    assert_eq!(items.len(), 2, "no preview wrote a row: {audit:?}");
    for item in releases {
        assert_eq!(item.get_text("project"), Some("app"));
        assert_eq!(item.get_text("branch"), Some("main"));
        assert_eq!(item.get_text("op").map(str::len), Some(32));
    }

    let _ = std::fs::remove_dir_all(&home);
}

/// #776 G1: a merge's project-scope intent left by an owner this process
/// cannot judge (another PID namespace or host; a shared Store never proves
/// it gone) holds every mount of the project. The repair lists it with its
/// kind, settles it only when forced, and releases it, and the project can
/// be mounted again.
#[test]
fn an_orphaned_operation_row_is_repaired_on_postgresql() {
    let Some(source) = url() else {
        return;
    };
    let (_database, home, open, branch, stack) = initialized(&source);
    let merge = GcIntent::shared(
        [0x41; 16],
        GcIntentKind::Merge,
        GcScopeKind::Project,
        stack.as_slice().to_vec(),
    )
    .in_project(stack.as_slice().to_vec())
    .named(Some("app"), None)
    .owned(Some("pid:1@1@ns:4000000000"), Some("boot:1:op:41"));
    with_service(&open, |service| {
        service.store.history().gc_register(&merge).unwrap();
        Ok(())
    })
    .unwrap();
    let refused = with_service(&open, |service| {
        Ok(service
            .store
            .history()
            .gc_register(&mount(branch.as_slice(), stack.as_slice(), 0x42)))
    })
    .unwrap()
    .unwrap_err();
    assert!(
        matches!(refused, layerfs_history::HistoryError::BusyOp(_)),
        "the orphaned merge holds the project: {refused:?}"
    );

    let repair = |action: &str, op: Option<u8>, force: bool| {
        repair_at(
            &open,
            &Repair {
                project: "app".to_owned(),
                branch: "main".to_owned(),
                action: action.to_owned(),
                op: op.map(|op| [op; 16]),
                force,
                operator: "alice".to_owned(),
                limit: 100,
                ..Repair::default()
            },
        )
    };
    let preview = repair("preview", None, false).expect("preview");
    let rows = holders(&preview);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].get_text("op"), Some("41".repeat(16).as_str()));
    assert_eq!(rows[0].get_text("kind"), Some("merge"));
    assert_eq!(rows[0].get_text("liveness"), Some("live"));
    assert_eq!(rows[0].get("legacy"), Some(&SdkValue::Bool(false)));
    // `settle_legacy` never marks an operation row
    let settled = repair("settle_legacy", None, false).expect("settle_legacy");
    assert_eq!(settled.get_int("settled"), Some(0));
    let refused = repair("settle", Some(0x41), false).unwrap_err();
    assert_eq!(refused.code, "InUse", "{refused:?}");
    repair("settle", Some(0x41), true).expect("a forced settle");
    repair("release", Some(0x41), false).expect("release");
    assert!(holders(&repair("preview", None, false).unwrap()).is_empty());

    let mounted = mount(branch.as_slice(), stack.as_slice(), 0x43);
    with_service(&open, |service| {
        service.store.history().gc_register(&mounted).unwrap();
        service.store.history().gc_release(&mounted.op_id).unwrap();
        Ok(())
    })
    .unwrap();
    let _ = std::fs::remove_dir_all(&home);
}
