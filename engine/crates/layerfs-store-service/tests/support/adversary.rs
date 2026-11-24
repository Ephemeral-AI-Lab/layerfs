//! #768 adversarial Commit tests: one disposable Store on SQLite or
//! PostgreSQL, Store services opened as the daemon opens them, Workspace
//! changes at a fixed time (so equal change sequences build equal roots on
//! both backends), tree walks through a freshly bound Workspace, and the
//! history and integrity checks every adversarial test ends with. Everything
//! here goes through public entry points: `open_store_observed`,
//! `StoreService`, `BoundWorkspace::commit_captured`, the Workspace view, the
//! persistence `Handles` and their `diagnose`.
//!
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL` and reports
//! NOT_RUN without it; each Store lives in its own schema, dropped at the end.
use layerfs_content::filesystem::{PathName, SymlinkTarget};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_history::{
    BranchId, CommitId, CommitOutcome, CommitRecord, ForkRequest, ForkSource, HistoryCatalog,
    HistoryCatalogConfig, HistoryName, LayerId, LayerStackId, WorkspaceId,
};
use layerfs_persistence::diagnostic::{Options, Outcome};
use layerfs_persistence::{
    Handles, PersistenceConfig, PostgresHandles, PostgresSelection, SqliteAcquisitionSchema,
    SqliteHandles, SqlitePersistenceProfile, StoreSelection,
};
use layerfs_project::{init, InitRequest};
use layerfs_storage::{ReservationBlocks, Storage, StoragePolicy};
use layerfs_store_service::bootstrap::open_store_observed;
use layerfs_store_service::store::{
    BindRequest, BoundWorkspace, CommitFailure, CommitOptions, CommitSuccess,
};
use layerfs_store_service::write::mutation::Mutation;
use layerfs_store_service::{Owner, OwnerConfig, StoreService};
use layerfs_telemetry::timer::Timing;
use layerfs_workspace::{Operation, Outcome as Applied, Position, Time};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub const CURSOR: [u8; 32] = [97; 32];
pub const PROJECT: &str = "adv";
/// The fixed mutation time: equal change sequences build equal roots.
pub const NOW: Time = Time {
    seconds: 123,
    nanoseconds: 0,
};

/// Sets `TMPDIR` to the target's test directory once, as the crate's
/// other Commit tests do.
pub fn tmp() {
    static SET: std::sync::Once = std::sync::Once::new();
    SET.call_once(|| std::env::set_var("TMPDIR", env!("CARGO_TARGET_TMPDIR")));
}

static SERIAL: AtomicU64 = AtomicU64::new(1);

fn next() -> u64 {
    SERIAL.fetch_add(1, Ordering::Relaxed)
}

/// The PostgreSQL test server's URL, or `None` with the NOT_RUN note.
pub fn pg_url() -> Option<String> {
    let url = std::env::var("LAYERFS_TEST_POSTGRES_URL").ok();
    if url.is_none() {
        eprintln!("NOT_RUN: LAYERFS_TEST_POSTGRES_URL is unset");
    }
    url
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Sqlite,
    Postgres,
}

/// One disposable Store holding project `adv`, initialized from `seed`.
pub struct TestStore {
    pub kind: Kind,
    /// This Store's own history binding key: PostgreSQL advisory keys
    /// derive from it, so Stores in one test database never share one.
    pub binding: Vec<u8>,
    pub dir: PathBuf,
    pub selection: StoreSelection,
    /// The server URL and schema of a PostgreSQL Store.
    pub postgres: Option<(String, String)>,
    pub stack: LayerStackId,
    pub layer: LayerId,
}

fn binding() -> Vec<u8> {
    format!("commit-adversary-{}-{}", std::process::id(), next()).into_bytes()
}

fn catalog(binding: &[u8]) -> HistoryCatalogConfig {
    HistoryCatalogConfig {
        binding_key: binding.to_vec(),
        cursor_key: CURSOR,
        incarnation: 1,
    }
}

fn scratch(label: &str) -> PathBuf {
    tmp();
    let dir = std::env::temp_dir().join(format!(
        "lfs768adv-{label}-{}-{}",
        std::process::id(),
        next()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn initialize(
    dir: &Path,
    storage: &Storage,
    history: &dyn HistoryCatalog,
    acquisition: &dyn layerfs_storage::port::acquisition::Acquisition,
    seed: &[(&str, &[u8])],
) -> (LayerStackId, LayerId) {
    let source = dir.join("source");
    std::fs::create_dir_all(&source).unwrap();
    for (path, bytes) in seed {
        let path = source.join(path.trim_start_matches('/'));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    // One fixed mtime on every seed entry, the directories last: two Stores
    // initialized from equal seeds hold equal roots, whenever they ran.
    let fixed = std::time::UNIX_EPOCH + Duration::from_secs(1_000_000);
    let mut entries = vec![source.clone()];
    let mut at = 0;
    while at < entries.len() {
        if entries[at].is_dir() {
            for entry in std::fs::read_dir(&entries[at]).unwrap() {
                entries.push(entry.unwrap().path());
            }
        }
        at += 1;
    }
    for entry in entries.iter().rev() {
        std::fs::File::open(entry)
            .unwrap()
            .set_modified(fixed)
            .unwrap();
    }
    let initialized = Timing::disabled("init", |timing| {
        init(
            storage,
            history,
            InitRequest {
                source: &source,
                acquisition,
                stack: LayerStackId::from_authority([97; 16]),
                name: HistoryName::new(PROJECT).unwrap(),
                scope_seed: [97; 32],
                deadline: Instant::now() + Duration::from_secs(60),
            },
            timing,
        )
    })
    .0
    .unwrap();
    std::fs::remove_dir_all(source).unwrap();
    (initialized.stack.id, initialized.stack.head_layer)
}

impl TestStore {
    /// A sealed SQLite Store (Normal profile).
    pub fn sqlite(label: &str, seed: &[(&str, &[u8])]) -> Self {
        let dir = scratch(label);
        let config = PersistenceConfig::sqlite(dir.join("store.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Normal)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables);
        let binding = binding();
        let handles =
            SqliteHandles::create(config, StoragePolicy::frozen_default(), &catalog(&binding))
                .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let (stack, layer) =
            initialize(&dir, &storage, &handles.history, &handles.acquisition, seed);
        drop(storage);
        let sealed = handles.seal().unwrap();
        let selection = StoreSelection::Sqlite(
            PersistenceConfig::sqlite(&sealed.path)
                .with_sqlite_profile(SqlitePersistenceProfile::Normal),
        );
        Self {
            kind: Kind::Sqlite,
            binding,
            dir,
            selection,
            postgres: None,
            stack,
            layer,
        }
    }

    /// A PostgreSQL Store in its own schema, or `None` (NOT_RUN) without a
    /// server.
    pub fn postgres(label: &str, seed: &[(&str, &[u8])]) -> Option<Self> {
        let url = pg_url()?;
        let dir = scratch(label);
        let schema = format!(
            "lfs768adv_{}_{}_{}",
            label.replace('-', "_"),
            std::process::id(),
            next()
        );
        drop_schema(&url, &schema);
        let selection = PostgresSelection::new(&url, &schema);
        let binding = binding();
        let handles = PostgresHandles::create(
            &selection,
            StoragePolicy::frozen_default(),
            &catalog(&binding),
        )
        .unwrap();
        let storage = Storage::new(handles.storage.clone()).unwrap();
        let (stack, layer) =
            initialize(&dir, &storage, &handles.history, &handles.acquisition, seed);
        drop(storage);
        drop(handles);
        Some(Self {
            kind: Kind::Postgres,
            binding,
            dir,
            selection: StoreSelection::Postgres(selection),
            postgres: Some((url, schema)),
            stack,
            layer,
        })
    }

    /// SQLite always, PostgreSQL when the server is configured.
    pub fn both(label: &str, seed: &[(&str, &[u8])]) -> Vec<Self> {
        let mut stores = vec![Self::sqlite(&format!("{label}-sqlite"), seed)];
        stores.extend(Self::postgres(&format!("{label}-pg"), seed));
        stores
    }

    /// This Store's PostgreSQL selection reached through `url` instead.
    pub fn selection_via(&self, url: &str) -> StoreSelection {
        match &self.selection {
            StoreSelection::Postgres(selection) => {
                let mut selection = selection.clone();
                selection.url = url.to_owned();
                StoreSelection::Postgres(selection)
            }
            StoreSelection::Sqlite(_) => panic!("only a PostgreSQL Store has a URL"),
        }
    }

    /// The selection with another lane wait (PostgreSQL only).
    pub fn with_lock_timeout(&self, selection: &StoreSelection, ms: u32) -> StoreSelection {
        match selection {
            StoreSelection::Postgres(selection) => {
                let mut selection = selection.clone();
                selection.lock_timeout_ms = ms;
                StoreSelection::Postgres(selection)
            }
            other => other.clone(),
        }
    }

    pub fn handles(&self) -> Handles {
        Handles::open_writable(&self.selection, &self.binding, CURSOR).unwrap()
    }

    /// The Branch id of `name` in this project.
    pub fn branch(&self, name: &str) -> BranchId {
        BranchId::named(self.stack, &HistoryName::new(name).unwrap())
    }

    /// Forks Branch `name` from the project's initial Layer on a session
    /// of its own, which is closed again.
    pub fn fork(&self, name: &str) -> BranchId {
        let branch = self.branch(name);
        let handles = self.handles();
        history(&handles)
            .fork(&ForkRequest {
                stack: self.stack,
                branch,
                name: HistoryName::new(name).unwrap(),
                source: ForkSource::Layer(self.layer),
            })
            .unwrap();
        handles.close();
        branch
    }

    /// Opens a Store service on this Store, as the daemon opens it.
    pub fn open(&self) -> Service {
        Service::open(self, self.selection.clone())
    }

    /// An admin connection to the server (PostgreSQL only), with the
    /// Store's schema first on its search path.
    pub fn admin(&self) -> postgres::Client {
        let (url, schema) = self.postgres.as_ref().expect("a PostgreSQL Store");
        let mut client = postgres::Client::connect(url, postgres::NoTls).unwrap();
        client
            .batch_execute(&format!("SET search_path TO {schema}"))
            .unwrap();
        client
    }

    /// Rows per object id in `object_location` that appear more than once,
    /// read straight from the private test database.
    /// True once no backend of this Store (PostgreSQL) sits idle in a
    /// transaction, waiting up to 10 s: one holding a lock on this Store's
    /// schema or whose last statement named it. Other tests' Stores in the
    /// same database are not counted.
    pub fn no_open_transaction(&self) -> bool {
        let schema = self
            .postgres
            .as_ref()
            .expect("a PostgreSQL Store")
            .1
            .clone();
        let mut admin = self.admin();
        wait_until(Duration::from_secs(10), || {
            admin
                .query_one(
                    "SELECT count(*) FROM pg_stat_activity a WHERE a.datname = current_database() \
                     AND a.pid <> pg_backend_pid() AND a.state LIKE 'idle in transaction%' \
                     AND (position($1 || '.' IN coalesce(a.query, '')) > 0 OR EXISTS ( \
                     SELECT 1 FROM pg_locks l JOIN pg_class c ON c.oid = l.relation \
                     JOIN pg_namespace n ON n.oid = c.relnamespace \
                     WHERE l.pid = a.pid AND n.nspname = $1))",
                    &[&schema],
                )
                .unwrap()
                .get::<_, i64>(0)
                == 0
        })
    }

    pub fn duplicate_locators(&self) -> i64 {
        const SQL: &str = "SELECT count(*) FROM (SELECT object_id FROM object_location \
                           GROUP BY object_id HAVING count(*) > 1) d";
        match self.kind {
            Kind::Postgres => self.admin().query_one(SQL, &[]).unwrap().get(0),
            Kind::Sqlite => {
                let StoreSelection::Sqlite(config) = &self.selection else {
                    unreachable!()
                };
                rusqlite::Connection::open(&config.path)
                    .unwrap()
                    .query_row(SQL, [], |row| row.get(0))
                    .unwrap()
            }
        }
    }

    /// Commit rows the Store holds, read straight from the private test
    /// database: with every Branch's chain, a Commit row no chain reaches
    /// would be a publication nothing answered for.
    pub fn commit_rows(&self) -> i64 {
        const SQL: &str = "SELECT count(*) FROM \"commit\"";
        match self.kind {
            Kind::Postgres => self.admin().query_one(SQL, &[]).unwrap().get(0),
            Kind::Sqlite => {
                let StoreSelection::Sqlite(config) = &self.selection else {
                    unreachable!()
                };
                rusqlite::Connection::open(&config.path)
                    .unwrap()
                    .query_row(SQL, [], |row| row.get(0))
                    .unwrap()
            }
        }
    }

    /// The Branch head and its first-parent chain, newest first, from a
    /// fresh read-only session.
    pub fn chain(&self, branch: BranchId) -> Vec<CommitRecord> {
        let handles = Handles::open_read_only(&self.selection, &self.binding, CURSOR).unwrap();
        let chain = chain_of(history(&handles), branch);
        handles.close();
        chain
    }

    /// The tree Branch `branch` holds, read through a Workspace bound on it
    /// by a fresh Store service.
    pub fn tree(&self, branch: BranchId) -> Tree {
        let service = self.open();
        let workspace = service.bind(branch);
        let tree = tree_of(&workspace);
        service.unmount(workspace);
        service.stop();
        tree
    }

    /// The Store's own audit (`diagnose`, full) on a fresh session: every
    /// `Fail` observation, as text. Bounded by its deadline.
    pub fn integrity_failures(&self) -> Vec<String> {
        let handles = Handles::open_read_only(&self.selection, &self.binding, CURSOR).unwrap();
        let mut failures = Vec::new();
        let options = Options {
            quick: false,
            deadline: Some(Instant::now() + Duration::from_secs(30)),
            ..Options::default()
        };
        let mut observe = |o: layerfs_persistence::diagnostic::Observation| {
            if o.outcome == Outcome::Fail {
                failures.push(format!("{}: {}", o.id, o.message));
            }
        };
        let done = match &handles {
            Handles::Sqlite(h) => h.diagnose(options, &mut observe),
            Handles::Postgres(h) => h.diagnose(options, &mut observe),
        };
        if let Err(error) = done {
            failures.push(format!("diagnose: {error:?}"));
        }
        handles.close();
        failures
    }
}

impl Drop for TestStore {
    fn drop(&mut self) {
        if let Some((url, schema)) = &self.postgres {
            drop_schema(url, schema);
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn drop_schema(url: &str, schema: &str) {
    if let Ok(mut client) = postgres::Client::connect(url, postgres::NoTls) {
        let _ = client.batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"));
    }
}

pub fn history(handles: &Handles) -> &dyn HistoryCatalog {
    match handles {
        Handles::Sqlite(h) => &h.history,
        Handles::Postgres(h) => &h.history,
    }
}

/// The head and its first parents, newest first.
pub fn chain_of(history: &dyn HistoryCatalog, branch: BranchId) -> Vec<CommitRecord> {
    let mut out = Vec::new();
    let mut at = history.branch(branch).unwrap().unwrap().head_commit;
    while let Some(id) = at {
        assert!(out.len() < 100_000, "a bounded chain");
        let record = history.commit(id).unwrap().expect("a recorded parent");
        at = record.parent;
        out.push(record);
    }
    out
}

/// Every Commit appears once in `chain`, and each one's parent is the next.
pub fn assert_linear(chain: &[CommitRecord]) {
    let mut seen = std::collections::HashSet::new();
    for (n, record) in chain.iter().enumerate() {
        assert!(seen.insert(record.id), "Commit {} appears twice", record.id);
        assert_eq!(
            record.parent,
            chain.get(n + 1).map(|parent| parent.id),
            "first-parent chain"
        );
    }
}

/// Checks one Commit attempt on `branch`, whose head was `old`, against the
/// Branch from fresh sessions. `first` is the attempt's answer and `follow`
/// the answer of one later Commit on the same Workspace with no new change.
///
/// The Branch is exactly at `old`, or exactly one Commit past it whose tree
/// is `expected`; a Commit answered published is that head; a definite
/// refusal left `old` unless the follow-up published; an unknown outcome
/// allows either state and the follow-up must be refused. Returns "old" or
/// "new".
pub fn check_attempt(
    store: &TestStore,
    branch: BranchId,
    old: Option<CommitId>,
    expected: &Tree,
    first: &Verdict,
    follow: &Verdict,
    label: &str,
) -> &'static str {
    let chain = store.chain(branch);
    assert_linear(&chain);
    let label = format!("{label}: first {first:?}, follow {follow:?}");
    if let Verdict::Unknown(_) = first {
        assert!(
            matches!(follow, Verdict::Refused(_) | Verdict::Unknown(_)),
            "{label}: a Commit after an unknown outcome must be refused"
        );
    }
    let head = chain.first();
    if head.map(|r| r.id) == old {
        assert!(
            matches!(first, Verdict::Refused(_) | Verdict::Unknown(_)),
            "{label}: the call answered a publication the Branch does not hold"
        );
        assert!(
            !matches!(follow, Verdict::Committed(_)),
            "{label}: the follow-up answered a publication the Branch does not hold"
        );
        "old"
    } else {
        let head = head.expect("a head past the old one");
        assert_eq!(
            head.parent, old,
            "{label}: exactly one Commit past the old head"
        );
        assert_eq!(
            &store.tree(branch),
            expected,
            "{label}: the new head's tree is the intended change"
        );
        let allowed = match first {
            Verdict::Committed(id) | Verdict::PublishedThenFailed(Some(id)) => *id == head.id,
            Verdict::PublishedThenFailed(None) | Verdict::Unknown(_) => true,
            Verdict::Refused(_) => *follow == Verdict::Committed(head.id),
            Verdict::UpToDate | Verdict::Conflicted => false,
        };
        assert!(allowed, "{label}: head {} not explained", head.id);
        if matches!(first, Verdict::Committed(_)) {
            assert!(
                !matches!(follow, Verdict::Committed(_)),
                "{label}: the follow-up published again"
            );
        }
        "new"
    }
}

/// One Store open as the daemon opens it, with its overlay owner.
pub struct Service {
    pub service: StoreService,
    pub owner: Owner,
}

impl Service {
    /// A service on `store` through `selection` (the Store's own, or one
    /// reaching it another way).
    pub fn open(store: &TestStore, selection: StoreSelection) -> Self {
        Self::try_open(store, selection).unwrap_or_else(|error| panic!("open: {error}"))
    }

    pub fn try_open(store: &TestStore, selection: StoreSelection) -> Result<Self, String> {
        let dir = &store.dir;
        let opened = open_store_observed(
            selection,
            &store.binding,
            CURSOR,
            OwnerConfig::default().namespaces,
            64 * 1024 * 1024,
            ReservationBlocks::default(),
        )
        .map_err(|error| format!("{error:?}"))?;
        let overlay = dir.join(format!("overlay-{}", next()));
        std::fs::create_dir_all(&overlay).unwrap();
        let owner = Owner::start(
            &overlay.join("overlay.sqlite"),
            layerfs_overlay::ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let service = StoreService::new(opened, &owner);
        Ok(Self { service, owner })
    }

    pub fn bind(&self, branch: BranchId) -> BoundWorkspace {
        self.try_bind(branch)
            .unwrap_or_else(|error| panic!("bind: {error}"))
    }

    pub fn try_bind(&self, branch: BranchId) -> Result<BoundWorkspace, String> {
        let mut id = [0x5a; 32];
        id[..8].copy_from_slice(&next().to_le_bytes());
        self.service
            .store
            .bind(
                self.owner.client(),
                BindRequest {
                    branch,
                    workspace: WorkspaceId::from_authority(id).unwrap(),
                },
            )
            .map(|bound| bound.workspace)
            .map_err(|error| format!("{error:?}"))
    }

    /// Closes the Workspace and retires its overlay namespace; a failure is
    /// returned, not raised (a quarantined Store may refuse it).
    pub fn try_unmount(&self, workspace: BoundWorkspace) -> Result<(), String> {
        let namespace = workspace.route().namespace();
        workspace.close().map_err(|error| format!("{error:?}"))?;
        self.owner
            .client()
            .retire_namespace(namespace)
            .map_err(|error| format!("{error:?}"))
    }

    pub fn unmount(&self, workspace: BoundWorkspace) {
        self.try_unmount(workspace).unwrap();
    }

    pub fn stop(self) {
        drop(self.service);
        self.owner.stop().unwrap();
    }

    /// Stops without asserting: a quarantined Store's owner may report it.
    pub fn abandon(self) {
        drop(self.service);
        let _ = self.owner.stop();
    }

    /// An SDK Store call on project `adv`.
    pub fn call(
        &self,
        method: &str,
        fields: Vec<(&str, layerfs_bridge::control::SdkValue)>,
    ) -> Result<layerfs_bridge::control::SdkValue, layerfs_bridge::control::SdkRefusal> {
        let mut fields: Vec<(String, layerfs_bridge::control::SdkValue)> = fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect();
        fields.push((
            "project".into(),
            layerfs_bridge::control::SdkValue::text(PROJECT),
        ));
        let call = layerfs_bridge::control::SdkCall {
            method: method.into(),
            args: layerfs_bridge::control::SdkValue::Map(fields),
        };
        layerfs_store_service::methods::call(&self.service, &call)
    }
}

/// One Workspace Commit, as the daemon's `workspace.commit` runs it.
pub fn commit(workspace: &BoundWorkspace) -> Result<CommitSuccess, Box<CommitFailure>> {
    workspace.commit_captured(&CommitOptions {
        force: false,
        message: Some("adversary".to_owned()),
        expected: None,
    })
}

/// One forced Workspace Commit (`--force`): never fused, it takes the lane
/// on the separate path (the held-lane connection on PostgreSQL).
pub fn commit_forced(workspace: &BoundWorkspace) -> Result<CommitSuccess, Box<CommitFailure>> {
    workspace.commit_captured(&CommitOptions {
        force: true,
        message: Some("adversary forced".to_owned()),
        expected: None,
    })
}

/// What one Commit call answered, by the classes its contract names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Verdict {
    /// A new Commit is published.
    Committed(CommitId),
    /// Nothing to publish.
    UpToDate,
    /// A conflicted merge was installed locally; nothing published.
    Conflicted,
    /// A definite refusal: nothing in history changed.
    Refused(String),
    /// The publication is known, a later local step failed.
    PublishedThenFailed(Option<CommitId>),
    /// Unknown outcome: either state is allowed.
    Unknown(String),
}

pub fn verdict(result: &Result<CommitSuccess, Box<CommitFailure>>) -> Verdict {
    match result {
        Ok(success) => match &success.history {
            Some(CommitOutcome::Committed { record, .. }) => Verdict::Committed(record.id),
            Some(CommitOutcome::UpToDate { .. }) => Verdict::UpToDate,
            None => Verdict::Conflicted,
        },
        Err(failure) => match &failure.published {
            Some(CommitOutcome::Committed { record, .. }) => {
                Verdict::PublishedThenFailed(Some(record.id))
            }
            Some(_) => Verdict::PublishedThenFailed(None),
            None if failure.uncertain() => Verdict::Unknown(failure.to_string()),
            None => Verdict::Refused(failure.to_string()),
        },
    }
}

/// One change at a path, applied at [`NOW`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Op {
    /// Create or replace a regular file's bytes.
    Write(String, Vec<u8>),
    /// Append to an existing regular file.
    Append(String, Vec<u8>),
    Mkdir(String),
    Symlink(String, Vec<u8>),
    /// Rename, replacing a file at the destination.
    Rename(String, String),
    Unlink(String),
    Rmdir(String),
}

fn split(path: &str) -> (&str, &str) {
    let (parent, name) = path.rsplit_once('/').expect("an absolute path");
    (if parent.is_empty() { "/" } else { parent }, name)
}

fn name(text: &str) -> PathName {
    PathName::new(text).unwrap()
}

/// The bytes of one write operation: well inside the Workspace's window.
const WINDOW: usize = 32 * 1024;

/// Applies `op` to the Workspace's live view; a refusal is returned as text.
pub fn apply(workspace: &BoundWorkspace, op: &Op) -> Result<(), String> {
    let operation = workspace.operation().map_err(|e| format!("{e:?}"))?;
    let route = operation.workspace().route();
    let source = operation
        .overlay()
        .acquire_base_source(route, 9301)
        .map_err(|e| format!("{e:?}"))?;
    let view = operation
        .workspace()
        .view_for_source(source)
        .map_err(|e| format!("{e:?}"))?;
    let lookup = |path: &str| -> Result<Option<u64>, String> {
        let mut serial = view.root_serial();
        for part in path.split('/').filter(|part| !part.is_empty()) {
            match view.lookup(operation.overlay(), serial, &name(part)) {
                Ok(stat) => serial = stat.serial,
                Err(
                    layerfs_workspace::WorkspaceError::Content(
                        layerfs_content::ContentError::PathNotFound,
                    )
                    | layerfs_workspace::WorkspaceError::Refused(layerfs_workspace::Refusal::Missing),
                ) => return Ok(None),
                Err(error) => return Err(format!("{path}: {error:?}")),
            }
        }
        Ok(Some(serial))
    };
    let must = |path: &str| -> Result<u64, String> {
        lookup(path)?.ok_or_else(|| format!("{path}: absent"))
    };
    let run = |op: Operation| -> Result<(), String> {
        match operation
            .workspace()
            .mutate(operation.overlay(), operation.ports(), &view, op, NOW)
            .map_err(|error| format!("{error:?}"))?
        {
            Applied::Applied { publication, .. } => operation
                .overlay()
                .reply_attempted(publication)
                .map_err(|error| format!("{error:?}")),
            Applied::Unchanged { .. } => Ok(()),
        }
    };
    let done = (|| match op {
        Op::Write(path, bytes) => {
            let serial = match lookup(path)? {
                Some(serial) => {
                    run(Operation::SetAttributes {
                        serial,
                        mode: None,
                        mtime: None,
                        size: Some(0),
                    })?;
                    serial
                }
                None => {
                    let (parent, file) = split(path);
                    run(Operation::Create {
                        parent: must(parent)?,
                        name: name(file),
                        mode: 0o644,
                    })?;
                    must(path)?
                }
            };
            for (n, piece) in bytes.chunks(WINDOW).enumerate() {
                run(Operation::Write {
                    serial,
                    position: Position::At((n * WINDOW) as u64),
                    data: piece.to_vec().into(),
                })?;
            }
            Ok(())
        }
        Op::Append(path, bytes) => {
            let serial = must(path)?;
            for piece in bytes.chunks(WINDOW) {
                run(Operation::Write {
                    serial,
                    position: Position::End,
                    data: piece.to_vec().into(),
                })?;
            }
            Ok(())
        }
        Op::Mkdir(path) => {
            let (parent, dir) = split(path);
            run(Operation::Mkdir {
                parent: must(parent)?,
                name: name(dir),
                mode: 0o755,
            })
        }
        Op::Symlink(path, target) => {
            let (parent, link) = split(path);
            run(Operation::Symlink {
                parent: must(parent)?,
                name: name(link),
                target: SymlinkTarget::new(target.clone()).unwrap(),
            })
        }
        Op::Rename(from, to) => {
            let (parent, old) = split(from);
            let (new_parent, new) = split(to);
            run(Operation::Rename {
                parent: must(parent)?,
                name: name(old),
                new_parent: must(new_parent)?,
                new_name: name(new),
                replace: true,
                destination_path: Some(
                    new_parent
                        .split('/')
                        .filter(|part| !part.is_empty())
                        .map(name)
                        .collect(),
                ),
            })
        }
        Op::Unlink(path) => {
            let (parent, file) = split(path);
            run(Operation::Unlink {
                parent: must(parent)?,
                name: name(file),
            })
        }
        Op::Rmdir(path) => {
            let (parent, dir) = split(path);
            run(Operation::Rmdir {
                parent: must(parent)?,
                name: name(dir),
            })
        }
    })();
    drop(view);
    operation
        .overlay()
        .release_base_source(route, source)
        .map_err(|e| format!("{e:?}"))?;
    done
}

pub fn write(workspace: &BoundWorkspace, path: &str, bytes: &[u8]) {
    apply(workspace, &Op::Write(path.to_owned(), bytes.to_vec()))
        .unwrap_or_else(|error| panic!("write {path}: {error}"));
}

/// One entry of a tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Node {
    File(Vec<u8>),
    Dir,
    Link(Vec<u8>),
}

pub type Tree = BTreeMap<String, Node>;

/// Every entry below the root of the Workspace's live view.
pub fn tree_of(workspace: &BoundWorkspace) -> Tree {
    let m = Mutation::new(workspace).unwrap();
    let mut out = Tree::new();
    let mut stack = vec![(String::new(), m.resolve("/").unwrap().serial)];
    while let Some((path, serial)) = stack.pop() {
        let mut after: Option<Vec<u8>> = None;
        loop {
            let page = m
                .view
                .list(m.op.overlay(), serial, after.as_deref())
                .unwrap();
            for (entry, child) in page.entries {
                let child_path = format!("{path}/{}", String::from_utf8(entry).unwrap());
                let stat = m.view.stat(m.op.overlay(), child).unwrap();
                let node = match stat.kind {
                    InodeKind::Directory => {
                        stack.push((child_path.clone(), child));
                        Node::Dir
                    }
                    InodeKind::Symlink => Node::Link(
                        m.view
                            .readlink(m.op.overlay(), child)
                            .unwrap()
                            .as_bytes()
                            .to_vec(),
                    ),
                    _ => {
                        let mut bytes = Vec::with_capacity(stat.logical_len as usize);
                        while (bytes.len() as u64) < stat.logical_len {
                            let piece = m.read(child, bytes.len() as u64, 64 * 1024).unwrap();
                            assert!(!piece.is_empty(), "{child_path}: short read");
                            bytes.extend_from_slice(&piece);
                        }
                        Node::File(bytes)
                    }
                };
                out.insert(child_path, node);
            }
            after = page.continuation;
            if after.is_none() {
                break;
            }
        }
    }
    m.finish().unwrap();
    out
}

/// The tree with `files` written over it.
pub fn with(mut tree: Tree, files: &[(&str, &[u8])]) -> Tree {
    for (path, bytes) in files {
        tree.insert((*path).to_owned(), Node::File(bytes.to_vec()));
    }
    tree
}

/// Waits up to `limit` for `done`, polling every 5 ms.
pub fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + limit;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// A reusable-once rendezvous whose wait is bounded: a party that never
/// arrives (it panicked) makes the others' wait fail after `GATE`, never
/// hang.
pub struct Gate {
    arrived: AtomicU64,
    parties: u64,
}

/// How long a party waits for the others at a [`Gate`].
pub const GATE: Duration = Duration::from_secs(20);

impl Gate {
    pub fn new(parties: usize) -> Self {
        Self {
            arrived: AtomicU64::new(0),
            parties: parties as u64,
        }
    }

    /// Arrives and waits (bounded) for every party; false on timeout.
    pub fn wait(&self) -> bool {
        self.arrived.fetch_add(1, Ordering::SeqCst);
        let until = Instant::now() + GATE;
        while self.arrived.load(Ordering::SeqCst) < self.parties {
            if Instant::now() >= until {
                return false;
            }
            std::hint::spin_loop();
            std::thread::yield_now();
        }
        true
    }
}

/// Deterministic printable bytes: `length` of them from `seed`.
pub fn text_bytes(seed: u64, length: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    (0..length)
        .map(|n| {
            if n % 64 == 63 {
                return b'\n';
            }
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            b'a' + (state % 26) as u8
        })
        .collect()
}

/// The seed files of a test as a tree.
pub fn seed_tree(seed: &[(&str, &[u8])]) -> Tree {
    let mut tree = Tree::new();
    for (path, bytes) in seed {
        let mut at = String::new();
        let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        for dir in &parts[..parts.len() - 1] {
            at.push('/');
            at.push_str(dir);
            tree.insert(at.clone(), Node::Dir);
        }
        tree.insert((*path).to_owned(), Node::File(bytes.to_vec()));
    }
    tree
}

/// Deterministic incompressible bytes: `length` of them from `seed`.
pub fn random_bytes(seed: u64, length: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}
