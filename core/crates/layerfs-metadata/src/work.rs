//! Bounded per-statement client work observation; no parameters or SQL bodies.
use std::{sync::Mutex, time::Instant};
const STATEMENTS: usize = 128;
/// Aggregated synchronous client work for one shipped SQL template.
#[derive(Clone, Copy, Debug, Default)]
pub struct PgStatementWork {
    /// Deterministic FNV-1a identifier of the unexpanded SQL template.
    pub sql_id: u64,
    /// Completed or failed caller attempts observed.
    pub calls: u64,
    /// Attempts marked potentially mutating by the provider.
    pub mutating_calls: u64,
    /// Caller wall time, including queue, driver and return handoff.
    pub caller_ns: u64,
    /// Time from submitting the job until the I/O worker accepts it.
    pub queue_ns: u64,
    /// I/O-worker wall around the published driver's query future.
    pub driver_ns: u64,
}
/// Bounded aggregate statement observation, separate from physical wire counts.
#[derive(Clone, Debug, Default)]
pub struct PgWork {
    /// Statement classes retained, at most 128.
    pub statements: Vec<PgStatementWork>,
    /// Observations omitted because the class bound or observation lock failed.
    pub omitted: u64,
}
#[derive(Default)]
pub(crate) struct Work(Mutex<PgWork>);
impl Work {
    pub(crate) fn record(&self, id: u64, update: impl FnOnce(&mut PgStatementWork)) {
        let Ok(mut work) = self.0.lock() else { return };
        if let Some(row) = work.statements.iter_mut().find(|row| row.sql_id == id) {
            update(row);
        } else if work.statements.len() < STATEMENTS {
            let mut row = PgStatementWork {
                sql_id: id,
                ..Default::default()
            };
            update(&mut row);
            work.statements.push(row);
        } else {
            work.omitted += 1;
        }
    }
    pub(crate) fn snapshot(&self) -> PgWork {
        self.0.lock().map(|value| value.clone()).unwrap_or(PgWork {
            statements: Vec::new(),
            omitted: 1,
        })
    }
}
pub(crate) fn sql_id(sql: &str) -> u64 {
    sql.bytes().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
pub(crate) fn elapsed(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}
