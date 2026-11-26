//! A host worker's held PostgreSQL read sessions (#819 P-B), driven in
//! process: the record orders login revisions, a panicking call gives its
//! slot back, a held session is checked as a Light check would, and a Store
//! with no session forgets its login.
//!
//! Needs a writable PostgreSQL server: set `LAYERFS_TEST_POSTGRES_URL` to a
//! URL. Each test creates and drops its own database. Without the variable
//! it reports NOT_RUN. The sessions are process-wide: run with
//! `--test-threads 1`.
use layerfs_bridge::control::SdkValue;
use layerfs_history::HistoryCatalogConfig;
use layerfs_home::home;
use layerfs_home::names::ProjectRef;
use layerfs_persistence::{PostgresHandles, PostgresSelection};
use layerfs_storage::StoragePolicy;
use layerfs_store::open_args::OpenArgs;
use layerfs_store::postgresql::pool;
use layerfs_store::project::family::{init, ProjectInit};
use layerfs_store::record::StoreRecord;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

const BINDING: &[u8] = b"held-sessions";
const INCARNATION: u64 = 7;

static NEXT: AtomicU64 = AtomicU64::new(0);

/// The held sessions are process-wide: one test at a time uses them, so
/// the file is safe under a plain `cargo test`.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|error| error.into_inner())
}

fn url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "layerfs-held-{tag}-{}-{}",
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

/// The URL of `database` on the server of `source` (a URL).
fn database_url(source: &str, database: &str) -> String {
    let (base, query) = match source.split_once('?') {
        Some((base, query)) => (base, format!("?{query}")),
        None => (source, String::new()),
    };
    let scheme = base.find("://").expect("a PostgreSQL URL") + 3;
    match base[scheme..].find('/').map(|at| scheme + at) {
        Some(at) => format!("{}/{database}{query}", &base[..at]),
        None => format!("{base}/{database}{query}"),
    }
}

/// One disposable database with a Store schema of `binding`.
struct Database {
    source: String,
    name: String,
}

