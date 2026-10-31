//! Name bindings a new saved tree adds, recorded as path hints for readers.
//! The recording runs on the (Unix) Init path only; the module stays portable.
#[cfg(unix)] // the whole body is called only by the (Unix) Init's publication
use crate::{ProjectError, ProjectResult};
#[cfg(unix)]
use layerfs_content::compare::{new_bindings, TreeTable};
#[cfg(unix)]
use layerfs_content::filesystem::FilesystemRootId;
#[cfg(unix)]
use layerfs_content::{AuthenticatedObjects, ContentError};
#[cfg(unix)]
use layerfs_history::{BindingHint, HistoryCatalog};

/// Rows one hint write transaction carries.
#[cfg(unix)] // recorded only by the (Unix) Init path
const BATCH: usize = 4096;

/// Records every name binding `right` holds that `left` does not, in bounded
/// write transactions, and returns how many rows it recorded. `left` is a
/// saved tree whose bindings are already recorded; Init, which has none,
/// records every binding of its tree. Work follows the change between them.
#[cfg(unix)] // called only by the (Unix) Init's history publication
pub(crate) fn record_new_bindings(
    reader: &dyn AuthenticatedObjects,
    catalog: &dyn HistoryCatalog,
    left: Option<FilesystemRootId>,
    right: FilesystemRootId,
) -> ProjectResult<u64> {
    let left = left
        .map(|root| TreeTable::open(reader, root))
        .transpose()
        .map_err(ProjectError::Content)?;
    let right = TreeTable::open(reader, right).map_err(ProjectError::Content)?;
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
        return Err(ProjectError::History(error));
    }
    walked.map_err(ProjectError::Content)?;
    if !rows.is_empty() {
        catalog
            .record_bindings(&rows)
            .map_err(ProjectError::History)?;
        recorded += rows.len() as u64;
    }
    Ok(recorded)
}
