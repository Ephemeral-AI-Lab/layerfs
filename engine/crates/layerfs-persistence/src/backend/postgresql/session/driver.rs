//! The ordinary driver and the deadline-owned observational driver, both
//! tokio-postgres on a current-thread runtime of their own.
use super::{
    connection::Dial,
    pipeline::{self, Bound, Flight},
    rows,
};
use crate::backend::records::BackendError;
use crate::PostgresSelection;
use postgres::{types::ToSql, Row, SimpleQueryMessage, Statement};
use postgres_native_tls::MakeTlsConnector;
use std::{
    future::Future,
    pin::Pin,
    str::FromStr,
    time::{Duration, Instant},
};

/// How long closing an ordinary connection waits for its connection task to
/// send `Terminate` and shut the socket before the task is aborted, which
/// closes the socket at once. A connection with no reply outstanding ends
/// within one local write; the limit matters only for a silent server.
const CLOSE: Duration = Duration::from_secs(1);

pub(crate) enum Error {
    Unsupported(&'static str),
    Database(postgres::Error),
    Deadline {
        released: bool,
    },
    Refused(BackendError),
    /// A pipelined request never sent: it, or a request before it, could
    /// not be encoded (`pipeline`). Nothing of it reached the server.
    Unsent,
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
            Self::Unsupported(_) => BackendError::Unknown,
            Self::Database(error) => rows::error(error),
            Self::Deadline { released: true } => BackendError::Cancelled,
            Self::Deadline { released: false } => BackendError::Unknown,
            Self::Refused(error) => *error,
            Self::Unsent => BackendError::Integrity,
        }
    }
    pub(crate) fn code(&self) -> Option<&postgres::error::SqlState> {
        match self {
            Self::Database(error) => error.code(),
            _ => None,
        }
    }
    /// The failure of a request the client never sent (`pipeline::in_order`
    /// answered it on its first poll): nothing of it reached the server, so
    /// a client error the ordinary classification calls unknown (an
    /// `io::Error` while encoding: a NUL byte in a text, a message past the
    /// protocol's length) is a definite refusal ([`Self::Unsent`]). A closed
    /// connection stays as it is.
    pub(crate) fn unsent(self) -> Self {
        match self {
            Self::Database(error) if error.code().is_none() && !error.is_closed() => Self::Unsent,
            other => other,
        }
    }
}

/// What one prepare flight answered ([`Driver::prepare_all`]).
pub(crate) struct Prepared {
    /// Prepares sent: a prefix of them.
    pub(crate) sent: usize,
    /// One reply per prepare, in order.
    pub(crate) statements: Vec<Result<Statement, Error>>,
    /// The `begin` text's and the `COMMIT`'s answers, the first failure of
    /// the two (`Ok` without `begin`).
    pub(crate) around: Result<(), Error>,
}

pub(crate) enum Driver {
    Ordinary(Ordinary),
    Diagnostic(Diagnostic),
    /// An ordinary connection closed by quarantine (`State::quarantine`);
    /// every call is refused as an unknown outcome without reaching a server.
    Closed,
}
/// One ordinary connection: a tokio-postgres client, the current-thread
/// runtime it runs on and the task that drives its socket. Each call blocks
/// on the runtime, so the socket is served only while a call waits for it,
/// exactly as the synchronous `postgres` client did (it is the same client
/// with its connection polled inline). Unlike that client, several requests
/// can be sent before any reply is awaited ([`Driver::pipeline`]). There is
/// no deadline: a call waits for its reply or for the connection to fail.
pub(crate) struct Ordinary {
    pub(super) client: tokio_postgres::Client,
    connection: tokio::task::JoinHandle<()>,
    pub(super) runtime: tokio::runtime::Runtime,
}

