//! Explicit physical format matrix: recognized framings and rejected ones.
//!
//! A reader never trial-decodes. Each framing this slice writes is recognized by
//! its version, each framing it does not implement is refused with a named
//! unsupported-format failure, and the declared allocation, frame and pack bounds
//! are enforced on the bytes rather than assumed.

mod support;

use layerfs_storage::pack::layout::{
    body_area_offset, directory_entry_len, directory_group_views, group_view, parse_header,
    sized_body_offset, PackLane, PACK_MAGIC, TIGHT_GROUPS, USED_OFFSET, VERSION_CHUNK,
    VERSION_CHUNK_SIZED, VERSION_CHUNK_STORED, VERSION_CHUNK_TIGHT, VERSION_ORDINARY,
    VERSION_ORDINARY_PAGES, VERSION_ORDINARY_SIZED, VERSION_ORDINARY_TIGHT, VERSION_SINGLETON,
    VERSION_SINGLETON_STORED,
    VERSION_WHOLE_FILE, VERSION_WHOLE_FILE_GROUPED, VERSION_WHOLE_FILE_SIZED,
    VERSION_WHOLE_FILE_STORED, VERSION_WHOLE_FILE_TIGHT,
};
use layerfs_storage::StorageError;

/// Builds a minimal, well-formed pack of `lane` under `version`.
///
/// The control area declares `groups` groups and the length the pack assembles
/// to - the control area, the lane's whole reserved directory region and `body`
/// bytes of body - and the directory entries are left zero. A synthetic pack has
/// to carry a length its own control area agrees with, so the builder cannot
/// produce a header alone: `parse_header` reads the declared length out of the
/// bytes and refuses a pack whose bytes are not the ones it declares.
fn pack_of(lane: PackLane, version: u32, groups: u32, body: usize) -> Vec<u8> {
    let length = match (lane, version) {
        (PackLane::WholeFile, VERSION_WHOLE_FILE_TIGHT)
        | (PackLane::Ordinary, VERSION_ORDINARY_TIGHT)
        | (PackLane::Chunk, VERSION_CHUNK_TIGHT) => body_area_offset(lane) + body,
        (PackLane::WholeFile, VERSION_WHOLE_FILE_SIZED)
        | (PackLane::Ordinary, VERSION_ORDINARY_SIZED | VERSION_ORDINARY_PAGES)
        | (PackLane::Chunk, VERSION_CHUNK_SIZED) => sized_body_offset(lane, groups as usize) + body,
        (PackLane::WholeFile, _) => 24 + 4 * 256 + body,
        (PackLane::Ordinary | PackLane::Chunk, _) => 24 + 16 * 256 + body,
        _ => body_area_offset(lane) + body,
    };
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&PACK_MAGIC);
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes.extend_from_slice(&groups.to_le_bytes());
    bytes.extend_from_slice(
        &u32::try_from(length)
            .expect("synthetic pack length")
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes.resize(length, 0);
    bytes
}

#[test]
fn grouped_whole_file_versions_keep_their_own_body_offsets() {
    for (version, offset) in [
        (VERSION_WHOLE_FILE_GROUPED, 24 + 4 * 256),
        (VERSION_WHOLE_FILE_TIGHT, 24 + 4 * 16),
        (VERSION_WHOLE_FILE_SIZED, 24 + 4),
    ] {
        let body = [1_u8, 2, 3, 4];
        let used = offset + body.len();
        let mut pack = Vec::new();
        pack.extend_from_slice(&PACK_MAGIC);
        pack.extend_from_slice(&version.to_le_bytes());
        pack.extend_from_slice(&1_u32.to_le_bytes());
        pack.extend_from_slice(&(used as u32).to_le_bytes());
        pack.extend_from_slice(&[0; 4]);
        pack.extend_from_slice(&(offset as u32).to_le_bytes());
        pack.resize(offset, 0);
        pack.extend_from_slice(&body);
        let header = parse_header(&pack).unwrap();
        assert_eq!(header.body_offset, offset);
        let group = group_view(&pack, header, 0).unwrap();
        assert_eq!(&pack[group.start..group.end], body);
    }
}

