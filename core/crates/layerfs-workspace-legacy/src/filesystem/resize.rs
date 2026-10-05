//! One checked portable-attribute request applied to the selected inode version.
use crate::{runtime::coherence::MutationOrigin, *};
use std::time::Instant;
impl Workspace {
    /// Changes portable mode and mtime with an optional length change. Every
    /// field is checked against the selected kind and version before any page is
    /// written; an unsupported or invalid field changes nothing. A metadata-only
    /// change keeps the selected content and republishes only attributes.
    pub fn set_attributes(
        &self,
        serial: u64,
        request: PortableAttributes,
        deadline: Instant,
    ) -> Result<NodeAttributes, WorkspaceError> {
        self.set_attributes_from(serial, request, deadline, MutationOrigin::Local)
    }
    pub(crate) fn set_attributes_from(
        &self,
        serial: u64,
        request: PortableAttributes,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<NodeAttributes, WorkspaceError> {
        if self.inner.access != WorkspaceAccess::LocalEdit {
            return Err(WorkspaceError::ReadOnly);
        }
        if request.is_empty() {
            return Err(WorkspaceError::InvalidInput);
        }
        let deadline = Self::callback_deadline(deadline);
        let mut operation = self.begin(false, deadline)?;
        let attr = {
            let state = self.state()?;
            self.available(&state)?;
            state.node(serial)?.attr
        };
        request.check(attr.kind, attr.size)?;
        if origin.projected() {
            let state = self.state()?;
            self.available(&state)?;
            self.check_projected_mutation(&state, false)?;
        }
        if attr.kind == NodeKind::Directory {
            if request.size.is_some() {
                return Err(WorkspaceError::Unsupported);
            }
            return self
                .mutate_directory(attr, request, deadline, origin)
                .map(|published| published.attributes);
        }
        let original = self.serial_original(serial, deadline, &mut operation)?;
        if original.0.kind != attr.kind {
            return Err(WorkspaceError::WrongKind);
        }
        if attr.kind != NodeKind::File {
            return Err(WorkspaceError::WrongKind);
        }
        self.mutate_file(
            original,
            super::write::FileMutation::Attributes {
                request,
                handle: None,
                origin,
            },
            deadline,
            None,
        )
        .map(|published| published.attributes)
    }
}
impl Workspace {
    fn mutate_directory(
        &self,
        attr: NodeAttributes,
        request: PortableAttributes,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<super::write::Publication, WorkspaceError> {
        if attr.kind != NodeKind::Directory || request.size.is_some() {
            return Err(WorkspaceError::Unsupported);
        }
        self.publish_active_directory_attributes(attr, request, deadline, origin)
    }
}
