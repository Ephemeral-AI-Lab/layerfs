//! Real kernel mount: a control Commit taken while a page stored through a
//! shared mapping is still dirty in the kernel (rows FP-15 Commit inclusion,
//! R3-STEP-6, R5-9 and R4-3 row six, decision L-6).
//!
//! The contract is that Commit forces no kernel writeback and captures exactly
//! the locally published frontier: a store the kernel has not yet written back
//! is outside it. The precondition is kernel timing and cannot be forced. It is
//! observed instead: the mount's received request frames must be the same
//! before the store and after the Commit returned. If they moved, the attempt
//! is `not established` and the test fails with that message; nothing loops
//! until the precondition holds.
//!
//! Between the store and the first Commit's return nothing here calls `msync`,
//! `munmap`, `close` or `open` on the file, and nothing reads it with
//! `O_DIRECT`: each of those makes the kernel write the page first.
#![cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "support/complete_bytes.rs"]
mod bytes;
#[allow(dead_code)]
#[path = "support/complete_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/installed_store.rs"]
mod installed;
#[allow(dead_code)]
#[path = "support/namespace_model.rs"]
mod model;
#[allow(dead_code)]
#[path = "support/mounted.rs"]
mod mounted;
#[allow(dead_code)]
#[path = "support/mutating.rs"]
mod mutating;
#[allow(dead_code)]
#[path = "support/mounted_commit.rs"]
mod rig;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_bridge::control::{Activity, WorkspaceToken};
use layerfs_fuse::request::Opcode;
use model::{assert_same, Flat, FILE};
use mounted::{until, COMMAND};
use mutating::{counter, direct, frames, opcodes, Mapping};
use rig::{assert_kernel, bash_in, kernel, native, passed, pattern, root, Rig};
use std::{
    fs::{self, OpenOptions},
    os::unix::fs::FileExt,
    time::Duration,
};

const PAGE: usize = 4096;
/// The exact first words of the failure of an attempt whose dirty-page
/// precondition was not shown to hold.
const NOT_ESTABLISHED: &str = "not established";
const APP: &str = "target/debug/app";
/// Three pages: one inside page 3, one across the boundary of pages 39 and 40.
const STORES: [(usize, &[u8]); 2] = [
    (3 * PAGE + 17, b"dirty-at-the-first-commit"),
    (40 * PAGE - 4, b"dirty-across-a-page-boundary"),
];

fn file<'a>(flat: &'a Flat, path: &str) -> &'a [u8] {
    let seen = flat
        .get(path.as_bytes())
        .unwrap_or_else(|| panic!("missing {path}"));
    assert_eq!(seen.kind, FILE, "{path}");
    &seen.payload
}
/// Whole-content equality that reports where two byte strings part.
fn same_bytes(actual: &[u8], expected: &[u8], what: &str) {
    let first = actual
        .iter()
        .zip(expected)
        .position(|(a, b)| a != b)
        .unwrap_or(actual.len().min(expected.len()));
    assert!(
        actual == expected,
        "{what}: {} bytes against {} expected, first difference at {first}",
        actual.len(),
        expected.len()
    );
}
/// The file's bytes through an already open descriptor: no OPEN and no close.
fn through(handle: &fs::File, length: usize) -> Vec<u8> {
    let mut bytes = vec![0; length];
    handle.read_exact_at(&mut bytes, 0).unwrap();
    bytes
}

/// The mount's received frames once two readings a tenth of a second apart
/// agree. Closing a file queues its RELEASE in the kernel, which sends queued
/// requests one at a time, so the daemon can be momentarily idle while more
/// are still to come. A readiness wait before the attempt, bounded.
fn settled(rig: &Rig, token: WorkspaceToken) -> u64 {
    let mut last = frames(&rig.harness, token);
    let mut stable = None;
    until("request frames settled", || {
        std::thread::sleep(Duration::from_millis(100));
        let now = frames(&rig.harness, token);
        if now == last {
            stable = Some(now);
        }
        last = now;
        stable.is_some()
    });
    stable.unwrap()
}

