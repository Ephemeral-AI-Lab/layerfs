use crate::capture::put_metadata_observed;
use crate::driver::NativeMetadata;
use crate::workspace::{LayerVfs, VfsError, VfsResult};
use crate::OperationCounters;
use layerfs_core::content::rope::{
    build, read_plan, read_range_with_plan, replace, FileStateRoot, ObjectRead, ReadPlan,
};
use layerfs_core::inode::{
    inode_table_lookup, inode_table_upsert, InodeId, InodeKind, InodeRecordV1, InodeTableCounters,
    InodeTableRoot,
};
use layerfs_core::namespace::{
    directory_insert, directory_lookup, DirectoryStateRoot, NamespaceCounters, NamespaceRootV1,
};
use layerfs_core::namespace_codec::{
    decode_inode_record, decode_namespace_root, encode_inode_record, encode_namespace_root,
};
use layerfs_core::{CanonicalName, CanonicalPath, ObjectId};
use layerfs_engine::publication::Publication;
use layerfs_engine::refs::RefState;
use std::io::{Read, Write};
use std::ops::Range;
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(crate) struct ResolvedReadCache(Mutex<Option<ResolvedRead>>);

struct ResolvedRead {
    root: ObjectId,
    path: CanonicalPath,
    plan: Arc<ReadPlan>,
}

impl ResolvedReadCache {
    fn get(&self, root: ObjectId, path: &CanonicalPath) -> VfsResult<Option<Arc<ReadPlan>>> {
        let cache = self.0.lock().map_err(|_| VfsError::InvalidState)?;
        Ok(cache
            .as_ref()
            .filter(|entry| entry.root == root && entry.path == *path)
            .map(|entry| entry.plan.clone()))
    }

    fn put(&self, root: ObjectId, path: &CanonicalPath, plan: Arc<ReadPlan>) -> VfsResult<()> {
        *self.0.lock().map_err(|_| VfsError::InvalidState)? = Some(ResolvedRead {
            root,
            path: path.clone(),
            plan,
        });
        Ok(())
    }
}

pub(crate) fn namespace<S: ObjectRead>(store: &S, root: ObjectId) -> VfsResult<NamespaceRootV1> {
    Ok(store.with_authenticated_canonical(root, decode_namespace_root)?)
}

pub(crate) fn resolve<S: ObjectRead>(
    store: &S,
    namespace: NamespaceRootV1,
    path: &CanonicalPath,
    counters: &mut OperationCounters,
) -> VfsResult<(InodeId, InodeRecordV1)> {
    let table = InodeTableRoot(namespace.inode_table_root);
    let mut inode = namespace.root_directory_inode;
    let mut record = load_record(store, table, inode, counters)?;
    for component in path.components() {
        if record.kind != InodeKind::Directory {
            return Err(VfsError::InvalidState);
        }
        let mut visits = NamespaceCounters::default();
        inode = directory_lookup(
            store,
            DirectoryStateRoot(record.content_root),
            &CanonicalName::from_bytes(component)?,
            &mut visits,
        )?
        .ok_or(VfsError::InvalidState)?;
        counters.add_namespace(visits)?;
        record = load_record(store, table, inode, counters)?;
    }
    Ok((inode, record))
}

fn resolve_parent<S: ObjectRead>(
    store: &S,
    namespace: NamespaceRootV1,
    path: &CanonicalPath,
    counters: &mut OperationCounters,
) -> VfsResult<(InodeId, InodeRecordV1, CanonicalName)> {
    let bytes = path.as_bytes();
    let (name, parent) = bytes
        .iter()
        .rposition(|byte| *byte == b'/')
        .map_or((bytes, &[][..]), |separator| {
            (&bytes[separator + 1..], &bytes[..separator])
        });
    if name.is_empty() {
        return Err(VfsError::InvalidState);
    }
    let (inode, record) = resolve(
        store,
        namespace,
        &CanonicalPath::from_bytes(parent)?,
        counters,
    )?;
    if record.kind != InodeKind::Directory {
        return Err(VfsError::InvalidState);
    }
    Ok((inode, record, CanonicalName::from_bytes(name)?))
}

fn load_record<S: ObjectRead>(
    store: &S,
    table: InodeTableRoot,
    inode: InodeId,
    counters: &mut OperationCounters,
) -> VfsResult<InodeRecordV1> {
    let mut visits = InodeTableCounters::default();
    let id = inode_table_lookup(store, table, inode, &mut visits)?.ok_or(VfsError::InvalidState)?;
    counters.add_inode_table(visits)?;
    Ok(store.with_authenticated_canonical(id, decode_inode_record)?)
}

