//! Pack grammars, framing lanes and exact fit arithmetic.
//!
//! A pack is a control area, a **reserved directory region** and group bodies.
//! Five framings are implemented: v9 ordinary groups, v10 native chunk records,
//! v11 compact whole-file records, v12 pooled metadata groups and v13 singleton
//! packs. A lane is a framing, not a worker or a second store, and existing
//! locators stay stable when a pack's directory grows.
//!
//! **The directory region is reserved at the lane's own width**, so a body never
//! moves when a group is appended and an append can write only the bytes it adds.
//! The region is `directory_entry_len(lane) * group_count_limit(lane)` bytes
//! immediately after the control area; the entries actually in use are the first
//! `group_count` of them and the remainder stays zero. Before v9 the directory
//! grew into the pack's front, so every append shifted every body behind it and
//! rewrote the whole BLOB: measured at 7.59x write amplification over 1,250 packs
//! in `#219` (2,292,865,337 bytes handed to the pager for 302,023,232 persisted).
//!
//! The control area carries the pack's own assembled length (`USED_OFFSET`), so a
//! pack row may be allocated with spare capacity and still declare exactly how
//! many of its bytes are a pack. That is what lets the write path use incremental
//! BLOB I/O: a small column elsewhere on the row would be cheaper to update, but
//! any `UPDATE` of a row holding a 256 KiB BLOB rewrites the whole BLOB, which is
//! the cost this format exists to remove.
//!
//! Versions 1, 2, 4, 6 and 7 are the **pre-v9 framings**: a directory that grows
//! at the front and no declared length. They belong to the schema-8 Store, which
//! is refused at open, and the reader refuses them here as well rather than
//! guessing a layout: `parse_header` maps a version to a lane and refuses every
//! version it does not implement, exactly as it refuses 3 and 5, which belong to
//! other profiles. No reader trial-decodes.

use layerfs_content::ObjectRole;

use crate::error::{StorageError, StorageResult};
use crate::policy::{
    GROUP_COUNT_LIMIT, GROUP_LIMIT, METADATA_GROUP_LIMIT, PACK_LIMIT, RECORD_COUNT_LIMIT,
    SINGLETON_PACK_LIMIT,
};

/// Pack magic shared by every implemented framing.
pub const PACK_MAGIC: [u8; 8] = *b"LFPACK\0\0";
/// Control-area width in bytes.
///
/// `[magic 8][version 4][group count 4][assembled length 4][reserved 4]`. The
/// reserved word is written zero and validated zero, so a framing that later
/// needs a fifth field can take it without moving the directory.
pub const HEADER_LEN: usize = 24;
/// Offset of the group count inside the control area.
pub const GROUP_COUNT_OFFSET: usize = 12;
/// Offset of the pack's assembled length inside the control area.
pub const USED_OFFSET: usize = 16;
/// Directory entry width of the ordinary and native framings.
pub const DIRECTORY_ENTRY_LEN: usize = 16;
/// Directory entry width of the compact whole-file framing.
pub const WHOLE_FILE_ENTRY_LEN: usize = 4;
/// Framing bytes the compact whole-file assembly drops from each record.
pub const WHOLE_FILE_COMPACT_DROP: usize = 8;

/// Ordinary framing version: canonical objects in multi-record groups.
pub const VERSION_ORDINARY: u32 = 9;
/// Native framing version: one chunk record per group entry.
///
/// Still read, and no longer written: a native pack may carry a payload record
/// stored verbatim, which this version predates. See [`VERSION_NATIVE_STORED`].
pub const VERSION_NATIVE: u32 = 10;
/// Compact whole-file framing version.
///
/// Still read, and no longer written, for the same reason as [`VERSION_NATIVE`].
pub const VERSION_WHOLE_FILE: u32 = 11;
/// Pooled physical-metadata framing version.
pub const VERSION_POOLED_METADATA: u32 = 12;
/// Singleton framing version: one oversized record in one pack.
///
/// Still read, and no longer written, for the same reason as [`VERSION_NATIVE`].
pub const VERSION_SINGLETON: u32 = 13;
/// Native framing that may carry a payload stored verbatim.
///
/// The three payload lanes moved because a record's grammar gained a tag value:
/// a reader that predates it must refuse the pack by version rather than meet an
/// unknown tag in the middle of a decode, and a reader that has it reads both
/// versions of every payload lane. The ordinary and pooled lanes carry no payload
/// frame, so their grammar is unchanged and their versions stay where they were.
pub const VERSION_NATIVE_STORED: u32 = 15;
/// Compact whole-file framing that may carry a payload stored verbatim.
pub const VERSION_WHOLE_FILE_STORED: u32 = 14;
/// Singleton framing that may carry a payload stored verbatim.
pub const VERSION_SINGLETON_STORED: u32 = 16;

