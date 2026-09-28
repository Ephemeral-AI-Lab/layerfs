//! Active directory portable-attribute publication.
use super::active_names::hot;
use crate::{
    backing::active::{dirty_key, inode_key, HotInode},
    runtime::coherence::MutationOrigin,
    *,
};
use std::time::Instant;

impl Workspace {
    pub(super) fn publish_active_directory_attributes(
        &self,
        attr: NodeAttributes,
        request: PortableAttributes,
        deadline: Instant,
        origin: MutationOrigin,
    ) -> Result<super::write::Publication, WorkspaceError> {
        let active = self
            .inner
            .active
            .as_ref()
            .ok_or(WorkspaceError::Unsupported)?;
        let mut state = self.state()?;
        self.available(&state)?;
        self.check_mutation_coherence(&state, origin, true)?;
        let node = state.node(attr.serial)?;
        if node.attr.kind != NodeKind::Directory || node.attr != attr {
            return Err(WorkspaceError::Busy);
        }
        let generation = state.generation;
        let already_dirty = active.get(&dirty_key(generation, attr.serial))?.is_some();
        state.frontier_bytes(
            &self.host,
            state.dirty_inodes + usize::from(!already_dirty),
            state.dirty_directories + usize::from(!already_dirty),
            state.fresh_files,
            state.fresh_symlinks,
            state.directory_names,
            state.directory_bytes,
        )?;
        let next = state
            .revision
            .checked_add(1)
            .ok_or(WorkspaceError::Capacity)?;
        let mut inode = match active.get(&inode_key(attr.serial))? {
            Some(value) => HotInode::parse(&value)?,
            None => hot(
                attr,
                node.content,
                node.metadata,
                generation,
                next,
                node.baseline == 0,
            ),
        };
        (inode.mode, inode.seconds, inode.nanos) = request.selected(attr);
        inode.revision = next;
        inode.generation = generation;
        let updates = [
            (dirty_key(generation, attr.serial).to_vec(), Some(vec![1])),
            (
                inode_key(attr.serial).to_vec(),
                Some(inode.value()?.to_vec()),
            ),
        ];
        crate::backing::payload::clock(deadline).map_err(|_| WorkspaceError::Deadline)?;
        let published = active.publish_records(&updates)?;
        if published != next {
            return Err(WorkspaceError::Io);
        }
        let attributes = inode.attributes(attr)?;
        for node in &mut state.nodes {
            if node.attr.serial == attr.serial {
                node.attr = attributes;
            }
        }
        state.revision = published;
        if !already_dirty {
            state.dirty_inodes += 1;
            state.dirty_directories += 1;
        }
        let receipt = MutationReceipt {
            incarnation: self.inner.incarnation,
            generation,
            inode: attr.serial,
            revision: published,
            accepted_bytes: 0,
        };
        let delivery = if origin.projected() {
            None
        } else {
            state
                .projection
                .as_ref()
                .and_then(|projection| projection.delivery.clone())
        };
        if let (Some(projection), Some(_)) = (&mut state.projection, &delivery) {
            projection.status = CoherenceStatus::Pending {
                receipt,
                published_handle: None,
            };
        }
        drop(state);
        if let Some(delivery) = delivery {
            self.complete_projection_mutation(delivery, receipt, None, None, deadline)?;
        }
        Ok(super::write::Publication {
            receipt,
            attributes,
            delivery: None,
            published_handle: None,
            cleanup_error: None,
        })
    }
}
