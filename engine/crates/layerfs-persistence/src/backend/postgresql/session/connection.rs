//! One PostgreSQL connection per session, with TLS and counted work.
use super::{profile::Settings, rows};
use crate::backend::postgresql::status::PostgresStatusError;
use crate::backend::{admission::Admission, records::BackendError};
use crate::{PostgresSelection, SqlWork};
use layerfs_storage::StoragePolicy;
use postgres::{Client, NoTls, SimpleQueryMessage, SimpleQueryRow, Statement};
use postgres_native_tls::MakeTlsConnector;
use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    str::FromStr,
    sync::{
        atomic::{AtomicI64, AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use tokio_postgres::{
    config::{Host, SslMode},
    Config,
};

/// Prepared statements kept per connection before the cache is cleared.
const STATEMENT_CACHE: usize = 256;

pub(crate) struct State {
    pub(crate) client: super::driver::Driver,
    statements: HashMap<String, Statement>,
    pub(crate) quarantined: bool,
    pub(crate) work: SqlWork,
}

impl State {
    /// Quarantines the session after an uncertain outcome. An ordinary
    /// connection is closed (finding #3), so the server ends its backend.
    /// Kept open, that backend stayed until this process exited: a
    /// transaction the server aborted with an error has already released its
    /// row locks, but session advisory keys survive any abort, and an
    /// uncertain outcome that left the transaction live (a lost reply, or a
    /// body error classed uncertain) kept every row lock, a write's
    /// `store_lane` row included, so every other writer met `Busy` after its
    /// lock wait. Closing ends all of it. Nothing is replayed and the outcome
    /// stays unknown; every later call is refused before it reaches a
    /// connection, and the caller opens a new session. GC intent rows are not
    /// touched: an intent's owner stays live by its recorded process, not by
    /// this connection. A diagnostic driver keeps its connection: it is
    /// read-only, holds no lane, and its deadline and cancellation outcome
    /// are still read.
    pub(crate) fn quarantine(&mut self) {
        self.quarantined = true;
        if matches!(self.client, super::driver::Driver::Ordinary(_)) {
            // Close first: a cached statement holds only a weak reference to
            // the client, so once it is closed dropping the cache queues no
            // `Close` for a silent server to leave unanswered.
            let _ = std::mem::replace(&mut self.client, super::driver::Driver::Closed).close();
            self.statements.clear();
        }
    }

    /// One prepared statement for `sql`, prepared once per connection.
    pub(crate) fn statement(&mut self, sql: &str) -> Result<Statement, super::driver::Error> {
        if let Some(statement) = self.statements.get(sql) {
            return Ok(statement.clone());
        }
        self.work.prepares += 1;
        self.work.round_trips += 1;
        let statement = self.client.prepare(sql)?;
        if self.statements.len() >= STATEMENT_CACHE {
            self.statements.clear();
        }
        self.statements.insert(sql.to_owned(), statement.clone());
        Ok(statement)
    }

    /// One prepared statement for each of `sqls`, in order. Those not yet
    /// prepared on this connection are prepared in one flight (one round
    /// trip, however many), each counted in `prepares`; every reply is
    /// collected, and the first failure is returned once the others are
    /// cached. A prepare the client never sent (unencodable, or after one
    /// that was) fails definitely ([`super::driver::Error::unsent`]). With
    /// `begin`, it is sent before the prepares and `COMMIT` after them in the
    /// same flight: a read transaction whose `SET LOCAL search_path` resolves
    /// the unqualified names of the texts, counted as one read transaction
    /// when it is sent; its `begin` or `COMMIT` failing fails the call.
    pub(crate) fn statements_in(
        &mut self,
        sqls: &[&str],
        begin: Option<&str>,
    ) -> Result<Vec<Statement>, super::driver::Error> {
        // Resolve cached statements first: inserting may clear the cache.
        let mut found: Vec<Option<Statement>> = sqls
            .iter()
            .map(|sql| self.statements.get(*sql).cloned())
            .collect();
        let mut seen = HashSet::new();
        let missing: Vec<&str> = sqls
            .iter()
            .zip(&found)
            .filter(|(sql, cached)| cached.is_none() && seen.insert(**sql))
            .map(|(sql, _)| *sql)
            .collect();
        if missing.is_empty() {
            return Ok(found.into_iter().flatten().collect());
        }
        self.work.prepares += missing.len() as u64;
        self.work.round_trips += 1;
        let answered = self.client.prepare_all(&missing, begin)?;
        if begin.is_some() {
            self.work.transactions += 1;
            self.work.commits += u64::from(answered.around.is_ok());
        }
        answered.around?;
        let (sent, replies) = (answered.sent, answered.statements);
        let mut prepared = HashMap::with_capacity(missing.len());
        let mut failed = None;
        for (index, (sql, reply)) in missing.into_iter().zip(replies).enumerate() {
            // A prepare never sent lost no reply: its failure is definite.
            let reply = reply.map_err(|e| if index < sent { e } else { e.unsent() });
            match reply {
                Ok(statement) => {
                    if self.statements.len() >= STATEMENT_CACHE {
                        self.statements.clear();
                    }
                    self.statements.insert(sql.to_owned(), statement.clone());
                    prepared.insert(sql, statement);
                }
                Err(e) => {
                    failed.get_or_insert(e);
                }
            }
        }
        if let Some(e) = failed {
            return Err(e);
        }
        for (sql, slot) in sqls.iter().zip(&mut found) {
            if slot.is_none() {
                *slot = prepared.get(sql).cloned();
            }
        }
        found
            .into_iter()
            .map(|statement| statement.ok_or(super::driver::Error::Unsent))
            .collect()
    }
}

impl Drop for State {
    /// Closes an ordinary connection in order (`Terminate`), as the
    /// synchronous client did when it was dropped.
    fn drop(&mut self) {
        let _ = std::mem::replace(&mut self.client, super::driver::Driver::Closed).close();
    }
}

/// The process's one TLS connector (#63): building one loads the system's
/// trust roots, so every connection shares it. `None` when it could not be
/// built; every connection that may negotiate TLS is then refused as before.
/// A connection string with `sslmode=disable` never builds it (#96): the
/// driver never negotiates TLS for one, and on Linux the build costs a
/// process about 40-70 ms (`postgresql_first_open`).
fn tls() -> Result<MakeTlsConnector, BackendError> {
    static TLS: OnceLock<Option<MakeTlsConnector>> = OnceLock::new();
    TLS.get_or_init(|| {
        native_tls::TlsConnector::new()
            .ok()
            .map(MakeTlsConnector::new)
    })
    .clone()
    .ok_or(BackendError::Integrity)
}

/// Connects new clients for this session's selection; TLS follows the
/// connection string's `sslmode` exactly as given (the driver's default is
/// `prefer`, which costs one SSLRequest round trip on a server without TLS);
/// nothing here changes it.
#[derive(Clone)]
pub(crate) struct Connector {
    config: Config,
    /// `None` for `sslmode=disable`, which never uses TLS.
    tls: Option<MakeTlsConnector>,
}

impl Connector {
    pub(crate) fn new(selection: &PostgresSelection) -> Result<Self, BackendError> {
        let mut config = Config::from_str(&selection.url).map_err(|_| BackendError::Integrity)?;
        if config.get_hostaddrs().is_empty() {
            if let [Host::Tcp(host)] = config.get_hosts() {
                if host.eq_ignore_ascii_case("localhost") {
                    let name = host.clone();
                    config.host(&name);
                    config.hostaddr(IpAddr::V4(Ipv4Addr::LOCALHOST));
                    config.hostaddr(IpAddr::V6(Ipv6Addr::LOCALHOST));
                }
            }
        }
        let tls = match config.get_ssl_mode() {
            SslMode::Disable => None,
            _ => Some(tls()?),
        };
        Ok(Self { config, tls })
    }
    /// One new client, made within [`HANDSHAKE`] (the URL's
    /// `connect_timeout` when it sets one): the driver bounds only the
    /// socket, so a server that accepts it and then says nothing (paused or
    /// stopped behind a live port) would hold the call forever. The dial
    /// runs on its own thread; past the limit the call answers
    /// [`Dial::Silent`] and the thread, left with no one to answer, ends
    /// when the server answers or the socket closes. One attempt, no retry.
    pub(crate) fn connect_raw(&self) -> Result<Client, Dial> {
        let limit = self.limit();
        let config = postgres::Config::from(self.config.clone());
        let tls = self.tls.clone();
        let (answer, answered) = std::sync::mpsc::sync_channel(1);
        let dial = move || {
            let _ = answer.send(match tls {
                Some(tls) => config.connect(tls),
                None => config.connect(NoTls),
            });
        };
        let thread = std::thread::Builder::new().name("layerfs-pg-dial".to_owned());
        thread
            .spawn(dial)
            .map_err(|_| Dial::Silent(Duration::ZERO))?;
        match answered.recv_timeout(limit) {
            Ok(made) => made.map_err(Dial::Driver),
            Err(_) => Err(Dial::Silent(limit)),
        }
    }
    /// The dial limit: the URL's `connect_timeout`, else [`HANDSHAKE`].
    fn limit(&self) -> Duration {
        self.config
            .get_connect_timeout()
            .copied()
            .unwrap_or(HANDSHAKE)
    }
    /// One new synchronous client (the lane, recovery and status reads).
    pub(crate) fn connect(&self) -> Result<Client, BackendError> {
        self.connect_raw().map_err(Dial::refused)
    }
    /// One new ordinary session connection, within the same limit, dialed
    /// on its own runtime instead of a thread.
    pub(crate) fn connect_ordinary(&self) -> Result<super::driver::Ordinary, BackendError> {
        super::driver::Ordinary::connect(&self.config, self.tls.as_ref(), self.limit())
            .map_err(Dial::refused)
    }
    /// One new client for a status read: a refused login or a database that
    /// does not exist is `Denied`, anything else `Unreachable`, each with
    /// the step the doctor's connection check gives it. An error the server
    /// sent is said in the server's words (the driver's own text says only
    /// `db error`).
    pub(crate) fn connect_status(&self) -> Result<Client, PostgresStatusError> {
        use crate::diagnostic::ConnectionStep;
        self.connect_raw().map_err(|dial| match dial {
            Dial::Driver(error) => {
                let why = match error.as_db_error() {
                    Some(db) => db.message().to_owned(),
                    None => error.to_string(),
                };
                match ConnectionStep::of_refused_connection(error.code().map(|c| c.code())) {
                    ConnectionStep::Transport => {
                        PostgresStatusError::Unreachable(ConnectionStep::Transport, why)
                    }
                    step => PostgresStatusError::Denied(step, why),
                }
            }
            Dial::Silent(_) => {
                PostgresStatusError::Unreachable(ConnectionStep::Transport, dial.to_string())
            }
        })
    }
}

/// How long a new connection may take to be made and logged in when its
/// URL sets no `connect_timeout`, and the diagnostic driver's admission
/// limit: half a Store doctor's 10 s budget, so a silent server fails its
/// check with time left to say so (the worker's diagnostic connect has 5 s
/// too).
pub const HANDSHAKE: Duration = Duration::from_secs(5);

/// Why [`Connector::connect_raw`] made no client.
pub(crate) enum Dial {
    /// The driver's own error: the socket refused, or the server answered
    /// with an error.
    Driver(postgres::Error),
    /// No answer within the limit (zero: the dial could not be started).
    Silent(Duration),
}

impl Dial {
    /// The refusal of a dial: no session exists, so no work can have
    /// started: the server refused it with an error, or no connection was
    /// made.
    fn refused(self) -> BackendError {
        match self {
            Self::Driver(e) => match e.code().map(|c| rows::sqlstate(c.code())) {
                Some(BackendError::Unknown) => BackendError::Rejected,
                Some(other) => other,
                None => BackendError::Closed,
            },
            Self::Silent(_) => BackendError::Closed,
        }
    }
}

impl std::fmt::Display for Dial {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Driver(error) => error.fmt(f),
            Self::Silent(limit) if limit.is_zero() => f.write_str("the dial could not be started"),
            Self::Silent(limit) => write!(
                f,
                "the server did not answer within {} s (connect and login)",
                limit.as_secs_f64()
            ),
        }
    }
}

