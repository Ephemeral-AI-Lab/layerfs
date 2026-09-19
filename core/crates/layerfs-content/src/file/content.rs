//! Complete-file construction and the result it returns.
//!
//! A file constructor returns the root identity that opens the file and its
//! logical byte length; nothing else is needed to read it back. The representation
//! is chosen from the logical length under the frozen policy: zero uses the defined
//! empty file-state form, below the cutoff uses one whole-file object, and at or
//! above the cutoff uses a file state over a chunked extent tree.
//!
//! Construction emits each finalized object to a bounded consumer as soon as its
//! bytes can no longer change, in child-before-parent order.

use std::io::Read;

use layerfs_telemetry::timer::{Active, TimingScope};

use crate::error::{ContentError, ContentResult};
use crate::file::mapping::{self, FileState, MappingBuild, PredecessorBase};
use crate::object::{
    encode_bytes_object, AuthenticatedObjects, FinalizedConsumer, FinalizedObject, ObjectId,
    ObjectRole,
};
use crate::policy::{ConstructionCapacities, ConstructionPolicy, Representation};

const WHOLE_MAGIC: &[u8; 8] = b"LFS5SML\0";
const WHOLE_VERSION: u16 = 1;
/// Bytes the whole-file value header occupies before its payload.
pub const WHOLE_VALUE_HEADER: usize = 10;
/// Root of a constructed file plus the logical length it opens.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConstructedFile {
    /// Identity that opens the constructed logical structure.
    pub root: ObjectId,
    /// Logical byte length of the file.
    pub logical_len: u64,
    /// Work the localized-edit frontier performed.
    ///
    /// Complete construction and a whole-file result hold no unfinished mapping
    /// node, so every field is zero there; a chunked edit reports the nodes it
    /// actually published and the largest frontier it held.
    pub counters: crate::file::edit::EditCounters,
}

/// Recorded representation of an already-stored file root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileContent {
    /// The root is a whole-file object of this logical length.
    WholeFile {
        /// Logical byte length.
        logical_len: u64,
    },
    /// The root is a chunked file state.
    Chunked(FileState),
}

impl FileContent {
    /// Logical byte length of the file.
    pub const fn logical_len(self) -> u64 {
        match self {
            Self::WholeFile { logical_len } => logical_len,
            Self::Chunked(state) => state.logical_len,
        }
    }
}

/// Borrows the payload of a whole-file canonical object, if it is one.
pub fn whole_file_payload(canonical: &[u8]) -> ContentResult<Option<&[u8]>> {
    let value = crate::object::decode_bytes_object(canonical)?;
    if !value.starts_with(WHOLE_MAGIC) {
        return Ok(None);
    }
    if value.get(8..10) != Some(&WHOLE_VERSION.to_be_bytes()) {
        return Err(ContentError::InvalidRecord("whole-file framing"));
    }
    Ok(Some(&value[WHOLE_VALUE_HEADER..]))
}

/// Encodes the canonical object of a whole-file payload.
///
/// The value is assembled in its own allocation and the canonical envelope is a
/// second one, so an accepted payload of `n` bytes peaks at roughly `2n` plus the
/// framing, as `content-io.md`'s memory ledger records. The single-allocation cut
/// is **not** implemented here; this comment says what the code does.
pub fn encode_whole_file(
    capacities: &ConstructionCapacities,
    bytes: &[u8],
) -> ContentResult<Vec<u8>> {
    // Complete construction still builds the value in its own allocation and
    // `encode_bytes_object` allocates the canonical object around it, so this
    // route's peak is roughly twice the value plus its framing. The whole-file
    // *edit* route does not use this function: it assembles into the canonical
    // object's own pre-sized allocation
    // ([`begin_whole_file_object`] + the assembled payload), which is the cut this
    // comment used to claim for construction. The memory ledger states that
    // figure; the comment no longer claims otherwise.
    if bytes.is_empty() || bytes.len() > capacities.whole_file_raw_limit {
        return Err(ContentError::BoundedCapacityExceeded {
            what: "construction.whole_file",
            limit: capacities.whole_file_raw_limit as u64,
            actual: bytes.len() as u64,
        });
    }
    let mut value = Vec::with_capacity(WHOLE_VALUE_HEADER + bytes.len());
    value.extend_from_slice(WHOLE_MAGIC);
    value.extend_from_slice(&WHOLE_VERSION.to_be_bytes());
    value.extend_from_slice(bytes);
    let canonical = encode_bytes_object(&value)?;
    if canonical.len() > capacities.whole_file_canonical_limit {
        return Err(ContentError::BoundedCapacityExceeded {
            what: "construction.whole_file_canonical",
            limit: capacities.whole_file_canonical_limit as u64,
            actual: canonical.len() as u64,
        });
    }
    Ok(canonical)
}

