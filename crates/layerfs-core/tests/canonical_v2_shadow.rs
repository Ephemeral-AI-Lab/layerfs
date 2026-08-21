//! Nonpersistent canonical-v2 authority/format shadow.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::ops::Range;

use layerfs_core::content::persistence::{self as v1, FileReference};
use layerfs_core::{
    chunk_id, decode_bytes_object, encode_bytes_object, encode_object, CoreError, CoreResult,
    Object, ObjectId,
};

const K: usize = 64;
const F: usize = 64;
const MAX_RAW_BYTES: u32 = 32 * 1024;
const V2_VERSION: u16 = 2;
const V2_REF_BYTES: usize = 36;
const ORDERED_COMMITMENT_CONTEXT: &str = "layerfs/canonical-v2/ordered-occurrence/v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct V2Reference {
    raw_length: u32,
    object_id: ObjectId,
}

impl From<FileReference> for V2Reference {
    fn from(reference: FileReference) -> Self {
        Self {
            raw_length: reference.raw_length,
            object_id: reference.object_id,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Child {
    cumulative_end: u64,
    object_id: ObjectId,
}

#[derive(Default)]
struct ShadowStore {
    objects: BTreeMap<ObjectId, Vec<u8>>,
}

impl ShadowStore {
    fn put(&mut self, canonical: Vec<u8>) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(&canonical);
        if let Some(existing) = self.objects.get(&id) {
            if existing != &canonical {
                return Err(CoreError::IdentityMismatch);
            }
        } else {
            self.objects.insert(id, canonical);
        }
        Ok(id)
    }

    fn authenticated(&self, id: ObjectId) -> CoreResult<&[u8]> {
        let bytes = self.objects.get(&id).ok_or(CoreError::MissingObject)?;
        if ObjectId::for_bytes(bytes) != id {
            return Err(CoreError::IdentityMismatch);
        }
        Ok(bytes)
    }
}

fn v2_profile_id() -> ObjectId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"layerfs/mapping-profile/v2\0");
    for value in [K as u32, F as u32, 262_144_u32, 8_388_608_u32] {
        hasher.update(&value.to_be_bytes());
    }
    ObjectId::from_bytes(hasher.finalize().as_bytes()).expect("BLAKE3 is 32 bytes")
}

fn ordered_commitment(references: &[V2Reference]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key(ORDERED_COMMITMENT_CONTEXT);
    for reference in references {
        hasher.update(&reference.raw_length.to_be_bytes());
        hasher.update(reference.object_id.as_bytes());
    }
    *hasher.finalize().as_bytes()
}

fn v2_mapping(tag: u8, body: &[u8]) -> CoreResult<Vec<u8>> {
    let mut inner = Vec::with_capacity(11 + body.len());
    inner.extend_from_slice(&v1::MAPPING_MAGIC);
    inner.extend_from_slice(&V2_VERSION.to_be_bytes());
    inner.push(tag);
    inner.extend_from_slice(body);
    encode_bytes_object(&inner)
}

fn decode_v2_mapping(canonical: &[u8], expected_tag: u8) -> CoreResult<&[u8]> {
    let inner = decode_bytes_object(canonical)?;
    if inner.len() < 11 {
        return Err(CoreError::UnexpectedEof);
    }
    if inner[..8] != v1::MAPPING_MAGIC {
        return Err(CoreError::InvalidMappingTag { tag: 0 });
    }
    let version = u16::from_be_bytes([inner[8], inner[9]]);
    if version != V2_VERSION {
        return Err(CoreError::UnsupportedMappingVersion { version });
    }
    if inner[10] != expected_tag {
        return Err(match inner[10] {
            v1::FILE_ROOT_TAG | v1::FILE_LEAF_TAG | v1::FILE_BRANCH_TAG => {
                CoreError::WrongLogicalRole
            }
            tag => CoreError::InvalidMappingTag { tag },
        });
    }
    Ok(&inner[11..])
}

fn encode_reference(reference: V2Reference, output: &mut Vec<u8>) {
    output.extend_from_slice(&reference.raw_length.to_be_bytes());
    output.extend_from_slice(reference.object_id.as_bytes());
}

fn decode_reference(bytes: &[u8]) -> CoreResult<V2Reference> {
    if bytes.len() != V2_REF_BYTES {
        return Err(CoreError::UnexpectedEof);
    }
    let reference = V2Reference {
        raw_length: u32::from_be_bytes(
            bytes[..4]
                .try_into()
                .map_err(|_| CoreError::UnexpectedEof)?,
        ),
        object_id: ObjectId::from_bytes(&bytes[4..])?,
    };
    if reference.raw_length > MAX_RAW_BYTES {
        return Err(CoreError::ObjectLimitExceeded);
    }
    Ok(reference)
}

