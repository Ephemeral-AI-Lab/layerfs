//! Name bindings a new saved tree adds, recorded as path hints for readers
//! (the daemon's Commit and merge; host Init records its own tree's).
use super::commit_types::CommitError;
use layerfs_content::compare::tree::directory_bindings_from;
use layerfs_content::compare::{
    directory_bindings, new_bindings, BindingHints, ChangedDirectories, TreeTable,
};
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_content::filesystem::{binding_changes::BindingChanges, path::PathName};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult};
use layerfs_history::{BindingHint, HistoryCatalog, HistoryError};
use layerfs_overlay::IndexedOperationRecordScope;
use layerfs_workspace::IndexedEditRecords;
use std::cell::RefCell;

/// Rows one hint write transaction carries.
const BATCH: usize = layerfs_persistence::HINT_BATCH_ROWS;

/// Records every name binding `right` holds that `left` does not, in bounded
/// write transactions, and returns how many rows it recorded. `left` is a
/// saved tree whose bindings are already recorded; Init, which has none,
/// records every binding of its tree. Work follows the change between them.
pub fn record_new_bindings(
    reader: &dyn AuthenticatedObjects,
    catalog: &dyn HistoryCatalog,
    left: Option<FilesystemRootId>,
    right: FilesystemRootId,
) -> Result<u64, CommitError> {
    let left = left
        .map(|root| TreeTable::open(reader, root))
        .transpose()
        .map_err(CommitError::from)?;
    let right = TreeTable::open(reader, right).map_err(CommitError::from)?;
    record_binding_rows(
        |rows| catalog.record_bindings(rows),
        |visit| new_bindings(reader, left.as_ref(), &right, visit),
    )
}

/// Records the new bindings of the changed `directories`, read a window of
/// directories at a time (#810 F1, `directory_bindings`). A Commit takes
/// them from its logical count's walk (`logical_after`, #810 F4), so the
/// two inode tables are walked once.
pub(super) fn record_directory_bindings(
    reader: &dyn AuthenticatedObjects,
    catalog: &dyn HistoryCatalog,
    directories: &ChangedDirectories,
) -> Result<u64, CommitError> {
    record_directory_bindings_from(
        reader,
        |rows| catalog.record_bindings(rows),
        directories,
        None,
    )
}

fn record_directory_bindings_from(
    reader: &dyn AuthenticatedObjects,
    record: impl FnMut(&[BindingHint]) -> Result<(), HistoryError>,
    directories: &ChangedDirectories,
    start: Option<(u64, &PathName)>,
) -> Result<u64, CommitError> {
    record_binding_rows(record, |visit| {
        directory_bindings_from(reader, directories, start, visit)
    })
}

fn record_binding_rows(
    mut record: impl FnMut(&[BindingHint]) -> Result<(), HistoryError>,
    walk: impl FnOnce(&mut dyn FnMut(u64, u64, &PathName) -> ContentResult<()>) -> ContentResult<()>,
) -> Result<u64, CommitError> {
    let (mut rows, mut recorded, mut failure) = (Vec::new(), 0u64, None);
    let walked = walk(&mut |child, parent, name| {
        rows.push(BindingHint {
            child,
            parent,
            name: name.as_bytes().to_vec(),
        });
        if rows.len() == BATCH {
            if let Err(error) = record(&rows) {
                failure = Some(error);
                return Err(ContentError::OutputRejected);
            }
            recorded += rows.len() as u64;
            rows.clear();
        }
        Ok(())
    });
    if let Some(error) = failure {
        return Err(CommitError::History(error));
    }
    walked.map_err(CommitError::from)?;
    if !rows.is_empty() {
        record(&rows).map_err(CommitError::History)?;
        recorded += rows.len() as u64;
    }
    Ok(recorded)
}

pub(super) enum CollectedBindings {
    Fits(Vec<BindingHint>),
    Overflow(BindingOverflow),
}

pub(super) struct CapturedBindings {
    changes: BindingChanges,
    scope: IndexedOperationRecordScope,
}

impl CapturedBindings {
    pub(super) fn new(changes: BindingChanges, scope: IndexedOperationRecordScope) -> Self {
        Self { changes, scope }
    }

    fn walk(
        self,
        owner: &crate::OwnerClient,
        visit: &mut dyn FnMut(u64, u64, &PathName) -> ContentResult<()>,
    ) -> Result<(), CommitError> {
        let mut records = IndexedEditRecords::new(owner, self.scope);
        self.changes.visit(&mut records, visit).map_err(|error| {
            super::captured::original_cause(error, records.into_custody().failure, || None)
        })
    }

    pub(super) fn collect(
        self,
        owner: &crate::OwnerClient,
    ) -> Result<CollectedBindings, CommitError> {
        if self.changes.len() > BATCH as u64 {
            return Ok(CollectedBindings::Overflow(BindingOverflow::Captured(self)));
        }
        let mut rows = Vec::with_capacity(self.changes.len() as usize);
        self.walk(owner, &mut |child, parent, name| {
            rows.push(BindingHint {
                child,
                parent,
                name: name.as_bytes().to_vec(),
            });
            Ok(())
        })?;
        Ok(CollectedBindings::Fits(rows))
    }

    pub(super) fn record(
        self,
        owner: &crate::OwnerClient,
        catalog: &dyn HistoryCatalog,
    ) -> Result<(), CommitError> {
        self.record_with(owner, |rows| catalog.record_bindings(rows))
    }

