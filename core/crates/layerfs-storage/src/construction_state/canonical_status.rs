//! Deferred canonical owners and exact failure custody in the selected session.
use super::session::Resource;
use crate::{StorageError, StorageResult};
use rusqlite::Connection;
impl Resource {
    pub(crate) fn check_engine(&self) -> StorageResult<()> {
        if let Some(engine) = self.engine {
            if let Err(error) = engine.validate() {
                if self.connection.as_ref().is_some_and(|c| !c.is_autocommit()) {
                    self.unknown.set(true);
                    return Err(StorageError::UnknownOutcome {
                        original: Box::new(error),
                    });
                }
                return Err(error);
            }
        }
        Ok(())
    }

    pub(crate) fn canonical_persistent_bytes(&self) -> usize {
        let counts = self.counts.as_ref().map_or(0, |s| {
            super::graph_layout::COUNT_PERSISTENT
                + if s.retirement.is_some() {
                    super::graph_layout::COUNT_RETIRE_PROOF
                } else {
                    0
                }
        });
        counts
            + self
                .releasing
                .as_ref()
                .map_or(0, |_| super::graph_layout::RELEASE_PERSISTENT)
    }

    pub(crate) fn verify_canonical(&self, connection: &Connection) -> StorageResult<()> {
        if self.canonical_capacity.is_some() {
            if let Some(counts) = &self.counts {
                super::count_index::verify(connection, counts)?;
            } else {
                super::count_index::deferred(connection)?;
            }
            if let Some(releasing) = &self.releasing {
                super::release_index::verify(connection, releasing)?;
            } else {
                super::release_index::deferred(connection)?;
            }
        }
        Ok(())
    }
    pub(crate) fn fail_canonical(&self) {
        if let Some(counts) = &self.counts {
            counts.failed.set(true);
        }
        if let Some(releasing) = &self.releasing {
            releasing.failed.set(true);
        }
    }
    pub(crate) fn canonical_failed(&self) -> bool {
        self.counts.as_ref().is_some_and(|s| s.failed.get())
            || self.releasing.as_ref().is_some_and(|s| s.failed.get())
            || self.pool_attempt.is_some()
    }
    pub(crate) fn canonical_failure(&self, mut failure: Option<String>) -> Option<String> {
        if let Some(counts) = &self.counts {
            if failure.is_some() || counts.failed.get() || counts.attempt.is_some() {
                failure = Some(format!(
                    "{}; {}",
                    failure.unwrap_or_default(),
                    counts.description()
                ));
            }
        }
        if let Some(releasing) = &self.releasing {
            if failure.is_some() || releasing.failed.get() || releasing.attempt.is_some() {
                failure = Some(format!(
                    "{}; {}",
                    failure.unwrap_or_default(),
                    releasing.description()
                ));
            }
        }
        if let Some(attempt) = &self.pool_attempt {
            failure = Some(format!(
                "{}; {}",
                failure.unwrap_or_default(),
                attempt.description()
            ));
        }
        failure
    }
}
