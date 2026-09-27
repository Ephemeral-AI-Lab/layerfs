//! New page owners finish with their first ledger write on real Linux backing.
#![cfg(target_os = "linux")]

use layerfs_bridge::contract::Source;
use layerfs_workspace::{
    sequence::{
        metadata_pages::{self, Cell, PageRef},
        Budget, MetadataHost, PayloadHost,
    },
    Piece, PieceKind, WorkspaceError,
};
use std::{
    fs,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[test]
fn first_owner_write_covers_keyed_extent_and_shared_custody() {
    let parent = std::env::var("LAYERFS_OWNER_TEST_ROOT").expect("ext4 test root");
    let path =
        std::path::Path::new(&parent).join(format!("owner-finalization-{}", std::process::id()));
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(&path).unwrap();
    let incarnation = [41; 32];
    let budget = Budget::new(256 << 20);
    let payloads = PayloadHost::new(path.join("payloads"), 256 << 20, &budget).unwrap();
    let directory = payloads.directory(path.join("pages"), incarnation).unwrap();
    payloads.initialize(&directory).unwrap();
    let host = MetadataHost::new(payloads.clone()).unwrap();
    let arena = host.arena(directory.clone()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    let payload = payloads
        .acquire(
            directory,
            1,
            &mut &b"x"[..],
            deadline,
            &AtomicBool::new(false),
        )
        .unwrap();
    let payload_id = *payloads
        .state
        .lock()
        .unwrap()
        .records
        .keys()
        .next()
        .unwrap();
    let mut lease = payloads.window(1, 3).unwrap();
    let window = lease.window.as_mut().unwrap();

    let keyed = host.candidate(&arena, 1, false, None).unwrap();
    let key = metadata_pages::dirty_key(1, 1);
    let keyed_root = keyed
        .update(
            PageRef::NULL,
            vec![Cell::new(&key, &[1]).unwrap()],
            window,
            deadline,
        )
        .unwrap();
    assert!(
        arena
            .read_owner(keyed_root, window, deadline)
            .unwrap()
            .edges
    );
    keyed.seal(keyed_root, window, deadline).unwrap();

    let old = host.candidate(&arena, 1, false, None).unwrap();
    let custody = arena.custody(&old, &payload, window, deadline).unwrap();
    let piece = Piece {
        kind: PieceKind::Local,
        length: 1,
        offset: 0,
        payload: payload_id,
        custody,
    };
    let old_root = old.build_pieces(&[piece], 1, window).unwrap();
    assert!(arena.read_owner(old_root, window, deadline).unwrap().edges);
    old.seal(old_root, window, deadline).unwrap();
    let old_bytes = arena.load_raw(old_root, window, deadline).unwrap();

    let new = host.candidate(&arena, 2, false, Some(old.clone())).unwrap();
    assert_eq!(
        arena.custody(&new, &payload, window, deadline).unwrap(),
        custody
    );
    let new_root = new.build_pieces(&[piece], 1, window).unwrap();
    new.seal(new_root, window, deadline).unwrap();
    let new_bytes = arena.load_raw(new_root, window, deadline).unwrap();
    assert_ne!(old_root, new_root);
    assert_eq!(
        arena.load_raw(old_root, window, deadline).unwrap(),
        old_bytes
    );
    assert_eq!(arena.owner_finalizations.load(Ordering::Relaxed), 3);

    let metadata_before = host.status().unwrap();
    let backing_before = payloads.status().unwrap();
    let refs_before = arena.read_owner(custody, window, deadline).unwrap().refs;
    let removed_before = arena.extent_custody_edges_removed.load(Ordering::Relaxed);
    // The second custody ref is structurally valid but has no ledger. Its
    // definite failure follows the acknowledged first edge on another ledger.
    let missing = Piece {
        custody: PageRef { slot: 63, epoch: 1 },
        ..piece
    };
    let failed = host.candidate(&arena, 3, false, Some(new.clone())).unwrap();
    let failure = failed.build_pieces(&[piece, missing], 2, window);
    assert!(matches!(failure, Err(WorkspaceError::Io)));
    let (page, progress) = {
        let state = failed.state.lock().unwrap();
        assert_eq!(state.temporary.len(), 1);
        (state.temporary[0], state.edge_progress)
    };
    assert_eq!(progress, Some((page, 1)));
    assert!(arena.read_owner(page, window, deadline).unwrap().edges);
    assert_eq!(
        arena.read_owner(custody, window, deadline).unwrap().refs,
        refs_before + 1
    );
    assert_eq!(arena.owner_finalizations.load(Ordering::Relaxed), 3);
    assert!(!host.status().unwrap().admission_stopped);

    drop(failed);
    drop(lease);
    let cleaned = host.reclaim(incarnation, deadline).unwrap();
    assert_eq!((cleaned.roots_released, cleaned.pages_reclaimed), (1, 1));
    assert_eq!(cleaned.remaining_roots, 3);
    assert_eq!(
        arena.extent_custody_edges_removed.load(Ordering::Relaxed),
        removed_before + 1
    );
    let metadata_after = host.status().unwrap();
    let backing_after = payloads.status().unwrap();
    assert_eq!(
        metadata_after.allocated_pages,
        metadata_before.allocated_pages
    );
    assert_eq!(
        metadata_after.allocated_bytes,
        metadata_before.allocated_bytes
    );
    assert_eq!(
        metadata_after.reserved_bytes,
        metadata_before.reserved_bytes
    );
    assert_eq!(
        backing_after.allocated_bytes,
        backing_before.allocated_bytes
    );
    assert_eq!(backing_after.reserved_bytes, backing_before.reserved_bytes);
    let mut lease = payloads.window(1, 3).unwrap();
    let window = lease.window.as_mut().unwrap();
    assert_eq!(
        arena.read_owner(custody, window, deadline).unwrap().refs,
        refs_before
    );
    assert_eq!(
        arena.load_raw(old_root, window, deadline).unwrap(),
        old_bytes
    );
    assert_eq!(
        arena.load_raw(new_root, window, deadline).unwrap(),
        new_bytes
    );
    let retained = arena.payload(payload_id, custody).unwrap();
    let mut reader = retained.reader(0..1).unwrap();
    let mut byte = [0];
    assert_eq!(
        reader
            .read(&mut byte, deadline, &AtomicBool::new(false))
            .unwrap(),
        1
    );
    assert_eq!(byte, *b"x");
    drop(reader);
    drop(retained);
    drop(lease);

    drop(new);
    drop(old);
    drop(keyed);
    drop(payload);
    let cleaned = host.reclaim(incarnation, deadline).unwrap();
    assert_eq!(cleaned.remaining_roots, 0);
    assert_eq!(host.status().unwrap().allocated_pages, 0);
    host.close(&arena, deadline).unwrap();
    payloads.reclaim(incarnation, deadline).unwrap();
    assert_eq!(payloads.status().unwrap().payloads, 0);
    assert_eq!(payloads.status().unwrap().allocated_bytes, 0);
    assert_eq!(payloads.status().unwrap().reserved_bytes, 0);
    drop(host);
    drop(arena);
    drop(payloads);
    fs::remove_dir_all(path).unwrap();
}
