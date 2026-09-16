//! Streaming construction of the extent tree.
//!
//! Chunks are emitted as the scanner produces them, full pages are sealed as
//! soon as they can no longer change, and the file state is emitted last from
//! facts already established. Only unfinished boundary pages are retained, so
//! construction never holds the whole mapping.

use std::io::Read;

use crate::error::{ContentError, ContentResult};
use crate::file::cdc::FastCdc;
use crate::file::mapping::codec::{
    encode_chunk_object, encode_file_state, encode_node, profile_id,
};
use crate::file::mapping::types::{
    ChildDescriptor, ExtentNode, ExtentSlice, FileState, NodeSummary, MAX_ENTRIES, MAX_LEVEL,
};
use crate::object::{FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole};
use crate::policy::ConstructionCapacities;

/// Facts established while building; no root reread is required afterwards.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MappingBuild {
    /// Mapping root, or `None` when the input produced no extents at all.
    pub root: Option<NodeSummary>,
    /// Logical bytes scanned.
    pub logical_len: u64,
    /// Extents in the tree.
    pub extent_count: u64,
    /// Level of the mapping root.
    pub tree_level: u8,
    /// Chunk objects emitted.
    pub chunks: u64,
    /// Mapping pages emitted.
    pub nodes: u64,
}

enum Pending {
    Extents(Vec<ExtentSlice>),
    Children(Vec<NodeSummary>),
}

impl Pending {
    fn len(&self) -> usize {
        match self {
            Self::Extents(entries) => entries.len(),
            Self::Children(entries) => entries.len(),
        }
    }
}

/// Builds the extent tree for `source`, emitting every finalized object in
/// child-before-parent order.
pub fn build_streaming<R: Read>(
    capacities: &ConstructionCapacities,
    source: R,
    consumer: &mut dyn FinalizedConsumer,
) -> ContentResult<MappingBuild> {
    let flush_at = capacities.stream_flush_entries.max(MAX_ENTRIES + 1);
    let mut levels = vec![Pending::Extents(Vec::with_capacity(flush_at + 1))];
    let mut build = MappingBuild::default();
    let cdc = FastCdc::new().scan(source, |chunk| {
        let object = FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(chunk)?)?;
        let id = object.id();
        consumer.accept(object)?;
        build.chunks = add(build.chunks, 1)?;
        build.logical_len = add(build.logical_len, chunk.len() as u64)?;
        match &mut levels[0] {
            Pending::Extents(extents) => {
                extents.push(ExtentSlice::new(id, 0, chunk.len() as u32)?);
            }
            Pending::Children(_) => return Err(ContentError::InvalidRecord("builder level zero")),
        }
        flush_streaming(consumer, &mut levels, 0, flush_at, &mut build)
    })?;
    if cdc.bytes_scanned != build.logical_len {
        return Err(ContentError::InvalidRecord("chunk accounting"));
    }
    if build.logical_len == 0 {
        return Ok(build);
    }
    let root = finish(consumer, &mut levels, &mut build)?;
    build.root = Some(root);
    build.extent_count = root.extents;
    build.tree_level = root.level;
    Ok(build)
}

/// Emits the defined empty mapping page and returns its summary.
pub fn emit_empty_leaf(
    consumer: &mut dyn FinalizedConsumer,
    build: &mut MappingBuild,
) -> ContentResult<NodeSummary> {
    emit_node(
        consumer,
        &ExtentNode::Leaf {
            subtree_logical_bytes: 0,
            extents: Vec::new(),
        },
        build,
    )
}

/// Emits the file-state root for an already-finished mapping.
pub fn emit_file_state(
    consumer: &mut dyn FinalizedConsumer,
    mapping_root: NodeSummary,
) -> ContentResult<ObjectId> {
    let state = FileState {
        logical_len: mapping_root.bytes,
        extent_count: mapping_root.extents,
        tree_level: mapping_root.level,
        profile_id: profile_id(),
        mapping_root: mapping_root.id,
    };
    let object = FinalizedObject::new(ObjectRole::FileState, encode_file_state(state)?)?
        .with_references(vec![mapping_root.id]);
    let id = object.id();
    consumer.accept(object)?;
    Ok(id)
}

