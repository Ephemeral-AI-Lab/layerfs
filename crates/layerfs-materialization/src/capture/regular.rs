use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn capture_regular(
    workspace: &dyn ProjectionWorkspace,
    parent: &dyn DirectoryHandle,
    digest_cache: &SemanticDigestCache,
    entry: &NativeEntry,
    path: &[u8],
    publication: &mut impl CaptureStore,
    table: &mut Option<GeneratedInodeTable>,
    hard_links: &DiskTable,
    existing: &DiskNamespace<'_>,
    existing_links: Option<&DiskNamespace<'_>>,
    prior_links: Option<&DiskNamespace<'_>>,
    prior_table: Option<InodeTableRoot>,
    counters: &mut OperationCounters,
) -> VfsResult<InodeId> {
    let key = entry.hard_link_key.clone().ok_or(VfsError::InvalidState)?;
    if let Some(bytes) = hard_links.get(&key)? {
        let mut link = HardLink::decode(&bytes)?;
        link.observed += 1;
        link.record.namespace_ref_count = link
            .record
            .namespace_ref_count
            .checked_add(1)
            .ok_or(VfsError::InvalidState)?;
        put_record(publication, table, link.inode, link.record, counters)?;
        hard_links.put(&key, &link.encode())?;
        return Ok(link.inode);
    }
    let retained_inode = existing_links
        .map(|links| links.get(&key))
        .transpose()?
        .flatten()
        .map(|bytes| InodeId::from_slice(&bytes))
        .transpose()?;
    let inode = match retained_inode {
        Some(inode) => inode,
        None => publication.allocate_inode_id()?,
    };
    let grouped_prior_inode = prior_links
        .map(|links| links.get(&key))
        .transpose()?
        .flatten()
        .filter(|bytes| bytes.len() == 32)
        .map(|bytes| InodeId::from_slice(&bytes))
        .transpose()?;
    let prior_inode = retained_inode.or(grouped_prior_inode).or(existing_inode(
        existing,
        path,
        InodeKind::RegularFile,
    )?);
    let prior = prior_inode
        .zip(prior_table)
        .map(|(inode, table)| existing_record(&*publication, table, inode, counters))
        .transpose()?;
    let mut file = workspace.open_regular_read_at(parent, &entry.name, Some(&entry.token))?;
    let mut current_digest = layerfs_core::identity::ContentDigestWriter::new();
    let current_bytes = std::io::copy(&mut file, &mut current_digest)?;
    counters.current_digest_bytes = counters
        .current_digest_bytes
        .checked_add(current_bytes)
        .ok_or(VfsError::InvalidState)?;
    counters.native.bytes_read = counters
        .native
        .bytes_read
        .checked_add(current_bytes)
        .ok_or(VfsError::InvalidState)?;
    let current_digest = current_digest.finish();
    let prior_digest = prior
        .map(|record| {
            let root = FileStateRoot(record.content_root);
            if let Some(digest) = digest_cache.get(root)? {
                return Ok(digest);
            }
            let mut digest = layerfs_core::identity::ContentDigestWriter::new();
            let rope = read_all(&*publication, root, &mut digest)?;
            counters.uncached_prior_digest_bytes = counters
                .uncached_prior_digest_bytes
                .checked_add(rope.payload_bytes_read)
                .ok_or(VfsError::InvalidState)?;
            counters.add_rope(rope)?;
            let digest = digest.finish();
            digest_cache.insert(root, digest)?;
            Ok::<_, VfsError>(digest)
        })
        .transpose()?;
    let content = if prior_digest == Some(current_digest) {
        counters.unchanged_file_roots_reused = counters
            .unchanged_file_roots_reused
            .checked_add(1)
            .ok_or(VfsError::InvalidState)?;
        FileStateRoot(prior.ok_or(VfsError::InvalidState)?.content_root)
    } else {
        file.seek(SeekFrom::Start(0))?;
        let (content, rope) = build(publication, &mut file)?;
        counters.changed_current_cdc_bytes = counters
            .changed_current_cdc_bytes
            .checked_add(rope.cdc_bytes_scanned)
            .ok_or(VfsError::InvalidState)?;
        counters.native.bytes_read = counters
            .native
            .bytes_read
            .checked_add(rope.cdc_bytes_scanned)
            .ok_or(VfsError::InvalidState)?;
        counters.add_rope(rope)?;
        content
    };
    let metadata = put_metadata_observed(
        publication,
        InodeKind::RegularFile,
        &workspace.read_metadata_at(parent, &entry.name, Some(&entry.token))?,
        counters,
    )?;
    let record = prior
        .filter(|record| {
            record.kind == InodeKind::RegularFile
                && record.namespace_ref_count == 1
                && record.content_root == content.0
                && record.metadata_root == metadata
        })
        .unwrap_or(InodeRecordV1 {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 1,
            content_root: content.0,
            metadata_root: metadata,
        });
    put_record(publication, table, inode, record, counters)?;
    hard_links.put(
        &key,
        &HardLink {
            inode,
            record,
            expected: entry.link_count,
            observed: 1,
        }
        .encode(),
    )?;
    Ok(inode)
}
