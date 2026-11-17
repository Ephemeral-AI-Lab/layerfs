//! Rewriting one sealed pack so that it holds only the records a caller keeps.
//!
//! A Store compaction keeps a pack when any of its records is still reached,
//! and until now copied it whole, so the records nothing reaches stayed in the
//! Store. [`retain_records`] builds the pack again from the kept records only:
//! a group that keeps every record is carried over byte for byte, a group that
//! keeps none is left out, and a group that keeps some is framed again through
//! the same group builders a Save uses ([`build_group`], and for the compact
//! whole-file lane the group framing its records already carry). A compressed
//! ordinary group is decoded and compressed again by the pinned group codec.
//!
//! A record is copied exactly as stored. A delta record names its base by
//! identity, never by position, so moving a record changes no other record;
//! the caller keeps a base by naming it in the kept set like any other record.
//! Group and record ordinals do change: [`Repacked::groups`] maps each old
//! group to its new number, and a kept record's new ordinal is its position in
//! the caller's kept list for that group.
//!
//! The result is checked before it is returned: its framing is validated like
//! any stored pack's ([`validate_records`]), it holds exactly the kept records,
//! and each one is byte-identical to the record it was copied from.

use std::borrow::Cow;

use crate::encoding::codec::{CompressionWorkspace, DecompressionWorkspace};
use crate::encoding::delta::edge::RecordDirectory;
use crate::encoding::framed_record;
use crate::error::{StorageError, StorageResult};
use crate::pack::assemble::{
    assemble_consuming, build_group, frame_group_bounded, WHOLE_FILE_GROUP_BODY_LIMIT,
};
use crate::pack::audit::validate_records;
use crate::pack::layout::{group_view, parse_header, EncodedGroup, GroupCodec, PackLane};

/// One pack rebuilt from the records its caller kept.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Repacked {
    /// The complete new pack, sealed in its lane's current framing version.
    pub body: Vec<u8>,
    /// For each group of the old pack, its number in the new pack; `None` for
    /// a group that kept no record.
    pub groups: Vec<Option<usize>>,
    /// Records the new pack holds.
    pub records: u64,
}

/// One kept group as the old pack stored it, held for the final comparison.
struct Kept<'a> {
    /// The decoded group body of the old pack.
    old: Cow<'a, [u8]>,
    /// Its record directory.
    directory: RecordDirectory,
    /// Whether every record was kept, so the stored bytes were carried over.
    whole: bool,
}

/// Rebuilds `body` with only the records `keep` names.
///
/// `keep[g]` lists the record ordinals of group `g` to keep, strictly
/// ascending; an empty list drops the group. Returns `None` when the pack
/// already holds exactly those records, and for the singleton lane, whose one
/// record is either kept or not there at all.
///
/// A kept set that names a record the pack does not hold, repeats one, keeps
/// nothing at all or does not cover every group is an integrity failure: the
/// caller's locators disagree with the pack, and nothing is rebuilt from them.
pub fn retain_records(
    body: &[u8],
    keep: &[Vec<usize>],
    compression: &mut CompressionWorkspace,
    decompression: &mut DecompressionWorkspace,
) -> StorageResult<Option<Repacked>> {
    let header = parse_header(body)?;
    if keep.len() != header.group_count {
        return Err(StorageError::Integrity("repack group count"));
    }
    if header.lane == PackLane::Singleton {
        return Ok(None);
    }
    let mut groups = Vec::new();
    let mut numbers = Vec::with_capacity(header.group_count);
    let mut kept_groups = Vec::new();
    let mut records = 0_u64;
    let mut changed = false;
    for (number, kept) in keep.iter().enumerate() {
        if kept.is_empty() {
            numbers.push(None);
            changed = true;
            continue;
        }
        let view = group_view(body, header, number)?;
        let stored = body
            .get(view.start..view.end)
            .ok_or(StorageError::Integrity("repack group extent"))?;
        let old: Cow<'_, [u8]> = match view.codec {
            GroupCodec::Raw => Cow::Borrowed(stored),
            GroupCodec::Zstandard => {
                if header.lane != PackLane::Ordinary {
                    return Err(StorageError::Integrity("repack group codec"));
                }
                Cow::Owned(decompression.decompress_group(stored, view.decoded_length)?)
            }
        };
        let directory = RecordDirectory::of(&old)?;
        if kept.windows(2).any(|pair| pair[0] >= pair[1])
            || kept.last().is_some_and(|last| *last >= directory.count())
        {
            return Err(StorageError::Integrity("repack kept record"));
        }
        numbers.push(Some(groups.len()));
        records += kept.len() as u64;
        let whole = kept.len() == directory.count();
        if whole {
            groups.push(EncodedGroup {
                bytes: stored.to_vec(),
                decoded_length: view.decoded_length,
                records: kept.len(),
                codec: view.codec,
            });
        } else {
            changed = true;
            let mut selected = Vec::with_capacity(kept.len());
            for record in kept {
                let (start, end) = directory.range(*record)?;
                selected.push(old[start..end].to_vec());
            }
            groups.push(regroup(header.lane, &selected, compression)?);
        }
        kept_groups.push(Kept {
            old,
            directory,
            whole,
        });
    }
    if !changed {
        return Ok(None);
    }
    if groups.is_empty() {
        return Err(StorageError::Integrity("repack keeps no record"));
    }
    let rebuilt = assemble_consuming(header.lane, groups)?;
    check(&rebuilt, keep, &kept_groups, records, decompression)?;
    Ok(Some(Repacked {
        body: rebuilt,
        groups: numbers,
        records,
    }))
}

