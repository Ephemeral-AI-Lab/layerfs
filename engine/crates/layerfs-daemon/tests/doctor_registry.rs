//! The diagnostic endpoint reads registry facts without Store SQL or overlay jobs.
#[path = "../../layerfs-store-service/tests/support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "../../layerfs-store-service/tests/support/sdk.rs"]
#[allow(dead_code)]
mod support;
use layerfs_bridge::control::{Reply, Request, SdkCall, SdkValue, UnmountControl};
use layerfs_daemon::{control::Service, ServiceClass};
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
    // Exercise the producer's actual nonblocking registry admission against a
    // registry holder this test parks, not against a second caller it hopes
    // to collide with. A Mount's admission reads each live Workspace's
    // retained maintenance failure while it holds the registry, and that read
    // takes the Workspace's connection lock. So:
    //
    //   holder  runs one owner job on the bound Workspace and parks inside it
    //           (the public job entry runs its caller's closure under the
    //           connection lock);
    //   mounter sends a second Mount, whose admission takes the registry and
    //           then waits for that connection lock;
    //   here    every admission is a complete page until the mounter holds the
    //           registry, and a typed Busy from then until the holder is let go.
    //
    // The order holder-in-job -> mounter-holds-registry -> Busy is forced by
    // those two locks: no sleep, attempt count or time budget decides the
    // outcome. There is no private lock/test hook and no synthetic Busy reply.
    // GUARD only turns a broken construction (a Mount admission that no longer
    // waits while it holds the registry) into a failure instead of a hang.
    const GUARD: std::time::Duration = std::time::Duration::from_secs(30);
    // Admissions repeated while the registry stays held.
    const HELD: usize = 8;
    use std::sync::mpsc;
    let (route, overlay) = {
        let operation = service.operation(token).unwrap();
        (operation.workspace().route(), operation.overlay().clone())
    };
    let second = WorkspaceId::from_authority([8; 32]).unwrap();
    let branch = fixture.host.branch;
    let (entered, parked) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let before_busy = fixture.work();
    let (pages, second_token) = std::thread::scope(|scope| {
        // Owned by this closure: a failed assertion below drops it while
        // unwinding, which lets the holder go before the scope joins it.
        let release = release;
        let holder = scope.spawn(move || {
            overlay.call(route, ServiceClass::General, None, move |_| {
                let _ = entered.send(());
                // Returns when the test releases it, or when the test is gone.
                let _ = released.recv();
                Ok(())
            })
        });
        parked
            .recv_timeout(GUARD)
            .expect("owner job entered the Workspace connection lock");
        let mounter = scope.spawn(|| {
            service.execute_control(&Request::Mount {
                workspace: second,
                branch,
            })
        });
        let started = std::time::Instant::now();
        let mut pages = 0usize;
        let first = loop {
            match page(&service, 0, None) {
                // The mounter has not taken the registry yet.
                Reply::Sdk(value) => {
                    assert!(
                        !mounter.is_finished(),
                        "Mount returned while the owner job was parked"
                    );
                    assert!(
                        started.elapsed() < GUARD,
                        "Mount admission did not hold the registry after {pages} complete pages"
                    );
                    assert_eq!(value, one);
                    pages += 1;
                    std::thread::yield_now();
                }
                Reply::SdkRefused(refusal) => break refusal,
                other => panic!("unexpected registry admission reply: {other:?}"),
            }
        };
        assert_eq!(first.code, "Busy");
        // Both threads stay parked, so every later admission is refused too,
        // and none of them reaches the Store.
        for _ in 0..HELD {
            // A holder that was only preempted would run and finish here.
            std::thread::yield_now();
            let Reply::SdkRefused(refusal) = page(&service, 0, None) else {
                panic!("registry admission completed while a Mount admission held it")
            };
            assert_eq!(refusal.code, "Busy");
        }
        let after_busy = fixture.work();
        assert_eq!(
            (before_busy.statements, before_busy.object_batches),
            (after_busy.statements, after_busy.object_batches)
        );
        assert!(
            !mounter.is_finished(),
            "Mount returned while the owner job was parked"
        );
        release.send(()).expect("holder still parked");
        holder.join().unwrap().expect("parked owner job");
        let Reply::Bound { token, .. } = mounter.join().unwrap().unwrap().reply else {
            panic!("second bind")
        };
        (pages, token)
    });
    // Released: the admission completes again and shows the Mount it waited on.
    let Reply::Sdk(both) = page(&service, 0, None) else {
        panic!("registry admission not released")
    };
    assert_eq!(both.get_int("total"), Some(2));
    service
        .execute_control(&Request::Unmount(UnmountControl {
            token: second_token,
            discard: true,
        }))
        .unwrap();
    let Reply::Sdk(released) = page(&service, 0, None) else {
        panic!("registry admission not released")
    };
    assert_eq!(released, one);
    println!(
        "actual_registry_producer_busy=true pages_before_hold={pages} refused_while_held={} threads_joined=2 rows_unchanged=true store_sql_delta=0 object_batch_delta=0",
        HELD + 1
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