/// R5-9, FP-15 (Commit inclusion), R3-STEP-6, R4-3 row six.
#[test]
fn r5_9_a_still_dirty_mapped_page_is_outside_the_commit_and_in_the_next_after_msync() {
    let rig = Rig::new("r5-9-dirty");
    let ready = rig.mount(1);
    let mount = root(&ready);
    let bound = rig.harness.status(ready.token).binding;
    let app = mount.join(APP);
    let old = pattern(7, 300_017);
    same_bytes(&fs::read(&app).unwrap(), &old, "base app");

    // An ordinary published change beside the mapped file, so that the first
    // Commit publishes a record of its own.
    passed(
        &bash_in(
            COMMAND,
            mount,
            "set -euo pipefail; printf 'published before the store\\n' > beside.txt",
        ),
        "change beside the mapped file",
    );

    // The committed base file, mapped shared. The mapping and the writable
    // descriptor stay live until after the second Commit.
    let handle = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&app)
        .unwrap();
    let length = 64 * PAGE;
    let map = Mapping::shared(&handle, length);
    same_bytes(&map.load(0, length), &old[..length], "mapped read");

    // The model of the published frontier, read before the store.
    let frontier = native(mount);
    same_bytes(file(&frontier, APP), &old, "model before the store");
    assert_eq!(
        file(&frontier, "beside.txt"),
        b"published before the store\n"
    );
    assert_ne!(frontier, rig.base, "the beside change is in the model");

    // A read through the kept descriptor of pages the kernel already holds
    // sends the daemon nothing: the cached-view check below is itself
    // outside the frame comparison's doubt.
    let quiet_before = settled(&rig, ready.token);
    same_bytes(&through(&handle, old.len()), &old, "descriptor read");
    same_bytes(&map.load(0, length), &old[..length], "mapped read again");
    let recorded = frames(&rig.harness, ready.token);
    assert_eq!(
        recorded, quiet_before,
        "a cached read through the kept descriptor reached the daemon; the cached-view check cannot be used"
    );

    // The store. No msync, munmap, close, open or O_DIRECT read follows
    // before the first Commit has returned.
    let mut new = old.clone();
    for (offset, bytes) in STORES {
        map.store(offset, bytes);
        new[offset..offset + bytes.len()].copy_from_slice(bytes);
    }
    assert_ne!(new, old);
    // The cached view has the new bytes: the mapping and the page cache.
    same_bytes(
        &map.load(0, length),
        &new[..length],
        "mapping after the store",
    );
    same_bytes(
        &through(&handle, new.len()),
        &new,
        "cached read after the store",
    );

    // The product control Commit, at once.
    let done = rig.commit(ready.token);
    let returned = frames(&rig.harness, ready.token);
    println!(
        "R5-9-DIRTY frames recorded={recorded} after_first_commit={returned} stored_pages=3 stored_bytes={}",
        STORES.iter().map(|(_, bytes)| bytes.len()).sum::<usize>()
    );
    assert!(
        returned == recorded,
        "{NOT_ESTABLISHED}: request frames moved from {recorded} to {returned} before the first Commit returned; the stored pages were not shown to be dirty for the whole Commit ({done:?})"
    );
    // Established: no WRITE, and no request of any kind, reached the daemon
    // between the record and the Commit's return.
    let done = done.unwrap_or_else(|failure| panic!("first Commit: {failure:?}"));
    rig::receipt(&done, "R5-9-DIRTY first");
    let first = match &done.reply {
        layerfs_bridge::control::Reply::Committed(
            layerfs_history::CommitStagedOutcome::Committed(record),
        ) => record.clone(),
        other => panic!("first Commit: {other:?}"),
    };
    assert_eq!(first.parent, bound.branch.head_commit);
    assert_ne!(first.root, bound.effective_root);

    // The published root is exactly the frontier: the old bytes at the stored
    // ranges, whole, with no torn or discarded tail, and the model elsewhere.
    let published = rig.published(first.root);
    assert_same(&frontier, &published, "the first published root");
    let committed = file(&published, APP);
    same_bytes(committed, &old, "first published bytes of the mapped file");
    for (offset, bytes) in STORES {
        let range = offset..offset + bytes.len();
        assert_eq!(committed[range.clone()], old[range.clone()], "old range");
        assert_ne!(committed[range.clone()], new[range], "not the store");
    }
    assert_eq!(
        file(&published, "beside.txt"),
        b"published before the store\n"
    );
    let status = rig.harness.status(ready.token);
    assert_eq!(status.activity, Activity::Idle);
    assert_eq!(status.binding.effective_root, first.root);
    // The live view is unchanged by install: the store is still there.
    same_bytes(
        &map.load(0, length),
        &new[..length],
        "mapping after install",
    );
    same_bytes(
        &through(&handle, new.len()),
        &new,
        "cached read after install",
    );

    // msync: the kernel writes the pages back, and the daemon has them.
    map.sync();
    let synced = frames(&rig.harness, ready.token);
    assert!(
        synced > returned,
        "msync sent nothing: {returned} -> {synced}"
    );
    same_bytes(&direct(&app), &new, "daemon read after msync");
    let expected = native(mount);
    same_bytes(file(&expected, APP), &new, "model after msync");
    assert_ne!(expected, frontier);

    // The second Commit has the store, and the first as its parent.
    let second = rig.committed(ready.token, "R5-9-DIRTY second");
    assert_eq!(second.parent, Some(first.id));
    assert_ne!(second.root, first.root);
    let published = rig.published(second.root);
    assert_same(&expected, &published, "the second published root");
    same_bytes(
        file(&published, APP),
        &new,
        "second published bytes of the mapped file",
    );
    assert_same(&expected, &native(mount), "the same mount after install");
    same_bytes(
        &map.load(0, length),
        &new[..length],
        "mapping after the second install",
    );
    map.unmap();
    drop(handle);

    let drained = rig.unmount(&ready);
    let stores = counter(&drained, "store_units");
    let counts = opcodes(&drained);
    // Pages 3, 39 and 40 arrived as WRITEs carrying the page-cache flag; the
    // kernel may send adjacent pages together. The only other WRITE of this
    // mount is the one `printf` above.
    assert!((2..=3).contains(&stores), "{drained}");
    assert_eq!(counts[Opcode::Write as usize], stores + 1, "{drained}");

    let fresh = rig.mount(2);
    let again = root(&fresh);
    assert_eq!(
        rig.harness.status(fresh.token).binding.effective_root,
        second.root
    );
    same_bytes(&fs::read(again.join(APP)).unwrap(), &new, "fresh mount");
    same_bytes(&direct(&again.join(APP)), &new, "fresh mount, daemon read");
    assert_same(&expected, &native(again), "the fresh mount");
    assert_kernel(&expected, &kernel(again), "R5-9-DIRTY fresh mount");
    println!(
        "R5-9-DIRTY established=true frames(recorded={recorded} after_first_commit={returned} after_msync={synced}) first_commit=old_bytes second_commit=stored_bytes parent=first store_units={stores} write_frames={} paths={}",
        counts[Opcode::Write as usize],
        expected.len()
    );
    rig.unmount(&fresh);
    rig.finish();
}