impl Database {
    fn create(source: &str) -> Self {
        let name = format!(
            "lfs_held_{}_{}",
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
        let database = Self {
            source: source.to_owned(),
            name,
        };
        database.schema(BINDING);
        database
    }

    fn url(&self) -> String {
        database_url(&self.source, &self.name)
    }

    /// A Store schema of `binding` at incarnation [`INCARNATION`].
    fn schema(&self, binding: &[u8]) {
        let made = PostgresHandles::create(
            &PostgresSelection::new(self.url(), "layerfs"),
            StoragePolicy::frozen_default(),
            &HistoryCatalogConfig {
                binding_key: binding.to_vec(),
                cursor_key: [71; 32],
                incarnation: INCARNATION,
            },
        )
        .unwrap();
        drop(made);
    }

    /// The schema dropped and made again elsewhere, at the same
    /// incarnation, for another binding.
    fn reinitialize(&self, binding: &[u8]) {
        let mut client = postgres::Client::connect(&self.url(), postgres::NoTls).unwrap();
        client.batch_execute("DROP SCHEMA layerfs CASCADE").unwrap();
        drop(client);
        self.schema(binding);
    }

    /// Backends other than this test's admin connections.
    fn backends(&self) -> i64 {
        let mut admin = postgres::Client::connect(&self.source, postgres::NoTls).unwrap();
        let row = admin
            .query_one(
                "SELECT count(*) FROM pg_stat_activity WHERE datname = $1 \
                 AND pid <> pg_backend_pid()",
                &[&self.name],
            )
            .unwrap();
        row.get(0)
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

/// Store `pgs` with project `app` on a fresh database, its record at login
/// revision `r1`, and the arguments a host worker opens it with.
fn connected(source: &str) -> (Database, PathBuf, OpenArgs) {
    let database = Database::create(source);
    let url = database.url();
    let home = tempdir("home");
    home::ensure(&home).unwrap();
    let read: postgres::Config = url.parse().expect("a URL");
    let user = read.get_user().unwrap_or("postgres");
    let password = read.get_password().map_or("unused".to_owned(), |password| {
        String::from_utf8(password.to_vec()).expect("a UTF-8 password")
    });
    layerfs_store::postgresql::login::save(&home, "pgs", &password).unwrap();
    let address = match layerfs_store::postgresql::connect::holds_login(&url) {
        false => url.clone(),
        true => {
            let (scheme, after) = url.split_once("://").unwrap();
            format!(
                "{scheme}://{}",
                after.split_once('@').map_or(after, |(_, at)| at)
            )
        }
    };
    StoreRecord::connected("pgs", &address, user, "layerfs", "local")
        .with("binding", SdkValue::Bytes(BINDING.to_vec()))
        .with("cursor", SdkValue::Bytes(vec![71; 32]))
        .with("login_revision", SdkValue::text("r1"))
        .write(&home)
        .unwrap();
    let folder = tempdir("folder");
    std::fs::write(folder.join("a.txt"), b"a\n").unwrap();
    let mut sink = |_| {};
    let mut events = layerfs_home::events::Events::new(&mut sink);
    init(
        &home,
        &mut events,
        &ProjectInit {
            project: ProjectRef::parse("pgs/app").unwrap(),
            folder: folder.display().to_string(),
            timeout: Some(60_000),
        },
    )
    .expect("project.init on the PostgreSQL Store");
    let record = StoreRecord::read(&home, "pgs").unwrap().unwrap();
    let open = OpenArgs::of_record(&home, "pgs", &record)
        .unwrap()
        .with_project("app");
    (database, home, open)
}

fn ok(_: &layerfs_store_service::StoreService) -> layerfs_home::outcome::Outcome {
    Ok(SdkValue::Null)
}

#[test]
fn a_late_call_at_an_old_revision_leaves_the_current_sessions() {
    let Some(source) = url() else { return };
    let _serial = serial();
    // the sessions are process-wide: none of another test's are left
    pool::close_idle(Duration::ZERO);
    let (database, home, open) = connected(&source);
    pool::run(&home, "pgs", "r1", &open, ok).unwrap();
    assert_eq!(pool::counts().idle, 1);
    let before = database.backends();

    // admitted at r0 before the record moved to r1, served after
    let ran = AtomicBool::new(false);
    let late = pool::run(&home, "pgs", "r0", &open, |_| {
        ran.store(true, Ordering::SeqCst);
        Ok(SdkValue::Null)
    });
    assert_eq!(late.unwrap_err().code, "Conflict");
    assert!(!ran.load(Ordering::SeqCst), "nothing of the late call ran");

    // a retire naming the current revision changes nothing; an old one
    // closes nothing of the current login
    assert_eq!(pool::retire(&home, "pgs", "r1").unwrap(), 0);
    assert_eq!(pool::retire(&home, "pgs", "r0").unwrap(), 0);
    pool::run(&home, "pgs", "r1", &open, ok).unwrap();
    assert_eq!(pool::counts().idle, 1, "the r1 session is still held");
    assert_eq!(database.backends(), before, "no new login");

    // once the record moves on, the old login retires
    let record = StoreRecord::read(&home, "pgs").unwrap().unwrap();
    record
        .with("login_revision", SdkValue::text("r2"))
        .write(&home)
        .unwrap();
    assert_eq!(pool::retire(&home, "pgs", "r1").unwrap(), 1);
    assert_eq!(pool::counts().stores, 0, "nothing of the old login is kept");
}

#[test]
fn a_panicking_call_gives_its_session_slot_back() {
    let Some(source) = url() else { return };
    let _serial = serial();
    // the sessions are process-wide: none of another test's are left
    pool::close_idle(Duration::ZERO);
    let (_database, home, open) = connected(&source);
    for _ in 0..=pool::PER_STORE {
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pool::run(&home, "pgs", "r1", &open, |_| panic!("a call that panics"))
        }));
        assert!(panicked.is_err());
    }
    assert_eq!(pool::counts().lent, 0, "every slot came back");
    let (sent, received) = std::sync::mpsc::channel();
    let (home, open) = (Arc::new(home), Arc::new(open));
    std::thread::spawn(move || sent.send(pool::run(&home, "pgs", "r1", &open, ok)));
    let answer = received
        .recv_timeout(Duration::from_secs(20))
        .expect("a later call is served, not left waiting");
    answer.unwrap();
}

