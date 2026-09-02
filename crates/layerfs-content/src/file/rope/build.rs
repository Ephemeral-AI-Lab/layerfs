use super::edit::emit_leaf;
use super::state::{
    DeferredNodes, FileStateRoot, ObjectStore, Pending, ReplacementScan, RopeCounters, Summary,
};
use crate::error::{CoreError, CoreResult};
use crate::file::cdc::FastCdc;
use crate::file::extent::{
    ChildDescriptorV3, ExtentNodeV3, ExtentSliceV3, FileStateV3, MAX_ENTRIES,
};
use crate::file::extent_codec::{encode_chunk_object, encode_file_state, encode_node, profile_id};
use crate::object::ObjectId;
use std::io::Read;

const STREAM_FLUSH_AT: usize = MAX_ENTRIES + 64;

pub fn build<S: ObjectStore, R: Read>(
    store: &mut S,
    source: R,
) -> CoreResult<(FileStateRoot, RopeCounters)> {
    let (root, mut counters) = build_mapping(store, source)?;
    let root = match root {
        Some(root) => root,
        None => emit_leaf(store, Vec::new(), &mut counters)?,
    };
    let state = FileStateV3 {
        logical_len: root.bytes,
        extent_count: root.extents,
        tree_level: root.level,
        profile_id: profile_id(),
        mapping_root: root.id,
    };
    let canonical = encode_file_state(state)?;
    let id = store.put(&canonical)?;
    Ok((FileStateRoot(id), counters))
}

/// Builds a known byte slice without starting the streaming CDC scanner when
/// the frozen profile guarantees that it is exactly one chunk.
pub fn build_bytes<S: ObjectStore>(
    store: &mut S,
    bytes: &[u8],
) -> CoreResult<(FileStateRoot, RopeCounters)> {
    if bytes.is_empty() || bytes.len() >= crate::file::cdc::MINIMUM_CHUNK_BYTES {
        return build(store, bytes);
    }

    let canonical = encode_chunk_object(bytes)?;
    let payload = store.put(&canonical)?;
    let logical_len = u64::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
    let logical_length = u32::try_from(bytes.len()).map_err(|_| CoreError::LengthOverflow)?;
    let node = ExtentNodeV3::Leaf {
        subtree_logical_bytes: logical_len,
        extents: vec![ExtentSliceV3::new(payload, 0, logical_length)?],
    };
    let mapping_root = store.put(&encode_node(&node)?)?;
    let state = FileStateV3 {
        logical_len,
        extent_count: 1,
        tree_level: 0,
        profile_id: profile_id(),
        mapping_root,
    };
    let root = store.put(&encode_file_state(state)?)?;
    Ok((
        FileStateRoot(root),
        RopeCounters {
            payload_bytes_written: logical_len,
            cdc_bytes_scanned: logical_len,
            chunks_created: 1,
            nodes_created: 1,
            ..RopeCounters::default()
        },
    ))
}

fn build_mapping<S: ObjectStore, R: Read>(
    store: &mut S,
    source: R,
) -> CoreResult<(Option<Summary>, RopeCounters)> {
    let (mut levels, mut counters, bytes_scanned) = scan_mapping(store, source)?;
    if bytes_scanned == 0 {
        Ok((None, counters))
    } else {
        let root = finish(store, &mut levels, &mut counters)?;
        Ok((Some(root), counters))
    }
}

fn scan_mapping<S: ObjectStore, R: Read>(
    store: &mut S,
    source: R,
) -> CoreResult<(Vec<Pending>, RopeCounters, u64)> {
    let mut levels = vec![Pending::Extents(Vec::with_capacity(STREAM_FLUSH_AT + 1))];
    let mut counters = RopeCounters::default();
    let cdc = FastCdc::new().scan(source, |chunk| {
        let canonical = encode_chunk_object(chunk)?;
        let payload = store.put(&canonical)?;
        counters.payload_bytes_written = add(counters.payload_bytes_written, chunk.len() as u64)?;
        counters.chunks_created = add(counters.chunks_created, 1)?;
        match &mut levels[0] {
            Pending::Extents(extents) => {
                extents.push(ExtentSliceV3::new(payload, 0, chunk.len() as u32)?)
            }
            Pending::Children(_) => unreachable!(),
        }
        flush_streaming(store, &mut levels, 0, &mut counters)
    })?;
    counters.cdc_bytes_scanned = cdc.bytes_scanned;
    Ok((levels, counters, cdc.bytes_scanned))
}

