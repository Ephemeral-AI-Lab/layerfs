//! Route-free Resources uses one original bounded read job and actual SQL work.
use layerfs_daemon::application::{ResourceDiagnosticError, ResourceFailure, ResourceObservation};
use layerfs_daemon::{Command, Owner, OwnerConfig, OwnerError, Response, ServiceClass};
use layerfs_overlay::{CleanupState, ProfileConfig, StatementKind};
use std::{
    fs,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
struct Noop;
impl Wake for Noop {
    fn wake(self: Arc<Self>) {}
}
#[test]
fn stopped_operator_observation_retains_the_exact_unattempted_command() {
    let directory =
        std::env::temp_dir().join(format!("layerfs-resource-stopped-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let owner = Owner::start(
        &directory.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    drop(owner);
    let before = client.diagnostics().unwrap();
    let error = ResourceObservation::acquire(&client)
        .into_diagnostic_error(None)
        .unwrap();
    let carrier = error
        .get_ref()
        .unwrap()
        .downcast_ref::<ResourceDiagnosticError>()
        .unwrap();
    assert!(carrier.output.is_none());
    {
        let original = carrier.original.lock().unwrap();
        assert!(matches!(
            &*original,
            ResourceFailure::Admission {
                cause: OwnerError::Stopped,
                command: Command::Resources { global: true }
            }
        ));
    }
    let after = client.diagnostics().unwrap();
    assert_eq!(after.admitted, before.admitted);
    assert_eq!(after.sql_foreground, before.sql_foreground);
    drop(error);
    drop(client);
    fs::remove_dir_all(directory).unwrap();
}
#[cfg(target_os = "linux")]
#[test]
fn failed_resource_receipt_and_real_output_failure_keep_both_originals_and_credit() {
    use std::io::Write;
    let directory =
        std::env::temp_dir().join(format!("layerfs-resource-identity-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let backing = directory.join("overlay.sqlite");
    let owner = Owner::start(&backing, ProfileConfig::default(), OwnerConfig::default()).unwrap();
    let client = owner.client();
    let before = client.diagnostics().unwrap();
    // A real external identity change in this test's own backing directory;
    // no product hook, mocked engine or failed-operation replay is involved.
    fs::rename(&backing, directory.join("original.sqlite")).unwrap();
    fs::write(&backing, b"different physical inode").unwrap();
    let observed = ResourceObservation::acquire(&client);
    let held = client.diagnostics().unwrap();
    assert_eq!(held.admitted - before.admitted, 1);
    assert_eq!(held.outstanding, before.outstanding + 1);
    assert!(held.credited_bytes > before.credited_bytes);
    let output = fs::File::options()
        .write(true)
        .open("/dev/full")
        .unwrap()
        .write(b"numeric diagnostic fixture")
        .unwrap_err();
    assert_eq!(output.raw_os_error(), Some(28));
    let error = observed.into_diagnostic_error(Some(output)).unwrap();
    let carrier = error
        .get_ref()
        .unwrap()
        .downcast_ref::<ResourceDiagnosticError>()
        .unwrap();
    assert_eq!(carrier.output.as_ref().unwrap().raw_os_error(), Some(28));
    {
        let original = carrier.original.lock().unwrap();
        let completion = match &*original {
            ResourceFailure::Completion(completion) => completion,
            other => panic!("{other:?}"),
        };
        assert!(matches!(
            completion.result(),
            Err(OwnerError::Overlay(layerfs_overlay::OverlayError::Invalid(
                "allocation path identity"
            )))
        ));
        assert_eq!(completion.work().sql.total().executions, 3);
    }
    assert_eq!(
        client.diagnostics().unwrap().credited_bytes,
        held.credited_bytes
    );
    assert_eq!(client.diagnostics().unwrap().outstanding, held.outstanding);
    drop(error);
    drop(client);
    drop(owner);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn global_resources_is_read_only_before_bind_and_after_terminal_cleanup() {
    let directory = std::env::temp_dir().join(format!(
        "layerfs-resource-observation-{}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    let owner = Owner::start(
        &directory.join("overlay.sqlite"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let before = client.diagnostics().unwrap();
    let done = client
        .try_submit(None, Command::Resources { global: true })
        .unwrap()
        .wait()
        .unwrap();
    let resources = match done.result() {
        Ok(Response::Resources(resources)) => **resources,
        other => panic!("{other:?}"),
    };
    assert_eq!(resources.counts.namespaces, 0);
    let sql = done.work().sql.family(StatementKind::Startup).unwrap();
    assert_eq!(sql.attempts, 3);
    assert_eq!(sql.executions, 3);
    assert_eq!(sql.rows_returned, 3);
    assert_eq!(sql.rows_changed, 0);
    assert!(sql.vm_steps > 0);
    assert!(done.work().sql.family(StatementKind::Begin).is_none());
    assert!(done.work().sql.family(StatementKind::Commit).is_none());
    assert_eq!(done.work().allocation.observations, 1);
    let held = client.diagnostics().unwrap();
    assert_eq!(held.outstanding, before.outstanding + 1);
    assert!(held.credited_bytes > before.credited_bytes);
    drop(done);
    let after = client.diagnostics().unwrap();
    assert_eq!(after.admitted - before.admitted, 1);
    assert_eq!(
        after.completed[ServiceClass::Read as usize]
            - before.completed[ServiceClass::Read as usize],
        1
    );
    assert_eq!(after.scheduler_bytes, before.scheduler_bytes);
    // The caller released its reference; the original publisher's reference
    // may still hold that same credit. No wait or retry forces a zero snapshot.
    assert!((before.credited_bytes..=held.credited_bytes).contains(&after.credited_bytes));
    assert!((before.outstanding..=held.outstanding).contains(&after.outstanding));
    assert!(matches!(
        client.try_submit(None, Command::Resources { global: false }),
        Err((OwnerError::InvalidAdmission, _))
    ));
    let open = client
        .try_submit(
            None,
            Command::Open {
                incarnation: [7; 32],
                base_root: [8; 32],
            },
        )
        .unwrap()
        .wait()
        .unwrap();
    let route = match open.result() {
        Ok(Response::Opened(route)) => *route,
        other => panic!("{other:?}"),
    };
    drop(open);
    let routed = client
        .try_submit(Some(route), Command::Resources { global: true })
        .unwrap()
        .wait()
        .unwrap();
    assert!(
        matches!(routed.result(),Ok(Response::Resources(resources)) if resources.counts.namespaces==1)
    );
    assert_eq!(
        routed
            .work()
            .sql
            .family(StatementKind::Workspace)
            .unwrap()
            .executions,
        1,
        "routed validation retained"
    );
    drop(routed);
    let closed = client
        .try_submit(Some(route), Command::Close)
        .unwrap()
        .wait()
        .unwrap();
    assert!(matches!(closed.result(), Ok(Response::Done)));
    drop(closed);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let observed = client
            .try_submit(
                None,
                Command::ObserveCleanup {
                    namespace: route.namespace(),
                    incarnation: [7; 32],
                },
            )
            .unwrap()
            .wait()
            .unwrap();
        let gone = matches!(
            observed.result(),
            Ok(Response::CleanupState(CleanupState::Gone))
        );
        drop(observed);
        if gone {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "terminal cleanup failed to reach Gone"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let mut admission = client
        .submit_when_available(None, Command::Resources { global: true })
        .unwrap();
    let waker = Waker::from(Arc::new(Noop));
    let pending = match Pin::new(&mut admission).poll(&mut Context::from_waker(&waker)) {
        Poll::Ready(result) => result.unwrap(),
        Poll::Pending => panic!("idle resource admission unexpectedly pending"),
    };
    let final_resources = pending.wait().unwrap();
    assert!(
        matches!(final_resources.result(),Ok(Response::Resources(resources)) if resources.counts.namespaces==0)
    );
    assert_eq!(
        final_resources
            .work()
            .sql
            .family(StatementKind::Startup)
            .unwrap()
            .executions,
        3
    );
    drop(final_resources);
    drop(client);
    drop(owner);
    fs::remove_dir_all(directory).unwrap();
}