pub(crate) struct Session {
    pub(crate) admission: Admission,
    pub(crate) state: Mutex<State>,
    /// The connection that holds the publication lane, opened on first use.
    pub(crate) lane: Mutex<Option<crate::backend::postgresql::lane::lane::LaneConnection>>,
    pub(crate) writable: bool,
    pub(crate) settings: Settings,
    /// The server's version and the Store's frozen policy, read by the
    /// open's check (`schema::check`); unset on a session that only reads a
    /// binding.
    pub(crate) server_version: OnceLock<String>,
    pub(crate) policy: OnceLock<StoragePolicy>,
    pub(crate) connector: Connector,
    /// The lane connection's work, counted outside `state` because the lane
    /// is held and released without the session state's lock.
    pub(crate) lane_work: LaneWork,
    /// The one backend a session advisory lock may be held on (spec §5.2 item
    /// 34); `0` until the first lock statement names it. A transaction-pooling
    /// proxy changes it between transactions, which is `Unsupported` for
    /// mounts and compaction.
    pub(crate) backend_pid: AtomicI64,
    /// The backend of this session's connection as the open's check read
    /// it: its pid and, when the role may see it, its start (microseconds
    /// since the epoch), which together name it even if the pid is reused.
    /// A lost reply's resolution ends it before the lane pass
    /// (`Session::pass_lane`).
    pub(crate) opened_backend: OnceLock<(i64, Option<i64>)>,
    /// The `catalog_incarnation` this session opened at (spec §5.2, §10); the
    /// generation fence in [`super::transaction::Session::run_as`] compares
    /// every write transaction against it. `0` until an open's check set it,
    /// which keeps the check's own pre-open reads unfenced.
    pub(crate) incarnation: AtomicU64,
    /// Pack rows chain locates returned (#758, `metadata::chains`).
    pub(crate) chain_packs: Mutex<super::super::metadata::chains::ChainPacks>,
    /// The intents this session registered and still holds advisory keys
    /// for, each with the keys it took (the key and whether it is shared). A
    /// release unlocks an op's keys only when it takes the op from here, so a
    /// repeated release, or a release of a row another session registered,
    /// never drops a key level a live mount of this session holds
    /// (PostgreSQL counts advisory locks per session), and a release whose
    /// row is already gone still unlocks them.
    pub(crate) held_ops: Mutex<HashMap<[u8; 16], Vec<SessionKey>>>,
}

