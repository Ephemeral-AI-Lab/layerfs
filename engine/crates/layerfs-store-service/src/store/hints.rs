//! Name bindings a new saved tree adds, recorded as path hints for readers
//! (the daemon's Commit and merge; host Init records its own tree's).
use super::commit_types::CommitError;
use layerfs_content::compare::{new_bindings, BindingHints, TreeTable};
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
    let left = left
        .map(|root| TreeTable::open(reader, root))
        .transpose()
        .map_err(CommitError::from)?;
    let right = TreeTable::open(reader, right).map_err(CommitError::from)?;
    let (mut rows, mut recorded, mut failure) = (Vec::new(), 0u64, None);
    let walked = new_bindings(reader, left.as_ref(), &right, |child, parent, name| {
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
