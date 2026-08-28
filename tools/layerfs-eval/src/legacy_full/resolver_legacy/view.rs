use super::super::session_legacy::{LayerFs, VfsError, VfsResult};
use super::super::OperationCounters;
use super::mutation::{create_file, replace_range_at_ref, require_main, update_record};
use super::resolution::{load_record, namespace, resolve, resolve_parent};
use layerfs_core::content::rope::{
    build, read_plan, read_range_with_plan, FileStateRoot, ReadPlan,
};
use layerfs_core::inode::{InodeKind, InodeRecordV1, InodeTableRoot};
use layerfs_core::namespace::{directory_lookup, DirectoryStateRoot, NamespaceCounters};
use layerfs_core::namespace_codec::encode_namespace_root;
use layerfs_core::{CanonicalPath, ObjectId};
use layerfs_storage::refs::RefState;
use std::io::{Read, Write};
use std::ops::Range;
use std::sync::Arc;
impl LayerFs {
    fn resolve_regular_read(
        &self,
        root: ObjectId,
        path: &CanonicalPath,
        counters: &mut OperationCounters,
    ) -> VfsResult<Arc<ReadPlan>> {
        if let Some(plan) = self.resolved_read_cache.get(root, path)? {
            return Ok(plan);
        }
        let namespace = namespace(self.engine.as_ref(), root)?;
        let (_, record) = resolve(self.engine.as_ref(), namespace, path, counters)?;
        if record.kind != InodeKind::RegularFile {
            return Err(VfsError::InvalidState);
        }
        let mut rope = Default::default();
        let plan = Arc::new(read_plan(
            self.engine.as_ref(),
            FileStateRoot(record.content_root),
            &mut rope,
        )?);
        counters.add_rope(rope)?;
        self.resolved_read_cache.put(root, path, plan.clone())?;
        Ok(plan)
    }

    pub fn current_head(&self, name: &str) -> VfsResult<RefState> {
        if name != "main" {
            return Err(VfsError::InvalidState);
        }
        self.engine.read_ref(name)?.ok_or(VfsError::InvalidState)
    }

    fn read_range_canonical<W: Write>(
        &self,
        root: ObjectId,
        path: &CanonicalPath,
        range: Range<u64>,
        output: W,
    ) -> VfsResult<OperationCounters> {
        let reservation = self.operation_q.reserve();
        let mut counters = OperationCounters::default();
        let plan = self.resolve_regular_read(root, path, &mut counters)?;
        counters.add_rope(read_range_with_plan(
            self.engine.as_ref(),
            &plan,
            range,
            output,
        )?)?;
        reservation.finish(&mut counters);
        Ok(counters)
    }

    fn read_to_canonical<W: Write>(
        &self,
        root: ObjectId,
        path: &CanonicalPath,
        output: W,
    ) -> VfsResult<OperationCounters> {
        let reservation = self.operation_q.reserve();
        let mut counters = OperationCounters::default();
        let plan = self.resolve_regular_read(root, path, &mut counters)?;
        counters.add_rope(read_range_with_plan(
            self.engine.as_ref(),
            &plan,
            0..plan.logical_len(),
            output,
        )?)?;
        reservation.finish(&mut counters);
        Ok(counters)
    }

    fn replace_range_canonical<R: Read>(
        &self,
        expected: &RefState,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        input: R,
    ) -> VfsResult<(RefState, OperationCounters)> {
        let reservation = self.operation_q.reserve();
        require_main(expected)?;
        let (state, mut counters) = replace_range_at_ref(
            self.engine.as_ref(),
            expected,
            path,
            start,
            delete_len,
            input,
        )?;
        reservation.finish(&mut counters);
        Ok((state, counters))
    }