/// One physical framing lane.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum PackLane {
    /// Mapping nodes and file states.
    Ordinary,
    /// Chunk payload records.
    Native,
    /// Whole-file payload records.
    WholeFile,
    /// Pooled physical metadata value-group records.
    PooledMetadata,
    /// One oversized record alone in its pack.
    Singleton,
}

impl PackLane {
    /// Every lane, in deterministic order.
    pub const ALL: [Self; 5] = [
        Self::Ordinary,
        Self::Native,
        Self::WholeFile,
        Self::PooledMetadata,
        Self::Singleton,
    ];

    /// Framing version written into the pack header.
    ///
    /// The payload lanes write the version whose grammar they now use. The
    /// versions they no longer write are read by [`parse_header`] all the same,
    /// which is what keeps every pack this Store has already written readable.
    pub const fn version(self) -> u32 {
        match self {
            Self::Ordinary => VERSION_ORDINARY,
            Self::Native => VERSION_NATIVE_STORED,
            Self::WholeFile => VERSION_WHOLE_FILE_STORED,
            Self::PooledMetadata => VERSION_POOLED_METADATA,
            Self::Singleton => VERSION_SINGLETON_STORED,
        }
    }

    /// Stable index used for per-lane owner state.
    pub const fn index(self) -> usize {
        match self {
            Self::Ordinary => 0,
            Self::Native => 1,
            Self::WholeFile => 2,
            Self::PooledMetadata => 3,
            Self::Singleton => 4,
        }
    }

    /// Lane a logical role is stored in.
    pub const fn for_role(role: ObjectRole) -> Self {
        match role {
            ObjectRole::WholeFile => Self::WholeFile,
            ObjectRole::Chunk => Self::Native,
            ObjectRole::ExtentLeaf
            | ObjectRole::ExtentBranch
            | ObjectRole::FileState
            | ObjectRole::InodeLeaf
            | ObjectRole::DirectoryLeaf
            | ObjectRole::DirectoryBranch
            | ObjectRole::InodeBranch
            | ObjectRole::FilesystemRoot
            | ObjectRole::AttributeLeaf
            | ObjectRole::AttributeBranch
            | ObjectRole::Symlink => Self::Ordinary,
        }
    }

    /// Largest assembled pack of this lane.
    pub const fn pack_limit(self) -> usize {
        match self {
            Self::Singleton => SINGLETON_PACK_LIMIT,
            Self::Ordinary | Self::Native | Self::WholeFile | Self::PooledMetadata => PACK_LIMIT,
        }
    }

    /// True when the lane's directory stores only group starts.
    pub const fn directory_is_starts_only(self) -> bool {
        matches!(self, Self::WholeFile)
    }

    /// Largest decoded group body of this lane.
    pub const fn body_limit(self) -> usize {
        match self {
            Self::PooledMetadata => METADATA_GROUP_LIMIT,
            Self::Singleton => SINGLETON_PACK_LIMIT,
            Self::Ordinary | Self::Native | Self::WholeFile => GROUP_LIMIT,
        }
    }

    /// Largest group count of this lane.
    pub const fn group_count_limit(self) -> usize {
        match self {
            Self::Singleton => 1,
            Self::Ordinary | Self::Native | Self::WholeFile | Self::PooledMetadata => {
                GROUP_COUNT_LIMIT
            }
        }
    }
}

/// Group body codec recorded in the ordinary directory entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GroupCodec {
    /// Group body stored exactly as framed.
    Raw,
    /// Group body stored as one Zstandard frame.
    Zstandard,
}

/// One framed group ready for placement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedGroup {
    /// Bytes this lane assembles into the pack. The compact whole-file lane
    /// stores the record itself and drops its framing at assembly time.
    pub bytes: Vec<u8>,
    /// Body length before compression.
    pub decoded_length: usize,
    /// Records in this group.
    pub records: usize,
    /// Body codec.
    pub codec: GroupCodec,
}

impl EncodedGroup {
    /// Body bytes this group contributes to an assembled pack of `lane`.
    ///
    /// The per-group directory entry is charged by [`assembled_length`] and is
    /// deliberately not part of this value.
    pub fn body_size(&self, lane: PackLane) -> StorageResult<usize> {
        match lane {
            PackLane::WholeFile => self
                .bytes
                .len()
                .checked_sub(WHOLE_FILE_COMPACT_DROP)
                .ok_or(StorageError::Integrity("compact record width")),
            PackLane::Ordinary
            | PackLane::Native
            | PackLane::PooledMetadata
            | PackLane::Singleton => Ok(self.bytes.len()),
        }
    }
}

