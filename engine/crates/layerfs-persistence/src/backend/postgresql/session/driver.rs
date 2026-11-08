//! Ordinary synchronous driver and deadline-owned observational async driver.
use super::rows;
use crate::backend::records::BackendError;
use crate::PostgresSelection;
use postgres::{types::ToSql, Client, CopyInWriter, Row, SimpleQueryMessage, Statement};
use postgres_native_tls::MakeTlsConnector;
use std::{
    future::Future,
    str::FromStr,
    time::{Duration, Instant},
};

pub(crate) enum Error {
    Database(postgres::Error),
    Deadline { released: bool },
    Refused(BackendError),
}
impl From<BackendError> for Error {
    fn from(error: BackendError) -> Self {
        Self::Refused(error)
    }
}
impl From<postgres::Error> for Error {
    fn from(error: postgres::Error) -> Self {
        Self::Database(error)
    }
}
impl Error {
    pub(crate) fn backend(&self) -> BackendError {
        match self {
            Self::Database(error) => rows::error(error),
            Self::Deadline { released: true } => BackendError::Cancelled,
            Self::Deadline { released: false } => BackendError::Unknown,
            Self::Refused(error) => *error,
        }
    }
    pub(crate) fn code(&self) -> Option<&postgres::error::SqlState> {
        match self {
            Self::Database(error) => error.code(),
            _ => None,
        }
    }
}

pub(crate) enum Driver {
    Sync(Client),
    Diagnostic(Diagnostic),
}
pub(crate) struct Diagnostic {
    client: tokio_postgres::Client,
    connection: tokio::task::JoinHandle<()>,
    runtime: tokio::runtime::Runtime,
    deadline: Option<Instant>,
    tls: Option<MakeTlsConnector>,
    released: std::cell::Cell<Option<bool>>,
}

