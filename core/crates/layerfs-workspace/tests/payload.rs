//! Public S5 proofs over a real content-built root: writes without base
//! payload demand, composed reads, truncate/regrow across capture and install.
mod common;
mod harness;
use common::file;
use harness::*;
use layerfs_content::filesystem::{
    update_filesystem, FilesystemInput, FilesystemObjects, FilesystemResources, InodeUpdate,
};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::ContentError;
use layerfs_overlay::{READ_WINDOW, WRITE_WINDOW};
use layerfs_workspace::{Operation, Outcome, Position, Refusal, WorkspaceError};
use std::sync::atomic::Ordering;

const LARGE: u64 = 8;
fn put(expect: &mut Vec<u8>, offset: usize, data: &[u8]) {
    if expect.len() < offset + data.len() {
        expect.resize(offset + data.len(), 0);
    }
    expect[offset..offset + data.len()].copy_from_slice(data);
}

#[test]
fn writes_change_only_their_window_and_never_read_the_inherited_payload() {
    // No object cache: every upstream object and byte is visible.
    let b = Bench::with_cache("payload-write", 0);
    let mut expect = b.fixture.bytes.clone();
    assert_eq!(b.content(LARGE), expect);
    let served = || b.fixture.store.served.load(Ordering::Relaxed);
    let before = served();
    assert_eq!(
        b.read_at(LARGE, 131_072, READ_WINDOW as u32).len(),
        READ_WINDOW
    );
    let read_bytes = served() - before;
    // The metadata an inode's attributes cost: the only facts a write needs.
    let before = served();
    assert_eq!(b.stat(LARGE).unwrap().logical_len, 400_000);
    let stat_bytes = served() - before;

    // First touch of an inherited 400,000-byte file: one metadata fact round.
    let before = served();
    let rounds = b.rounds.get();
    let stat = b
        .applied(
            write(LARGE, 199_990, b"spans a cell boundary and stays small"),
            T1,
        )
        .unwrap();
    put(
        &mut expect,
        199_990,
        b"spans a cell boundary and stays small",
    );
    let write_bytes = served() - before;
    assert_eq!(b.rounds.get() - rounds, 2);
    assert_eq!(
        write_bytes, stat_bytes,
        "a write's only upstream demand is the inode's attribute facts"
    );
    assert_eq!(
        (
            stat.logical_len,
            stat.namespace_refs,
            stat.metadata.mtime_seconds
        ),
        (400_000, 4, T1.seconds)
    );
    // Later writes settle in one round with no upstream demand at all.
    let (before, rounds) = (served(), b.rounds.get());
    for (offset, data) in [
        (0_usize, &b"head"[..]),
        (4_095, b"xy"),
        (399_999, b"last byte and growth"),
        (450_000, b"beyond a hole"),
    ] {
        b.applied(write(LARGE, offset as u64, data), T2);
        put(&mut expect, offset, data);
    }
    let big = vec![0xA5_u8; WRITE_WINDOW];
    b.applied(write(LARGE, 262_000, &big), T2);
    put(&mut expect, 262_000, &big);
    let appended = b
        .applied(
            Operation::Write {
                serial: LARGE,
                position: Position::End,
                data: b"appended".as_slice().into(),
            },
            T2,
        )
        .unwrap();
    expect.extend_from_slice(b"appended");
    assert_eq!(appended.logical_len, expect.len() as u64);
    assert_eq!(b.rounds.get() - rounds, 6);
    assert_eq!(served(), before, "no upstream demand after the first touch");
    assert_eq!(b.content(LARGE), expect);
    // Every alias is the same inode and the same bytes.
    for (parent, alias) in [(4, "index"), (5, "pkg"), (6, "state"), (7, "result")] {
        let stat = b.lookup(parent, alias).unwrap();
        assert_eq!(
            (stat.serial, stat.logical_len),
            (LARGE, expect.len() as u64)
        );
    }
    // Short and empty reads at EOF.
    let end = expect.len() as u64;
    assert_eq!(b.read_at(LARGE, end - 3, 64), b"ded");
    assert!(b.read_at(LARGE, end, 64).is_empty());
    assert!(b.read_at(LARGE, end + 1_000_000, 64).is_empty());
    assert_eq!(
        b.read_at(LARGE, 449_990, 20),
        [&[0_u8; 10][..], b"beyond a h"].concat()
    );

    // Refusals publish nothing.
    assert_eq!(b.refused(write(4, 0, b"x")), Refusal::IsDirectory);
    assert_eq!(b.refused(write(3, 0, b"x")), Refusal::Invalid);
    assert_eq!(b.refused(write(999, 0, b"x")), Refusal::Missing);
    assert_eq!(
        b.refused(write(LARGE, i64::MAX as u64, b"x")),
        Refusal::TooLarge
    );
    assert_eq!(
        b.refused(write(LARGE, 0, &vec![0; WRITE_WINDOW + 1])),
        Refusal::Invalid
    );
    let state = b.overlay.state(b.route()).unwrap();
    assert!(matches!(
        b.run(write(LARGE, 5, b""), T1).unwrap(),
        Outcome::Unchanged { .. }
    ));
    assert_eq!(b.overlay.state(b.route()).unwrap(), state);
    assert!(matches!(
        b.window(|view| view.read(&b.overlay, 4, 0, 16, &mut Vec::new())),
        Err(WorkspaceError::Content(ContentError::WrongLogicalRole))
    ));
    assert!(matches!(
        b.window(|view| view.read(&b.overlay, 1, 0, READ_WINDOW as u32 + 1, &mut Vec::new())),
        Err(WorkspaceError::Content(
            ContentError::BoundedCapacityExceeded { .. }
        ))
    ));
    println!(
        "S5_WRITE_DEMAND inherited_file_bytes=400000 first_write_upstream_bytes={write_bytes} attribute_facts_upstream_bytes={stat_bytes} later_write_upstream_bytes=0 base_read_128k_upstream_bytes={read_bytes}"
    );
}