/// One session advisory key a registered intent holds: the key, and whether
/// it was taken shared.
pub(crate) type SessionKey = (i64, bool);

/// Connections opened and round trips made on the lane connection.
#[derive(Default)]
pub(crate) struct LaneWork {
    pub(crate) connects: AtomicU64,
    pub(crate) round_trips: AtomicU64,
}

impl LaneWork {
    pub(crate) fn add(&self, connects: u64, round_trips: u64) {
        self.connects.fetch_add(connects, Ordering::Relaxed);
        self.round_trips.fetch_add(round_trips, Ordering::Relaxed);
    }
}

impl Session {
    pub(crate) fn connect_diagnostic_admitted(
        selection: &PostgresSelection,
        deadline: Option<Instant>,
        admission: Instant,
        resolver: &mut crate::diagnostic::Resolver<'_>,
        cancelled: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<Self, BackendError> {
        let settings = Settings::new(selection)?;
        let connector = Connector::new(selection)?;
        let client = super::driver::Driver::diagnostic_admitted(
            selection, deadline, admission, resolver, cancelled,
        )
        .map_err(|error| error.backend())?;
        Ok(Self {
            admission: Admission::default(),
            state: Mutex::new(State {
                client,
                statements: HashMap::new(),
                quarantined: false,
                work: SqlWork {
                    connects: 1,
                    ..SqlWork::default()
                },
            }),
            lane: Mutex::new(None),
            writable: false,
            settings,
            server_version: OnceLock::new(),
            policy: OnceLock::new(),
            connector,
            lane_work: LaneWork::default(),
            backend_pid: AtomicI64::new(0),
            opened_backend: OnceLock::new(),
            incarnation: AtomicU64::new(0),
            chain_packs: Mutex::default(),
            held_ops: Mutex::default(),
        })
    }
    pub(crate) fn connect(
        selection: &PostgresSelection,
        writable: bool,
    ) -> Result<Self, BackendError> {
        let settings = Settings::new(selection)?;
        let connector = Connector::new(selection)?;
        let client = connector.connect_ordinary()?;
        Ok(Self {
            admission: Admission::default(),
            state: Mutex::new(State {
                client: super::driver::Driver::Ordinary(client),
                statements: HashMap::new(),
                quarantined: false,
                // The connection; the open's check reads the server version.
                work: SqlWork {
                    connects: 1,
                    ..SqlWork::default()
                },
            }),
            lane: Mutex::new(None),
            writable,
            settings,
            server_version: OnceLock::new(),
            policy: OnceLock::new(),
            connector,
            lane_work: LaneWork::default(),
            backend_pid: AtomicI64::new(0),
            opened_backend: OnceLock::new(),
            incarnation: AtomicU64::new(0),
            chain_packs: Mutex::default(),
            held_ops: Mutex::default(),
        })
    }

    /// One unprepared simple-query string, in one round trip: its rows, one
    /// list per statement in order. Used only by an open's check, before
    /// the session serves anything; a failure leaves the session unusable
    /// (it is dropped), so nothing here rolls back. Counted as one statement,
    /// as a script is (`Transaction::batch`); `transaction` says whether it
    /// was one.
    pub(crate) fn simple(
        &self,
        sql: &str,
        transaction: bool,
    ) -> Result<Vec<Vec<SimpleQueryRow>>, BackendError> {
        let start = Instant::now();
        let mut state = self.state.lock().map_err(|_| BackendError::Unknown)?;
        let result = state.client.simple_query(sql);
        let elapsed = start.elapsed().as_nanos() as u64;
        let work = &mut state.work;
        work.statements += 1;
        work.round_trips += 1;
        work.statement_ns += elapsed;
        let messages = result.map_err(|e| e.backend())?;
        let mut groups = Vec::new();
        let mut rows = Vec::new();
        for message in messages {
            match message {
                SimpleQueryMessage::Row(row) => rows.push(row),
                SimpleQueryMessage::CommandComplete(_) => groups.push(std::mem::take(&mut rows)),
                _ => {}
            }
        }
        let work = &mut state.work;
        work.returned_rows += groups.iter().map(|g| g.len() as u64).sum::<u64>();
        if transaction {
            work.transactions += 1;
            work.commits += 1;
            work.transaction_ns += elapsed;
        } else {
            work.read_snapshots += 1;
        }
        Ok(groups)
    }

    /// Notes that this session holds `keys` for registered op `op`. Returns
    /// the keys an earlier registration of the same op still held, which the
    /// caller unlocks so the op keeps one level of each key.
    pub(crate) fn hold_op(&self, op: [u8; 16], keys: Vec<SessionKey>) -> Option<Vec<SessionKey>> {
        self.held_ops
            .lock()
            .ok()
            .and_then(|mut held| held.insert(op, keys))
    }

    /// Takes `op` from the ops this session holds keys for, with its keys:
    /// `Some` only the first time, and only when this session registered it.
    pub(crate) fn take_op(&self, op: &[u8; 16]) -> Option<Vec<SessionKey>> {
        self.held_ops
            .lock()
            .ok()
            .and_then(|mut held| held.remove(op))
    }

    /// Records the backend that took a session advisory lock. The first lock
    /// statement sets it; every later statement must run on the same backend.
    /// `false` means the connection is transaction-pooled and cannot hold the
    /// lock across transactions (spec §5.2 item 34).
    pub(crate) fn note_backend(&self, pid: i64) -> bool {
        if pid <= 0 {
            return false;
        }
        match self
            .backend_pid
            .compare_exchange(0, pid, Ordering::SeqCst, Ordering::SeqCst)
        {
            Ok(_) => true,
            Err(current) => current == pid,
        }
    }

    pub(crate) fn diagnostics(&self) -> Result<SqlWork, BackendError> {
        let state = self.state.lock().map_err(|_| BackendError::Unknown)?;
        let mut work = state.work;
        work.connects += self.lane_work.connects.load(Ordering::Relaxed);
        work.round_trips += self.lane_work.round_trips.load(Ordering::Relaxed);
        Ok(work)
    }
}