pub(crate) struct Diagnostic {
    client: tokio_postgres::Client,
    connection: tokio::task::JoinHandle<()>,
    runtime: tokio::runtime::Runtime,
    deadline: Option<Instant>,
    tls: Option<MakeTlsConnector>,
    released: std::cell::Cell<Option<bool>>,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Driver {
    pub(crate) fn diagnostic(
        selection: &PostgresSelection,
        deadline: Option<Instant>,
        resolver: &mut crate::diagnostic::Resolver<'_>,
        cancelled: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<Self, Error> {
        // connect and login get the handshake limit even under a longer
        // deadline: a server that says nothing is unreachable, not slow
        let limit = Instant::now() + super::connection::HANDSHAKE;
        let admission = deadline.map_or(limit, |deadline| deadline.min(limit));
        Self::diagnostic_selection(selection, deadline, admission, resolver, cancelled)
    }
    pub(crate) fn diagnostic_admitted(
        selection: &PostgresSelection,
        deadline: Option<Instant>,
        admission: Instant,
        resolver: &mut crate::diagnostic::Resolver<'_>,
        cancelled: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<Self, Error> {
        Self::diagnostic_selection(selection, deadline, admission, resolver, cancelled)
    }
    fn diagnostic_selection(
        selection: &PostgresSelection,
        deadline: Option<Instant>,
        admission: Instant,
        resolver: &mut crate::diagnostic::Resolver<'_>,
        cancelled: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<Self, Error> {
        if cancelled.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
            return Err(BackendError::Cancelled.into());
        }
        let config = tokio_postgres::Config::from_str(&selection.url)
            .map_err(|_| BackendError::Integrity)?;
        let connect_deadline = deadline.map_or(admission, |total| total.min(admission));
        if !config.get_hostaddrs().is_empty() || config.get_hosts().is_empty() {
            return Self::diagnostic_config(config, deadline, connect_deadline, cancelled);
        }
        if config.get_hosts().len() > 1
            && config.get_load_balance_hosts() == tokio_postgres::config::LoadBalanceHosts::Random
        {
            return Err(Error::Unsupported(
                "randomized multi-host diagnostic admission is not implemented",
            ));
        }
        let mut last = Error::Refused(BackendError::Closed);
        for (index, host) in config.get_hosts().iter().enumerate() {
            if Instant::now() >= connect_deadline
                || cancelled.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
            {
                return Err(BackendError::Cancelled.into());
            }
            let port = config
                .get_ports()
                .get(index)
                .or_else(|| config.get_ports().first())
                .copied()
                .unwrap_or(5432);
            match host {
                tokio_postgres::config::Host::Tcp(host) => {
                    let addresses = match host.parse::<std::net::IpAddr>() {
                        Ok(ip) => vec![std::net::SocketAddr::new(ip, port)],
                        Err(_) => match resolver(host, port, connect_deadline) {
                            Ok(addresses) => addresses,
                            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
                                return Err(BackendError::Cancelled.into())
                            }
                            Err(_) => continue,
                        },
                    };
                    for address in addresses {
                        if cancelled
                            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
                        {
                            return Err(BackendError::Cancelled.into());
                        }
                        let candidate = resolved_config(&config, &[(host.clone(), address)])?;
                        match Self::diagnostic_config(
                            candidate,
                            deadline,
                            connect_deadline,
                            cancelled,
                        ) {
                            Ok(client) => return Ok(client),
                            Err(error) => last = error,
                        }
                    }
                }
                #[cfg(unix)]
                tokio_postgres::config::Host::Unix(path) => {
                    let mut candidate = resolved_config(&config, &[])?;
                    candidate.host_path(path).port(port);
                    match Self::diagnostic_config(candidate, deadline, connect_deadline, cancelled)
                    {
                        Ok(client) => return Ok(client),
                        Err(error) => last = error,
                    }
                }
            }
        }
        Err(last)
    }
    fn diagnostic_config(
        mut config: tokio_postgres::Config,
        deadline: Option<Instant>,
        connect_deadline: Instant,
        cancelled: Option<&std::sync::atomic::AtomicBool>,
    ) -> Result<Self, Error> {
        let remaining = connect_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(BackendError::Cancelled.into());
        }
        config.connect_timeout(
            config
                .get_connect_timeout()
                .map_or(remaining, |configured| remaining.min(*configured)),
        );
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
                    cancelled,
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
                    cancelled,
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
            cancelled: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }))
    }
    pub(crate) fn set_diagnostic_deadline(&mut self, deadline: Option<Instant>) {
        if let Self::Diagnostic(client) = self {
            client.deadline = deadline;
        }
    }
    pub(crate) fn prepare(&mut self, sql: &str) -> Result<Statement, Error> {
        match self {
            Self::Ordinary(client) => client.run(client.client.prepare(sql)),
            Self::Diagnostic(client) => client.run(client.client.prepare(sql)),
            Self::Closed => Err(Error::Refused(BackendError::Unknown)),
        }
    }
    pub(crate) fn query(
        &mut self,
        statement: &Statement,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<Row>, Error> {
        match self {
            Self::Ordinary(client) => client.run(client.client.query(statement, params)),
            Self::Diagnostic(client) => client.run(client.client.query(statement, params)),
            Self::Closed => Err(Error::Refused(BackendError::Unknown)),
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
            Self::Ordinary(client) => client.run(client.client.simple_query(sql)),
            Self::Diagnostic(client) => client.run(client.client.simple_query(sql)),
            Self::Closed => Err(Error::Refused(BackendError::Unknown)),
        }
    }
    pub(crate) fn batch_execute(&mut self, sql: &str) -> Result<(), Error> {
        match self {
            Self::Ordinary(client) => client.run(client.client.batch_execute(sql)),
            Self::Diagnostic(client) => client.run(client.client.batch_execute(sql)),
            Self::Closed => Err(Error::Refused(BackendError::Unknown)),
        }
    }
    /// One `COPY … FROM STDIN (FORMAT binary)` started (its CopyInResponse
    /// awaited); the rows are written and finished through [`CopyRows`].
    pub(crate) fn copy_in(
        &mut self,
        statement: &Statement,
        types: &[postgres::types::Type],
    ) -> Result<CopyRows<'_>, Error> {
        match self {
            Self::Ordinary(client) => {
                let sink = client.run(client.client.copy_in(statement))?;
                Ok(CopyRows {
                    runtime: &client.runtime,
                    writer: Box::pin(tokio_postgres::binary_copy::BinaryCopyInWriter::new(
                        sink, types,
                    )),
                })
            }
            Self::Diagnostic(_) => Err(Error::Refused(BackendError::ReadOnly)),
            Self::Closed => Err(Error::Refused(BackendError::Unknown)),
        }
    }
    /// Prepares every statement of `sqls` in one flight: each Parse/Describe
    /// is sent before any reply is awaited, and every reply is collected
    /// ([`pipeline::in_order`]): how many were sent, and every reply in
    /// order. With `begin`, the flight is that text, the prepares and
    /// `COMMIT`: statements whose names resolve through the `search_path`
    /// its `SET LOCAL` gives, and the answers of `begin` and `COMMIT`.
    pub(crate) fn prepare_all(
        &mut self,
        sqls: &[&str],
        begin: Option<&str>,
    ) -> Result<Prepared, Error> {
        type Answer = Option<Statement>;
        match self {
            Self::Ordinary(client) => {
                let c = &client.client;
                let mut pending: Vec<pipeline::Pending<'_, Answer>> =
                    Vec::with_capacity(sqls.len() + 2);
                if let Some(begin) = begin {
                    pending.push(Box::pin(async move {
                        c.batch_execute(begin).await.map(|()| None)
                    }));
                }
                for sql in sqls {
                    pending.push(Box::pin(async move { c.prepare(sql).await.map(Some) }));
                }
                if begin.is_some() {
                    pending.push(Box::pin(async move {
                        c.batch_execute("COMMIT").await.map(|()| None)
                    }));
                }
                let (sent, mut replies) = client.runtime.block_on(pipeline::in_order(pending));
                let around = |reply: Result<Answer, Error>| reply.map(drop);
                let (opened, closed) = match begin {
                    Some(_) => {
                        let closed = replies.pop().map_or(Err(Error::Unsent), around);
                        (around(replies.remove(0)), closed)
                    }
                    None => (Ok(()), Ok(())),
                };
                let skip = usize::from(begin.is_some());
                Ok(Prepared {
                    sent: sent.saturating_sub(skip).min(sqls.len()),
                    statements: replies
                        .into_iter()
                        .map(|reply| reply.and_then(|answer| answer.ok_or(Error::Unsent)))
                        .collect(),
                    around: opened.and(closed),
                })
            }
            Self::Diagnostic(_) => Err(Error::Refused(BackendError::ReadOnly)),
            Self::Closed => Err(Error::Refused(BackendError::Unknown)),
        }
    }
    /// Sends every request before awaiting any reply and collects every
    /// reply in order ([`pipeline::flight`]). `Err` only when nothing was
    /// sent: the connection is closed, or this is the read-only diagnostic
    /// driver.
    pub(crate) fn pipeline(&mut self, requests: &[Bound<'_>]) -> Result<Flight, Error> {
        match self {
            Self::Ordinary(client) => Ok(client
                .runtime
                .block_on(pipeline::flight(&client.client, requests))),
            Self::Diagnostic(_) => Err(Error::Refused(BackendError::ReadOnly)),
            Self::Closed => Err(Error::Refused(BackendError::Unknown)),
        }
    }
    /// Whether the connection still stands, judged locally with no round
    /// trip (#768): the runtime is given two turns without waiting, the
    /// first to poll the socket's readiness and the second to run the
    /// connection task it woke, which reads what the server sent and ends
    /// on an end of stream or an error (a server restart, a terminated
    /// backend, an idle timeout). A peer that went silent without closing
    /// is not seen here.
    pub(crate) fn alive(&mut self) -> bool {
        match self {
            Self::Ordinary(client) => {
                for _ in 0..2 {
                    client.runtime.block_on(tokio::task::yield_now());
                }
                !client.client.is_closed() && !client.connection.is_finished()
            }
            Self::Diagnostic(_) => true,
            Self::Closed => false,
        }
    }
    pub(crate) fn cancellation(&self) -> Option<std::sync::Arc<std::sync::atomic::AtomicBool>> {
        match self {
            Self::Ordinary(_) => None,
            Self::Diagnostic(client) => Some(client.cancelled.clone()),
            Self::Closed => None,
        }
    }
    pub(crate) fn released(&self) -> Option<bool> {
        match self {
            Self::Ordinary(_) => None,
            Self::Diagnostic(client) => client.released.get(),
            Self::Closed => None,
        }
    }
    /// Closes the connection; the server ends its backend, rolling back any
    /// transaction still open and releasing every lock it held.
    /// Nothing is awaited from the server, so there is no outcome to
    /// report: it is always `Ok`.
    pub(crate) fn close(self) -> Result<(), Error> {
        match self {
            Self::Ordinary(client) => client.close(),
            Self::Diagnostic(client) => drop(client),
            Self::Closed => {}
        }
        Ok(())
    }
}