#[test]
fn fragmented_inherited_bytes_are_read_as_one_base_range() {
    let b = Bench::with_cache("payload-gaps", 0);
    let mut expect = b.fixture.bytes.clone();
    let (from, length) = (131_072_u64, READ_WINDOW as u32);
    let demand = || (b.demand(), b.fixture.store.served.load(Ordering::Relaxed));
    let before = demand();
    assert_eq!(b.read_at(LARGE, from, length), expect[131_072..262_144]);
    let whole = (demand().0 - before.0, demand().1 - before.1);
    // 2,048 one-byte writes leave 2,048 inherited gaps inside the window.
    for at in (131_072..262_144).step_by(64) {
        b.applied(write(LARGE, at as u64, &[0xEE]), T1);
        expect[at] = 0xEE;
    }
    let before = demand();
    assert_eq!(b.read_at(LARGE, from, length), expect[131_072..262_144]);
    let gaps = (demand().0 - before.0, demand().1 - before.1);
    assert_eq!(
        gaps, whole,
        "one covering base range, not one demand per gap"
    );
    // Fully overwritten windows need no base at all.
    let window = vec![7_u8; WRITE_WINDOW];
    b.applied(write(LARGE, from, &window), T2);
    put(&mut expect, 131_072, &window);
    let before = demand();
    assert_eq!(b.read_at(LARGE, from, length), window);
    assert_eq!(demand(), before);
    assert_eq!(b.content(LARGE), expect);
    println!(
        "S5_READ_GAPS inherited_gaps=2048 objects={} upstream_bytes={} same_as_unfragmented_range=true overwritten_window_objects=0",
        gaps.0, gaps.1
    );
}