/// Bytes `lane` reserves for its group directory.
///
/// The region is fixed at the lane's own width and group bound, so a body's
/// offset never depends on how many groups precede it. An append writes one entry
/// into this region and its body after it; nothing else moves.
pub const fn directory_capacity(lane: PackLane) -> usize {
    directory_entry_len(lane) * lane.group_count_limit()
}

/// Offset of the first group body inside a pack of `lane`.
pub const fn body_area_offset(lane: PackLane) -> usize {
    HEADER_LEN + directory_capacity(lane)
}

/// Bytes a pack row allocates for a pack of `lane` that assembles to `used`.
///
/// Every lane but Singleton allocates its full pack limit, because the pack is
/// expected to be appended to and a BLOB's size cannot be changed in place:
/// growing it would be an `UPDATE`, which rewrites the whole row. A Singleton
/// pack holds one record for its whole life (`append_fits` refuses a second
/// group), so it allocates exactly what it uses and wastes nothing.
pub const fn pack_capacity(lane: PackLane, used: usize) -> usize {
    match lane {
        PackLane::Singleton => used,
        PackLane::Ordinary | PackLane::Native | PackLane::WholeFile | PackLane::PooledMetadata => {
            lane.pack_limit()
        }
    }
}

/// Bytes one group's directory entry costs in `lane`.
pub const fn directory_entry_len(lane: PackLane) -> usize {
    match lane {
        PackLane::WholeFile => WHOLE_FILE_ENTRY_LEN,
        PackLane::Ordinary | PackLane::Native | PackLane::PooledMetadata | PackLane::Singleton => {
            DIRECTORY_ENTRY_LEN
        }
    }
}

/// True when a pack that already assembles to `assembled` still fits `group`.
///
/// `assembled` is the caller's **running total** for the open pack - header, one
/// directory entry per existing group and every body - so the decision costs one
/// `body_size` and no pass over the groups. Re-summing them per candidate was
/// O(g²) over a placement wave, g bounded only by the lane's group count.
///
/// The decision is exact and identical to comparing the assembled length of the
/// existing groups plus `group` with the lane's pack limit; the caller owns the
/// total because it is the state that changes as groups land, and
/// `assembled_length` remains the canonical predicate the running total is
/// checked against.
pub fn append_fits(
    lane: PackLane,
    assembled: usize,
    group_count: usize,
    group: &EncodedGroup,
) -> StorageResult<bool> {
    if group_count == 0 || group_count >= lane.group_count_limit() {
        return Ok(false);
    }
    // The group's directory entry is already inside `assembled`: the region was
    // reserved when the pack was placed. Only its body moves the total.
    let body = group.body_size(lane)?;
    let total = assembled
        .checked_add(body)
        .ok_or(StorageError::Integrity("pack size"))?;
    Ok(total <= lane.pack_limit())
}

/// Exact assembled length of `groups` under `lane`, without allocating the pack.
pub fn assembled_length(lane: PackLane, groups: &[EncodedGroup]) -> StorageResult<usize> {
    if groups.is_empty() || groups.len() > lane.group_count_limit() {
        return Err(StorageError::Integrity("pack group count"));
    }
    // The whole reserved region counts, not the entries in use: the region is
    // allocated whether or not this pack fills it, and the fit decision has to
    // measure the pack that will exist rather than the pack that would exist if
    // the directory had been sized to the groups.
    let mut total = body_area_offset(lane);
    for group in groups {
        total = total
            .checked_add(group.body_size(lane)?)
            .ok_or(StorageError::Integrity("pack size"))?;
    }
    Ok(total)
}

/// Reads the assembled length a pack's own control area declares.
///
/// A pack row may hold more bytes than the pack uses - that is what the reserved
/// capacity is for - so the declared length is what a reader trusts, and the
/// caller truncates to it before the pack is parsed. A declared length outside
/// `[HEADER_LEN, bytes.len()]` is an integrity failure, never a short read.
pub fn declared_length(bytes: &[u8]) -> StorageResult<usize> {
    let field = bytes
        .get(USED_OFFSET..USED_OFFSET + 4)
        .ok_or(StorageError::Integrity("pack header"))?;
    let used = u32::from_le_bytes(
        field
            .try_into()
            .map_err(|_| StorageError::Integrity("pack length"))?,
    ) as usize;
    if used < HEADER_LEN || used > bytes.len() {
        return Err(StorageError::Integrity("pack length"));
    }
    Ok(used)
}

