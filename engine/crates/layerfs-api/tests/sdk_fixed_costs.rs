//! An SDK file call does not pay its fixed costs again on every call (#352
//! F-025 fix 4). Each call binds a private Workspace, and each used to open
//! its own read-only connection, make its own producer Storage (reading the
//! policy row and content hints) and reserve 64 inode serials in a Store
//! writer transaction. Now a closed private Workspace's reader goes back to
//! the Store's idle readers, its producer to the Store's idle slot, and
//! serials come from the Store's pool, refilled 4,096 at a time.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use std::fs;
use support::{write, Fixture};

const CALLS: u64 = 8;

#[test]
fn repeated_file_calls_reuse_their_reader_producer_and_serials() {
    let fixture = Fixture::new("sdk-fixed-costs", |source| {
        fs::write(source.join("a.txt"), "a\n").unwrap();
        1
    });
    fixture.fork("work");
    // The first call may open, make and reserve; it seeds what follows.
    fixture.apply("work", vec![write("/first.txt", "first\n")]);
    let store = &fixture.service.store;
    let (reuse, work) = (store.reuse_work(), store.work());
    for n in 0..CALLS {
        // each call creates a file, so each needs a new serial
        fixture.apply(
            "work",
            vec![write(&format!("/new-{n}.txt"), format!("{n}\n"))],
        );
    }
    let (reuse_after, work_after) = (store.reuse_work(), store.work());
    let opened = reuse_after.readers_opened - reuse.readers_opened;
    let reused = reuse_after.readers_reused - reuse.readers_reused;
    let producers = reuse_after.producers - reuse.producers;
    let reservations = work_after.serial_reservations - work.serial_reservations;
    println!(
        "SDK_FIXED_COSTS calls={CALLS} readers_opened={opened} readers_reused={reused} producers={producers} serial_reservations={reservations}"
    );
    // Before: one of each per call (8, 0, 8, 8).
    assert_eq!(opened, 0, "no reader opened after the first call");
    assert_eq!(reused, CALLS, "each call reuses an idle reader");
    assert_eq!(producers, 0, "no producer made after the first call");
    assert_eq!(reservations, 0, "serials come from the pool");
    fixture.cleanup();
}