impl LayerVfs {
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

    pub fn read_range<W: Write>(
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

    pub fn read_to<W: Write>(
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

    pub fn replace_range<R: Read>(
        &self,
        expected: &RefState,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        input: R,
    ) -> VfsResult<(RefState, OperationCounters)> {
        self.replace_existing(expected, path, |publication, record, counters| {
            let (content, rope) = replace(
                publication,
                FileStateRoot(record.content_root),
                start,
                delete_len,
                input,
            )?;
            counters.add_rope(rope)?;
            Ok(content)
        })
    }

    pub fn replace_range_for_refresh<R: Read>(
        &self,
        expected: &RefState,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        input: R,
    ) -> VfsResult<(crate::AcceptedSplice, OperationCounters)> {
        let (after, counters) = self.replace_range(expected, path, start, delete_len, input)?;
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
            crate::AcceptedSplice {
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

    pub fn replace_file<R: Read>(
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

    fn replace_existing(
        &self,
        expected: &RefState,
        path: &CanonicalPath,
        replace_content: impl FnOnce(
            &mut Publication<'_>,
            InodeRecordV1,
            &mut OperationCounters,
        ) -> VfsResult<FileStateRoot>,
    ) -> VfsResult<(RefState, OperationCounters)> {
        let reservation = self.operation_q.reserve();
        require_main(expected)?;
        let mut counters = OperationCounters::default();
        let mut publication = self.engine.begin_publication(Some(expected), "main")?;
        let namespace = namespace(&publication, expected.root)?;
        let (inode, record) = resolve(&publication, namespace, path, &mut counters)?;
        if record.kind != InodeKind::RegularFile {
            return Err(VfsError::InvalidState);
        }
        let content = replace_content(&mut publication, record, &mut counters)?;
        let namespace = update_record(
            &mut publication,
            namespace,
            inode,
            InodeRecordV1 {
                content_root: content.0,
                ..record
            },
            &mut counters,
        )?;
        let state = publication.publish_namespace(&encode_namespace_root(namespace)?)?;
        reservation.finish(&mut counters);
        Ok((state, counters))
    }
}

fn require_main(expected: &RefState) -> VfsResult<()> {
    if expected.name == "main" {
        Ok(())
    } else {
        Err(VfsError::InvalidState)
    }
}

fn update_record(
    publication: &mut Publication<'_>,
    namespace: NamespaceRootV1,
    inode: InodeId,
    record: InodeRecordV1,
    counters: &mut OperationCounters,
) -> VfsResult<NamespaceRootV1> {
    let record = publication.put_object(&encode_inode_record(record)?)?;
    let (table, visits) = inode_table_upsert(
        publication,
        InodeTableRoot(namespace.inode_table_root),
        inode,
        record,
    )?;
    counters.add_inode_table(visits)?;
    Ok(NamespaceRootV1 {
        inode_table_root: table.0,
        ..namespace
    })
}

#[allow(clippy::too_many_arguments)]
fn create_file(
    publication: &mut Publication<'_>,
    namespace: NamespaceRootV1,
    parent_inode: InodeId,
    parent_record: InodeRecordV1,
    name: CanonicalName,
    content: FileStateRoot,
    counters: &mut OperationCounters,
) -> VfsResult<NamespaceRootV1> {
    let inode = publication.allocate_inode_id()?;
    let metadata = put_metadata_observed(
        publication,
        InodeKind::RegularFile,
        &NativeMetadata {
            mode: 0o644,
            mtime_seconds: 0,
            mtime_nanoseconds: 0,
            xattrs: Vec::new(),
            acl: None,
            bsd_flags: 0,
        },
        counters,
    )?;
    let file_record = publication.put_object(&encode_inode_record(InodeRecordV1 {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: content.0,
        metadata_root: metadata,
    })?)?;
    let (directory, visits) = directory_insert(
        publication,
        DirectoryStateRoot(parent_record.content_root),
        name,
        inode,
    )?;
    counters.add_namespace(visits)?;
    let parent_record = publication.put_object(&encode_inode_record(InodeRecordV1 {
        content_root: directory.0,
        ..parent_record
    })?)?;
    let (table, visits) = inode_table_upsert(
        publication,
        InodeTableRoot(namespace.inode_table_root),
        inode,
        file_record,
    )?;
    counters.add_inode_table(visits)?;
    let (table, visits) = inode_table_upsert(publication, table, parent_inode, parent_record)?;
    counters.add_inode_table(visits)?;
    Ok(NamespaceRootV1 {
        inode_table_root: table.0,
        ..namespace
    })
}
