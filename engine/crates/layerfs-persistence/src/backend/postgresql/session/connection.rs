//! One PostgreSQL connection per session, with TLS and counted work.
use super::{profile::Settings, rows};
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
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::Instant,
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
    /// One new client, with the driver's own error (for a status read).
    pub(crate) fn connect_raw(&self) -> Result<Client, postgres::Error> {
        match &self.tls {
            Some(tls) => self.config.connect(tls.clone()),
            None => self.config.connect(NoTls),
        }
    }
    pub(crate) fn connect(&self) -> Result<Client, BackendError> {
        self.connect_raw()
            .map_err(|e| match (rows::error(&e), e.code()) {
                // No session exists, so no work can have started: the
                // server refused it with an error, or no connection was made.
                (BackendError::Unknown, Some(_)) => BackendError::Rejected,
                (BackendError::Unknown, None) => BackendError::Closed,
                (other, _) => other,
            })
    }
}

pub(crate) struct Session {
    pub(crate) admission: Admission,
    pub(crate) state: Mutex<State>,
    /// The connection that holds the publication lane, opened on first use.
    pub(crate) lane: Mutex<Option<Client>>,
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
    pub(crate) fn connect_diagnostic_admitted(
        selection: &PostgresSelection,
        deadline: Option<Instant>,
        admission: Instant,
        resolver: &mut crate::diagnostic::Resolver<'_>,
    ) -> Result<Self, BackendError> {
        let settings = Settings::new(selection)?;
        let connector = Connector::new(selection)?;
        let client =
            super::driver::Driver::diagnostic_admitted(selection, deadline, admission, resolver)
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
            writable,
            settings,
            server_version: OnceLock::new(),
            policy: OnceLock::new(),
            connector,
            lane_work: LaneWork::default(),
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

    pub(crate) fn diagnostics(&self) -> Result<SqlWork, BackendError> {
        let state = self.state.lock().map_err(|_| BackendError::Unknown)?;
        let mut work = state.work;
        work.connects += self.lane_work.connects.load(Ordering::Relaxed);
        work.round_trips += self.lane_work.round_trips.load(Ordering::Relaxed);
        Ok(work)
    }
}
