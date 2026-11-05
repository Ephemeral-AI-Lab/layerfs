//! One PostgreSQL connection per session, with TLS and counted work.
use super::{profile::Settings, rows};
use crate::backend::{admission::Admission, records::BackendError};
use crate::{PostgresSelection, SqlWork};
use postgres::{Client, Config, Statement};
use postgres_native_tls::MakeTlsConnector;
use std::{
    collections::HashMap,
    str::FromStr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

/// Prepared statements kept per connection before the cache is cleared.
const STATEMENT_CACHE: usize = 256;

pub(crate) struct State {
    pub(crate) client: Client,
    statements: HashMap<String, Statement>,
    pub(crate) quarantined: bool,
    pub(crate) work: SqlWork,
}

impl State {
    /// One prepared statement for `sql`, prepared once per connection.
    pub(crate) fn statement(&mut self, sql: &str) -> Result<Statement, postgres::Error> {
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

/// Connects new clients for this session's selection; TLS follows the
/// connection string's `sslmode`.
#[derive(Clone)]
pub(crate) struct Connector {
    config: Config,
    tls: MakeTlsConnector,
}

impl Connector {
    pub(crate) fn new(selection: &PostgresSelection) -> Result<Self, BackendError> {
        let config = Config::from_str(&selection.url).map_err(|_| BackendError::Integrity)?;
        let tls = native_tls::TlsConnector::new().map_err(|_| BackendError::Integrity)?;
        Ok(Self {
            config,
            tls: MakeTlsConnector::new(tls),
        })
    }
    /// One new client, with the driver's own error (for a status read).
    pub(crate) fn connect_raw(&self) -> Result<Client, postgres::Error> {
        self.config.connect(self.tls.clone())
    }
    pub(crate) fn connect(&self) -> Result<Client, BackendError> {
        self.config
            .connect(self.tls.clone())
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
    pub(crate) server_version: String,
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
    pub(crate) fn connect(
        selection: &PostgresSelection,
        writable: bool,
    ) -> Result<Self, BackendError> {
        let settings = Settings::new(selection)?;
        let connector = Connector::new(selection)?;
        let mut client = connector.connect()?;
        let version = client
            .query_one("SHOW server_version", &[])
            .map_err(|e| rows::error(&e))?
            .try_get::<_, String>(0)
            .map_err(|e| rows::error(&e))?;
        Ok(Self {
            admission: Admission::default(),
            state: Mutex::new(State {
                client,
                statements: HashMap::new(),
                quarantined: false,
                // The connection, then `SHOW server_version`: prepare and execute.
                work: SqlWork {
                    connects: 1,
                    round_trips: 2,
                    prepares: 1,
                    ..SqlWork::default()
                },
            }),
            lane: Mutex::new(None),
            writable,
            settings,
            server_version: version,
            connector,
            lane_work: LaneWork::default(),
        })
    }

    pub(crate) fn diagnostics(&self) -> Result<SqlWork, BackendError> {
        let state = self.state.lock().map_err(|_| BackendError::Unknown)?;
        let mut work = state.work;
        work.connects += self.lane_work.connects.load(Ordering::Relaxed);
        work.round_trips += self.lane_work.round_trips.load(Ordering::Relaxed);
        Ok(work)
    }
}
