//! Name bindings a new saved tree adds, recorded as path hints for readers
//! (the daemon's Commit and merge; host Init records its own tree's).
use super::commit_types::CommitError;
use layerfs_content::compare::tree::directory_bindings_from;
use layerfs_content::compare::{
    directory_bindings, inode_differences, BindingHints, ChangedDirectories, TreeTable,
};
use layerfs_content::filesystem::path::PathName;
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult};
use layerfs_history::{BindingHint, HistoryCatalog, HistoryError};
use std::cell::RefCell;

/// Rows one hint write transaction carries.
const BATCH: usize = 4096;

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
    let directories = changed_directories(reader, left, right)?;
    record_directory_bindings(reader, catalog, &directories)
}

/// The directories whose listing differs between `left` and `right`, from
/// the difference of their inode tables.
fn changed_directories(
    reader: &dyn AuthenticatedObjects,
    left: Option<FilesystemRootId>,
    right: FilesystemRootId,
) -> Result<ChangedDirectories, CommitError> {
    let left = left
        .map(|root| TreeTable::open(reader, root))
        .transpose()
        .map_err(CommitError::from)?;
    let right = TreeTable::open(reader, right).map_err(CommitError::from)?;
    let mut directories = ChangedDirectories::default();
    inode_differences(
        reader,
        left.as_ref(),
        &right,
        &mut Default::default(),
        |serial, before, after| {
            directories.note(serial, before, after);
            Ok(())
        },
    )
    .map_err(CommitError::from)?;
    Ok(directories)
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
    record_directory_bindings_from(reader, catalog, directories, None)
}

fn record_directory_bindings_from(
    reader: &dyn AuthenticatedObjects,
    catalog: &dyn HistoryCatalog,
    directories: &ChangedDirectories,
    start: Option<(u64, &PathName)>,
) -> Result<u64, CommitError> {
    let (mut rows, mut recorded, mut failure) = (Vec::new(), 0u64, None);
    let walked = directory_bindings_from(reader, directories, start, |child, parent, name| {
        rows.push(BindingHint {
            child,
            parent,
            name: name.as_bytes().to_vec(),
        });
        if rows.len() == BATCH {
            if let Err(error) = catalog.record_bindings(&rows) {
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
        catalog
            .record_bindings(&rows)
            .map_err(CommitError::History)?;
        recorded += rows.len() as u64;
    }
    Ok(recorded)
}

pub(super) enum CollectedBindings {
    Fits(Vec<BindingHint>),
    Overflow(BindingOverflow),
}

pub(super) struct BindingOverflow {
    prefix: Vec<BindingHint>,
    parent: u64,
    name: PathName,
}

impl BindingOverflow {
    /// Records the already read first batch, then resumes at the next row.
    /// The Save must be published before either write.
    pub(super) fn record(
        self,
        reader: &dyn AuthenticatedObjects,
        catalog: &dyn HistoryCatalog,
        directories: &ChangedDirectories,
    ) -> Result<(), CommitError> {
        record_rows(catalog, &self.prefix)?;
        drop(self.prefix);
        record_directory_bindings_from(
            reader,
            catalog,
            directories,
            Some((self.parent, &self.name)),
        )?;
        Ok(())
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
        return Ok(CollectedBindings::Overflow(BindingOverflow {
            prefix: rows,
            parent,
            name,
        }));
    }
    walked.map_err(CommitError::from)?;
    Ok(CollectedBindings::Fits(rows))
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