/// Frames records that a group of `lane` already stores into a new group.
///
/// The ordinary and chunk lanes store a record exactly as a Save hands it to
/// [`build_group`]. The whole-file lane stores the compact form, which
/// `build_group` derives by dropping two length fields; records read back from
/// a pack are already compact, so they take the group framing directly.
fn regroup(
    lane: PackLane,
    records: &[Vec<u8>],
    compression: &mut CompressionWorkspace,
) -> StorageResult<EncodedGroup> {
    match lane {
        PackLane::Ordinary => build_group(lane, records, Some(compression)),
        PackLane::Chunk => build_group(lane, records, None),
        PackLane::WholeFile => {
            let bytes = frame_group_bounded(records, WHOLE_FILE_GROUP_BODY_LIMIT)?;
            Ok(EncodedGroup {
                decoded_length: bytes.len(),
                bytes,
                records: records.len(),
                codec: GroupCodec::Raw,
            })
        }
        PackLane::Singleton => Err(StorageError::Integrity("repack singleton lane")),
    }
}

/// Proves the rebuilt pack is a well-formed pack holding exactly the kept
/// records, each byte-identical to its source.
fn check(
    rebuilt: &[u8],
    keep: &[Vec<usize>],
    kept_groups: &[Kept<'_>],
    records: u64,
    decompression: &mut DecompressionWorkspace,
) -> StorageResult<()> {
    if validate_records(rebuilt, &mut || true)? != records {
        return Err(StorageError::Integrity("repacked record count"));
    }
    let header = parse_header(rebuilt)?;
    if header.group_count != kept_groups.len() {
        return Err(StorageError::Integrity("repacked group count"));
    }
    let lists = keep.iter().filter(|kept| !kept.is_empty());
    for (number, (kept, source)) in lists.zip(kept_groups).enumerate() {
        let view = group_view(rebuilt, header, number)?;
        let stored = &rebuilt[view.start..view.end];
        let decoded;
        let group: &[u8] = match view.codec {
            GroupCodec::Raw => stored,
            GroupCodec::Zstandard => {
                decoded = decompression.decompress_group(stored, view.decoded_length)?;
                &decoded
            }
        };
        if source.whole {
            if group != &*source.old {
                return Err(StorageError::Integrity("repacked group bytes"));
            }
            continue;
        }
        for (position, record) in kept.iter().enumerate() {
            let (start, end) = source.directory.range(*record)?;
            if framed_record(group, position)? != &source.old[start..end] {
                return Err(StorageError::Integrity("repacked record bytes"));
            }
        }
    }
    Ok(())
}
