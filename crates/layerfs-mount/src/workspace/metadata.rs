#[derive(Clone)]
struct CheckpointNode {
    canonical: Option<InodeId>,
    record: Option<InodeRecordV1>,
    kind: MountedFileType,
    mode: u32,
    mtime_seconds: i64,
    mtime_nanoseconds: u32,
    namespace_refs: u64,
    dirty_content: bool,
    dirty_metadata: bool,
    content: NodeContent,
    metadata_entries: Option<Vec<MetadataEntryV1>>,
}

fn persist_metadata(
    publication: &mut WorkingCandidateWrite<'_>,
    node: &CheckpointNode,
) -> Result<ObjectId, MountedError> {
    let portable = PortableMetadataV1 {
        permission_mode: node.mode,
        mtime_seconds: node.mtime_seconds,
        mtime_nanoseconds: node.mtime_nanoseconds,
    };
    portable.validate(inode_kind(node.kind))?;
    let mode_key = MetadataKey::new("portable".to_owned(), b"mode".to_vec())?;
    let mtime_key = MetadataKey::new("portable".to_owned(), b"mtime".to_vec())?;
    let mode = metadata_value(
        publication,
        mode_key.clone(),
        &portable.mode_bytes(inode_kind(node.kind))?,
    )?;
    let mtime = metadata_value(publication, mtime_key.clone(), &portable.mtime_bytes()?)?;
    let mut tree = MetadataTreeBuilder::new();
    let mut inserted_mode = false;
    let mut inserted_mtime = false;
    for entry in node.metadata_entries.iter().flatten() {
        if entry.key == mode_key {
            tree.push(publication, mode.clone())?;
            inserted_mode = true;
        } else if entry.key == mtime_key {
            tree.push(publication, mtime.clone())?;
            inserted_mtime = true;
        } else {
            tree.push(publication, entry.clone())?;
        }
    }
    if node.metadata_entries.is_none() {
        tree.push(publication, mode)?;
        tree.push(publication, mtime)?;
    } else if !inserted_mode || !inserted_mtime {
        return Err(MountedError::Corrupt);
    }
    Ok(tree.finish(publication)?)
}

fn metadata_value(
    publication: &mut WorkingCandidateWrite<'_>,
    key: MetadataKey,
    value: &[u8],
) -> Result<MetadataEntryV1, MountedError> {
    let (root, _) = build(publication, Cursor::new(value))?;
    Ok(MetadataEntryV1 {
        key,
        value_file_root: root.0,
    })
}

fn read_portable_metadata(
    engine: &impl ObjectRead,
    record: InodeRecordV1,
) -> Result<PortableMetadataV1, MountedError> {
    let mode = metadata_lookup(
        engine,
        record.metadata_root,
        &MetadataKey::new("portable".to_owned(), b"mode".to_vec())?,
    )?
    .ok_or(MountedError::Corrupt)?;
    let mtime = metadata_lookup(
        engine,
        record.metadata_root,
        &MetadataKey::new("portable".to_owned(), b"mtime".to_vec())?,
    )?
    .ok_or(MountedError::Corrupt)?;
    let mut mode_bytes = Vec::new();
    read_all_bounded(
        engine,
        FileStateRoot(mode.value_file_root),
        4,
        &mut mode_bytes,
    )?;
    let mut mtime_bytes = Vec::new();
    read_all_bounded(
        engine,
        FileStateRoot(mtime.value_file_root),
        12,
        &mut mtime_bytes,
    )?;
    if mode_bytes.len() != 4 || mtime_bytes.len() != 12 {
        return Err(MountedError::Corrupt);
    }
    let metadata = PortableMetadataV1 {
        permission_mode: u32::from_be_bytes(mode_bytes.try_into().unwrap()),
        mtime_seconds: i64::from_be_bytes(mtime_bytes[..8].try_into().unwrap()),
        mtime_nanoseconds: u32::from_be_bytes(mtime_bytes[8..].try_into().unwrap()),
    };
    metadata.validate(record.kind)?;
    Ok(metadata)
}

fn accepted_logical_bytes(
    engine: &impl ObjectRead,
    inode_table: ObjectId,
) -> Result<u64, CoreError> {
    let mut total = 0_u64;
    visit_inode_table_entries(
        engine,
        InodeTableRoot(inode_table),
        &mut InodeTableCounters::default(),
        |entries| {
            for (_, record_id) in entries {
                let record =
                    engine.with_authenticated_canonical(*record_id, decode_inode_record)?;
                if record.kind == InodeKind::RegularFile {
                    let state = rope_state(
                        engine,
                        FileStateRoot(record.content_root),
                        &mut RopeCounters::default(),
                    )?;
                    total = total
                        .checked_add(state.logical_len)
                        .ok_or(CoreError::LengthOverflow)?;
                }
            }
            Ok(())
        },
    )?;
    Ok(total)
}

