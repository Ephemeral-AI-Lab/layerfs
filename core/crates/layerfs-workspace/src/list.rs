//! Bounded three-way ordered name merge; deletion work advances its resume key.
use crate::{OverlayRead, SourceView, WorkspaceResult};
use layerfs_content::{object::inode_leaf::InodeKind, ContentError};
use layerfs_overlay::{Dentry, PAGE_ROWS};
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewListing {
    pub entries: Vec<(Vec<u8>, u64)>,
    pub continuation: Option<Vec<u8>>,
    pub visited: usize,
}
impl SourceView {
    /// One processing window visits at most64 distinct keys, including whiteouts.
    /// Empty output may have continuation. Concurrent mutation has ordinary weak
    /// directory-read semantics; stable kernel cursor/reference ownership is S4.
    pub fn list(
        &self,
        overlay: &impl OverlayRead,
        parent: u64,
        after: Option<&[u8]>,
    ) -> WorkspaceResult<ViewListing> {
        if after.is_some_and(|key| key.len() > 255) {
            return Err(ContentError::PathLimitExceeded.into());
        }
        if self.stat(overlay, parent)?.kind != InodeKind::Directory {
            return Err(ContentError::WrongLogicalRole.into());
        }
        let base = match self
            .base
            .list_after_bytes(parent, after, PAGE_ROWS, PAGE_ROWS * 264)
        {
            Ok(page) => page,
            Err(ContentError::PathNotFound) => layerfs_content::filesystem::DirectoryListing {
                entries: Vec::new(),
                continuation: None,
            },
            Err(error) => return Err(error.into()),
        };
        let local = overlay.names(self.source, parent, after)?;
        if local.source != self.source || local.parent != parent {
            return Err(ContentError::InvalidRecord("source name response").into());
        }
        if let Some(inode) = &local.parent_inode {
            if inode.kind != layerfs_overlay::InodeKind::Directory {
                return Err(ContentError::WrongLogicalRole.into());
            }
        }
        for rows in [&local.active, &local.captured] {
            if rows.len() > PAGE_ROWS
                || rows.windows(2).any(|p| p[0].name >= p[1].name)
                || rows.iter().any(|row| {
                    row.parent != parent
                        || row.name.is_empty()
                        || row.name.len() > 255
                        || after.is_some_and(|key| row.name.as_slice() <= key)
                })
            {
                return Err(ContentError::InvalidRecord("source name window").into());
            }
        }
        let mut b = 0;
        let mut a = 0;
        let mut c = 0;
        let mut entries = Vec::new();
        let mut visited = 0;
        let mut last = None;
        while visited < PAGE_ROWS {
            let key = [
                base.entries.get(b).map(|(name, _)| name.as_bytes()),
                local.active.get(a).map(|row| row.name.as_slice()),
                local.captured.get(c).map(|row| row.name.as_slice()),
            ]
            .into_iter()
            .flatten()
            .min();
            let Some(key) = key else {
                break;
            };
            let mut serial = None;
            if base
                .entries
                .get(b)
                .is_some_and(|(name, _)| name.as_bytes() == key)
            {
                serial = Some(base.entries[b].1);
                b += 1;
            }
            if let Some(row) = matching(&local.captured, c, key) {
                serial = row.serial;
                c += 1;
            }
            if let Some(row) = matching(&local.active, a, key) {
                serial = row.serial;
                a += 1;
            }
            let key = key.to_vec();
            visited += 1;
            if let Some(serial) = serial {
                entries.push((key.clone(), serial));
            }
            last = Some(key);
        }
        let more = b < base.entries.len()
            || a < local.active.len()
            || c < local.captured.len()
            || base.continuation.is_some()
            || local.active.len() == PAGE_ROWS
            || local.captured.len() == PAGE_ROWS;
        Ok(ViewListing {
            entries,
            continuation: if more { last } else { None },
            visited,
        })
    }
}
fn matching<'a>(rows: &'a [Dentry], index: usize, key: &[u8]) -> Option<&'a Dentry> {
    rows.get(index).filter(|row| row.name == key)
}