#[test]
fn a_reinitialized_schema_fails_the_held_session_check() {
    let Some(source) = url() else { return };
    let _serial = serial();
    // the sessions are process-wide: none of another test's are left
    pool::close_idle(Duration::ZERO);
    let (database, home, open) = connected(&source);
    pool::run(&home, "pgs", "r1", &open, ok).unwrap();
    // the same incarnation, another catalog
    database.reinitialize(b"another-binding");
    let ran = AtomicBool::new(false);
    let answer = pool::run(&home, "pgs", "r1", &open, |_| {
        ran.store(true, Ordering::SeqCst);
        Ok(SdkValue::Null)
    });
    assert!(
        !ran.load(Ordering::SeqCst),
        "no call is served from another catalog"
    );
    assert!(answer.is_err(), "the fresh open refuses the other binding");
    assert_eq!(pool::counts().stores, 0);
}

#[test]
fn a_store_with_no_session_forgets_its_login() {
    let Some(source) = url() else { return };
    let _serial = serial();
    // the sessions are process-wide: none of another test's are left
    pool::close_idle(Duration::ZERO);
    let (_database, home, open) = connected(&source);
    pool::run(&home, "pgs", "r1", &open, ok).unwrap();
    assert_eq!(pool::counts().stores, 1);
    assert_eq!(pool::close_idle(Duration::ZERO), 1);
    assert_eq!(pool::counts(), pool::Counts::default());
}

#[test]
fn reservations_keep_both_bounds_and_never_wait() {
    let Some(source) = url() else { return };
    let _serial = serial();
    // the sessions are process-wide: none of another test's are left
    pool::close_idle(Duration::ZERO);
    let (_database, home, open) = connected(&source);
    pool::run(&home, "pgs", "r1", &open, ok).unwrap();
    assert_eq!(pool::counts().idle, 1);
    // the first slot takes the idle session; three more are room
    let slots: Vec<_> = (0..pool::PER_STORE)
        .map(|_| pool::reserve(&home, "pgs").expect("a free slot"))
        .collect();
    assert_eq!(pool::counts().lent, pool::PER_STORE);
    assert_eq!(pool::counts().idle, 0);
    // a full Store answers at once, with no slot
    let began = std::time::Instant::now();
    assert!(pool::reserve(&home, "pgs").is_none());
    assert!(began.elapsed() < Duration::from_secs(1));
    let refused = pool::run(&home, "pgs", "r1", &open, ok).unwrap_err();
    assert_eq!(refused.code, "Unavailable");
    // unused slots go back, and the session one took is kept
    drop(slots);
    assert_eq!(pool::counts().lent, 0);
    assert_eq!(pool::counts().idle, 1);
    pool::close_idle(Duration::ZERO);
}

