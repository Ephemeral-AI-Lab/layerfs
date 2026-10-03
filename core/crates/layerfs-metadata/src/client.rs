//! Synchronous bounded RPC facade over the complete published PostgreSQL driver.
use crate::{
    config::PgConfig,
    connection,
    params::Param,
    wire::{PgDiagnostics, Trace},
};
use layerfs_storage::port::MetadataError;
use postgres::Row;
use std::{
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread::JoinHandle,
};
pub(crate) struct Job {
    pub(crate) sql: String,
    pub(crate) params: Vec<Param>,
    pub(crate) bootstrap: bool,
    pub(crate) mutating: bool,
    pub(crate) reply: SyncSender<Result<Vec<Row>, MetadataError>>,
}
pub(crate) enum Message {
    Run(Job),
    Stop,
}
struct State {
    sender: SyncSender<Message>,
    failed: bool,
}
pub(crate) struct Client {
    state: Mutex<State>,
    thread: Mutex<Option<JoinHandle<()>>>,
    trace: Arc<Trace>,
    config: PgConfig,
}
impl Client {
    pub(crate) fn connect(config: PgConfig) -> Result<Arc<Self>, MetadataError> {
        config.check()?;
        let trace = Trace::new();
        let (sender, receiver) = mpsc::sync_channel(1);
        let (startup, ready) = mpsc::sync_channel(1);
        let profile = config.clone();
        let counts = Arc::clone(&trace);
        let thread = std::thread::Builder::new()
            .name("layerfs-pg-io".to_owned())
            .spawn(move || connection::run(profile, receiver, startup, counts))
            .map_err(|_| MetadataError::Uncertain)?;
        ready
            .recv_timeout(config.connect_timeout)
            .map_err(|_| MetadataError::Uncertain)??;
        Ok(Arc::new(Self {
            state: Mutex::new(State {
                sender,
                failed: false,
            }),
            thread: Mutex::new(Some(thread)),
            trace,
            config,
        }))
    }
    pub(crate) fn query(
        &self,
        template: &str,
        params: Vec<Param>,
        mutating: bool,
    ) -> Result<Vec<Row>, MetadataError> {
        self.call(self.config.sql(template), params, false, mutating)
    }
    pub(crate) fn bootstrap(&self, sql: String) -> Result<(), MetadataError> {
        self.call(sql, Vec::new(), true, true).map(|_| ())
    }
    fn call(
        &self,
        sql: String,
        params: Vec<Param>,
        bootstrap: bool,
        mutating: bool,
    ) -> Result<Vec<Row>, MetadataError> {
        let mut state = self.state.lock().map_err(|_| MetadataError::Uncertain)?;
        if state.failed {
            return Err(MetadataError::Uncertain);
        }
        self.trace
            .change(|counts| counts.operations += 1)
            .map_err(|_| MetadataError::Uncertain)?;
        let (reply, receiver) = mpsc::sync_channel(1);
        if state
            .sender
            .send(Message::Run(Job {
                sql,
                params,
                bootstrap,
                mutating,
                reply,
            }))
            .is_err()
        {
            state.failed = true;
            return Err(MetadataError::Uncertain);
        }
        let result = receiver
            .recv_timeout(self.config.request_timeout)
            .unwrap_or(Err(MetadataError::Uncertain));
        if matches!(
            result,
            Err(MetadataError::Uncertain | MetadataError::Malformed)
        ) {
            state.failed = true;
        }
        result
    }
    pub(crate) fn diagnostics(&self) -> Result<PgDiagnostics, MetadataError> {
        self.trace.snapshot().map_err(|_| MetadataError::Uncertain)
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let state = match self.state.get_mut() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        let _ = state.sender.send(Message::Stop);
        let thread = match self.thread.get_mut() {
            Ok(thread) => thread,
            Err(poison) => poison.into_inner(),
        };
        if let Some(thread) = thread.take() {
            let _ = thread.join();
        }
    }
}
