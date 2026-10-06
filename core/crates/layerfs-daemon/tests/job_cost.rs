use layerfs_daemon::*;
use layerfs_overlay::ProfileConfig;
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

#[test]
fn completion_cost_matches_exclusive_owner_aggregate_and_retained_credit() {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "layerfs-job-cost-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    let owner = Owner::start(
        &path.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let before = client.diagnostics().unwrap();
    let pending = client
        .try_submit(
            None,
            Command::Open {
                incarnation: [241; 32],
                base_root: [242; 32],
            },
        )
        .map_err(|(e, _)| e)
        .unwrap();
    let stop = Instant::now() + Duration::from_secs(2);
    let done = loop {
        if let Some(done) = pending.try_complete().unwrap() {
            break done;
        }
        assert!(Instant::now() < stop, "owner completion timeout");
        std::thread::yield_now();
    };
    assert!(matches!(done.result(), Ok(Response::Opened(_))));
    let after = client.diagnostics().unwrap();
    let sql = after.sql_foreground.since(&before.sql_foreground).total();
    assert_eq!(done.work().sql, sql);
    assert_eq!(
        done.work().allocation,
        after
            .allocation_foreground
            .since(before.allocation_foreground)
    );
    assert_eq!(done.work().allocation.freelist_queries, 1);
    assert_eq!(done.work().parked_turns, 0);
    assert_eq!(after.outstanding, 1);
    assert!(after.credited_bytes >= std::mem::size_of::<JobWork>());
    println!(
        "S7_OWNER_COST receipt={:?} aggregate={after:?}",
        done.work()
    );
    drop(done);
    drop(pending);
    assert_eq!(client.diagnostics().unwrap().outstanding, 0);
    owner.stop().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}