/// Parsed pack control area.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackHeader {
    /// Framing lane the pack was written in.
    pub lane: PackLane,
    /// Groups in the directory.
    pub group_count: usize,
    /// Assembled length the control area declares, which equals `bytes.len()`.
    pub used: usize,
}

/// Reads and validates the pack header before any body is touched.
pub fn parse_header(bytes: &[u8]) -> StorageResult<PackHeader> {
    let header = bytes
        .get(..HEADER_LEN)
        .ok_or(StorageError::Integrity("pack header"))?;
    if header[..8] != PACK_MAGIC {
        return Err(StorageError::Integrity("pack magic"));
    }
    let version = u32::from_le_bytes(
        header[8..12]
            .try_into()
            .map_err(|_| StorageError::Integrity("pack version"))?,
    );
    let lane = match version {
        VERSION_ORDINARY => PackLane::Ordinary,
        VERSION_NATIVE | VERSION_NATIVE_STORED => PackLane::Native,
        VERSION_WHOLE_FILE | VERSION_WHOLE_FILE_STORED => PackLane::WholeFile,
        VERSION_POOLED_METADATA => PackLane::PooledMetadata,
        VERSION_SINGLETON | VERSION_SINGLETON_STORED => PackLane::Singleton,
        _ => {
            return Err(StorageError::UnsupportedPolicy {
                field: "pack framing version",
            })
        }
    };
    let group_count = u32::from_le_bytes(
        header[GROUP_COUNT_OFFSET..GROUP_COUNT_OFFSET + 4]
            .try_into()
            .map_err(|_| StorageError::Integrity("pack group count"))?,
    ) as usize;
    if !(1..=GROUP_COUNT_LIMIT).contains(&group_count) {
        return Err(StorageError::Integrity("pack group count"));
    }
    if header[20..24] != [0, 0, 0, 0] {
        return Err(StorageError::Integrity("pack reserved field"));
    }
    let used = declared_length(bytes)?;
    if used != bytes.len() {
        return Err(StorageError::Integrity("pack length"));
    }
    if used < body_area_offset(lane) + 1 {
        return Err(StorageError::Integrity("pack directory width"));
    }
    if bytes.len() > lane.pack_limit() {
        return Err(StorageError::Integrity("pack length"));
    }
    if lane == PackLane::Singleton && group_count != 1 {
        return Err(StorageError::Integrity("singleton pack group count"));
    }
    if lane == PackLane::Native && !matches!(version, VERSION_NATIVE | VERSION_NATIVE_STORED) {
        return Err(StorageError::Integrity("native framing version"));
    }
    Ok(PackHeader {
        lane,
        group_count,
        used,
    })
}

/// Location of one group body inside a pack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GroupView {
    /// Body byte range.
    pub start: usize,
    /// Exclusive end of the body range.
    pub end: usize,
    /// Body length before decompression.
    pub decoded_length: usize,
    /// Body codec.
    pub codec: GroupCodec,
}

/// Resolves one group body, validating the whole bounded directory first.
pub fn group_view(bytes: &[u8], header: PackHeader, group: usize) -> StorageResult<GroupView> {
    if group >= header.group_count {
        return Err(StorageError::Integrity("group ordinal"));
    }
    match header.lane {
        PackLane::WholeFile => whole_file_group_view(bytes, header, group),
        PackLane::Ordinary | PackLane::Native | PackLane::PooledMetadata | PackLane::Singleton => {
            ordinary_group_view(bytes, header, group)
        }
    }
}

