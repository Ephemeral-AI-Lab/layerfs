//! Pack grammars, framing lanes and exact fit arithmetic.
//!
//! A pack is a header, a directory and group bodies. Three framings are
//! implemented: v1 ordinary groups, v2 native chunk records and v4 compact
//! whole-file records. A lane is a framing, not a worker or a second store, and
//! existing locators stay stable when a pack's directory grows.

use layerfs_content::ObjectRole;

use crate::error::{StorageError, StorageResult};
use crate::policy::{GROUP_COUNT_LIMIT, GROUP_LIMIT, PACK_LIMIT, RECORD_COUNT_LIMIT};

/// Pack magic shared by every implemented framing.
pub const PACK_MAGIC: [u8; 8] = *b"LFPACK\0\0";
/// Control-area width in bytes.
pub const HEADER_LEN: usize = 16;
/// Directory entry width of the ordinary and native framings.
pub const DIRECTORY_ENTRY_LEN: usize = 16;
/// Directory entry width of the compact whole-file framing.
pub const WHOLE_FILE_ENTRY_LEN: usize = 4;
/// Framing bytes the compact whole-file assembly drops from each record.
pub const WHOLE_FILE_COMPACT_DROP: usize = 8;

/// Ordinary framing version: canonical objects in multi-record groups.
pub const VERSION_ORDINARY: u32 = 1;
/// Native framing version: one chunk record per group entry.
pub const VERSION_NATIVE: u32 = 2;
/// Compact whole-file framing version.
pub const VERSION_WHOLE_FILE: u32 = 4;

/// One physical framing lane.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum PackLane {
    /// Mapping nodes and file states.
    Ordinary,
    /// Chunk payload records.
    Native,
    /// Whole-file payload records.
    WholeFile,
}

impl PackLane {
    /// Every lane, in deterministic order.
    pub const ALL: [Self; 3] = [Self::Ordinary, Self::Native, Self::WholeFile];

    /// Framing version written into the pack header.
    pub const fn version(self) -> u32 {
        match self {
            Self::Ordinary => VERSION_ORDINARY,
            Self::Native => VERSION_NATIVE,
            Self::WholeFile => VERSION_WHOLE_FILE,
        }
    }

    /// Stable index used for per-lane owner state.
    pub const fn index(self) -> usize {
        match self {
            Self::Ordinary => 0,
            Self::Native => 1,
            Self::WholeFile => 2,
        }
    }

    /// Lane a logical role is stored in.
    pub const fn for_role(role: ObjectRole) -> Self {
        match role {
            ObjectRole::WholeFile => Self::WholeFile,
            ObjectRole::Chunk => Self::Native,
            ObjectRole::ExtentLeaf | ObjectRole::ExtentBranch | ObjectRole::FileState => {
                Self::Ordinary
            }
        }
    }

    /// Records one whole-file group may hold.
    pub const fn records_per_group(self) -> usize {
        match self {
            Self::WholeFile => 1,
            Self::Ordinary | Self::Native => RECORD_COUNT_LIMIT,
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
            PackLane::Ordinary | PackLane::Native => Ok(self.bytes.len()),
        }
    }
}

/// Exact assembled length of `groups` under `lane`, without allocating the pack.
pub fn assembled_length(lane: PackLane, groups: &[EncodedGroup]) -> StorageResult<usize> {
    if groups.is_empty() || groups.len() > GROUP_COUNT_LIMIT {
        return Err(StorageError::Integrity("pack group count"));
    }
    let directory = match lane {
        PackLane::WholeFile => WHOLE_FILE_ENTRY_LEN,
        PackLane::Ordinary | PackLane::Native => DIRECTORY_ENTRY_LEN,
    };
    let mut total = HEADER_LEN
        .checked_add(
            directory
                .checked_mul(groups.len())
                .ok_or(StorageError::Integrity("pack directory"))?,
        )
        .ok_or(StorageError::Integrity("pack directory"))?;
    for group in groups {
        total = total
            .checked_add(group.body_size(lane)?)
            .ok_or(StorageError::Integrity("pack size"))?;
    }
    Ok(total)
}

/// True when `groups` fit one pack of `lane` exactly.
pub fn fits(lane: PackLane, groups: &[EncodedGroup]) -> bool {
    assembled_length(lane, groups).is_ok_and(|length| length <= PACK_LIMIT)
}

/// Parsed pack control area.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackHeader {
    /// Framing lane the pack was written in.
    pub lane: PackLane,
    /// Groups in the directory.
    pub group_count: usize,
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
        VERSION_NATIVE => PackLane::Native,
        VERSION_WHOLE_FILE => PackLane::WholeFile,
        _ => {
            return Err(StorageError::UnsupportedPolicy {
                field: "pack framing version",
            })
        }
    };
    let group_count = u32::from_le_bytes(
        header[12..16]
            .try_into()
            .map_err(|_| StorageError::Integrity("pack group count"))?,
    ) as usize;
    if !(1..=GROUP_COUNT_LIMIT).contains(&group_count) {
        return Err(StorageError::Integrity("pack group count"));
    }
    let directory = match lane {
        PackLane::WholeFile => WHOLE_FILE_ENTRY_LEN,
        PackLane::Ordinary | PackLane::Native => DIRECTORY_ENTRY_LEN,
    };
    if bytes.len() < HEADER_LEN + directory * group_count {
        return Err(StorageError::Integrity("pack directory width"));
    }
    if matches!(lane, PackLane::Ordinary | PackLane::Native) && bytes.len() > PACK_LIMIT {
        return Err(StorageError::Integrity("pack length"));
    }
    if lane == PackLane::Native && version != VERSION_NATIVE {
        return Err(StorageError::Integrity("native framing version"));
    }
    Ok(PackHeader { lane, group_count })
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
        PackLane::Ordinary | PackLane::Native => ordinary_group_view(bytes, header, group),
    }
}

fn ordinary_group_view(bytes: &[u8], header: PackHeader, group: usize) -> StorageResult<GroupView> {
    let mut offset = HEADER_LEN + DIRECTORY_ENTRY_LEN * header.group_count;
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
            1 if encoded <= decoded && decoded <= GROUP_LIMIT => GroupCodec::Zstandard,
            _ => return Err(StorageError::Integrity("group codec")),
        };
        if header.lane == PackLane::Native && codec != GroupCodec::Raw {
            return Err(StorageError::Integrity("native group codec"));
        }
        if decoded > GROUP_LIMIT {
            return Err(StorageError::Integrity("group decoded length"));
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
    let directory_end = HEADER_LEN + WHOLE_FILE_ENTRY_LEN * header.group_count;
    if bytes.len() > PACK_LIMIT {
        return Err(StorageError::Integrity("compact pack length"));
    }
    let mut start = directory_end;
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
        || group_length > GROUP_LIMIT
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
