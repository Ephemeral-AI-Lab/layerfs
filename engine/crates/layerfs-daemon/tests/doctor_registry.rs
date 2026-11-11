//! The diagnostic endpoint reads registry facts without Store SQL or overlay jobs.
#[path = "../../layerfs-store-service/tests/support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../../layerfs-store-service/tests/support/sdk.rs"]
#[allow(dead_code)]
mod support;
use layerfs_bridge::control::{Reply, Request, SdkCall, SdkValue, UnmountControl};
use layerfs_daemon::control::Service;
use layerfs_history::WorkspaceId;

fn page(service: &Service, offset: i64, generation: Option<String>) -> Reply {
    service
        .execute_control(&Request::Sdk(SdkCall {
            method: "sandbox.doctor_workspaces".into(),
            args: SdkValue::map([
                ("offset", SdkValue::Int(offset)),
                (
                    "generation",
                    generation.map_or(SdkValue::Null, SdkValue::text),
                ),
            ]),
        }))
        .unwrap()
        .reply
}
#[test]
fn registry_observation_is_complete_read_only_and_detects_membership_changes() {
    let fixture = support::Fixture::new("doctor-registry", |_| 0);
    let service = Service::new(fixture.service.store.clone(), &fixture.owner);
    let before = fixture.work();
    let Reply::Sdk(empty) = page(&service, 0, None) else {
        panic!("empty registry")
    };
    assert_eq!(empty.get_int("total"), Some(0));
    assert_eq!(empty.get("next_offset"), Some(&SdkValue::Null));
    let after = fixture.work();
    assert_eq!(
        (before.statements, before.object_batches),
        (after.statements, after.object_batches)
    );
    let bound = service
        .execute_control(&Request::Mount {
            workspace: WorkspaceId::from_authority([7; 32]).unwrap(),
            branch: fixture.host.branch,
        })
        .unwrap();
    let Reply::Bound { token, .. } = bound.reply else {
        panic!("bind")
    };
    let generation = empty.get_text("generation").unwrap().to_owned();
    let Reply::SdkRefused(changed) = page(&service, 0, Some(generation)) else {
        panic!("changed membership")
    };
    assert_eq!(changed.code, "Changed");
    let before = fixture.work();
    let Reply::Sdk(one) = page(&service, 0, None) else {
        panic!("registry")
    };
    assert_eq!(one.get_int("total"), Some(1));
    let Some(SdkValue::List(rows)) = one.get("workspaces") else {
        panic!("rows")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get_text("activity"), Some("idle"));
    assert!(!rows[0].get_bool("mounted"));
    let after = fixture.work();
    assert_eq!(
        (before.statements, before.object_batches),
        (after.statements, after.object_batches)
    );
    // Exercise the producer's actual nonblocking registry admission. Two
    // bounded read-only callers contend; there is no private lock/test hook,
    // synthetic Busy reply, mutation loop or unbounded pressure worker. The
    // refused admission is only produced while one caller is inside the
    // producer's short try-lock window, so the attempt budget is the declared
    // deadline below and not a fixed attempt count: a host that serializes
    // these two callers can spend a few thousand calls inside one scheduling
    // slice without ever co-scheduling them (measured 7 of 72 loaded runs,
    // and 2 of 15 on one CPU, with a 2048-attempt cap).
    //
    // This is a bounded contention observation, not a constructed rendezvous.
    // No product operation holds the registry mutex at a point a test can
    // hold it open (every acquisition is a short atomic step, and the daemon
    // never holds it across Store, overlay or provider I/O), so the reply is
    // not guaranteed to arrive on any host. Each caller measures its own
    // deadline from the barrier, so thread start-up cost cannot consume it.
    const DEADLINE: std::time::Duration = std::time::Duration::from_secs(1);
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Barrier,
    };
    let before_busy = fixture.work();
    let start = Barrier::new(2);
    let observed_busy = AtomicBool::new(false);
    let calls = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..2 {
            scope.spawn(|| {
                start.wait();
                let deadline = std::time::Instant::now() + DEADLINE;
                while std::time::Instant::now() < deadline && !observed_busy.load(Ordering::Acquire)
                {
                    calls.fetch_add(1, Ordering::Relaxed);
                    match page(&service, 0, None) {
                        Reply::Sdk(value) => assert_eq!(value.get_int("total"), Some(1)),
                        Reply::SdkRefused(refusal) => {
                            assert_eq!(refusal.code, "Busy");
                            observed_busy.store(true, Ordering::Release);
                        }
                        other => panic!("unexpected registry admission reply: {other:?}"),
                    }
                }
            });
        }
    });
    assert!(
        observed_busy.load(Ordering::Acquire),
        "no typed Busy refusal from {} admissions within {DEADLINE:?} on each caller",
        calls.load(Ordering::Relaxed)
    );
    let after_busy = fixture.work();
    assert_eq!(
        (before_busy.statements, before_busy.object_batches),
        (after_busy.statements, after_busy.object_batches)
    );
    let Reply::Sdk(released) = page(&service, 0, None) else {
        panic!("registry admission not released")
    };
    assert_eq!(released, one);
    println!(
        "actual_registry_producer_busy=true calls={} threads_joined=2 rows_unchanged=true store_sql_delta=0 object_batch_delta=0",
        calls.load(Ordering::Relaxed)
    );
    service
        .execute_control(&Request::Unmount(UnmountControl {
            token,
            discard: true,
        }))
        .unwrap();
    drop(service);
    fixture.cleanup();
}