/// Starts a whole-file canonical object with room for `payload_len` payload bytes.
///
/// The returned buffer is the canonical object: its envelope and value header are
/// written, its payload area is reserved exactly, and the caller appends exactly
/// `payload_len` bytes to it. That is one allocation for the whole object instead
/// of a payload, a value and a canonical copy, and the reserve is exact, so no
/// append can reallocate. Both capacity checks run before anything is written, so
/// an over-large object is refused with the same error the complete encoder
/// raises.
pub fn begin_whole_file_object(
    capacities: &ConstructionCapacities,
    payload_len: u64,
) -> ContentResult<Vec<u8>> {
    if payload_len == 0 || payload_len > capacities.whole_file_raw_limit as u64 {
        return Err(ContentError::BoundedCapacityExceeded {
            what: "construction.whole_file",
            limit: capacities.whole_file_raw_limit as u64,
            actual: payload_len,
        });
    }
    let payload_len = usize::try_from(payload_len).map_err(|_| ContentError::LengthOverflow)?;
    let value_len = payload_len
        .checked_add(WHOLE_VALUE_HEADER)
        .ok_or(ContentError::LengthOverflow)?;
    let total = crate::object::canonical_len(value_len)?;
    if total > capacities.whole_file_canonical_limit {
        return Err(ContentError::BoundedCapacityExceeded {
            what: "construction.whole_file_canonical",
            limit: capacities.whole_file_canonical_limit as u64,
            actual: total as u64,
        });
    }
    let mut canonical: Vec<u8> = Vec::new();
    canonical
        .try_reserve_exact(total)
        .map_err(|_| ContentError::BoundedCapacityExceeded {
            what: "construction.whole_file_canonical",
            limit: capacities.whole_file_canonical_limit as u64,
            actual: total as u64,
        })?;
    canonical.extend_from_slice(&crate::object::OBJECT_MAGIC);
    canonical.push(crate::object::BYTES_KIND);
    let value_len_u32 = u32::try_from(value_len).map_err(|_| ContentError::LengthOverflow)?;
    let payload_len_u32 = u32::try_from(value_len + crate::object::VALUE_LEN_BYTES)
        .map_err(|_| ContentError::LengthOverflow)?;
    canonical.extend_from_slice(&payload_len_u32.to_be_bytes());
    canonical.extend_from_slice(&value_len_u32.to_be_bytes());
    canonical.extend_from_slice(WHOLE_MAGIC);
    canonical.extend_from_slice(&WHOLE_VERSION.to_be_bytes());
    debug_assert_eq!(canonical.len(), total - payload_len);
    Ok(canonical)
}

/// Encodes a whole-file canonical object from raw payload bytes.
///
/// This is the envelope-only form used when a stored FULL record is
/// reconstructed: the payload length is already validated by the caller.
pub fn encode_whole_file_payload(raw: &[u8]) -> ContentResult<Vec<u8>> {
    let mut value = Vec::with_capacity(WHOLE_VALUE_HEADER + raw.len());
    value.extend_from_slice(WHOLE_MAGIC);
    value.extend_from_slice(&WHOLE_VERSION.to_be_bytes());
    value.extend_from_slice(raw);
    encode_bytes_object(&value)
}

/// Inspects an authenticated file root.
pub fn inspect(
    reader: &dyn AuthenticatedObjects,
    root: ObjectId,
    scope: TimingScope<'_>,
) -> ContentResult<FileContent> {
    scope.run(|inspect| {
        let canonical = inspect
            .child("content.inspect")
            .run(|_| reader.read_canonical(root))?;
        classify(&canonical)
    })
}

/// Classifies canonical file-root bytes into a whole-file or chunked root.
pub fn classify(canonical: &[u8]) -> ContentResult<FileContent> {
    match whole_file_payload(canonical)? {
        Some(bytes) => Ok(FileContent::WholeFile {
            logical_len: bytes.len() as u64,
        }),
        None => Ok(FileContent::Chunked(mapping::decode_file_state(canonical)?)),
    }
}

/// Constructs a complete file from a known byte slice.
pub fn construct_bytes(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    bytes: &[u8],
    consumer: &mut dyn FinalizedConsumer,
    scope: TimingScope<'_>,
) -> ContentResult<ConstructedFile> {
    construct_bytes_with_predecessor(policy, capacities, bytes, None, consumer, scope)
}