#[test]
fn truncate_and_regrow_never_resurrect_bytes_across_capture_and_known_install() {
    let b = Bench::with_cache("payload-truncate", 0);
    let base = b.fixture.bytes.clone();
    let mut expect = base.clone();
    let resized = |b: &Bench, expect: &mut Vec<u8>, size: usize| {
        b.applied(resize(LARGE, size as u64), T2);
        expect.resize(size, 0);
    };
    b.applied(write(LARGE, 900, b"local bytes around the cut 1000"), T1);
    put(&mut expect, 900, b"local bytes around the cut 1000");
    // Shrink below local and inherited bytes, then regrow: both read as zero.
    let stat = b.applied(resize(LARGE, 1_000), T2).unwrap();
    expect.truncate(1_000);
    assert_eq!(
        (stat.logical_len, stat.metadata.mtime_seconds),
        (1_000, T2.seconds)
    );
    resized(&b, &mut expect, 300_000);
    assert_eq!(b.content(LARGE), expect);
    assert!(b.read_at(LARGE, 1_000, 4_000).iter().all(|byte| *byte == 0));
    assert_eq!(b.read_at(LARGE, 990, 10), expect[990..1_000]);
    b.applied(write(LARGE, 250_000, b"after regrow"), T1);
    put(&mut expect, 250_000, b"after regrow");
    resized(&b, &mut expect, 260_000);
    assert_eq!(b.content(LARGE), expect);

    // Seal this view; the next layer cuts below sealed data and regrows.
    let capture = b.overlay.capture(b.route()).unwrap();
    let sealed = expect.clone();
    resized(&b, &mut expect, 500);
    resized(&b, &mut expect, 255_000);
    b.applied(write(LARGE, 249_990, b"later layer"), T1);
    put(&mut expect, 249_990, b"later layer");
    assert_eq!(b.content(LARGE), expect);
    assert!(b.read_at(LARGE, 250_001, 11).iter().all(|byte| *byte == 0));
    // The sealed view is still exactly what was captured.
    let mut offset = 0;
    while offset < sealed.len() {
        let local = b
            .overlay
            .captured_read(capture, LARGE, offset as u64, READ_WINDOW as u32)
            .unwrap()
            .unwrap();
        for (slot, byte) in local.data.iter().enumerate() {
            let inherit = local
                .inherited
                .get(slot / 8)
                .is_some_and(|bits| bits & (1 << (slot % 8)) != 0);
            let got = if inherit { base[offset + slot] } else { *byte };
            assert_eq!(got, sealed[offset + slot], "sealed byte {}", offset + slot);
        }
        offset += local.data.len();
    }

    // The host publishes the sealed file as the next root's content.
    let store = &b.fixture.store;
    let mut value = b.workspace.base().unwrap().inode(LARGE).unwrap().value;
    value.content_root = file(store, &sealed);
    let inodes = [InodeUpdate {
        serial: LARGE,
        value,
    }];
    let mut sink = store.clone();
    let mut objects = FilesystemObjects::new(store, &mut sink);
    let next = update_filesystem(
        &mut objects,
        &FilesystemInput {
            base: Some(b.fixture.root),
            scope: b.fixture.scope,
            root_serial: 1,
            directories: &[],
            inodes: &inodes,
            new_inodes: &[],
            resources: FilesystemResources::default(),
        },
        None,
    )
    .unwrap()
    .root;
    let prepared = b.workspace.prepare_base_install(capture, next).unwrap();
    b.workspace
        .install_prepared_base(&b.overlay, prepared)
        .unwrap();
    // The later layer is unchanged over a base that now holds the sealed bytes
    // it cut away: none of them comes back, before or after another regrow.
    assert_eq!(b.content(LARGE), expect);
    resized(&b, &mut expect, 260_000);
    assert_eq!(b.content(LARGE), expect);
    assert!(b.read_at(LARGE, 500, 100_000).iter().all(|byte| *byte == 0));

    // Zero truncate then rewrite, as O_TRUNC does.
    resized(&b, &mut expect, 0);
    b.applied(write(LARGE, 0, b"fresh content"), T1);
    put(&mut expect, 0, b"fresh content");
    assert_eq!(b.content(LARGE), expect);
    assert_eq!(b.lookup(4, "index").unwrap().logical_len, 13);

    // Size refusals and the unchanged case.
    assert_eq!(b.refused(resize(4, 0)), Refusal::IsDirectory);
    assert_eq!(b.refused(resize(3, 0)), Refusal::Invalid);
    assert_eq!(b.refused(resize(999, 0)), Refusal::Missing);
    assert_eq!(b.refused(resize(LARGE, u64::MAX)), Refusal::TooLarge);
    let state = b.overlay.state(b.route()).unwrap();
    assert!(matches!(
        b.run(resize(LARGE, 13), T2).unwrap(),
        Outcome::Unchanged { stat: Some(_) }
    ));
    assert_eq!(b.overlay.state(b.route()).unwrap(), state);
    // A small inherited whole-file object composes the same way.
    b.applied(write(2, 3, b"GIN"), T1);
    assert_eq!(b.content(2), b"oriGINal\0\xff");
    assert_eq!(b.lookup(1, "alias").unwrap().kind, InodeKind::RegularFile);
}