pub(super) fn scan_replacement_mapping_with<S, R, FP, FN>(
    store: &mut S,
    source: R,
    mut put_payload: FP,
    mut put_sealed_node: FN,
) -> CoreResult<ReplacementScan>
where
    S: ObjectStore,
    R: Read,
    FP: FnMut(&mut S, &[u8]) -> CoreResult<ObjectId>,
    FN: FnMut(&mut S, &[u8]) -> CoreResult<ObjectId>,
{
    let mut deferred = DeferredNodes::new(store);
    let mut levels = vec![Pending::Extents(Vec::with_capacity(STREAM_FLUSH_AT + 1))];
    let mut counters = RopeCounters::default();
    let mut flushed = 0_u64;
    let cdc = FastCdc::new().scan(source, |chunk| {
        let canonical = encode_chunk_object(chunk)?;
        let payload = put_payload(deferred.store, &canonical)?;
        counters.payload_bytes_written = add(counters.payload_bytes_written, chunk.len() as u64)?;
        counters.chunks_created = add(counters.chunks_created, 1)?;
        match &mut levels[0] {
            Pending::Extents(extents) => {
                extents.push(ExtentSliceV3::new(payload, 0, chunk.len() as u32)?)
            }
            Pending::Children(_) => unreachable!(),
        }
        flush_streaming(&mut deferred, &mut levels, 0, &mut counters)?;
        flushed = add(
            flushed,
            deferred.flush_sealed_with(&levels, &mut put_sealed_node)?,
        )?;
        Ok(())
    })?;
    counters.cdc_bytes_scanned = cdc.bytes_scanned;
    Ok(ReplacementScan {
        levels,
        counters,
        bytes_scanned: cdc.bytes_scanned,
        pending: deferred.into_nodes(),
        persisted_nodes: flushed,
    })
}

fn flush_streaming<S: ObjectStore>(
    store: &mut S,
    levels: &mut Vec<Pending>,
    level: usize,
    counters: &mut RopeCounters,
) -> CoreResult<()> {
    let len = match &levels[level] {
        Pending::Extents(v) => v.len(),
        Pending::Children(v) => v.len(),
    };
    if len <= STREAM_FLUSH_AT {
        return Ok(());
    }
    let summary = emit_prefix(
        store,
        &mut levels[level],
        MAX_ENTRIES,
        level as u8,
        counters,
    )?;
    push_summary(levels, level + 1, summary)?;
    flush_streaming(store, levels, level + 1, counters)
}

pub(super) fn finish<S: ObjectStore>(
    store: &mut S,
    levels: &mut Vec<Pending>,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    let mut level = 0;
    loop {
        let higher_nonempty = levels
            .iter()
            .skip(level + 1)
            .any(|pending| pending_len(pending) != 0);
        let len = pending_len(&levels[level]);
        if !higher_nonempty && len <= MAX_ENTRIES {
            if level > 0 && len == 1 {
                if let Pending::Children(children) = &levels[level] {
                    return Ok(children[0]);
                }
            }
            return emit_prefix(store, &mut levels[level], len, level as u8, counters);
        }
        if len != 0 {
            let first = if len > MAX_ENTRIES { len / 2 } else { len };
            let summary = emit_prefix(store, &mut levels[level], first, level as u8, counters)?;
            push_summary(levels, level + 1, summary)?;
            continue;
        }
        level += 1;
        if level >= levels.len() {
            return Err(CoreError::InvalidRecord("empty rope builder"));
        }
    }
}

