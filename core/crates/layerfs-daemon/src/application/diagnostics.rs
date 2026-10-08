//! Explicit per-control numeric observations of existing counters only.
mod native;
pub(super) mod resources;
mod schema;

use super::Application;
use crate::control::{Failure, Success};
use layerfs_bridge::control::{Call, ControlCode, Request};
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
        readers: u16,
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
            maximum: FIXED_RECORDS + 2 * usize::from(readers),
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

impl Application {
    pub(super) fn observed_control(
        &self,
        call: &Call,
        slot: usize,
    ) -> (Result<Success, Failure>, Option<io::Error>) {
        let operation = match call.request.without_observation() {
            Ok(operation) => operation,
            Err(_) => {
                return (
                    Err(Failure::Rejected(
                        ControlCode::Invalid,
                        "invalid observed operation",
                    )),
                    None,
                )
            }
        };
        let Request::Observed { scope, .. } = &call.request else {
            return (
                Err(Failure::Rejected(
                    ControlCode::Invalid,
                    "observation wrapper absent",
                )),
                None,
            );
        };
        let service = self
            .state
            .lock()
            .ok()
            .and_then(|state| state.startup.service.clone());
        #[cfg(target_os = "linux")]
        let observer = service.as_ref().and_then(|service| {
            crate::control::request_token(operation)
                .and_then(|token| service.observed_native(token).ok().flatten())
        });
        // The operation keeps its exact original result independently of every
        // later counter acquisition and diagnostic sink outcome.
        let outcome = self.control_operation(operation, slot);
        // One explicitly charged read-only observer job. The caller drops its
        // Completion before the snapshot, but publisher credit may remain
        // transiently. This observation never replaces the original outcome.
        let resources = resources::ResourceObservation::acquire(&self.owner.client());
        let store = service.as_ref().map(|service| service.observed_store());
        let result = (|| {
            let mut rows = Records::new(
                io::stderr().lock(),
                call.id,
                self.setup.limits.read_handles,
                scope,
                &self.instance,
                slot,
            );
            let token = match &outcome {
                Ok(success) => crate::control::reply_token(&success.reply),
                Err(_) => None,
            }
            .or_else(|| crate::control::request_token(operation));
            rows.row(
                0,
                token.map_or(0, |token| token.namespace),
                Some(&[
                    operation_code(operation),
                    u64::from(self.setup.limits.read_handles),
                    self.setup.limits.cache_bytes,
                ]),
            )?;
            resources::emit(&mut rows, &resources)?;
            schema::owner(&mut rows, self.owner.client().diagnostics().ok())?;
            if let Some(store) = &store {
                schema::store(&mut rows, store)?;
            } else {
                for section in [8, 9, 10] {
                    rows.row(section, 0, None)?;
                }
            }
            #[cfg(target_os = "linux")]
            schema::dispatch(
                &mut rows,
                self.native.as_ref().and_then(|native| native.work()),
            )?;
            #[cfg(not(target_os = "linux"))]
            rows.row(11, 0, None)?;
            #[cfg(target_os = "linux")]
            {
                let observer = observer.or_else(|| {
                    service.as_ref().and_then(|service| {
                        token.and_then(|token| service.observed_native(token).ok().flatten())
                    })
                });
                let drained = match &outcome {
                    Ok(success) => success.native.as_ref().map(|receipt| receipt.work),
                    Err(Failure::After { original, .. }) => {
                        original.native.as_ref().map(|receipt| receipt.work)
                    }
                    _ => None,
                };
                native::emit(
                    &mut rows,
                    observer.as_ref().map(|observer| observer.facts()),
                    drained,
                )?;
            }
            #[cfg(not(target_os = "linux"))]
            rows.row(12, 0, None)?;
            schema::sql(&mut rows, 13, -1, self.observed_sql(None))?;
            for index in 0..usize::from(self.setup.limits.read_handles) {
                schema::sql(&mut rows, 14, index as i64, self.observed_sql(Some(index)))?;
                schema::reader_storage(
                    &mut rows,
                    index as i64,
                    store
                        .as_ref()
                        .and_then(|store| store.reader_storage_diagnostics(index)),
                )?;
            }
            schema::construction(&mut rows, &outcome)?;
            rows.finish()
        })();
        (outcome, resources.into_diagnostic_error(result.err()))
    }
    fn observed_sql(&self, reader: Option<usize>) -> Option<layerfs_persistence::SqlWork> {
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
        opened.session_diagnostics(reader).ok().flatten()
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
        Request::Hello(_) => 7,
        Request::EndSession => 8,
        Request::Attach(_) => 9,
        Request::Locate(_) => 10,
        Request::ForceUnmount { .. } => 11,
        Request::Observed { .. } => 12,
        Request::Cleanup(_) => 13,
    }
}
