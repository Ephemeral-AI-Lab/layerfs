//! One PostgreSQL connection per session, with TLS and counted work.
use super::{profile::Settings, rows};
use crate::backend::{admission::Admission, records::BackendError};
use crate::{PostgresSelection, SqlWork};
use postgres::{Client, Config, Statement};
use postgres_native_tls::MakeTlsConnector;
use std::{collections::HashMap, str::FromStr, sync::Mutex};

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
            .map_err(|e| match rows::error(&e) {
                // No connection exists, so no work can have started.
                BackendError::Unknown => BackendError::Closed,
                other => other,
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
                work: SqlWork::default(),
            }),
            lane: Mutex::new(None),
            writable,
            settings,
            server_version: version,
            connector,
        })
    }

    pub(crate) fn diagnostics(&self) -> Result<SqlWork, BackendError> {
        let state = self.state.lock().map_err(|_| BackendError::Unknown)?;
        Ok(state.work)
    }
}