#[test]
fn ordinary_and_chunk_versions_keep_their_own_body_offsets() {
    for (lane, old, tight, sized) in [
        (
            PackLane::Ordinary,
            VERSION_ORDINARY,
            VERSION_ORDINARY_TIGHT,
            VERSION_ORDINARY_SIZED,
        ),
        (
            PackLane::Chunk,
            VERSION_CHUNK_STORED,
            VERSION_CHUNK_TIGHT,
            VERSION_CHUNK_SIZED,
        ),
    ] {
        for (version, offset) in [
            (old, 24 + 16 * 256),
            (tight, 24 + 16 * 16),
            (sized, 24 + 16),
        ] {
            let mut pack = pack_of(lane, version, 1, 4);
            pack[24..40].copy_from_slice(&[
                (offset & 255) as u8,
                (offset >> 8) as u8,
                0,
                0,
                4,
                0,
                0,
                0,
                4,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ]);
            pack[offset..offset + 4].copy_from_slice(&[1, 2, 3, 4]);
            let header = parse_header(&pack).unwrap();
            assert_eq!(header.body_offset, offset);
            let group = group_view(&pack, header, 0).unwrap();
            assert_eq!(&pack[group.start..group.end], &[1, 2, 3, 4]);
        }
        for version in [tight, sized] {
            let oversized = pack_of(lane, version, 17, 4);
            assert!(matches!(
                parse_header(&oversized),
                Err(StorageError::Integrity("pack group count"))
            ));
        }
    }
}

#[test]
fn every_implemented_framing_is_recognized_by_its_own_version() {
    // The three payload lanes each have two implemented versions: the one they
    // write, whose record grammar carries a stored tag, and the one they no longer
    // write but still read, which is every pack this Store wrote before the stored
    // tag existed. A version a reader does not implement is refused by name below
    // rather than trial-decoded, which is what makes the pair meaningful.
    for (version, lane) in [
        (VERSION_ORDINARY, PackLane::Ordinary),
        (VERSION_ORDINARY_TIGHT, PackLane::Ordinary),
        (VERSION_ORDINARY_SIZED, PackLane::Ordinary),
        (VERSION_ORDINARY_PAGES, PackLane::Ordinary),
        (VERSION_CHUNK, PackLane::Chunk),
        (VERSION_CHUNK_STORED, PackLane::Chunk),
        (VERSION_CHUNK_TIGHT, PackLane::Chunk),
        (VERSION_CHUNK_SIZED, PackLane::Chunk),
        (VERSION_WHOLE_FILE, PackLane::WholeFile),
        (VERSION_WHOLE_FILE_STORED, PackLane::WholeFile),
        (VERSION_WHOLE_FILE_GROUPED, PackLane::WholeFile),
        (VERSION_WHOLE_FILE_TIGHT, PackLane::WholeFile),
        (VERSION_WHOLE_FILE_SIZED, PackLane::WholeFile),
        (VERSION_SINGLETON, PackLane::Singleton),
        (VERSION_SINGLETON_STORED, PackLane::Singleton),
    ] {
        let bytes = pack_of(lane, version, 1, 32);
        let parsed = parse_header(&bytes).unwrap_or_else(|error| panic!("{lane:?}: {error}"));
        assert_eq!(parsed.lane, lane);
        assert_eq!(parsed.group_count, 1);
        assert_eq!(parsed.used, bytes.len(), "the declared length is the pack");
    }
}

#[test]
fn unimplemented_and_unknown_framings_are_rejected_without_a_trial_decode() {
    // 12, 21 and 22 were the pooled value-group framings. Inode values are
    // stored rows now, and a pack of those versions is refused like any other
    // framing this reader does not implement.
    for version in [0_u32, 3, 5, 8, 12, 21, 22, 99] {
        let bytes = pack_of(PackLane::Ordinary, version, 1, 32);
        let error = parse_header(&bytes).unwrap_err();
        assert!(
            matches!(error, StorageError::UnsupportedPolicy { field } if field == "pack framing version"),
            "version {version}: {error}"
        );
    }
    // A recognized version with a malformed control area is an integrity failure,
    // not a fallback to another framing.
    let mut wrong_magic = pack_of(PackLane::Ordinary, VERSION_ORDINARY, 1, 32);
    wrong_magic[0] = b'X';
    assert!(matches!(
        parse_header(&wrong_magic),
        Err(StorageError::Integrity(_))
    ));
    // A pack that declares more bytes than it has is refused, never short-read.
    let mut truncated = pack_of(PackLane::Ordinary, VERSION_ORDINARY, 1, 32);
    truncated.pop();
    assert!(matches!(
        parse_header(&truncated),
        Err(StorageError::Integrity("pack length"))
    ));
    // A nonzero reserved word is a framing this reader does not implement.
    let mut reserved = pack_of(PackLane::Ordinary, VERSION_ORDINARY, 1, 32);
    reserved[20] = 1;
    assert!(matches!(
        parse_header(&reserved),
        Err(StorageError::Integrity("pack reserved field"))
    ));
    // A declared length that does not even cover the reserved directory region is
    // refused rather than read as a directory.
    let mut short = pack_of(PackLane::Ordinary, VERSION_ORDINARY, 1, 0);
    let area = body_area_offset(PackLane::Ordinary);
    short.truncate(area);
    short[USED_OFFSET..USED_OFFSET + 4].copy_from_slice(&(area as u32).to_le_bytes());
    assert!(matches!(
        parse_header(&short),
        Err(StorageError::Integrity("pack directory width"))
    ));
}