fn flush_streaming(
    consumer: &mut dyn FinalizedConsumer,
    levels: &mut Vec<Pending>,
    level: usize,
    flush_at: usize,
    build: &mut MappingBuild,
) -> ContentResult<()> {
    loop {
        if levels[level].len() <= flush_at {
            return Ok(());
        }
        let summary = emit_prefix(
            consumer,
            &mut levels[level],
            MAX_ENTRIES,
            level as u8,
            build,
        )?;
        push_summary(levels, level + 1, summary)?;
    }
}

fn finish(
    consumer: &mut dyn FinalizedConsumer,
    levels: &mut Vec<Pending>,
    build: &mut MappingBuild,
) -> ContentResult<NodeSummary> {
    let mut level = 0;
    loop {
        let higher_nonempty = levels
            .iter()
            .skip(level + 1)
            .any(|pending| pending.len() != 0);
        let len = levels[level].len();
        if !higher_nonempty && len <= MAX_ENTRIES {
            if level > 0 && len == 1 {
                if let Pending::Children(children) = &levels[level] {
                    return Ok(children[0]);
                }
            }
            return emit_prefix(consumer, &mut levels[level], len, level as u8, build);
        }
        if len != 0 {
            let first = if len > MAX_ENTRIES { len / 2 } else { len };
            let summary = emit_prefix(consumer, &mut levels[level], first, level as u8, build)?;
            push_summary(levels, level + 1, summary)?;
            continue;
        }
        level += 1;
        if level >= levels.len() {
            return Err(ContentError::InvalidRecord("empty rope builder"));
        }
    }
}

fn emit_prefix(
    consumer: &mut dyn FinalizedConsumer,
    pending: &mut Pending,
    count: usize,
    level: u8,
    build: &mut MappingBuild,
) -> ContentResult<NodeSummary> {
    let node = match pending {
        Pending::Extents(entries) => {
            let entries: Vec<ExtentSlice> = entries.drain(..count).collect();
            let bytes = entries.iter().try_fold(0_u64, |sum, entry| {
                add(sum, u64::from(entry.logical_length()))
            })?;
            ExtentNode::Leaf {
                subtree_logical_bytes: bytes,
                extents: entries,
            }
        }
        Pending::Children(entries) => {
            let entries: Vec<NodeSummary> = entries.drain(..count).collect();
            let mut bytes = 0_u64;
            let mut extents = 0_u64;
            let children = entries
                .iter()
                .map(|entry| {
                    bytes = add(bytes, entry.bytes)?;
                    extents = add(extents, entry.extents)?;
                    Ok(ChildDescriptor {
                        cumulative_logical_end: bytes,
                        cumulative_extent_end: extents,
                        child_object_id: entry.id,
                    })
                })
                .collect::<ContentResult<Vec<_>>>()?;
            ExtentNode::Branch {
                level,
                subtree_logical_bytes: bytes,
                subtree_extent_count: extents,
                children,
            }
        }
    };
    emit_node(consumer, &node, build)
}

fn emit_node(
    consumer: &mut dyn FinalizedConsumer,
    node: &ExtentNode,
    build: &mut MappingBuild,
) -> ContentResult<NodeSummary> {
    if node.level() > MAX_LEVEL {
        return Err(ContentError::MappingDepthExceeded);
    }
    let role = match node {
        ExtentNode::Leaf { .. } => ObjectRole::ExtentLeaf,
        ExtentNode::Branch { .. } => ObjectRole::ExtentBranch,
    };
    let object = FinalizedObject::new(role, encode_node(node)?)?.with_references(node.references());
    let summary = NodeSummary {
        id: object.id(),
        bytes: node.logical_len(),
        extents: node.extent_count(),
        level: node.level(),
    };
    consumer.accept(object)?;
    build.nodes = add(build.nodes, 1)?;
    Ok(summary)
}

fn push_summary(
    levels: &mut Vec<Pending>,
    level: usize,
    summary: NodeSummary,
) -> ContentResult<()> {
    if usize::from(summary.level) + 1 != level {
        return Err(ContentError::InvalidRecord("rope builder level"));
    }
    while levels.len() <= level {
        levels.push(Pending::Children(Vec::with_capacity(MAX_ENTRIES + 1)));
    }
    match &mut levels[level] {
        Pending::Children(children) => children.push(summary),
        Pending::Extents(_) => return Err(ContentError::InvalidRecord("rope builder role")),
    }
    Ok(())
}

fn add(left: u64, right: u64) -> ContentResult<u64> {
    left.checked_add(right).ok_or(ContentError::LengthOverflow)
}