#[test]
fn three_stores_meet_the_process_bound_and_evict_idle_sessions() {
    let Some(source) = url() else { return };
    let _serial = serial();
    // the sessions are process-wide: none of another test's are left
    pool::close_idle(Duration::ZERO);
    let (_one, first, _) = connected(&source);
    let (two, second, open) = connected(&source);
    let (_three, third, _) = connected(&source);

    // Store two keeps three idle sessions: three calls at once
    let together = Arc::new(std::sync::Barrier::new(3));
    let (second, open) = (Arc::new(second), Arc::new(open));
    let calls: Vec<_> = (0..3)
        .map(|_| {
            let (home, open, together) = (second.clone(), open.clone(), together.clone());
            std::thread::spawn(move || {
                pool::run(&home, "pgs", "r1", &open, |_| {
                    together.wait();
                    Ok(SdkValue::Null)
                })
            })
        })
        .collect();
    for call in calls {
        call.join().unwrap().unwrap();
    }
    assert_eq!(pool::counts().idle, 3);
    assert_eq!(two.backends(), 3);

    // Store one takes its four slots; a fifth is refused at once
    let ones: Vec<_> = (0..pool::PER_STORE)
        .map(|_| pool::reserve(&first, "pgs").expect("a slot of Store one"))
        .collect();
    assert!(
        pool::reserve(&first, "pgs").is_none(),
        "the per-Store bound"
    );
    // Store three fills the process's eight; its next slot closes Store
    // two's least recently used idle session, without waiting
    let three = pool::reserve(&third, "pgs").expect("room under the bound");
    assert_eq!(pool::counts().lent + pool::counts().idle, pool::PER_PROCESS);
    let began = std::time::Instant::now();
    let evicting = pool::reserve(&third, "pgs").expect("an idle session evicted");
    assert!(began.elapsed() < Duration::from_secs(1));
    assert_eq!(
        pool::counts().idle,
        2,
        "one idle session of Store two closed"
    );
    drop(evicting);
    let end = std::time::Instant::now() + Duration::from_secs(10);
    while two.backends() != 2 && std::time::Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(two.backends(), 2, "the evicted session's backend ended");
    // with no idle session left to close, a full process refuses at once
    let held: Vec<_> = (0..2)
        .map(|_| pool::reserve(&second, "pgs").expect("an idle session"))
        .collect();
    assert_eq!(pool::counts().idle, 0);
    assert_eq!(pool::counts().lent, pool::PER_PROCESS - 1);
    let last = pool::reserve(&third, "pgs").expect("the eighth slot");
    assert!(pool::reserve(&third, "pgs").is_none(), "the process bound");
    drop((ones, three, held, last));
    assert_eq!(pool::counts().lent, 0);
    pool::close_idle(Duration::ZERO);
}

#[test]
fn an_upgrade_renews_the_login_revision() {
    let Some(source) = url() else { return };
    let _serial = serial();
    // the sessions are process-wide: none of another test's are left
    pool::close_idle(Duration::ZERO);
    let (_database, home, open) = connected(&source);
    pool::run(&home, "pgs", "r1", &open, ok).unwrap();
    assert_eq!(pool::counts().idle, 1);
    assert_eq!(
        pool::retire(&home, "pgs", "r1").unwrap(),
        0,
        "r1 is current"
    );
    let op = layerfs_home::op::Op::begin("upgrade", "check", SdkValue::Null).unwrap();
    let related = layerfs_store::Related {
        store_idle: idle,
        store_idle_except: None,
        can_remove_project: idle,
    };
    layerfs_store::postgresql::family::begin(&home, "pgs", &op, related, |_| Ok(())).unwrap();
    let record = StoreRecord::read(&home, "pgs").unwrap().unwrap();
    let renewed = record.login_revision().expect("a revision").to_owned();
    assert_ne!(renewed, "r1", "the upgrade renewed the login revision");
    // the retire the upgrade sends a worker now closes the session held at
    // r1, which is no longer the record's revision
    assert_eq!(pool::retire(&home, "pgs", "r1").unwrap(), 1);
    assert_eq!(pool::counts(), pool::Counts::default());
    // and a call at the renewed revision logs in again
    pool::run(&home, "pgs", &renewed, &open, ok).unwrap();
    assert_eq!(pool::counts().idle, 1);
    pool::close_idle(Duration::ZERO);
}

fn idle(_: &std::path::Path, _: &str) -> Result<(), layerfs_bridge::control::SdkRefusal> {
    Ok(())
}