impl Ordinary {
    /// One new connection made and logged in within `limit`, or
    /// [`Dial::Silent`]: the driver bounds only the socket, so a server that
    /// accepts it and then says nothing (paused or stopped behind a live
    /// port) would hold the call forever. One attempt, no retry.
    pub(crate) fn connect(
        config: &tokio_postgres::Config,
        tls: Option<&MakeTlsConnector>,
        limit: Duration,
    ) -> Result<Self, Dial> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| Dial::Silent(Duration::ZERO))?;
        // Each arm spawns its own connection: the TLS and plain connections
        // are different types.
        let made = runtime.block_on(async {
            match tls {
                Some(tls) => tokio::time::timeout(limit, config.connect(tls.clone()))
                    .await
                    .map(|made| {
                        made.map(|(client, connection)| {
                            let task = tokio::spawn(async move {
                                let _ = connection.await;
                            });
                            (client, task)
                        })
                    }),
                None => tokio::time::timeout(limit, config.connect(tokio_postgres::NoTls))
                    .await
                    .map(|made| {
                        made.map(|(client, connection)| {
                            let task = tokio::spawn(async move {
                                let _ = connection.await;
                            });
                            (client, task)
                        })
                    }),
            }
        });
        match made {
            Ok(Ok((client, connection))) => Ok(Self {
                client,
                connection,
                runtime,
            }),
            Ok(Err(error)) => Err(Dial::Driver(error)),
            Err(_) => {
                // A name lookup may still be running on the blocking pool;
                // the runtime does not wait for it.
                runtime.shutdown_background();
                Err(Dial::Silent(limit))
            }
        }
    }
    fn run<T>(&self, future: impl Future<Output = Result<T, postgres::Error>>) -> Result<T, Error> {
        self.runtime.block_on(future).map_err(Into::into)
    }
    /// Drops the client, which lets the connection task send `Terminate`
    /// when no reply is outstanding and shut the socket, then aborts the
    /// task and drops the runtime, which closes the socket if it is still
    /// open. Either way the server sees the connection end.
    fn close(self) {
        let Self {
            client,
            mut connection,
            runtime,
        } = self;
        drop(client);
        // The timer is made inside the runtime, which owns the clock.
        let _ = runtime.block_on(async { tokio::time::timeout(CLOSE, &mut connection).await });
        connection.abort();
        drop(runtime);
    }
}