/// Incompressible bytes: a 64-bit xorshift stream.
fn noise(length: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

#[test]
fn whole_windows_overwrites_and_cuts_inside_them_read_back_exactly() {
    let b = Bench::with_cache("payload-windows", 0);
    let mut expect = b.fixture.bytes.clone();
    let window = noise(WRITE_WINDOW, 1);
    let put_at = |b: &Bench, expect: &mut Vec<u8>, offset: usize, data: &[u8]| {
        b.applied(write(LARGE, offset as u64, data), T1);
        put(expect, offset, data);
    };
    // Windows over inherited bytes, aligned and straddling every boundary.
    put_at(&b, &mut expect, 0, &window);
    put_at(&b, &mut expect, 131_072, &window[..100_000]);
    put_at(&b, &mut expect, 100_001, &noise(WRITE_WINDOW, 2));
    assert_eq!(b.content(LARGE), expect);
    // Overwrites inside one window: a cell, one byte at a cell boundary, a
    // span across 32 KiB and 64 KiB boundaries.
    put_at(&b, &mut expect, 8_192, &noise(4_096, 3));
    put_at(&b, &mut expect, 12_288, &[0xA7]);
    put_at(&b, &mut expect, 28_672, &noise(12_288, 4));
    put_at(&b, &mut expect, 65_534, &noise(5, 5));
    assert_eq!(b.content(LARGE), expect);
    for (to, back) in [(20_000, 70_000), (16_384, 40_000), (32_768, 300_000)] {
        put_at(&b, &mut expect, 0, &window);
        b.applied(resize(LARGE, to as u64), T2);
        expect.truncate(to);
        assert_eq!(b.content(LARGE), expect);
        b.applied(resize(LARGE, back as u64), T2);
        expect.resize(back, 0);
        assert_eq!(b.content(LARGE), expect);
        put_at(&b, &mut expect, to + 3, &noise(9_000, 6));
        assert_eq!(b.content(LARGE), expect);
    }
    // The same over a sealed generation.
    let capture = b.overlay.capture(b.route()).unwrap();
    let sealed = expect.clone();
    put_at(&b, &mut expect, 40_960, &noise(4_096, 7));
    put_at(&b, &mut expect, 70_001, &noise(40_000, 8));
    b.applied(resize(LARGE, 50_000), T2);
    expect.truncate(50_000);
    put_at(&b, &mut expect, 60_000, &noise(70_000, 9));
    assert_eq!(b.content(LARGE), expect);
    let mut offset = 0;
    while offset < sealed.len() {
        let local = b
            .overlay
            .captured_read(capture, LARGE, offset as u64, READ_WINDOW as u32)
            .unwrap()
            .unwrap();
        for (slot, byte) in local.data.iter().enumerate() {
            let inherit = local
                .inherited
                .get(slot / 8)
                .is_some_and(|bits| bits & (1 << (slot % 8)) != 0);
            let got = if inherit {
                b.fixture.bytes[offset + slot]
            } else {
                *byte
            };
            assert_eq!(got, sealed[offset + slot], "sealed byte {}", offset + slot);
        }
        offset += local.data.len();
    }
}
