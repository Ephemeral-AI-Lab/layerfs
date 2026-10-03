//! Shared history transaction facade; backend owns acknowledgement and errors.
use super::{bindings::Parameters, provider::HistoryProvider};
use crate::backend::{records::Record, Transaction as BackendTransaction};
use layerfs_history::HistoryResult;
pub(crate) struct Transaction<'a> {
    backend: &'a BackendTransaction<'a>,
}
impl Transaction<'_> {
    pub(crate) fn query(&self, key: &str, params: impl Parameters) -> HistoryResult<Vec<Record>> {
        self.backend
            .history(key, params.into_params())
            .map_err(Into::into)
    }
    pub(crate) fn execute(&self, key: &str, params: impl Parameters) -> HistoryResult<usize> {
        self.query(key, params).map(|r| r.len())
    }
}
impl HistoryProvider {
    pub(crate) fn read<T>(
        &self,
        f: impl FnOnce(&Transaction<'_>) -> HistoryResult<T>,
    ) -> HistoryResult<T> {
        self.session.run(false, |b| f(&Transaction { backend: b }))
    }
    pub(crate) fn write<T>(
        &self,
        f: impl FnOnce(&Transaction<'_>) -> HistoryResult<T>,
    ) -> HistoryResult<T> {
        self.session.run(true, |b| f(&Transaction { backend: b }))
    }
}