/// The rows of one started binary `COPY`, written on the connection's
/// runtime (the synchronous client's `BinaryCopyInWriter`, which wraps the
/// same tokio-postgres writer). A copy dropped without [`Self::finish`] is
/// aborted: its `CopyFail` goes out with the next request.
pub(crate) struct CopyRows<'a> {
    runtime: &'a tokio::runtime::Runtime,
    writer: Pin<Box<tokio_postgres::binary_copy::BinaryCopyInWriter>>,
}

impl CopyRows<'_> {
    /// Writes one row; the writer sends its buffer once past 4 KiB.
    pub(crate) fn write(&mut self, values: &[&(dyn ToSql + Sync)]) -> Result<(), postgres::Error> {
        self.runtime.block_on(self.writer.as_mut().write(values))
    }
    /// Sends the rest and `CopyDone`, and awaits the server's count.
    pub(crate) fn finish(mut self) -> Result<u64, postgres::Error> {
        self.runtime.block_on(self.writer.as_mut().finish())
    }
}

impl Diagnostic {
    fn run<T>(&self, future: impl Future<Output = Result<T, postgres::Error>>) -> Result<T, Error> {
        let result = self.runtime.block_on(async {
            let mut future = std::pin::pin!(future);
            loop {
                if self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
                    || self.deadline.is_some_and(|d| Instant::now() >= d)
                {
                    break Err(());
                }
                let wait = self.deadline.map_or(Duration::from_millis(20), |d| {
                    d.saturating_duration_since(Instant::now())
                        .min(Duration::from_millis(20))
                });
                if let Ok(result) = tokio::time::timeout(wait, future.as_mut()).await {
                    break Ok(result);
                }
            }
        });
        match result {
            Ok(result) => result.map_err(Into::into),
            Err(()) => {
                if let Some(released) = self.released.get() {
                    return Err(Error::Deadline { released });
                }
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
    cancelled: Option<&std::sync::atomic::AtomicBool>,
    future: impl Future<Output = Result<T, postgres::Error>>,
) -> Result<T, Error> {
    runtime.block_on(async {
        let mut future = std::pin::pin!(future);
        loop {
            if cancelled.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
                || deadline.is_some_and(|until| Instant::now() >= until)
            {
                return Err(Error::Deadline { released: true });
            }
            let wait = deadline.map_or(Duration::from_millis(20), |until| {
                until
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(20))
            });
            if let Ok(result) = tokio::time::timeout(wait, future.as_mut()).await {
                return result.map_err(Error::Database);
            }
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