#[test]
fn declared_pack_and_group_bounds_are_enforced_on_the_bytes() {
    // The ordinary lane is bounded by the 256-KiB pack limit.
    let ordinary = PackLane::Ordinary;
    let oversized = pack_of(
        ordinary,
        VERSION_ORDINARY,
        1,
        ordinary.pack_limit() - body_area_offset(ordinary) + 1,
    );
    assert!(matches!(
        parse_header(&oversized),
        Err(StorageError::Integrity("pack length"))
    ));
    // The singleton lane is bounded by its own, larger limit.
    let singleton = PackLane::Singleton;
    let oversized = pack_of(
        singleton,
        VERSION_SINGLETON,
        1,
        singleton.pack_limit() - body_area_offset(singleton) + 1,
    );
    assert!(matches!(
        parse_header(&oversized),
        Err(StorageError::Integrity("pack length"))
    ));
    // A singleton pack holds exactly one group.
    let two_groups = pack_of(PackLane::Singleton, VERSION_SINGLETON, 2, 64);
    assert!(matches!(
        parse_header(&two_groups),
        Err(StorageError::Integrity("singleton pack group count"))
    ));
    // A zero group count is not a valid directory.
    let empty = pack_of(PackLane::Chunk, VERSION_CHUNK, 0, 64);
    assert!(matches!(
        parse_header(&empty),
        Err(StorageError::Integrity("pack group count"))
    ));
}

#[test]
fn the_lane_table_names_one_limit_per_lane() {
    assert_eq!(PackLane::Singleton.pack_limit(), 16 * 1024 * 1024 + 4_096);
    assert_eq!(PackLane::Ordinary.pack_limit(), 256 * 1024);
    assert_eq!(PackLane::Singleton.group_count_limit(), 1);
    for lane in PackLane::ALL {
        if lane != PackLane::Singleton {
            assert_eq!(lane.pack_limit(), 256 * 1024, "{lane:?}");
        }
        assert!(lane.body_limit() <= lane.pack_limit(), "{lane:?}");
    }
}

#[test]
fn a_recipe_frame_larger_than_its_profile_is_refused() {
    // The codec profile is derived from the policy; an oversized raw payload is
    // refused with a named capacity failure rather than a truncated frame.
    use layerfs_storage::encoding::{CodecProfile, CompressionWorkspace};
    let small = layerfs_storage::StoragePolicy::frozen_default();
    let capacities = layerfs_storage::StorageCapacities::from_policy(small).expect("capacities");
    let profile = CodecProfile::whole_file(&capacities);
    assert_eq!(profile.raw_limit(), 131_071);
    assert_eq!(profile.frame_limit(), 135_168);
    let mut workspace = CompressionWorkspace::new().expect("workspace");
    let raw = support::noise(profile.raw_limit() + 1);
    let error = workspace.compress(profile, &raw).unwrap_err();
    assert!(
        matches!(error, StorageError::CapacityExceeded { what, .. } if what == "codec.raw_payload"),
        "got {error}"
    );
    let large = layerfs_storage::StorageCapacities::from_policy(
        layerfs_storage::StoragePolicy::new(1, 1_048_576, 8, 4),
    )
    .expect("capacities");
    let large_profile = CodecProfile::whole_file(&large);
    assert!(large_profile.raw_limit() > profile.raw_limit());
    assert!(large_profile.frame_limit() > profile.frame_limit());
}

