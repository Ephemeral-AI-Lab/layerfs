//! Pack grammars, framing lanes and exact fit arithmetic.
//!
//! A pack is a control area, a **reserved directory region** and group bodies.
//! Five framings are implemented: v9 ordinary groups, v10 chunk chunk records,
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
//! **Sized directories (v23-v25, #352 F-027 item 1).** Saves seal every pack
//! whole and nothing appends to a published pack, so the reserved region only
//! stored zeros. The sized versions write exactly `group_count` entries; the
//! reserved versions stay readable at their own offsets. Placement still charges
//! the reserved width when it decides whether a group fits, which is safe
//! because a sized pack is never longer. v26 is the sized ordinary framing
//! unchanged, written since directory and extent pages may be delta programs
//! (#92): the version marks the record grammar, not the layout.
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
    GROUP_COUNT_LIMIT, GROUP_LIMIT, PACK_LIMIT, RECORD_COUNT_LIMIT, SINGLETON_PACK_LIMIT,
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
/// Directory entry width of the ordinary and chunk framings.
pub const DIRECTORY_ENTRY_LEN: usize = 16;
/// Directory entry width of the compact whole-file framing.
pub const WHOLE_FILE_ENTRY_LEN: usize = 4;
/// Framing bytes the compact whole-file assembly drops from each record.
pub const WHOLE_FILE_COMPACT_DROP: usize = 8;

/// Ordinary framing version: canonical objects in multi-record groups.
/// Still read; new packs use [`VERSION_ORDINARY_SIZED`].
pub const VERSION_ORDINARY: u32 = 9;
/// Chunk framing version: one chunk record per group entry.
///
/// Still read, and no longer written: a chunk pack may carry a payload record
/// stored verbatim, which this version predates. See [`VERSION_CHUNK_STORED`].
pub const VERSION_CHUNK: u32 = 10;
/// Compact whole-file framing version.
///
/// Still read, and no longer written, for the same reason as [`VERSION_CHUNK`].
pub const VERSION_WHOLE_FILE: u32 = 11;
/// Singleton framing version: one oversized record in one pack.
///
/// Still read, and no longer written, for the same reason as [`VERSION_CHUNK`].
pub const VERSION_SINGLETON: u32 = 13;
/// Chunk framing that may carry a payload stored verbatim.
///
/// The three payload lanes moved because a record's grammar gained a tag value:
/// a reader that predates it must refuse the pack by version rather than meet an
/// unknown tag in the middle of a decode, and a reader that has it reads both
/// versions of every payload lane. The ordinary and pooled lanes carry no payload
/// frame, so their grammar is unchanged and their versions stay where they were.
pub const VERSION_CHUNK_STORED: u32 = 15;
/// Compact whole-file framing that may carry a payload stored verbatim.
///
/// Still read, and no longer written: this version's whole-file groups hold
/// exactly one record, which is the shape [`VERSION_WHOLE_FILE_GROUPED`] removes.
pub const VERSION_WHOLE_FILE_STORED: u32 = 14;
/// Compact whole-file framing whose groups carry many records.
///
/// The group body gains the one thing the single-record form derived from the
/// group's own extent: the record boundaries. They are the same four-byte end
/// offsets the chunk lane's groups already carry, so the compact record form
/// itself - the tag, an optional base identity and the frame - is unchanged, and
/// so is this lane's starts-only directory.
pub const VERSION_WHOLE_FILE_GROUPED: u32 = 17;
/// Same grouped records with sixteen reserved directory slots instead of 256.
/// Existing version-17 packs keep their original body offset on read.
pub const VERSION_WHOLE_FILE_TIGHT: u32 = 18;
/// Ordinary groups with sixteen reserved directory slots instead of 256.
pub const VERSION_ORDINARY_TIGHT: u32 = 19;
/// Chunk groups with sixteen reserved directory slots instead of 256.
pub const VERSION_CHUNK_TIGHT: u32 = 20;
/// Maximum groups the tight directories can describe.
pub const TIGHT_GROUPS: usize = 16;
/// Compact whole-file groups whose directory holds exactly the pack's groups
/// (#352 F-027 item 1). The tight versions reserved sixteen slots and every
/// Save seals its packs whole, so the unused slots were stored as zeros.
/// 21 and 22 belonged to the retired pooled value-group lane and stay refused.
pub const VERSION_WHOLE_FILE_SIZED: u32 = 23;
/// Ordinary groups with a directory of exactly the pack's groups.
///
/// Still read, and no longer written: see [`VERSION_ORDINARY_PAGES`].
pub const VERSION_ORDINARY_SIZED: u32 = 24;
/// Sized ordinary groups whose directory and extent pages may be COPY/INSERT
/// programs against a predecessor page (layerfs-project #92).
///
/// The framing is [`VERSION_ORDINARY_SIZED`]'s, byte for byte; what moved is the
/// record grammar of four roles, which gained tag one (`encoding::pool::branch`).
/// A reader that predates it accepts tag one for inode-table branches only and
/// would fail inside a decode, so it refuses the pack by this version instead,
/// as the payload lanes did when their records gained the stored tag. A reader
/// that has it reads every ordinary version.
pub const VERSION_ORDINARY_PAGES: u32 = 26;
/// Chunk groups with a directory of exactly the pack's groups.
pub const VERSION_CHUNK_SIZED: u32 = 25;
/// Singleton framing that may carry a payload stored verbatim.
pub const VERSION_SINGLETON_STORED: u32 = 16;

