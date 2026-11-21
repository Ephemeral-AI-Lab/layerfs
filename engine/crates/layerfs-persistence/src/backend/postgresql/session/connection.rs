//! One PostgreSQL connection per session, with TLS and counted work.
use super::{profile::Settings, rows};
use crate::backend::postgresql::status::PostgresStatusError;
use crate::backend::{admission::Admission, records::BackendError};
use crate::{PostgresSelection, SqlWork};
use layerfs_storage::StoragePolicy;
use postgres::{
    config::SslMode, Client, Config, NoTls, SimpleQueryMessage, SimpleQueryRow, Statement,
};
use postgres_native_tls::MakeTlsConnector;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        atomic::{AtomicI64, AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::{Duration, Instant},
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
        let config = Config::from_str(&selection.url).map_err(|_| BackendError::Integrity)?;
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
        let limit = self.config.get_connect_timeout().copied();
        let limit = limit.unwrap_or(HANDSHAKE);
        let (config, tls) = (self.config.clone(), self.tls.clone());
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
    pub(crate) fn connect(&self) -> Result<Client, BackendError> {
        self.connect_raw().map_err(|dial| match dial {
            // No session exists, so no work can have started: the
            // server refused it with an error, or no connection was made.
            Dial::Driver(e) => match (rows::error(&e), e.code()) {
                (BackendError::Unknown, Some(_)) => BackendError::Rejected,
                (BackendError::Unknown, None) => BackendError::Closed,
                (other, _) => other,
            },
            Dial::Silent(_) => BackendError::Closed,
        })
    }
    /// One new client for a status read: a refused login or a database that
    /// does not exist is `Denied`, anything else `Unreachable`.
    pub(crate) fn connect_status(&self) -> Result<Client, PostgresStatusError> {
        self.connect_raw().map_err(|dial| match dial {
            Dial::Driver(error) => match error.code() {
                Some(code) if code.code().starts_with("28") || code.code() == "3D000" => {
                    PostgresStatusError::Denied(error.to_string())
                }
                _ => PostgresStatusError::Unreachable(error.to_string()),
            },
            Dial::Silent(_) => PostgresStatusError::Unreachable(dial.to_string()),
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
    pub(crate) lane: Mutex<Option<Client>>,
    /// Full transaction ID of this thread’s held lane, published under admission.
    pub(crate) lane_xid: AtomicU64,
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
    /// The `catalog_incarnation` this session opened at (spec §5.2, §10); the
    /// generation fence in [`super::transaction::Session::run_as`] compares
    /// every write transaction against it. `0` until an open's check set it,
    /// which keeps the check's own pre-open reads unfenced.
    pub(crate) incarnation: AtomicU64,
}

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
    /// Replace only the transaction connection. A diagnostic has its own deadline,
    /// and a session pinned for advisory locks cannot transfer those locks.
    pub(super) fn reconnect(&self, state: &mut State) -> Result<(), BackendError> {
        state.quarantined = true;
        if self.backend_pid.load(Ordering::SeqCst) != 0 {
            return Err(BackendError::Closed);
        }
        if matches!(state.client, super::driver::Driver::Diagnostic(_)) {
            return Err(BackendError::Unknown);
        }
        let client = self.connector.connect().map_err(|error| {
            if error == BackendError::Unknown {
                BackendError::Closed
            } else {
                error
            }
        })?;
        state.statements.clear();
        state.client = super::driver::Driver::Sync(client);
        state.work.connects += 1;
        state.quarantined = false;
        Ok(())
    }

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
            lane_xid: AtomicU64::new(0),
            writable: false,
            settings,
            server_version: OnceLock::new(),
            policy: OnceLock::new(),
            connector,
            lane_work: LaneWork::default(),
            backend_pid: AtomicI64::new(0),
            incarnation: AtomicU64::new(0),
        })
    }
    pub(crate) fn connect(
        selection: &PostgresSelection,
        writable: bool,
    ) -> Result<Self, BackendError> {
        let settings = Settings::new(selection)?;
        let connector = Connector::new(selection)?;
        let client = connector.connect()?;
        Ok(Self {
            admission: Admission::default(),
            state: Mutex::new(State {
                client: super::driver::Driver::Sync(client),
                statements: HashMap::new(),
                quarantined: false,
                // The connection; the open's check reads the server version.
                work: SqlWork {
                    connects: 1,
                    ..SqlWork::default()
                },
            }),
            lane: Mutex::new(None),
            lane_xid: AtomicU64::new(0),
            writable,
            settings,
            server_version: OnceLock::new(),
            policy: OnceLock::new(),
            connector,
            lane_work: LaneWork::default(),
            backend_pid: AtomicI64::new(0),
            incarnation: AtomicU64::new(0),
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
