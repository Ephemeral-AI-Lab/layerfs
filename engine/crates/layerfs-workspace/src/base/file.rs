//! Retained authenticated file classification and owning cheap-length facts.
use crate::{BaseView, WorkspaceError, WorkspaceResult};
use layerfs_content::{object::InodeKind, ContentError, ContentResult, FileView};
use layerfs_telemetry::timer::Timing;

pub(crate) struct BoundFile {
    pub view: FileView,
}
impl BaseView {
    /// Resolves the exact immutable serial. Only logical absence means a new
    /// file; unavailable/denied/malformed provider objects retain their cause.
    pub(crate) fn captured_file(&self, serial: u64) -> WorkspaceResult<Option<BoundFile>> {
        self.captured_file_in(serial, None)
    }
    /// [`Self::captured_file`] with a window's provided lengths: a root the
    /// window holds takes its length from there, asked of the provider for
    /// this window, instead of from a demand of its own.
    pub(crate) fn captured_file_in(
        &self,
        serial: u64,
        window: Option<&crate::base::batch::BaseFiles>,
    ) -> WorkspaceResult<Option<BoundFile>> {
        let value = match self.inode(serial) {
            Ok(value) => value.value,
            Err(ContentError::PathNotFound) => return Ok(None),
            Err(original) => return Err(original.into()),
        };
        if value.kind != InodeKind::RegularFile {
            return Err(ContentError::WrongLogicalRole.into());
        }
        let length = match window.and_then(|files| files.length(value.content_root)) {
            Some(length) => length,
            None => self.client().provided_length(value.content_root)?,
        };
        let view = self.file(value)?;
        if view.logical_len() != length {
            return Err(WorkspaceError::Content(ContentError::LengthMismatch {
                expected: length,
                actual: view.logical_len(),
            }));
        }
        Ok(Some(BoundFile { view }))
    }
    pub(super) fn file(
        &self,
        value: layerfs_content::object::InodeValue,
    ) -> ContentResult<FileView> {
        if value.kind != InodeKind::RegularFile {
            return Err(ContentError::WrongLogicalRole);
        }
        let client = self.client();
        Timing::disabled("base.file", |scope| {
            FileView::open(client.as_ref(), value.content_root, scope.child("file"))
        })
        .0
    }
}