    fn record_with(
        self,
        owner: &crate::OwnerClient,
        mut record: impl FnMut(&[BindingHint]) -> Result<(), HistoryError>,
    ) -> Result<(), CommitError> {
        let mut rows = Vec::new();
        let mut failure = None;
        let walked = self.walk(owner, &mut |child, parent, name| {
            rows.push(BindingHint {
                child,
                parent,
                name: name.as_bytes().to_vec(),
            });
            if rows.len() == BATCH {
                if let Err(error) = record(&rows) {
                    failure = Some(error);
                    return Err(ContentError::OutputRejected);
                }
                rows.clear();
            }
            Ok(())
        });
        if let Some(error) = failure {
            return Err(CommitError::History(error));
        }
        walked?;
        if !rows.is_empty() {
            record(&rows).map_err(CommitError::History)?;
        }
        Ok(())
    }
}

pub(super) enum BindingOverflow {
    Canonical {
        prefix: Vec<BindingHint>,
        parent: u64,
        name: PathName,
    },
    Captured(CapturedBindings),
}

impl BindingOverflow {
    /// Records the prefix and remaining rows after the held Save publishes.
    pub(super) fn record(
        self,
        owner: &crate::OwnerClient,
        reader: &dyn AuthenticatedObjects,
        catalog: &dyn HistoryCatalog,
        directories: &ChangedDirectories,
    ) -> Result<(), CommitError> {
        match self {
            Self::Canonical {
                prefix,
                parent,
                name,
            } => {
                catalog
                    .record_overflow_bindings(&prefix)
                    .map_err(CommitError::History)?;
                drop(prefix);
                record_directory_bindings_from(
                    reader,
                    |rows| catalog.record_overflow_bindings(rows),
                    directories,
                    Some((parent, &name)),
                )?;
                Ok(())
            }
            Self::Captured(plan) => {
                plan.record_with(owner, |rows| catalog.record_overflow_bindings(rows))
            }
        }
    }
}

/// [`record_directory_bindings`] for a fused Commit (#768). When the rows
/// exceed one write transaction, keep the first batch and the next row's
/// position so recording can resume there after the Save publishes.
/// A hint row is never written before the objects of the tree it describes.
pub(super) fn collect_directory_bindings(
    reader: &dyn AuthenticatedObjects,
    directories: &ChangedDirectories,
) -> Result<CollectedBindings, CommitError> {
    let (mut rows, mut next) = (Vec::new(), None);
    let walked = directory_bindings(reader, directories, |child, parent, name| {
        if rows.len() == BATCH {
            next = Some((parent, name.clone()));
            return Err(ContentError::OutputRejected);
        }
        rows.push(BindingHint {
            child,
            parent,
            name: name.as_bytes().to_vec(),
        });
        Ok(())
    });
    if let Some((parent, name)) = next {
        return Ok(CollectedBindings::Overflow(BindingOverflow::Canonical {
            prefix: rows,
            parent,
            name,
        }));
    }
    walked.map_err(CommitError::from)?;
    Ok(CollectedBindings::Fits(rows))
}

/// [`record_new_bindings`] for a fused Commit (#768), recording nothing:
/// the rows, when they fit one write transaction ([`BATCH`]), for the
/// Commit's own transaction. `None` when they do not: the walk stops at the
/// first row past [`BATCH`] and nothing is recorded, so the caller can
/// publish its Save first and record them then (`record_new_bindings`), as
/// the separate path does, and attempt the Commit alone.
/// A hint row is never written before the objects of the tree it describes.
pub fn collect_new_bindings(
    reader: &dyn AuthenticatedObjects,
    left: Option<FilesystemRootId>,
    right: FilesystemRootId,
) -> Result<Option<Vec<BindingHint>>, CommitError> {
    let left = left
        .map(|root| TreeTable::open(reader, root))
        .transpose()
        .map_err(CommitError::from)?;
    let right = TreeTable::open(reader, right).map_err(CommitError::from)?;
    let (mut rows, mut spilled) = (Vec::new(), false);
    let walked = new_bindings(reader, left.as_ref(), &right, |child, parent, name| {
        if rows.len() == BATCH {
            spilled = true;
            return Err(ContentError::OutputRejected);
        }
        rows.push(BindingHint {
            child,
            parent,
            name: name.as_bytes().to_vec(),
        });
        Ok(())
    });
    if spilled {
        return Ok(None);
    }
    walked.map_err(CommitError::from)?;
    Ok(Some(rows))
}

/// Records `rows` in bounded write transactions.
pub(super) fn record_rows(
    catalog: &dyn HistoryCatalog,
    rows: &[BindingHint],
) -> Result<(), CommitError> {
    for part in rows.chunks(BATCH) {
        catalog
            .record_bindings(part)
            .map_err(CommitError::History)?;
    }
    Ok(())
}

/// The History catalog's recorded bindings, as Content's hint port; a
/// catalog failure is kept to answer with its own cause.
pub struct CatalogHints<'a> {
    catalog: &'a dyn HistoryCatalog,
    /// The catalog failure that ended a lookup, if any.
    pub failure: RefCell<Option<HistoryError>>,
}

impl<'a> CatalogHints<'a> {
    /// Hints read from `catalog`.
    pub fn new(catalog: &'a dyn HistoryCatalog) -> Self {
        Self {
            catalog,
            failure: RefCell::new(None),
        }
    }
}

impl BindingHints for CatalogHints<'_> {
    fn bindings_of(&self, child: u64) -> ContentResult<Vec<(u64, PathName)>> {
        match self.catalog.bindings_of(child) {
            Ok(rows) => Ok(rows
                .into_iter()
                .filter_map(|row| Some((row.parent, PathName::from_bytes(&row.name).ok()?)))
                .collect()),
            Err(error) => {
                *self.failure.borrow_mut() = Some(error);
                Err(ContentError::ProviderFailure {
                    what: "binding hints",
                })
            }
        }
    }
}