impl Driver {
    pub(crate) fn diagnostic(
        selection: &PostgresSelection,
        deadline: Option<Instant>,
        resolver: &mut crate::diagnostic::Resolver<'_>,
    ) -> Result<Self, Error> {
        let mut config = tokio_postgres::Config::from_str(&selection.url)
            .map_err(|_| BackendError::Integrity)?;
        let connect_deadline = deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(10));
        {
            let admission_deadline = connect_deadline;
            let deadline = admission_deadline;
            if Instant::now() >= deadline {
                return Err(BackendError::Cancelled.into());
            }
            if config.get_hostaddrs().is_empty() {
                let hosts = config.get_hosts().to_vec();
                let ports = config.get_ports().to_vec();
                // Expanding all system addresses preserves every configured TCP
                // endpoint and its hostname for TLS identity, without async DNS
                // spawning an uncancellable blocking resolver thread.
                let mut expanded = Vec::new();
                for (index, host) in hosts.iter().enumerate() {
                    match host {
                        tokio_postgres::config::Host::Tcp(host) => {
                            let port = ports
                                .get(index)
                                .or_else(|| ports.first())
                                .copied()
                                .unwrap_or(5432);
                            let addresses = match host.parse::<std::net::IpAddr>() {
                                Ok(ip) => Ok(vec![std::net::SocketAddr::new(ip, port)]),
                                Err(_) => resolver(host, port, deadline),
                            }
                            .map_err(|error| {
                                if error.kind() == std::io::ErrorKind::TimedOut {
                                    BackendError::Cancelled
                                } else {
                                    BackendError::Closed
                                }
                            })?;
                            for address in addresses {
                                expanded.push((host.clone(), address));
                            }
                        }
                        #[cfg(unix)]
                        tokio_postgres::config::Host::Unix(_) => {
                            // Mixed TCP/Unix fallback cannot be expanded without
                            // changing configured ordering/cardinality.
                            if hosts
                                .iter()
                                .any(|h| matches!(h, tokio_postgres::config::Host::Tcp(_)))
                            {
                                return Err(BackendError::Integrity.into());
                            }
                        }
                    }
                }
                if !expanded.is_empty() {
                    // Config's host mutators append, so rebuild just the network
                    // list through a parsed copy whose host list is cleared by
                    // an explicit libpq-style host override in the URL.
                    config = resolved_config(&config, &expanded)?;
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            config.connect_timeout(
                config
                    .get_connect_timeout()
                    .map_or(remaining, |configured| remaining.min(*configured)),
            );
        }
        if let Some(deadline) = deadline {
            let milliseconds = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .max(1)
                .min(i32::MAX as u128);
            let original = config.get_options().unwrap_or("");
            config.options(format!(
                "{original} -c statement_timeout={milliseconds} -c lock_timeout={milliseconds}"
            ));
        }
        let tls = match config.get_ssl_mode() {
            tokio_postgres::config::SslMode::Disable => None,
            _ => Some(MakeTlsConnector::new(
                native_tls::TlsConnector::new().map_err(|_| BackendError::Integrity)?,
            )),
        };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| BackendError::Closed)?;
        let (client, connection) = match &tls {
            Some(tls) => {
                let (client, connection) = connect(
                    &runtime,
                    Some(connect_deadline),
                    config.connect(tls.clone()),
                )?;
                let connection = runtime.spawn(async move {
                    let _ = connection.await;
                });
                (client, connection)
            }
            None => {
                let (client, connection) = connect(
                    &runtime,
                    Some(connect_deadline),
                    config.connect(tokio_postgres::NoTls),
                )?;
                let connection = runtime.spawn(async move {
                    let _ = connection.await;
                });
                (client, connection)
            }
        };
        Ok(Self::Diagnostic(Diagnostic {
            client,
            connection,
            runtime,
            deadline,
            tls,
            released: std::cell::Cell::new(None),
        }))
    }
    pub(crate) fn prepare(&mut self, sql: &str) -> Result<Statement, Error> {
        match self {
            Self::Sync(client) => client.prepare(sql).map_err(Into::into),
            Self::Diagnostic(client) => client.run(client.client.prepare(sql)),
        }
    }
    pub(crate) fn query(
        &mut self,
        statement: &Statement,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, Error> {
        match self {
            Self::Sync(client) => client.query(statement, params).map_err(Into::into),
            Self::Diagnostic(client) => client.run(client.client.query(statement, params)),
        }
    }
    pub(crate) fn query_one(
        &mut self,
        sql: &str,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Row, Error> {
        let statement = self.prepare(sql)?;
        let mut rows = self.query(&statement, params)?;
        if rows.len() != 1 {
            return Err(Error::Refused(BackendError::Integrity));
        }
        rows.pop().ok_or(Error::Refused(BackendError::Integrity))
    }
    pub(crate) fn simple_query(&mut self, sql: &str) -> Result<Vec<SimpleQueryMessage>, Error> {
        match self {
            Self::Sync(client) => client.simple_query(sql).map_err(Into::into),
            Self::Diagnostic(client) => client.run(client.client.simple_query(sql)),
        }
    }
    pub(crate) fn batch_execute(&mut self, sql: &str) -> Result<(), Error> {
        match self {
            Self::Sync(client) => client.batch_execute(sql).map_err(Into::into),
            Self::Diagnostic(client) => client.run(client.client.batch_execute(sql)),
        }
    }
    pub(crate) fn copy_in(&mut self, statement: &Statement) -> Result<CopyInWriter<'_>, Error> {
        match self {
            Self::Sync(client) => client.copy_in(statement).map_err(Into::into),
            Self::Diagnostic(_) => Err(Error::Refused(BackendError::ReadOnly)),
        }
    }
    pub(crate) fn is_valid(&mut self, timeout: Duration) -> Result<(), Error> {
        match self {
            Self::Sync(client) => client.is_valid(timeout).map_err(Into::into),
            Self::Diagnostic(client) => client.run(client.client.batch_execute("")),
        }
    }
    pub(crate) fn released(&self) -> Option<bool> {
        match self {
            Self::Sync(_) => None,
            Self::Diagnostic(client) => client.released.get(),
        }
    }
    pub(crate) fn close(self) -> Result<(), Error> {
        match self {
            Self::Sync(client) => client.close().map_err(Into::into),
            Self::Diagnostic(client) => {
                drop(client);
                Ok(())
            }
        }
    }
}

impl Diagnostic {
    fn run<T>(&self, future: impl Future<Output = Result<T, postgres::Error>>) -> Result<T, Error> {
        let result = self.runtime.block_on(async {
            match self.deadline {
                Some(deadline) => {
                    tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), future)
                        .await
                        .map_err(|_| ())
                }
                None => Ok(future.await),
            }
        });
        match result {
            Ok(result) => result.map_err(Into::into),
            Err(()) => {
                // Cancellation uses the established numeric endpoint, then drains
                // a ROLLBACK reply before claiming source snapshot release.
                let token = self.client.cancel_token();
                let released = self.runtime.block_on(async {
                    tokio::time::timeout(Duration::from_secs(1), async {
                        match &self.tls {
                            Some(tls) => token.cancel_query(tls.clone()).await?,
                            None => token.cancel_query(tokio_postgres::NoTls).await?,
                        }
                        self.client.batch_execute("ROLLBACK").await
                    })
                    .await
                    .is_ok_and(|r| r.is_ok())
                });
                self.released.set(Some(released));
                Err(Error::Deadline { released })
            }
        }
    }
}
impl Drop for Diagnostic {
    fn drop(&mut self) {
        self.connection.abort();
    }
}
fn connect<T>(
    runtime: &tokio::runtime::Runtime,
    deadline: Option<Instant>,
    future: impl Future<Output = Result<T, postgres::Error>>,
) -> Result<T, Error> {
    runtime.block_on(async {
        match deadline {
            Some(deadline) => {
                tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), future)
                    .await
                    .map_err(|_| Error::Deadline { released: true })?
                    .map_err(Error::Database)
            }
            None => future.await.map_err(Error::Database),
        }
    })
}

fn resolved_config(
    original: &tokio_postgres::Config,
    expanded: &[(String, std::net::SocketAddr)],
) -> Result<tokio_postgres::Config, BackendError> {
    let mut config = tokio_postgres::Config::new();
    if let Some(user) = original.get_user() {
        config.user(user);
    }
    if let Some(password) = original.get_password() {
        config.password(password);
    }
    if let Some(database) = original.get_dbname() {
        config.dbname(database);
    }
    if let Some(options) = original.get_options() {
        config.options(options);
    }
    if let Some(application) = original.get_application_name() {
        config.application_name(application);
    }
    config
        .ssl_mode(original.get_ssl_mode())
        .ssl_negotiation(original.get_ssl_negotiation())
        .keepalives(original.get_keepalives())
        .keepalives_idle(original.get_keepalives_idle())
        .target_session_attrs(original.get_target_session_attrs())
        .channel_binding(original.get_channel_binding())
        .load_balance_hosts(original.get_load_balance_hosts());
    if let Some(timeout) = original.get_connect_timeout() {
        config.connect_timeout(*timeout);
    }
    if let Some(timeout) = original.get_tcp_user_timeout() {
        config.tcp_user_timeout(*timeout);
    }
    if let Some(interval) = original.get_keepalives_interval() {
        config.keepalives_interval(interval);
    }
    if let Some(retries) = original.get_keepalives_retries() {
        config.keepalives_retries(retries);
    }
    for (host, address) in expanded {
        config
            .host(host)
            .hostaddr(address.ip())
            .port(address.port());
    }
    Ok(config)
}