    fn replace_range_for_refresh_canonical<R: Read>(
        &self,
        expected: &RefState,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        input: R,
    ) -> VfsResult<(super::super::AcceptedSplice, OperationCounters)> {
        let (after, counters) =
            self.replace_range_canonical(expected, path, start, delete_len, input)?;
        let old_len = counters
            .rope
            .logical_len_before
            .ok_or(VfsError::InvalidState)?;
        let new_len = counters
            .rope
            .logical_len_after
            .ok_or(VfsError::InvalidState)?;
        let insert_len = counters.rope.payload_bytes_written;
        if old_len
            .checked_sub(delete_len)
            .and_then(|length| length.checked_add(insert_len))
            != Some(new_len)
        {
            return Err(VfsError::InvalidState);
        }
        Ok((
            super::super::AcceptedSplice {
                before: expected.clone(),
                after,
                path: path.clone(),
                start,
                delete_len,
                insert_len,
                old_len,
                new_len,
            },
            counters,
        ))
    }

    fn replace_file_canonical<R: Read>(
        &self,
        expected: &RefState,
        path: &CanonicalPath,
        input: R,
    ) -> VfsResult<(RefState, OperationCounters)> {
        let reservation = self.operation_q.reserve();
        require_main(expected)?;
        let mut counters = OperationCounters::default();
        let mut publication = self.engine.begin_publication(Some(expected), "main")?;
        let namespace = namespace(&publication, expected.root)?;
        let (parent_inode, parent_record, name) =
            resolve_parent(&publication, namespace, path, &mut counters)?;
        let mut lookup = NamespaceCounters::default();
        let existing = directory_lookup(
            &publication,
            DirectoryStateRoot(parent_record.content_root),
            &name,
            &mut lookup,
        )?;
        counters.add_namespace(lookup)?;
        let (content, rope) = build(&mut publication, input)?;
        counters.add_rope(rope)?;
        let next_namespace = if let Some(inode) = existing {
            let record = load_record(
                &publication,
                InodeTableRoot(namespace.inode_table_root),
                inode,
                &mut counters,
            )?;
            if record.kind != InodeKind::RegularFile {
                return Err(VfsError::InvalidState);
            }
            update_record(
                &mut publication,
                namespace,
                inode,
                InodeRecordV1 {
                    content_root: content.0,
                    ..record
                },
                &mut counters,
            )?
        } else {
            create_file(
                &mut publication,
                namespace,
                parent_inode,
                parent_record,
                name,
                content,
                &mut counters,
            )?
        };
        let state = publication.publish_namespace(&encode_namespace_root(next_namespace)?)?;
        reservation.finish(&mut counters);
        Ok((state, counters))
    }

    pub fn read_range<W: Write>(
        &self,
        root: ObjectId,
        path: &str,
        range: Range<u64>,
        output: W,
    ) -> VfsResult<OperationCounters> {
        self.read_range_canonical(root, &CanonicalPath::new(path)?, range, output)
    }

    pub fn read_to<W: Write>(
        &self,
        root: ObjectId,
        path: &str,
        output: W,
    ) -> VfsResult<OperationCounters> {
        self.read_to_canonical(root, &CanonicalPath::new(path)?, output)
    }

    pub fn replace_range_observed<R: Read>(
        &self,
        expected: &RefState,
        path: &str,
        start: u64,
        delete_len: u64,
        input: R,
    ) -> VfsResult<(RefState, OperationCounters)> {
        self.replace_range_canonical(
            expected,
            &CanonicalPath::new(path)?,
            start,
            delete_len,
            input,
        )
    }

    pub fn replace_range_for_refresh_observed<R: Read>(
        &self,
        expected: &RefState,
        path: &str,
        start: u64,
        delete_len: u64,
        input: R,
    ) -> VfsResult<(super::super::AcceptedSplice, OperationCounters)> {
        self.replace_range_for_refresh_canonical(
            expected,
            &CanonicalPath::new(path)?,
            start,
            delete_len,
            input,
        )
    }

    pub fn replace_file_observed<R: Read>(
        &self,
        expected: &RefState,
        path: &str,
        input: R,
    ) -> VfsResult<(RefState, OperationCounters)> {
        self.replace_file_canonical(expected, &CanonicalPath::new(path)?, input)
    }
}