fn ordinary_group_view(bytes: &[u8], header: PackHeader, group: usize) -> StorageResult<GroupView> {
    let mut offset = body_area_offset(header.lane);
    let mut selected = None;
    for index in 0..header.group_count {
        let start = HEADER_LEN + DIRECTORY_ENTRY_LEN * index;
        let entry = bytes
            .get(start..start + DIRECTORY_ENTRY_LEN)
            .ok_or(StorageError::Integrity("group directory"))?;
        let body_start = u32::from_le_bytes(
            entry[0..4]
                .try_into()
                .map_err(|_| StorageError::Integrity("group start"))?,
        ) as usize;
        let encoded = u32::from_le_bytes(
            entry[4..8]
                .try_into()
                .map_err(|_| StorageError::Integrity("group encoded length"))?,
        ) as usize;
        let decoded = u32::from_le_bytes(
            entry[8..12]
                .try_into()
                .map_err(|_| StorageError::Integrity("group decoded length"))?,
        ) as usize;
        if entry[12..16] != [0, 0, 0, 0] && entry[13..16] != [0, 0, 0] {
            return Err(StorageError::Integrity("group directory flags"));
        }
        if body_start != offset {
            return Err(StorageError::Integrity("group directory continuity"));
        }
        let end = body_start
            .checked_add(encoded)
            .ok_or(StorageError::Integrity("group end"))?;
        if encoded == 0 || decoded == 0 || end > bytes.len() {
            return Err(StorageError::Integrity("group extent"));
        }
        let codec = match entry[12] {
            0 if encoded == decoded => GroupCodec::Raw,
            1 if encoded <= decoded && decoded <= header.lane.body_limit() => GroupCodec::Zstandard,
            _ => return Err(StorageError::Integrity("group codec")),
        };
        if matches!(header.lane, PackLane::Native | PackLane::Singleton) && codec != GroupCodec::Raw
        {
            return Err(StorageError::Integrity("lane group codec"));
        }
        if decoded > header.lane.body_limit() {
            return Err(StorageError::Integrity("group decoded length"));
        }
        if header.lane == PackLane::Singleton && header.group_count != 1 {
            return Err(StorageError::Integrity("singleton pack group count"));
        }
        if index == group {
            selected = Some(GroupView {
                start: body_start,
                end,
                decoded_length: decoded,
                codec,
            });
        }
        offset = end;
    }
    if offset != bytes.len() {
        return Err(StorageError::Integrity("pack trailing bytes"));
    }
    selected.ok_or(StorageError::Integrity("group ordinal"))
}

fn whole_file_group_view(
    bytes: &[u8],
    header: PackHeader,
    group: usize,
) -> StorageResult<GroupView> {
    if bytes.len() > header.lane.pack_limit() {
        return Err(StorageError::Integrity("compact pack length"));
    }
    let mut start = body_area_offset(header.lane);
    let mut selected = None;
    for index in 0..header.group_count {
        let entry = bytes
            .get(
                HEADER_LEN + WHOLE_FILE_ENTRY_LEN * index
                    ..HEADER_LEN + WHOLE_FILE_ENTRY_LEN * (index + 1),
            )
            .ok_or(StorageError::Integrity("compact directory"))?;
        let declared = u32::from_le_bytes(
            entry
                .try_into()
                .map_err(|_| StorageError::Integrity("compact start"))?,
        ) as usize;
        if declared != start {
            return Err(StorageError::Integrity("compact directory continuity"));
        }
        let end = if index + 1 == header.group_count {
            bytes.len()
        } else {
            let next = bytes
                .get(
                    HEADER_LEN + WHOLE_FILE_ENTRY_LEN * (index + 1)
                        ..HEADER_LEN + WHOLE_FILE_ENTRY_LEN * (index + 2),
                )
                .ok_or(StorageError::Integrity("compact directory"))?;
            u32::from_le_bytes(
                next.try_into()
                    .map_err(|_| StorageError::Integrity("compact start"))?,
            ) as usize
        };
        let size = end
            .checked_sub(start)
            .ok_or(StorageError::Integrity("compact group range"))?;
        if size < 2 || end > bytes.len() {
            return Err(StorageError::Integrity("compact group extent"));
        }
        if index == group {
            selected = Some(GroupView {
                start,
                end,
                decoded_length: size,
                codec: GroupCodec::Raw,
            });
        }
        start = end;
    }
    selected.ok_or(StorageError::Integrity("group ordinal"))
}

/// Index of the record whose end offset equals `offset` in a group directory.
pub fn record_range(
    count: usize,
    ends: &[u8],
    group_length: usize,
    ordinal: usize,
) -> StorageResult<(usize, usize)> {
    if !(1..=RECORD_COUNT_LIMIT).contains(&count)
        || ordinal >= count
        || group_length > SINGLETON_PACK_LIMIT
        || ends.len() != 4 * count
    {
        return Err(StorageError::Integrity("group record directory"));
    }
    let area_start = 4 + ends.len();
    let area_length = group_length
        .checked_sub(area_start)
        .ok_or(StorageError::Integrity("group record area"))?;
    let mut previous = 0;
    let mut selected = (0, 0);
    for index in 0..count {
        let end = u32::from_le_bytes(
            ends[4 * index..4 * index + 4]
                .try_into()
                .map_err(|_| StorageError::Integrity("record end"))?,
        ) as usize;
        if end <= previous || end > area_length {
            return Err(StorageError::Integrity("record end ordering"));
        }
        if index == ordinal {
            selected = (area_start + previous, area_start + end);
        }
        previous = end;
    }
    if previous != area_length {
        return Err(StorageError::Integrity("record area length"));
    }
    Ok(selected)
}
