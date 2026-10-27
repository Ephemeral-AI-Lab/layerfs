//! Explicit per-control numeric observations of existing counters only.
mod kernel;
pub(super) mod resources;
mod schema;
pub(crate) use schema::{commit_parts, namespace_values, storage_values};

use super::Application;
use crate::control::{Failure, Success};
use layerfs_bridge::control::{Call, ControlCode, Request, SdkValue};
use std::io::{self, Write};

const RECORD_BYTES: usize = 4096;
const FIXED_RECORDS: usize = 64;

/// Streaming framing with no owned record buffer. A short sink write ends the
/// diagnostic attempt; no tail or original operation is resent.
pub(super) struct Records<'a, W> {
    sink: W,
    call: u64,
    scope: &'a [u8; 32],
    daemon: &'a [u8; 32],
    slot: usize,
    maximum: usize,
    bytes: usize,
    records: u64,
    numbers: u64,
    unavailable: u64,
}
impl<'a, W: Write> Records<'a, W> {
    fn new(
        sink: W,
        call: u64,
        readers: usize,
        scope: &'a [u8; 32],
        daemon: &'a [u8; 32],
        slot: usize,
    ) -> Self {
        Self {
            sink,
            call,
            scope,
            daemon,
            slot,
            maximum: FIXED_RECORDS + 2 * readers,
            bytes: 0,
            records: 0,
            numbers: 0,
            unavailable: 0,
        }
    }
    fn row(&mut self, section: u16, index: i64, values: Option<&[u64]>) -> io::Result<()> {
        if self.records >= self.maximum as u64 {
            return Err(io::Error::other("diagnostic record count exceeded"));
        }
        self.bytes = 0;
        let call = self.call;
        let slot = self.slot;
        write!(
            self,
            "{{\"layerfs_observation\":1,\"call\":{call},\"slot\":{slot},\"scope\":["
        )?;
        for i in 0..4 {
            if i != 0 {
                self.write_all(b",")?;
            }
            let word = u64::from_be_bytes(
                self.scope[i * 8..i * 8 + 8]
                    .try_into()
                    .expect("scope word width"),
            );
            write!(self, "{word}")?;
        }
        self.write_all(b"],\"daemon\":[")?;
        for i in 0..4 {
            if i != 0 {
                self.write_all(b",")?;
            }
            let word = u64::from_be_bytes(
                self.daemon[i * 8..i * 8 + 8]
                    .try_into()
                    .expect("daemon word width"),
            );
            write!(self, "{word}")?;
        }
        write!(
            self,
            "],\"section\":{section},\"index\":{index},\"available\":{},\"values\":[",
            values.is_some()
        )?;
        let values = match values {
            Some(values) => values,
            None => {
                self.unavailable += 1;
                &[]
            }
        };
        for (i, value) in values.iter().enumerate() {
            if i != 0 {
                self.write_all(b",")?;
            }
            write!(self, "{value}")?;
        }
        self.write_all(b"]}\n")?;
        self.records += 1;
        self.numbers += values.len() as u64;
        Ok(())
    }
    fn finish(mut self) -> io::Result<()> {
        self.row(99, 0, Some(&[self.records, self.numbers, self.unavailable]))
    }
}
impl<W: Write> Write for Records<'_, W> {
    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        // Override Write's default Interrupted/short-write loop: this sink has
        // one attempt for each delivered fragment and retains its first error.
        self.write(bytes).map(|_| ())
    }
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > RECORD_BYTES.saturating_sub(self.bytes) {
            return Err(io::Error::other("diagnostic record bytes exceeded"));
        }
        let written = self.sink.write(bytes)?;
        self.bytes += written;
        if written != bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "short diagnostic sink write",
            ));
        }
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.sink.flush()
    }
}