fn install_dirty_range(
    ranges: &mut BTreeMap<u64, DirtyRange>,
    start: u64,
    end: u64,
    spool_offset: u64,
) -> Result<(u64, u64), MountedError> {
    let mut removed = 0_u64;
    let mut preserved = 0_u64;
    if let Some((&key, range)) = ranges.range(..start).next_back() {
        if range.end > start {
            let range = ranges.remove(&key).ok_or(MountedError::Indeterminate)?;
            removed += range.end - key;
            ranges.insert(
                key,
                DirtyRange {
                    end: start,
                    spool_offset: range.spool_offset,
                },
            );
            preserved += start - key;
            if range.end > end {
                ranges.insert(
                    end,
                    DirtyRange {
                        end: range.end,
                        spool_offset: range.spool_offset + (end - key),
                    },
                );
                preserved += range.end - end;
            }
        }
    }
    while let Some((&key, range)) = ranges.range(start..end).next() {
        let range = range.clone();
        ranges.remove(&key);
        removed += range.end - key;
        if range.end > end {
            ranges.insert(
                end,
                DirtyRange {
                    end: range.end,
                    spool_offset: range.spool_offset + (end - key),
                },
            );
            preserved += range.end - end;
            break;
        }
    }
    ranges.insert(start, DirtyRange { end, spool_offset });
    merge_adjacent_ranges(ranges, start)?;
    Ok((removed, preserved))
}

fn merge_adjacent_ranges(
    ranges: &mut BTreeMap<u64, DirtyRange>,
    mut key: u64,
) -> Result<(), MountedError> {
    if let Some((&previous, range)) = ranges.range(..key).next_back() {
        let current = ranges.get(&key).ok_or(MountedError::Indeterminate)?;
        if range.end == key && range.spool_offset + (range.end - previous) == current.spool_offset {
            let end = current.end;
            ranges.remove(&key);
            ranges
                .get_mut(&previous)
                .ok_or(MountedError::Indeterminate)?
                .end = end;
            key = previous;
        }
    }
    let current = ranges.get(&key).ok_or(MountedError::Indeterminate)?.clone();
    if let Some((&next, range)) = ranges.range((Excluded(key), Unbounded)).next() {
        if current.end == next && current.spool_offset + (current.end - key) == range.spool_offset {
            let end = range.end;
            ranges.remove(&next);
            ranges.get_mut(&key).ok_or(MountedError::Indeterminate)?.end = end;
        }
    }
    Ok(())
}

fn truncate_dirty_ranges(ranges: &mut BTreeMap<u64, DirtyRange>, length: u64) -> u64 {
    let mut removed = 0;
    if let Some((&start, range)) = ranges.range(..length).next_back() {
        if range.end > length {
            removed += range.end - length;
            if let Some(range) = ranges.get_mut(&start) {
                range.end = length;
            }
        }
    }
    while let Some((&start, range)) = ranges.range(length..).next() {
        removed += range.end - start;
        ranges.remove(&start);
    }
    removed
}

fn inode_kind(kind: MountedFileType) -> InodeKind {
    match kind {
        MountedFileType::RegularFile => InodeKind::RegularFile,
        MountedFileType::Directory => InodeKind::Directory,
        MountedFileType::Symlink => InodeKind::Symlink,
    }
}

fn now_timestamp() -> Result<(i64, u32), MountedError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MountedError::Indeterminate)?;
    Ok((
        i64::try_from(now.as_secs()).map_err(|_| MountedError::Indeterminate)?,
        now.subsec_nanos(),
    ))
}

fn startup(step: &'static str, error: impl std::fmt::Debug) -> MountedError {
    MountedError::Startup(step, format!("{error:?}"))
}

fn merge_rope(target: &mut RopeCounters, source: RopeCounters) -> Result<(), MountedError> {
    target.payload_bytes_read = target
        .payload_bytes_read
        .checked_add(source.payload_bytes_read)
        .ok_or(MountedError::ResourceExhausted)?;
    target.nodes_read = target
        .nodes_read
        .checked_add(source.nodes_read)
        .ok_or(MountedError::ResourceExhausted)?;
    Ok(())
}

struct ZeroReader(u64);

impl Read for ZeroReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = output.len().min(self.0 as usize);
        output[..count].fill(0);
        self.0 -= count as u64;
        Ok(count)
    }
}