/// Constructs a complete file from a known byte slice, offering a stored earlier
/// version as the positional delta-base correspondence for a chunked result.
///
/// `base` is the earlier version's stored root together with the provider that
/// serves it. It is consulted only when the result is chunked: a whole-file result
/// is offered no chunk base, and a base that is itself a whole-file object offers
/// nothing, because admitting a base across the representation transition is the
/// deferred cross-role half and is not part of this design. With no base this is
/// byte for byte `construct_bytes`.
pub fn construct_bytes_with_predecessor(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    bytes: &[u8],
    base: Option<PredecessorBase<'_>>,
    consumer: &mut dyn FinalizedConsumer,
    scope: TimingScope<'_>,
) -> ContentResult<ConstructedFile> {
    // A policy this profile does not support is refused before any work: the
    // entry point is where a caller learns its inputs are unusable, not the first
    // object that happens to depend on the unsupported field.
    policy.validated()?;
    scope.run(|construct| construct_bytes_in(policy, capacities, bytes, base, consumer, construct))
}

fn construct_bytes_in(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    bytes: &[u8],
    base: Option<PredecessorBase<'_>>,
    consumer: &mut dyn FinalizedConsumer,
    construct: &TimingScope<'_, Active>,
) -> ContentResult<ConstructedFile> {
    match policy.representation(bytes.len() as u64) {
        Representation::WholeFile => {
            let canonical = construct
                .child("content.encode")
                .run(|_| encode_whole_file(capacities, bytes))?;
            let object = construct
                .child("content.identify")
                .run(|_| FinalizedObject::new(ObjectRole::WholeFile, canonical))?;
            let root = object.id();
            construct
                .child("content.emit")
                .run(|_| consumer.accept(object))?;
            Ok(ConstructedFile {
                root,
                logical_len: bytes.len() as u64,
                counters: crate::file::edit::EditCounters::default(),
            })
        }
        Representation::Empty | Representation::Chunked => {
            construct_chunked(capacities, bytes, base, consumer, construct)
        }
    }
}

/// Constructs a complete file from a streaming source of unknown length.
///
/// The frozen cutoff bounds the threshold probe: at most `cutoff` bytes are
/// buffered before the scanner takes over. The probe is an ordinary bounded read
/// of the stable source, not a retry, and a short read that reaches end of input
/// below the cutoff is a definitive result.
pub fn construct_stream<R: Read>(
    policy: ConstructionPolicy,
    capacities: &ConstructionCapacities,
    mut source: R,
    consumer: &mut dyn FinalizedConsumer,
    scope: TimingScope<'_>,
) -> ContentResult<ConstructedFile> {
    policy.validated()?;
    scope.run(|construct| {
        let cutoff = policy.small_file_threshold_bytes();
        let mut prefix = Vec::new();
        prefix.try_reserve_exact(cutoff as usize).map_err(|_| {
            ContentError::BoundedCapacityExceeded {
                what: "construction.threshold_probe",
                limit: cutoff,
                actual: cutoff,
            }
        })?;
        construct
            .child("content.probe")
            .run(|_| read_at_most(&mut source, cutoff, &mut prefix))?;
        if (prefix.len() as u64) < cutoff {
            return construct_bytes_in(policy, capacities, &prefix, None, consumer, construct);
        }
        construct_chunked(
            capacities,
            std::io::Cursor::new(prefix).chain(source),
            None,
            consumer,
            construct,
        )
    })
}

fn read_at_most<R: Read>(source: &mut R, limit: u64, prefix: &mut Vec<u8>) -> ContentResult<()> {
    let mut probe = source.take(limit);
    probe.read_to_end(prefix).map_err(|_| ContentError::Io)?;
    Ok(())
}

fn construct_chunked<R: Read>(
    capacities: &ConstructionCapacities,
    source: R,
    base: Option<PredecessorBase<'_>>,
    consumer: &mut dyn FinalizedConsumer,
    scope: &TimingScope<'_, Active>,
) -> ContentResult<ConstructedFile> {
    let build = scope.child("content.chunk").run(|chunk| {
        // Opening the correspondence is one read of the base root, and it is the
        // read that declines a base which is not chunked. No base is opened when
        // none was offered, so a base-less construction performs no read at all.
        let mut cursor = match base {
            Some(base) => mapping::PredecessorCursor::open(base, chunk)?,
            None => None,
        };
        mapping::build_streaming_with_predecessor(capacities, source, cursor.as_mut(), consumer)
    })?;
    let mapping_root = match build.root {
        Some(root) => root,
        None => {
            let mut empty = MappingBuild::default();
            scope
                .child("content.emit")
                .run(|_| mapping::emit_empty_leaf(consumer, &mut empty))?
        }
    };
    let root = scope
        .child("content.emit")
        .run(|_| mapping::emit_file_state(consumer, mapping_root))?;
    Ok(ConstructedFile {
        root,
        logical_len: mapping_root.bytes,
        counters: crate::file::edit::EditCounters::default(),
    })
}