fn emit_prefix<S: ObjectStore>(
    store: &mut S,
    pending: &mut Pending,
    count: usize,
    level: u8,
    counters: &mut RopeCounters,
) -> CoreResult<Summary> {
    let node = match pending {
        Pending::Extents(entries) => {
            let entries: Vec<_> = entries.drain(..count).collect();
            let bytes = entries.iter().try_fold(0_u64, |sum, entry| {
                add(sum, u64::from(entry.logical_length))
            })?;
            ExtentNodeV3::Leaf {
                subtree_logical_bytes: bytes,
                extents: entries,
            }
        }
        Pending::Children(entries) => {
            let entries: Vec<_> = entries.drain(..count).collect();
            let mut bytes = 0_u64;
            let mut extents = 0_u64;
            let children = entries
                .iter()
                .map(|entry| {
                    bytes = add(bytes, entry.bytes)?;
                    extents = add(extents, entry.extents)?;
                    Ok(ChildDescriptorV3 {
                        cumulative_logical_end: bytes,
                        cumulative_extent_end: extents,
                        child_object_id: entry.id,
                    })
                })
                .collect::<CoreResult<Vec<_>>>()?;
            ExtentNodeV3::Branch {
                level,
                subtree_logical_bytes: bytes,
                subtree_extent_count: extents,
                children,
            }
        }
    };
    let canonical = encode_node(&node)?;
    let id = store.put(&canonical)?;
    counters.nodes_created = add(counters.nodes_created, 1)?;
    Ok(Summary {
        id,
        bytes: node.logical_len(),
        extents: node.extent_count(),
        level,
    })
}

fn push_summary(levels: &mut Vec<Pending>, level: usize, summary: Summary) -> CoreResult<()> {
    if summary.level as usize + 1 != level {
        return Err(CoreError::InvalidRecord("rope builder level"));
    }
    while levels.len() <= level {
        levels.push(Pending::Children(Vec::with_capacity(STREAM_FLUSH_AT + 1)));
    }
    match &mut levels[level] {
        Pending::Children(children) => children.push(summary),
        Pending::Extents(_) => return Err(CoreError::InvalidRecord("rope builder role")),
    }
    Ok(())
}

fn pending_len(pending: &Pending) -> usize {
    match pending {
        Pending::Extents(v) => v.len(),
        Pending::Children(v) => v.len(),
    }
}

pub(super) fn add(left: u64, right: u64) -> CoreResult<u64> {
    left.checked_add(right).ok_or(CoreError::LengthOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::rope::read_all;
    use std::collections::BTreeMap;

    #[derive(Default)]
    struct MemoryStore(BTreeMap<ObjectId, Vec<u8>>);

    impl ObjectStore for MemoryStore {
        fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
            self.0.get(&id).cloned().ok_or(CoreError::MissingObject)
        }

        fn put(&mut self, canonical: &[u8]) -> CoreResult<ObjectId> {
            let id = ObjectId::for_bytes(canonical);
            if self
                .0
                .insert(id, canonical.to_vec())
                .is_some_and(|prior| prior != canonical)
            {
                return Err(CoreError::IdentityMismatch);
            }
            Ok(id)
        }
    }

    #[test]
    fn known_single_chunk_bytes_match_streaming_builder_exactly() {
        for length in [1, 2_500, 8_191, 8_192] {
            let bytes = (0..length)
                .map(|index| (index as u8).wrapping_mul(37).wrapping_add(11))
                .collect::<Vec<_>>();
            let mut streaming = MemoryStore::default();
            let mut known = MemoryStore::default();
            let (streaming_root, streaming_counters) =
                build(&mut streaming, bytes.as_slice()).unwrap();
            let (known_root, known_counters) = build_bytes(&mut known, &bytes).unwrap();

            assert_eq!(known_root, streaming_root, "length={length}");
            assert_eq!(known.0, streaming.0, "length={length}");
            assert_eq!(known_counters, streaming_counters, "length={length}");
            let mut actual = Vec::new();
            read_all(&known, known_root, &mut actual).unwrap();
            assert_eq!(actual, bytes, "length={length}");
        }
    }
}