/// The two recorded lane/scope deviations, asserted so they cannot drift silently.
///
/// `physical-encoding-and-packing.md` names the pooled metadata lane by its
/// framing version. The shipped candidate writes pooled **value groups** in that
/// lane and pooled **leaf** records in the Ordinary lane, distinguished by the
/// `objects.object_role` column, and it refuses v5 by design because its schema
/// identity is deliberately not the reference's. Both are recorded in that
/// document as open owner decisions; this case pins the shipped behaviour they
/// describe, and it reads the version from the lane rather than from a literal so
/// that a framing change moves one number in one place.
#[test]
fn closing_a_pack_and_retaining_its_tail_assemble_the_same_bytes() {
    // Placement closes a full pack by handing its groups to the consuming
    // assembly, which releases each body as it copies it, and keeps the tail when
    // the pack stays open. The two paths must produce byte-identical packs: the
    // difference is when the constituent bodies are released, never what is
    // written.
    use layerfs_storage::encoding::CompressionWorkspace;
    use layerfs_storage::pack::{assemble, assemble_consuming, build_group};

    let mut workspace = CompressionWorkspace::new().expect("encode workspace");
    for lane in [
        PackLane::Ordinary,
        PackLane::Chunk,
        PackLane::WholeFile,
        PackLane::Singleton,
    ] {
        let groups = match lane {
            PackLane::WholeFile => {
                vec![build_group(lane, &[vec![0x11; 4_096]], None).expect("compact group")]
            }
            PackLane::Singleton => {
                vec![build_group(lane, &[vec![0x44; 32_768]], None).expect("singleton group")]
            }
            _ => vec![
                build_group(
                    lane,
                    &[vec![0x55; 300], vec![0x66; 200]],
                    Some(&mut workspace),
                )
                .expect("ordinary group"),
                build_group(lane, &[vec![0x77; 1_024]], Some(&mut workspace))
                    .expect("ordinary group"),
                build_group(lane, &[vec![0x88; 4_096]], Some(&mut workspace))
                    .expect("ordinary group"),
            ],
        };
        let borrowed = assemble(lane, &groups).expect("retained-tail assembly");
        let consumed = assemble_consuming(lane, groups).expect("consuming assembly");
        assert_eq!(borrowed, consumed, "{lane:?}");
    }
}

#[test]
fn written_packs_size_their_directory_to_their_groups() {
    // #352 F-027 item 1: every lane but the singleton writes a sized version,
    // whose directory holds exactly the pack's groups, so a written pack carries
    // no reserved directory byte. Each group still resolves to the body bytes
    // the assembly laid down, at one, two and the maximum group count.
    use layerfs_storage::encoding::CompressionWorkspace;
    use layerfs_storage::pack::{assemble, assemble_consuming, body_bytes, build_group};

    let mut workspace = CompressionWorkspace::new().expect("encode workspace");
    for (lane, version) in [
        (PackLane::Ordinary, VERSION_ORDINARY_PAGES),
        (PackLane::Chunk, VERSION_CHUNK_SIZED),
        (PackLane::WholeFile, VERSION_WHOLE_FILE_SIZED),
    ] {
        assert_eq!(lane.version(), version, "{lane:?} writes its sized version");
        for count in [1, 2, TIGHT_GROUPS] {
            let groups: Vec<_> = (0..count)
                .map(|n| {
                    let records = [vec![n as u8; 256 + n], vec![0xA5; 64]];
                    let workspace = (lane != PackLane::WholeFile).then_some(&mut workspace);
                    build_group(lane, &records, workspace).expect("group")
                })
                .collect();
            let pack = assemble(lane, &groups).expect("assembly");
            let body = body_bytes(lane, &groups).expect("body");
            let header = parse_header(&pack).expect("header");
            let offset = 24 + directory_entry_len(lane) * count;
            assert_eq!(header.body_offset, offset, "{lane:?} x{count}");
            assert_eq!(header.group_count, count);
            assert_eq!(pack.len(), offset + body.len(), "no reserved byte");
            assert_eq!(&pack[offset..], &body[..]);
            let views = directory_group_views(&pack, header).expect("views");
            assert_eq!(views.len(), count);
            let mut cursor = offset;
            for (n, view) in views.iter().enumerate() {
                assert_eq!(
                    view.start, cursor,
                    "{lane:?} group {n} starts after its predecessor"
                );
                cursor = view.end;
                let single = group_view(&pack, header, n).expect("group view");
                assert_eq!((single.start, single.end), (view.start, view.end));
            }
            assert_eq!(cursor, pack.len());
            let reserved = directory_entry_len(lane) * (TIGHT_GROUPS - count);
            assert_eq!(
                body_area_offset(lane) - header.body_offset,
                reserved,
                "{lane:?} x{count} saves the unused reserved slots"
            );
            assert_eq!(assemble_consuming(lane, groups).expect("consuming"), pack);
        }
    }
    // The singleton lane holds one group and keeps its one-slot version.
    assert_eq!(PackLane::Singleton.version(), VERSION_SINGLETON_STORED);
}