/// One physical framing lane.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum PackLane {
    /// Mapping nodes and file states.
    Ordinary,
    /// Chunk payload records.
    Chunk,
    /// Whole-file payload records.
    WholeFile,
    /// One oversized record alone in its pack.
    Singleton,
}

impl PackLane {
    /// Every lane, in deterministic order.
    pub const ALL: [Self; 4] = [
        Self::Ordinary,
        Self::Chunk,
        Self::WholeFile,
        Self::Singleton,
    ];

    /// Framing version written into the pack header.
    ///
    /// The payload lanes write the version whose grammar they now use. The
    /// versions they no longer write are read by [`parse_header`] all the same,
    /// which is what keeps every pack this Store has already written readable.
    pub const fn version(self) -> u32 {
        match self {
            Self::Ordinary => VERSION_ORDINARY_PAGES,
            Self::Chunk => VERSION_CHUNK_SIZED,
            Self::WholeFile => VERSION_WHOLE_FILE_SIZED,
            Self::Singleton => VERSION_SINGLETON_STORED,
        }
    }

    /// Stable index used for per-lane owner state.
    pub const fn index(self) -> usize {
        match self {
            Self::Ordinary => 0,
            Self::Chunk => 1,
            Self::WholeFile => 2,
            Self::Singleton => 3,
        }
    }

    /// Lane a logical role is stored in.
    pub const fn for_role(role: ObjectRole) -> Self {
        match role {
            ObjectRole::WholeFile => Self::WholeFile,
            ObjectRole::Chunk => Self::Chunk,
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
            Self::Ordinary | Self::Chunk | Self::WholeFile => PACK_LIMIT,
        }
    }

    /// Largest decoded group body of this lane.
    ///
    /// The whole-file lane's groups are bounded by the pack they have to fit, not
    /// by the group codec's ceiling: its bodies are stored raw and a whole-file
    /// payload may be a whole construction cutoff wide, so a group of one can
    /// legitimately exceed [`GROUP_LIMIT`].
    pub const fn body_limit(self) -> usize {
        match self {
            Self::Singleton => SINGLETON_PACK_LIMIT,
            Self::WholeFile => PACK_LIMIT,
            Self::Ordinary | Self::Chunk => GROUP_LIMIT,
        }
    }