fn encode_leaf(references: &[V2Reference]) -> CoreResult<Vec<u8>> {
    if references.is_empty() || references.len() > K {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    if references
        .iter()
        .any(|reference| reference.raw_length > MAX_RAW_BYTES)
    {
        return Err(CoreError::ObjectLimitExceeded);
    }
    let mut body = Vec::with_capacity(4 + references.len() * V2_REF_BYTES);
    body.extend_from_slice(
        &u32::try_from(references.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    for reference in references {
        encode_reference(*reference, &mut body);
    }
    v2_mapping(v1::FILE_LEAF_TAG, &body)
}

fn parse_leaf(canonical: &[u8], final_leaf: bool) -> CoreResult<Vec<V2Reference>> {
    let body = decode_v2_mapping(canonical, v1::FILE_LEAF_TAG)?;
    if body.len() < 4 {
        return Err(CoreError::UnexpectedEof);
    }
    let count = usize::try_from(u32::from_be_bytes(
        body[..4].try_into().map_err(|_| CoreError::UnexpectedEof)?,
    ))
    .map_err(|_| CoreError::LengthOverflow)?;
    if count == 0 || count > K || (!final_leaf && count != K) {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let expected = 4usize
        .checked_add(
            count
                .checked_mul(V2_REF_BYTES)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .ok_or(CoreError::LengthOverflow)?;
    if expected != body.len() {
        return Err(if expected > body.len() {
            CoreError::UnexpectedEof
        } else {
            CoreError::TrailingBytes
        });
    }
    body[4..]
        .chunks_exact(V2_REF_BYTES)
        .map(decode_reference)
        .collect()
}

fn encode_children(tag: u8, level: Option<u8>, children: &[Child]) -> CoreResult<Vec<u8>> {
    if children.is_empty() || children.len() > F {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let mut body = Vec::with_capacity(level.map_or(4, |_| 5) + children.len() * 40);
    if let Some(level) = level {
        if level == 0 {
            return Err(CoreError::NonCanonicalPagePartition);
        }
        body.push(level);
    }
    body.extend_from_slice(
        &u32::try_from(children.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    for child in children {
        body.extend_from_slice(&child.cumulative_end.to_be_bytes());
        body.extend_from_slice(child.object_id.as_bytes());
    }
    v2_mapping(tag, &body)
}

fn parse_children(canonical: &[u8], level: bool, final_node: bool) -> CoreResult<(u8, Vec<Child>)> {
    let tag = if level {
        v1::FILE_BRANCH_TAG
    } else {
        v1::FILE_ROOT_TAG
    };
    let body = decode_v2_mapping(canonical, tag)?;
    let header = if level { 5 } else { 4 };
    if body.len() < header {
        return Err(CoreError::UnexpectedEof);
    }
    let encoded_level = if level { body[0] } else { 0 };
    if level && encoded_level == 0 {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let offset = usize::from(level);
    let count = usize::try_from(u32::from_be_bytes(
        body[offset..offset + 4]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    ))
    .map_err(|_| CoreError::LengthOverflow)?;
    if count == 0 || count > F || (!final_node && count != F) {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let expected = header
        .checked_add(count.checked_mul(40).ok_or(CoreError::LengthOverflow)?)
        .ok_or(CoreError::LengthOverflow)?;
    if expected != body.len() {
        return Err(if expected > body.len() {
            CoreError::UnexpectedEof
        } else {
            CoreError::TrailingBytes
        });
    }
    let mut previous = 0;
    let children = body[header..]
        .chunks_exact(40)
        .map(|bytes| {
            let cumulative_end = u64::from_be_bytes(
                bytes[..8]
                    .try_into()
                    .map_err(|_| CoreError::UnexpectedEof)?,
            );
            if cumulative_end < previous {
                return Err(CoreError::NonCanonicalOrdering);
            }
            previous = cumulative_end;
            Ok(Child {
                cumulative_end,
                object_id: ObjectId::from_bytes(&bytes[8..])?,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok((encoded_level, children))
}

fn encode_root(total: u64, count: u64, level: u8, children: &[Child]) -> CoreResult<Vec<u8>> {
    if count == 0 {
        if total != 0 || level != 0 || !children.is_empty() {
            return Err(CoreError::NonCanonicalPagePartition);
        }
    } else if children.is_empty() || children.len() > F {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let mut body = Vec::with_capacity(25 + children.len() * 40);
    body.extend_from_slice(&0_u32.to_be_bytes());
    body.extend_from_slice(&total.to_be_bytes());
    body.extend_from_slice(&count.to_be_bytes());
    body.push(level);
    body.extend_from_slice(
        &u32::try_from(children.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    for child in children {
        body.extend_from_slice(&child.cumulative_end.to_be_bytes());
        body.extend_from_slice(child.object_id.as_bytes());
    }
    v2_mapping(v1::FILE_ROOT_TAG, &body)
}

fn parse_root(canonical: &[u8]) -> CoreResult<(u64, u64, u8, Vec<Child>)> {
    let body = decode_v2_mapping(canonical, v1::FILE_ROOT_TAG)?;
    if body.len() < 25 {
        return Err(CoreError::UnexpectedEof);
    }
    let total = u64::from_be_bytes(
        body[4..12]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    );
    let count = u64::from_be_bytes(
        body[12..20]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    );
    let level = body[20];
    let child_count = usize::try_from(u32::from_be_bytes(
        body[21..25]
            .try_into()
            .map_err(|_| CoreError::UnexpectedEof)?,
    ))
    .map_err(|_| CoreError::LengthOverflow)?;
    if child_count > F || (count == 0) != (child_count == 0) {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let expected = 25usize
        .checked_add(
            child_count
                .checked_mul(40)
                .ok_or(CoreError::LengthOverflow)?,
        )
        .ok_or(CoreError::LengthOverflow)?;
    if expected != body.len() {
        return Err(if expected > body.len() {
            CoreError::UnexpectedEof
        } else {
            CoreError::TrailingBytes
        });
    }
    let mut previous = 0;
    let children = body[25..]
        .chunks_exact(40)
        .map(|bytes| {
            let cumulative_end = u64::from_be_bytes(
                bytes[..8]
                    .try_into()
                    .map_err(|_| CoreError::UnexpectedEof)?,
            );
            if cumulative_end < previous {
                return Err(CoreError::NonCanonicalOrdering);
            }
            previous = cumulative_end;
            Ok(Child {
                cumulative_end,
                object_id: ObjectId::from_bytes(&bytes[8..])?,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    if children.last().map_or(0, |child| child.cumulative_end) != total {
        return Err(CoreError::LengthMismatch {
            expected: total,
            actual: children.last().map_or(0, |child| child.cumulative_end),
        });
    }
    Ok((total, count, level, children))
}

fn expected_level(count: u64) -> CoreResult<u8> {
    if count == 0 {
        return Ok(0);
    }
    let leaves = count
        .checked_add(K as u64 - 1)
        .ok_or(CoreError::LengthOverflow)?
        / K as u64;
    let mut capacity = F as u64;
    let mut level = 0_u8;
    while leaves > capacity {
        capacity = capacity
            .checked_mul(F as u64)
            .ok_or(CoreError::LengthOverflow)?;
        level = level
            .checked_add(1)
            .ok_or(CoreError::MappingDepthExceeded)?;
    }
    Ok(level)
}

struct Built {
    root: ObjectId,
    commitment: [u8; 32],
    mapping_bytes: usize,
}

fn build_v2(references: &[V2Reference], store: &mut ShadowStore) -> CoreResult<Built> {
    let mut mapping_bytes = 0usize;
    let mut current = Vec::new();
    let mut total = 0_u64;
    for leaf in references.chunks(K) {
        let canonical = encode_leaf(leaf)?;
        mapping_bytes = mapping_bytes
            .checked_add(canonical.len())
            .ok_or(CoreError::LengthOverflow)?;
        let id = store.put(canonical)?;
        total = leaf.iter().try_fold(total, |total, reference| {
            total
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)
        })?;
        current.push(Child {
            cumulative_end: total,
            object_id: id,
        });
    }

    let mut level = 0_u8;
    while current.len() > F {
        level = level
            .checked_add(1)
            .ok_or(CoreError::MappingDepthExceeded)?;
        let mut next = Vec::new();
        let mut global_end = 0_u64;
        for group in current.chunks(F) {
            let group_start = global_end;
            let relative = group
                .iter()
                .map(|child| Child {
                    cumulative_end: child.cumulative_end - group_start,
                    object_id: child.object_id,
                })
                .collect::<Vec<_>>();
            let canonical = encode_children(v1::FILE_BRANCH_TAG, Some(level), &relative)?;
            mapping_bytes = mapping_bytes
                .checked_add(canonical.len())
                .ok_or(CoreError::LengthOverflow)?;
            let id = store.put(canonical)?;
            global_end = group
                .last()
                .map_or(global_end, |child| child.cumulative_end);
            next.push(Child {
                cumulative_end: global_end,
                object_id: id,
            });
        }
        current = next;
    }
    let root = encode_root(
        total,
        u64::try_from(references.len()).map_err(|_| CoreError::LengthOverflow)?,
        level,
        &current,
    )?;
    mapping_bytes = mapping_bytes
        .checked_add(root.len())
        .ok_or(CoreError::LengthOverflow)?;
    let root = store.put(root)?;
    Ok(Built {
        root,
        commitment: ordered_commitment(references),
        mapping_bytes,
    })
}

fn independent_mapping(tag: u8, body: &[u8]) -> CoreResult<Vec<u8>> {
    let mut inner = Vec::with_capacity(11 + body.len());
    inner.extend_from_slice(b"LFS4MAP\0");
    inner.extend_from_slice(&2_u16.to_be_bytes());
    inner.push(tag);
    inner.extend_from_slice(body);
    let payload_len = 4usize
        .checked_add(inner.len())
        .ok_or(CoreError::LengthOverflow)?;
    let mut canonical = Vec::with_capacity(9 + payload_len);
    canonical.extend_from_slice(b"LFSO");
    canonical.push(1);
    canonical.extend_from_slice(
        &u32::try_from(payload_len)
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    canonical.extend_from_slice(
        &u32::try_from(inner.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    canonical.extend_from_slice(&inner);
    Ok(canonical)
}

fn build_v2_independently(references: &[V2Reference]) -> CoreResult<Built> {
    let mut mapping_bytes = 0usize;
    let mut current = Vec::new();
    let mut total = 0_u64;
    for leaf in references.chunks(64) {
        let mut body = Vec::with_capacity(4 + leaf.len() * 36);
        body.extend_from_slice(
            &u32::try_from(leaf.len())
                .map_err(|_| CoreError::LengthOverflow)?
                .to_be_bytes(),
        );
        for reference in leaf {
            body.extend_from_slice(&reference.raw_length.to_be_bytes());
            body.extend_from_slice(reference.object_id.as_bytes());
            total = total
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)?;
        }
        let canonical = independent_mapping(0x02, &body)?;
        mapping_bytes = mapping_bytes
            .checked_add(canonical.len())
            .ok_or(CoreError::LengthOverflow)?;
        current.push(Child {
            cumulative_end: total,
            object_id: ObjectId::for_bytes(&canonical),
        });
    }

    let mut level = 0_u8;
    while current.len() > 64 {
        level = level
            .checked_add(1)
            .ok_or(CoreError::MappingDepthExceeded)?;
        let mut next = Vec::new();
        let mut group_start = 0_u64;
        for group in current.chunks(64) {
            let mut body = Vec::with_capacity(5 + group.len() * 40);
            body.push(level);
            body.extend_from_slice(
                &u32::try_from(group.len())
                    .map_err(|_| CoreError::LengthOverflow)?
                    .to_be_bytes(),
            );
            for child in group {
                body.extend_from_slice(
                    &child
                        .cumulative_end
                        .checked_sub(group_start)
                        .ok_or(CoreError::LengthOverflow)?
                        .to_be_bytes(),
                );
                body.extend_from_slice(child.object_id.as_bytes());
            }
            let canonical = independent_mapping(0x07, &body)?;
            mapping_bytes = mapping_bytes
                .checked_add(canonical.len())
                .ok_or(CoreError::LengthOverflow)?;
            group_start = group
                .last()
                .ok_or(CoreError::NonCanonicalPagePartition)?
                .cumulative_end;
            next.push(Child {
                cumulative_end: group_start,
                object_id: ObjectId::for_bytes(&canonical),
            });
        }
        current = next;
    }

    let mut body = Vec::with_capacity(25 + current.len() * 40);
    body.extend_from_slice(&0_u32.to_be_bytes());
    body.extend_from_slice(&total.to_be_bytes());
    body.extend_from_slice(
        &u64::try_from(references.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    body.push(level);
    body.extend_from_slice(
        &u32::try_from(current.len())
            .map_err(|_| CoreError::LengthOverflow)?
            .to_be_bytes(),
    );
    for child in current {
        body.extend_from_slice(&child.cumulative_end.to_be_bytes());
        body.extend_from_slice(child.object_id.as_bytes());
    }
    let canonical = independent_mapping(0x01, &body)?;
    mapping_bytes = mapping_bytes
        .checked_add(canonical.len())
        .ok_or(CoreError::LengthOverflow)?;

    let mut commitment =
        blake3::Hasher::new_derive_key("layerfs/canonical-v2/ordered-occurrence/v1");
    for reference in references {
        commitment.update(&reference.raw_length.to_be_bytes());
        commitment.update(reference.object_id.as_bytes());
    }
    Ok(Built {
        root: ObjectId::for_bytes(&canonical),
        commitment: *commitment.finalize().as_bytes(),
        mapping_bytes,
    })
}

fn raw_references(raw_chunks: &[&[u8]], store: &mut ShadowStore) -> CoreResult<Vec<V2Reference>> {
    raw_chunks
        .iter()
        .map(|raw| {
            let canonical = encode_bytes_object(raw)?;
            Ok(V2Reference {
                raw_length: u32::try_from(raw.len()).map_err(|_| CoreError::LengthOverflow)?,
                object_id: store.put(canonical)?,
            })
        })
        .collect()
}

#[derive(Default)]
struct Scrub {
    references: Vec<V2Reference>,
    mapping_objects: usize,
}

fn walk_v2(store: &ShadowStore, id: ObjectId, level: u8, final_node: bool) -> CoreResult<Scrub> {
    let canonical = store.authenticated(id)?;
    if level == 0 {
        return Ok(Scrub {
            references: parse_leaf(canonical, final_node)?,
            mapping_objects: 1,
        });
    }
    let (actual_level, children) = parse_children(canonical, true, final_node)?;
    if actual_level != level {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let mut result = Scrub {
        references: Vec::new(),
        mapping_objects: 1,
    };
    let mut previous = 0_u64;
    let count = children.len();
    for (index, child) in children.into_iter().enumerate() {
        let child_scrub = walk_v2(
            store,
            child.object_id,
            level - 1,
            final_node && index + 1 == count,
        )?;
        let child_total = child_scrub
            .references
            .iter()
            .try_fold(0_u64, |total, reference| {
                total
                    .checked_add(u64::from(reference.raw_length))
                    .ok_or(CoreError::LengthOverflow)
            })?;
        previous = previous
            .checked_add(child_total)
            .ok_or(CoreError::LengthOverflow)?;
        if child.cumulative_end != previous {
            return Err(CoreError::LengthMismatch {
                expected: child.cumulative_end,
                actual: previous,
            });
        }
        result.mapping_objects += child_scrub.mapping_objects;
        result.references.extend(child_scrub.references);
    }
    Ok(result)
}

fn scrub_v2(store: &ShadowStore, root: ObjectId) -> CoreResult<Scrub> {
    let canonical = store.authenticated(root)?;
    let (total, count, level, children) = parse_root(canonical)?;
    if level != expected_level(count)? {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    if count == 0 {
        return Ok(Scrub {
            references: Vec::new(),
            mapping_objects: 1,
        });
    }
    let mut result = Scrub {
        references: Vec::new(),
        mapping_objects: 1,
    };
    let mut previous = 0_u64;
    let child_count = children.len();
    for (index, child) in children.into_iter().enumerate() {
        let child_scrub = walk_v2(store, child.object_id, level, index + 1 == child_count)?;
        let child_total = child_scrub
            .references
            .iter()
            .try_fold(0_u64, |total, reference| {
                total
                    .checked_add(u64::from(reference.raw_length))
                    .ok_or(CoreError::LengthOverflow)
            })?;
        previous = previous
            .checked_add(child_total)
            .ok_or(CoreError::LengthOverflow)?;
        if child.cumulative_end != previous {
            return Err(CoreError::LengthMismatch {
                expected: child.cumulative_end,
                actual: previous,
            });
        }
        result.mapping_objects += child_scrub.mapping_objects;
        result.references.extend(child_scrub.references);
    }
    if previous != total || result.references.len() as u64 != count {
        return Err(CoreError::LengthMismatch {
            expected: total,
            actual: previous,
        });
    }
    Ok(result)
}

#[derive(Default, Debug, Eq, PartialEq)]
struct Work {
    mapping_objects: usize,
    chunk_objects: usize,
    canonical_bytes: usize,
    raw_hashes: usize,
}

fn read_v2(store: &ShadowStore, root: ObjectId, range: Range<u64>) -> CoreResult<(Vec<u8>, Work)> {
    let canonical = store.authenticated(root)?;
    let mut work = Work {
        mapping_objects: 1,
        canonical_bytes: canonical.len(),
        ..Work::default()
    };
    let (total, count, level, children) = parse_root(canonical)?;
    if level != expected_level(count)? || range.start > range.end || range.end > total {
        return Err(if range.start > range.end || range.end > total {
            CoreError::InvalidRange {
                start: range.start,
                end: range.end,
                length: total,
            }
        } else {
            CoreError::NonCanonicalPagePartition
        });
    }
    if range.start == range.end {
        return Ok((Vec::new(), work));
    }
    let mut output = Vec::new();
    let mut previous = 0_u64;
    let child_count = children.len();
    for (index, child) in children.into_iter().enumerate() {
        if child.cumulative_end > range.start && previous < range.end {
            route_v2(
                store,
                child.object_id,
                level,
                index + 1 == child_count,
                previous,
                &range,
                &mut output,
                &mut work,
            )?;
        }
        previous = child.cumulative_end;
    }
    Ok((output, work))
}

#[allow(clippy::too_many_arguments)]
fn route_v2(
    store: &ShadowStore,
    id: ObjectId,
    level: u8,
    final_node: bool,
    node_start: u64,
    range: &Range<u64>,
    output: &mut Vec<u8>,
    work: &mut Work,
) -> CoreResult<()> {
    let canonical = store.authenticated(id)?;
    work.mapping_objects += 1;
    work.canonical_bytes += canonical.len();
    if level == 0 {
        let references = parse_leaf(canonical, final_node)?;
        let mut offset = node_start;
        for reference in references {
            let end = offset
                .checked_add(u64::from(reference.raw_length))
                .ok_or(CoreError::LengthOverflow)?;
            if end > range.start && offset < range.end && reference.raw_length != 0 {
                let chunk = store.authenticated(reference.object_id)?;
                work.chunk_objects += 1;
                work.canonical_bytes += chunk.len();
                let raw = decode_bytes_object(chunk)?;
                if raw.len() != reference.raw_length as usize {
                    return Err(CoreError::ChunkLengthMismatch);
                }
                let start = usize::try_from(range.start.saturating_sub(offset))
                    .map_err(|_| CoreError::LengthOverflow)?;
                let finish = usize::try_from(range.end.min(end) - offset)
                    .map_err(|_| CoreError::LengthOverflow)?;
                output.extend_from_slice(&raw[start..finish]);
            }
            offset = end;
        }
        return Ok(());
    }
    let (actual_level, children) = parse_children(canonical, true, final_node)?;
    if actual_level != level {
        return Err(CoreError::NonCanonicalPagePartition);
    }
    let mut previous = 0_u64;
    let child_count = children.len();
    for (index, child) in children.into_iter().enumerate() {
        let start = node_start
            .checked_add(previous)
            .ok_or(CoreError::LengthOverflow)?;
        let end = node_start
            .checked_add(child.cumulative_end)
            .ok_or(CoreError::LengthOverflow)?;
        if end > range.start && start < range.end {
            route_v2(
                store,
                child.object_id,
                level - 1,
                final_node && index + 1 == child_count,
                start,
                range,
                output,
                work,
            )?;
        }
        previous = child.cumulative_end;
    }
    Ok(())
}

fn reconstruct_v2(store: &ShadowStore, root: ObjectId) -> CoreResult<(Vec<u8>, Work)> {
    let scrub = scrub_v2(store, root)?;
    let mut output = Vec::new();
    let mut work = Work {
        mapping_objects: scrub.mapping_objects,
        ..Work::default()
    };
    for reference in scrub.references {
        let chunk = store.authenticated(reference.object_id)?;
        work.chunk_objects += 1;
        work.canonical_bytes += chunk.len();
        let raw = decode_bytes_object(chunk)?;
        if raw.len() != reference.raw_length as usize {
            return Err(CoreError::ChunkLengthMismatch);
        }
        output.extend_from_slice(raw);
    }
    Ok((output, work))
}

fn reconstruct_v1(store: &ShadowStore, references: &[FileReference]) -> CoreResult<Vec<u8>> {
    let mut output = Vec::new();
    for reference in references {
        let canonical = store.authenticated(reference.object_id)?;
        let raw = decode_bytes_object(canonical)?;
        if raw.len() != reference.raw_length as usize {
            return Err(CoreError::ChunkLengthMismatch);
        }
        if chunk_id(raw) != reference.raw_id {
            return Err(CoreError::ChunkIdentityMismatch);
        }
        output.extend_from_slice(raw);
    }
    Ok(output)
}

fn splice_at_rejoin(
    old: &[V2Reference],
    retained_prefix: usize,
    scanned: &[V2Reference],
) -> CoreResult<Vec<V2Reference>> {
    for old_index in retained_prefix..old.len().saturating_sub(1) {
        for scanned_index in 0..scanned.len().saturating_sub(1) {
            if old[old_index..old_index + 2] == scanned[scanned_index..scanned_index + 2] {
                let mut result = old[..retained_prefix].to_vec();
                result.extend_from_slice(&scanned[..scanned_index]);
                result.extend_from_slice(&old[old_index..]);
                return Ok(result);
            }
        }
    }
    Err(CoreError::BoundedResynchronization {
        scanned: scanned.len() as u64,
        limit: scanned.len() as u64,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Profile {
    V1,
    V2,
}

fn authorize_normalization(authenticated_parent: bool) -> CoreResult<()> {
    authenticated_parent
        .then_some(())
        .ok_or(CoreError::ValidationAuthorityUnavailable)
}

fn authorize_transition(parent: Profile, child: Profile) -> CoreResult<()> {
    match (parent, child) {
        (Profile::V1, Profile::V2) => Err(CoreError::SchemaMigrationRequired),
        (Profile::V2, Profile::V1) => Err(CoreError::ProfileMismatch),
        _ => Ok(()),
    }
}

struct Receipt {
    profile: ObjectId,
    root: ObjectId,
}

impl Receipt {
    fn validate(&self, profile: ObjectId, root: ObjectId) -> CoreResult<()> {
        if self.profile != profile || self.root != root {
            return Err(CoreError::InvalidValidationReceipt);
        }
        Ok(())
    }
}

struct Budget {
    live: Cell<u64>,
    high_water: Cell<u64>,
    limit: u64,
}

impl Budget {
    fn with_charge<T>(&self, bytes: u64, work: impl FnOnce() -> CoreResult<T>) -> CoreResult<T> {
        let next = self
            .live
            .get()
            .checked_add(bytes)
            .ok_or(CoreError::LengthOverflow)?;
        if next > self.limit {
            return Err(CoreError::AllocationBudgetExceeded);
        }
        self.live.set(next);
        self.high_water.set(self.high_water.get().max(next));
        let result = work();
        self.live.set(self.live.get() - bytes);
        result
    }
}

#[test]
fn canonical_v2_shadow_closes_format_authority_edit_and_range_questions() {
    let mut store = ShadowStore::default();
    let raw = [b"left".as_slice(), b"right", b"left"];
    let direct = raw_references(&raw, &mut store).expect("direct v2 references");
    let v1_references = raw
        .iter()
        .zip(&direct)
        .map(|(raw, reference)| FileReference {
            raw_id: chunk_id(raw),
            raw_length: reference.raw_length,
            object_id: reference.object_id,
        })
        .collect::<Vec<_>>();
    let normalized = v1_references
        .iter()
        .copied()
        .map(V2Reference::from)
        .collect::<Vec<_>>();
    assert_eq!(normalized, direct);

    let object_count = store.objects.len();
    for raw in raw {
        store
            .put(encode_bytes_object(raw).expect("canonical chunk"))
            .expect("equal-only reuse");
    }
    assert_eq!(
        store.objects.len(),
        object_count,
        "v1/v2 reuse the same chunk BLOBs"
    );

    let from_raw = build_v2(&direct, &mut store).expect("direct tree");
    let from_v1 = build_v2(&normalized, &mut store).expect("normalized tree");
    assert_eq!(from_raw.root, from_v1.root);
    assert_eq!(from_raw.commitment, from_v1.commitment);
    assert_eq!(
        scrub_v2(&store, from_raw.root).expect("scrub").references,
        direct
    );
    let (reconstructed, work) = reconstruct_v2(&store, from_raw.root).expect("reconstruction");
    assert_eq!(reconstructed, b"leftrightleft");
    assert_eq!(work.raw_hashes, 0);
    assert_eq!(
        reconstruct_v1(&store, &v1_references).expect("v1 reconstruction"),
        reconstructed
    );

    let mut wrong_raw = v1_references.clone();
    wrong_raw[0].raw_id = chunk_id(b"not-left");
    assert_eq!(
        reconstruct_v1(&store, &wrong_raw),
        Err(CoreError::ChunkIdentityMismatch)
    );
    assert_eq!(
        authorize_normalization(false),
        Err(CoreError::ValidationAuthorityUnavailable),
        "payload-free normalization needs prior v1 closure authority"
    );
    authorize_normalization(true).expect("authenticated v1 normalization");
    assert_eq!(
        authorize_transition(Profile::V1, Profile::V2),
        Err(CoreError::SchemaMigrationRequired),
        "current transitions cannot bind both mapping profiles"
    );
    assert_eq!(
        authorize_transition(Profile::V2, Profile::V1),
        Err(CoreError::ProfileMismatch)
    );

    let receipt = Receipt {
        profile: v2_profile_id(),
        root: from_raw.root,
    };
    receipt
        .validate(v2_profile_id(), from_raw.root)
        .expect("v2 receipt");
    assert_eq!(
        receipt.validate(v1::selected_mapping_profile_id(), from_raw.root),
        Err(CoreError::InvalidValidationReceipt)
    );
    assert_eq!(
        receipt.validate(v2_profile_id(), ObjectId::for_bytes(b"other root")),
        Err(CoreError::InvalidValidationReceipt)
    );

    let direct_again = {
        let mut history = vec![direct[0], direct[1], direct[0], direct[2]];
        history.remove(2);
        history
    };
    assert_eq!(
        build_v2(&direct_again, &mut store)
            .expect("history tree")
            .root,
        from_raw.root
    );

    let a = direct[0];
    let b = direct[1];
    let c = raw_references(&[b"c", b"d", b"x", b"y"], &mut store).expect("edit refs");
    let old = [a, b, c[0], c[1]];
    assert_eq!(
        splice_at_rejoin(&old, 1, &[c[2], c[0], c[1]]).expect("same count"),
        [a, c[2], c[0], c[1]]
    );
    assert_eq!(
        splice_at_rejoin(&old, 1, &[c[2], c[3], c[0], c[1]]).expect("plus one"),
        [a, c[2], c[3], c[0], c[1]]
    );
    assert_eq!(
        splice_at_rejoin(&old, 1, &[c[0], c[1]]).expect("minus one"),
        [a, c[0], c[1]]
    );

    let omitted = build_v2(&[direct[0], direct[2]], &mut store).expect("omitted");
    let duplicated =
        build_v2(&[direct[0], direct[1], direct[1], direct[2]], &mut store).expect("duplicated");
    let reordered = build_v2(&[direct[1], direct[0], direct[2]], &mut store).expect("reordered");
    for candidate in [&omitted, &duplicated, &reordered] {
        assert_ne!(candidate.root, from_raw.root);
        assert_ne!(candidate.commitment, from_raw.commitment);
    }

    let mut wrong_length = direct.clone();
    wrong_length[0].raw_length += 1;
    let wrong_length = build_v2(&wrong_length, &mut store).expect("wrong length tree");
    assert_eq!(
        reconstruct_v2(&store, wrong_length.root),
        Err(CoreError::ChunkLengthMismatch)
    );

    let directory = encode_object(&Object::directory(Vec::new()).expect("directory"))
        .expect("directory encoding");
    let directory_id = store.put(directory).expect("directory put");
    let wrong_role = build_v2(
        &[V2Reference {
            raw_length: 0,
            object_id: directory_id,
        }],
        &mut store,
    )
    .expect("wrong-role tree");
    assert_eq!(
        reconstruct_v2(&store, wrong_role.root),
        Err(CoreError::WrongLogicalRole)
    );

    let mut trailing_body = Vec::new();
    trailing_body.extend_from_slice(&1_u32.to_be_bytes());
    encode_reference(direct[0], &mut trailing_body);
    trailing_body.push(0);
    let trailing = v2_mapping(v1::FILE_LEAF_TAG, &trailing_body).expect("trailing leaf");
    assert_eq!(parse_leaf(&trailing, true), Err(CoreError::TrailingBytes));
    assert_eq!(
        encode_leaf(&[V2Reference {
            raw_length: MAX_RAW_BYTES + 1,
            object_id: direct[0].object_id,
        }]),
        Err(CoreError::ObjectLimitExceeded)
    );
    let truncated = &encode_leaf(&direct).expect("leaf")[..20];
    assert_eq!(parse_leaf(truncated, true), Err(CoreError::UnexpectedEof));

    let v1_leaf = encode_object(
        &Object::bytes(v1::encode_file_leaf(&v1_references).expect("v1 leaf"))
            .expect("v1 mapping object"),
    )
    .expect("v1 canonical leaf");
    assert_eq!(
        parse_leaf(&v1_leaf, true),
        Err(CoreError::UnsupportedMappingVersion { version: 1 })
    );
    assert_eq!(
        v1::decode_mapping(&encode_leaf(&direct).expect("v2 leaf"), v1::FILE_LEAF_TAG,),
        Err(CoreError::UnsupportedMappingVersion { version: 2 })
    );

    let budget = Budget {
        live: Cell::new(0),
        high_water: Cell::new(0),
        limit: 64,
    };
    assert_eq!(
        budget.with_charge(64, || Err::<(), _>(CoreError::TrailingBytes)),
        Err(CoreError::TrailingBytes)
    );
    assert_eq!(budget.live.get(), 0);
    assert_eq!(budget.high_water.get(), 64);
    budget.live.set(u64::MAX);
    assert_eq!(
        budget.with_charge(1, || Ok(())),
        Err(CoreError::LengthOverflow)
    );
    assert_eq!(budget.live.get(), u64::MAX);
}

#[test]
fn canonical_v2_shadow_covers_radix_and_range_boundaries() {
    let id = ObjectId::for_bytes(b"repeated canonical chunk");
    for count in [0, 1, K, K + 1, K * F, K * F + 1] {
        let references = vec![
            V2Reference {
                raw_length: 0,
                object_id: id,
            };
            count
        ];
        let mut store = ShadowStore::default();
        let built = build_v2(&references, &mut store).expect("boundary tree");
        let independent = build_v2_independently(&references).expect("independent boundary tree");
        assert_eq!(built.root, independent.root);
        assert_eq!(built.commitment, independent.commitment);
        assert_eq!(built.mapping_bytes, independent.mapping_bytes);
        let scrub = scrub_v2(&store, built.root).expect("boundary scrub");
        assert_eq!(scrub.references, references);
        assert_eq!(
            expected_level(count as u64).expect("level"),
            u8::from(count > K * F)
        );
    }

    assert_eq!(
        encode_leaf(&vec![
            V2Reference {
                raw_length: 0,
                object_id: id
            };
            K
        ])
        .unwrap()
        .len(),
        2_332
    );

    let mut store = ShadowStore::default();
    let chunk = raw_references(&[b"x"], &mut store).expect("one-byte chunk")[0];
    let references = vec![chunk; K * F + K + 1];
    let built = build_v2(&references, &mut store).expect("range tree");
    for range in [
        0..0,
        63..65,
        4_095..4_097,
        4_096..4_161,
        0..references.len() as u64,
    ] {
        let (bytes, work) = read_v2(&store, built.root, range.clone()).expect("range");
        assert_eq!(bytes, vec![b'x'; (range.end - range.start) as usize]);
        assert_eq!(work.raw_hashes, 0);
        if range.start == range.end {
            assert_eq!(work.mapping_objects, 1);
            assert_eq!(work.chunk_objects, 0);
        }
    }
}

#[test]
fn canonical_v2_shadow_freezes_vectors_and_retained_size_model() {
    let mut store = ShadowStore::default();
    let abc = raw_references(&[b"abc"], &mut store).expect("abc reference");
    assert_eq!(
        hex(&encode_leaf(&abc).expect("abc leaf")),
        "4c46534f0100000037000000334c4653344d415000000202000000010000000343bf78cf00944d56aa2f6ff8de5e585e6a1d61764be26aaca754b6d1f84cb94b"
    );
    let empty = build_v2(&[], &mut store).expect("empty tree");
    assert_eq!(
        hex(store.authenticated(empty.root).expect("empty root")),
        "4c46534f0100000028000000244c4653344d41500000020100000000000000000000000000000000000000000000000000"
    );
    let references = raw_references(&[b"left", b"right", b"left"], &mut store).expect("refs");
    let built = build_v2(&references, &mut store).expect("tree");
    let independent = build_v2_independently(&references).expect("independent tree");
    assert_eq!(built.root, independent.root);
    assert_eq!(built.commitment, independent.commitment);
    assert_eq!(built.mapping_bytes, independent.mapping_bytes);

    assert_eq!(
        v2_profile_id().to_string(),
        "94a03ba7b6c97b5ff37c0ec62ef1d801b9896494b45456bd3df23e2cb278d13b"
    );
    assert_eq!(
        built.root.to_string(),
        "d618ebc7b5309eb8cb3c777f4de134759e165e2d36d7b8004c7b2c51ac3d9031"
    );
    assert_eq!(
        hex(&built.commitment),
        "1f680efae4a11ed98904fec40cd2e28fa0e607b48e43942f77d8c42e82310a2d"
    );

    let retained = vec![
        V2Reference {
            raw_length: 1,
            object_id: ObjectId::for_bytes(b"same"),
        };
        5_284
    ];
    let retained = build_v2(&retained, &mut ShadowStore::default()).expect("retained model");
    let retained_independent = build_v2_independently(&vec![
        V2Reference {
            raw_length: 1,
            object_id: ObjectId::for_bytes(b"same"),
        };
        5_284
    ])
    .expect("independent retained model");
    assert_eq!(retained.root, retained_independent.root);
    assert_eq!(retained.mapping_bytes, retained_independent.mapping_bytes);
    assert_eq!(retained.mapping_bytes, 196_055);
    assert_eq!(retained.mapping_bytes + 119, 196_174);
    assert_eq!(365_262 - (retained.mapping_bytes + 119), 169_088);
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