/// An observed control's outcome, the diagnostic failure that ends its
/// connection (untraced only), and a traced request's telemetry.
pub(super) type Observed = (
    Result<Success, Failure>,
    Option<io::Error>,
    Option<SdkValue>,
);
impl Application {
    pub(super) fn observed_control(&self, call: &Call, slot: usize) -> Observed {
        let operation = match call.request.without_observation() {
            Ok(operation) => operation,
            Err(_) => {
                return (
                    Err(Failure::Rejected(
                        ControlCode::Invalid,
                        "invalid observed operation",
                    )),
                    None,
                    None,
                )
            }
        };
        let Request::Observed { scope, traced, .. } = &call.request else {
            return (
                Err(Failure::Rejected(
                    ControlCode::Invalid,
                    "observation wrapper absent",
                )),
                None,
                None,
            );
        };
        let observing = std::time::Instant::now();
        let service = self
            .state
            .lock()
            .ok()
            .and_then(|state| state.startup.service.clone());
        #[cfg(target_os = "linux")]
        let observer = service.as_ref().and_then(|service| {
            crate::control::request_token(operation)
                .and_then(|token| service.observed_kernel(token).ok().flatten())
        });
        // The operation keeps its exact original result independently of every
        // later counter acquisition and diagnostic sink outcome.
        // A traced request records this thread's timing roots; the guard
        // ends recording on every exit, unwinding included.
        // A traced call's SQL work is the Store sessions' totals after it
        // minus before it.
        let sessions = traced
            .then(|| self.observed_store(crate::bootstrap::OpenedStore::sql_sessions))
            .flatten();
        let before = sessions.as_ref().map(crate::bootstrap::SqlSessions::totals);
        // And its delta selection work: the Store's Save totals after it minus
        // before it.
        let saves = traced
            .then(|| self.observed_store(|opened| opened.store.clone()))
            .flatten();
        let saves_before = saves.as_ref().map(|store| store.save_counters());
        let observed_before = observing.elapsed();
        let recording = traced.then(crate::telemetry::Recording::start);
        let outcome = self.control_operation(operation, slot);
        let reports = recording.map(crate::telemetry::Recording::finish);
        let observed_after = std::time::Instant::now();
        let sql = sessions
            .as_ref()
            .zip(before)
            .and_then(|(sessions, before)| crate::telemetry::sql(before, sessions.totals()));
        let delta = saves
            .as_ref()
            .zip(saves_before)
            .and_then(|(store, before)| crate::telemetry::delta(before, store.save_counters()));
        // A traced status or unmount carries the Workspace's kernel opcode
        // counts since attach.
        #[cfg(target_os = "linux")]
        let fuse = observer
            .as_ref()
            .filter(|_| *traced && matches!(operation, Request::Status(_) | Request::Unmount(_)))
            .map(|observer| crate::telemetry::fuse(&observer.facts().opcodes));
        #[cfg(not(target_os = "linux"))]
        let fuse = None;
        // One explicitly charged read-only observer call. It transfers its typed
        // value before the snapshot. This observation never replaces the
        // original outcome.
        let token = match &outcome {
            Ok(success) => crate::control::reply_token(&success.reply),
            Err(_) => None,
        }
        .or_else(|| crate::control::request_token(operation));
        let resources = match (token, &service) {
            (Some(token), Some(service)) => match service.observed_overlay(token) {
                Ok(client) => resources::ResourceObservation::acquire(&client),
                Err(Failure::Rejected(ControlCode::Missing, _)) => {
                    resources::ResourceObservation::unavailable()
                }
                Err(cause) => resources::ResourceObservation::selection_failed(cause),
            },
            _ => resources::ResourceObservation::unavailable(),
        };
        let store = service.as_ref().map(|service| service.observed_store());
        let result = (|| {
            let mut rows = Records::new(
                io::stderr().lock(),
                call.id,
                self.setup.limits.namespaces as usize + 2,
                scope,
                &self.instance,
                slot,
            );
            rows.row(
                0,
                token.map_or(0, |token| token.namespace),
                Some(&[
                    operation_code(operation),
                    u64::from(self.setup.limits.namespaces) + 2,
                    self.setup.limits.cache_bytes,
                ]),
            )?;
            rows.row(38, 0, Some(&[1]))?;
            resources::emit(&mut rows, &resources)?;
            schema::owner(&mut rows, self.owner.client().diagnostics().ok())?;
            rows.row(
                33,
                0,
                Some(&[u64::from(
                    self.owner
                        .profile()
                        .compile_options
                        .iter()
                        .any(|option| option == "ENABLE_MEMORY_MANAGEMENT"),
                )]),
            )?;
            if let Some(store) = &store {
                schema::store(&mut rows, store)?;
            } else {
                for section in [8, 9, 10] {
                    rows.row(section, 0, None)?;
                }
            }
            #[cfg(target_os = "linux")]
            {
                let observer = observer.or_else(|| {
                    service.as_ref().and_then(|service| {
                        token.and_then(|token| service.observed_kernel(token).ok().flatten())
                    })
                });
                let drained = match &outcome {
                    Ok(success) => success.kernel.as_ref().map(|receipt| receipt.work),
                    Err(Failure::After { original, .. }) => {
                        original.kernel.as_ref().map(|receipt| receipt.work)
                    }
                    _ => None,
                };
                kernel::emit(
                    &mut rows,
                    observer.as_ref().map(|observer| observer.facts()),
                    drained,
                )?;
            }
            #[cfg(not(target_os = "linux"))]
            rows.row(12, 0, None)?;
            schema::sql(&mut rows, 13, -1, self.observed_sql(None))?;
            for index in self.observed_reader_indices() {
                let reader_index = if index == usize::MAX {
                    -2
                } else {
                    index as i64
                };
                schema::sql(&mut rows, 14, reader_index, self.observed_sql(Some(index)))?;
                schema::reader_storage(
                    &mut rows,
                    reader_index,
                    store
                        .as_ref()
                        .and_then(|store| store.reader_storage_diagnostics(index)),
                )?;
            }
            schema::construction(&mut rows, &outcome)?;
            rows.finish()
        })();
        let diagnostic = resources.into_diagnostic_error(result.err());
        match reports {
            // The SDK trace route reports a failed diagnostic write beside
            // the reply and keeps its connection.
            Some(mut reports) => {
                // The observation's own spans around the operation: its
                // counter snapshots before, and after it the diagnostic
                // records and counters.
                for (name, elapsed) in [
                    ("observe.before", observed_before),
                    ("observe.after", observed_after.elapsed()),
                ] {
                    reports.push(layerfs_telemetry::timer::TimingReport::from_root(
                        layerfs_telemetry::timer::TimingNode::new(name, elapsed),
                    ));
                }
                let telemetry = crate::telemetry::value(
                    reports,
                    &outcome,
                    diagnostic.as_ref(),
                    [sql, delta, fuse],
                );
                (outcome, None, Some(telemetry))
            }
            None => (outcome, diagnostic, None),
        }
    }
    fn observed_reader_indices(&self) -> Vec<usize> {
        let Ok(state) = self.state.lock() else {
            return Vec::new();
        };
        state
            .startup
            .installed
            .as_ref()
            .map(|value| &value.opened)
            .or(state.startup.existing.as_ref())
            .map_or_else(Vec::new, |opened| opened.reader_indices())
    }
    fn observed_sql(&self, reader: Option<usize>) -> Option<layerfs_persistence::SqlWork> {
        self.observed_store(|opened| opened.session_diagnostics(reader).ok().flatten())
            .flatten()
    }
    /// `read` over the installed, existing or failed-install opened Store.
    fn observed_store<T>(
        &self,
        read: impl FnOnce(&crate::bootstrap::OpenedStore) -> T,
    ) -> Option<T> {
        let state = self.state.lock().ok()?;
        let startup = &state.startup;
        let opened = startup
            .installed
            .as_ref()
            .map(|installed| &installed.opened)
            .or(startup.existing.as_ref())
            .or_else(|| {
                startup
                    .install_failure
                    .as_ref()
                    .and_then(|failed| failed.opened.as_ref())
            })?;
        Some(read(opened))
    }
}
fn operation_code(operation: &Request) -> u64 {
    match operation {
        Request::Mount { .. } => 1,
        Request::Commit(_) => 2,
        Request::Status(_) => 3,
        Request::Unmount(_) => 4,
        Request::Fork(_) => 5,
        Request::History(_) => 6,
        Request::Branches { .. } => 14,
        Request::Hello(_) => 7,
        Request::EndSession => 8,
        Request::Attach(_) => 9,
        Request::Locate(_) => 10,
        Request::ForceUnmount { .. } => 11,
        Request::Observed { .. } => 12,
        Request::Cleanup(_) => 13,
        Request::Sdk(_) => 15,
    }
}