    /// Largest group count of this lane.
    pub const fn group_count_limit(self) -> usize {
        match self {
            Self::Singleton => 1,
            Self::Ordinary | Self::Chunk | Self::WholeFile => TIGHT_GROUPS,
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
    pub fn body_size(&self, _lane: PackLane) -> StorageResult<usize> {
        // Every lane stores its body as assembled. The compact whole-file lane
        // used to be the exception, dropping two length fields per record here;
        // those are dropped where the group's own end offsets are built instead
        // (`pack::assemble::frame_compact_group`), so a single-record group's body
        // is no longer narrower than the group that describes it.
        Ok(self.bytes.len())
    }
}

/// Bytes the tight versions reserved for a group directory of `lane`.
///
/// Placement still charges this width when it decides whether a group fits
/// (`append_fits`, the Save's queue budget): a pack written in a sized version
/// is never longer, so the decision stays safe.
pub const fn directory_capacity(lane: PackLane) -> usize {
    directory_entry_len(lane) * lane.group_count_limit()
}

/// Offset of the first group body under a full reserved directory of `lane`.
pub const fn body_area_offset(lane: PackLane) -> usize {
    HEADER_LEN + directory_capacity(lane)
}

/// Offset of the first group body in a written pack of `lane` holding
/// `groups` groups: the control area and one directory entry per group
/// (#352 F-027 item 1). A sealed pack never gains a group, so its directory
/// needs no spare slot.
pub const fn sized_body_offset(lane: PackLane, groups: usize) -> usize {
    HEADER_LEN + directory_entry_len(lane) * groups
}

/// Bytes one group's directory entry costs in `lane`.
pub const fn directory_entry_len(lane: PackLane) -> usize {
    match lane {
        PackLane::WholeFile => WHOLE_FILE_ENTRY_LEN,
        PackLane::Ordinary | PackLane::Chunk | PackLane::Singleton => DIRECTORY_ENTRY_LEN,
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
    // The directory is sized to the groups (#352 F-027 item 1): this is the
    // length the pack is written at.
    let mut total = sized_body_offset(lane, groups.len());
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
    /// First group body's version-specific offset inside this pack.
    pub body_offset: usize,
}

/// Reads and validates the pack header before any body is touched.
pub fn parse_header(bytes: &[u8]) -> StorageResult<PackHeader> {
    parse_directory_header(bytes, bytes.len())
}

/// Validates a bounded control/directory prefix against the separately declared
/// complete physical length. This checks framing only, never pack authentication.
pub fn parse_directory_header(bytes: &[u8], total_length: usize) -> StorageResult<PackHeader> {
    let header = parse_control_header(bytes, total_length)?;
    if bytes.len() < header.body_offset {
        return Err(StorageError::Integrity("pack directory width"));
    }
    Ok(header)
}

/// Validates fixed control fields and derives the complete reserved directory
/// width before directory I/O. This does not validate directory entries or bodies.
pub fn parse_control_header(bytes: &[u8], total_length: usize) -> StorageResult<PackHeader> {
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
        VERSION_ORDINARY
        | VERSION_ORDINARY_TIGHT
        | VERSION_ORDINARY_SIZED
        | VERSION_ORDINARY_PAGES => PackLane::Ordinary,
        VERSION_CHUNK | VERSION_CHUNK_STORED | VERSION_CHUNK_TIGHT | VERSION_CHUNK_SIZED => {
            PackLane::Chunk
        }
        VERSION_WHOLE_FILE
        | VERSION_WHOLE_FILE_STORED
        | VERSION_WHOLE_FILE_GROUPED
        | VERSION_WHOLE_FILE_TIGHT
        | VERSION_WHOLE_FILE_SIZED => PackLane::WholeFile,
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
    let sized = matches!(
        version,
        VERSION_ORDINARY_SIZED
            | VERSION_ORDINARY_PAGES
            | VERSION_CHUNK_SIZED
            | VERSION_WHOLE_FILE_SIZED
    );
    let slots = match version {
        VERSION_ORDINARY_TIGHT | VERSION_CHUNK_TIGHT | VERSION_WHOLE_FILE_TIGHT => TIGHT_GROUPS,
        VERSION_SINGLETON | VERSION_SINGLETON_STORED => 1,
        _ if sized => TIGHT_GROUPS,
        _ => GROUP_COUNT_LIMIT,
    };
    let count_limit = if lane == PackLane::Singleton {
        GROUP_COUNT_LIMIT
    } else {
        slots
    };
    if !(1..=count_limit).contains(&group_count) {
        return Err(StorageError::Integrity("pack group count"));
    }
    // A sized directory holds exactly the pack's groups.
    let slots = if sized { group_count } else { slots };
    if header[20..24] != [0, 0, 0, 0] {
        return Err(StorageError::Integrity("pack reserved field"));
    }
    let used = u32::from_le_bytes(
        header[USED_OFFSET..USED_OFFSET + 4]
            .try_into()
            .map_err(|_| StorageError::Integrity("pack length"))?,
    ) as usize;
    if used != total_length || bytes.len() > total_length {
        return Err(StorageError::Integrity("pack length"));
    }
    let body_offset = HEADER_LEN + directory_entry_len(lane) * slots;
    if used < body_offset + 1 {
        return Err(StorageError::Integrity("pack directory width"));
    }
    if total_length > lane.pack_limit() {
        return Err(StorageError::Integrity("pack length"));
    }
    if lane == PackLane::Singleton && group_count != 1 {
        return Err(StorageError::Integrity("singleton pack group count"));
    }
    if lane == PackLane::Chunk
        && !matches!(
            version,
            VERSION_CHUNK | VERSION_CHUNK_STORED | VERSION_CHUNK_TIGHT | VERSION_CHUNK_SIZED
        )
    {
        return Err(StorageError::Integrity("chunk framing version"));
    }
    Ok(PackHeader {
        lane,
        group_count,
        used,
        body_offset,
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
    if bytes.len() != header.used {
        return Err(StorageError::Integrity("pack length"));
    }
    directory_group_view(bytes, header, group)
}

/// Resolves a complete encoded group from a full bounded directory, validating
/// every directory extent against the header's complete pack length.
pub fn directory_group_view(
    bytes: &[u8],
    header: PackHeader,
    group: usize,
) -> StorageResult<GroupView> {
    if bytes.len() < header.body_offset || bytes.len() > header.used {
        return Err(StorageError::Integrity("pack directory width"));
    }
    if group >= header.group_count {
        return Err(StorageError::Integrity("group ordinal"));
    }
    let mut selected = None;
    visit_directory(bytes, header, &mut |number, view| {
        if number == group {
            selected = Some(view);
        }
    })?;
    selected.ok_or(StorageError::Integrity("group ordinal"))
}

/// Validates the directory once and returns every bounded encoded-group view.
pub fn directory_group_views(bytes: &[u8], header: PackHeader) -> StorageResult<Vec<GroupView>> {
    if bytes.len() < header.body_offset || bytes.len() > header.used {
        return Err(StorageError::Integrity("pack directory width"));
    }
    let mut views = Vec::with_capacity(header.group_count);
    visit_directory(bytes, header, &mut |_, view| views.push(view))?;
    Ok(views)
}
/// Validates every directory extent once for one bounded whole-body demand.
pub(crate) fn validate_group_demand(
    bytes: &[u8],
    header: PackHeader,
    groups: &std::collections::BTreeSet<usize>,
) -> StorageResult<()> {
    if bytes.len() != header.used {
        return Err(StorageError::Integrity("pack length"));
    }
    if groups.iter().any(|group| *group >= header.group_count) {
        return Err(StorageError::Integrity("group ordinal"));
    }
    visit_directory(bytes, header, &mut |_, _| {})
}

fn visit_directory(
    bytes: &[u8],
    header: PackHeader,
    visit: &mut impl FnMut(usize, GroupView),
) -> StorageResult<()> {
    match header.lane {
        PackLane::WholeFile => whole_file_directory(bytes, header, visit),
        _ => ordinary_directory(bytes, header, visit),
    }
}

fn ordinary_directory(
    bytes: &[u8],
    header: PackHeader,
    visit: &mut impl FnMut(usize, GroupView),
) -> StorageResult<()> {
    let mut offset = header.body_offset;
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
        if encoded == 0 || decoded == 0 || end > header.used {
            return Err(StorageError::Integrity("group extent"));
        }
        let codec = match entry[12] {
            0 if encoded == decoded => GroupCodec::Raw,
            1 if encoded <= decoded && decoded <= header.lane.body_limit() => GroupCodec::Zstandard,
            _ => return Err(StorageError::Integrity("group codec")),
        };
        if matches!(header.lane, PackLane::Chunk | PackLane::Singleton) && codec != GroupCodec::Raw
        {
            return Err(StorageError::Integrity("lane group codec"));
        }
        if decoded > header.lane.body_limit() {
            return Err(StorageError::Integrity("group decoded length"));
        }
        if header.lane == PackLane::Singleton && header.group_count != 1 {
            return Err(StorageError::Integrity("singleton pack group count"));
        }
        visit(
            index,
            GroupView {
                start: body_start,
                end,
                decoded_length: decoded,
                codec,
            },
        );
        offset = end;
    }
    if offset != header.used {
        return Err(StorageError::Integrity("pack trailing bytes"));
    }
    Ok(())
}

fn whole_file_directory(
    bytes: &[u8],
    header: PackHeader,
    visit: &mut impl FnMut(usize, GroupView),
) -> StorageResult<()> {
    if header.used > header.lane.pack_limit() {
        return Err(StorageError::Integrity("compact pack length"));
    }
    let mut start = header.body_offset;
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
            header.used
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
        if size < 2 || end > header.used {
            return Err(StorageError::Integrity("compact group extent"));
        }
        visit(
            index,
            GroupView {
                start,
                end,
                decoded_length: size,
                codec: GroupCodec::Raw,
            },
        );
        start = end;
    }
    Ok(())
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
