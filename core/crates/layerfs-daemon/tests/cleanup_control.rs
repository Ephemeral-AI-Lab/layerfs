#[allow(dead_code)]
#[path = "support/control_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_bridge::control::{CleanupObservation, Reply, Request, WorkspaceToken};
use layerfs_daemon::{Command, OwnerError, Response};
use layerfs_history::WorkspaceId;
use std::time::{Duration, Instant};

#[test]
fn explicit_cleanup_observes_after_registry_removal_without_recreating_a_binding() {
    let f = fixture::Fixture::new("r7-cleanup-control");
    let workspace = WorkspaceId::from_authority([94; 32]).unwrap();
    let bound = f
        .service
        .execute_control(&Request::Mount {
            workspace,
            branch: layerfs_history::BranchId::from_bytes(f.native.project.manifest.branch)
                .unwrap(),
        })
        .unwrap();
    let token = match bound.reply {
        Reply::Bound { token, .. } => token,
        other => panic!("{other:?}"),
    };
    drop(bound);
    let observed = f.service.execute_control(&Request::Cleanup(token)).unwrap();
    assert_eq!(
        observed.reply,
        Reply::Cleanup {
            token,
            state: CleanupObservation::Live
        }
    );
    drop(observed);
    let wrong = WorkspaceToken {
        workspace: WorkspaceId::from_authority([95; 32]).unwrap(),
        ..token
    };
    assert!(f.service.execute_control(&Request::Cleanup(wrong)).is_err());
    let closed = f.service.execute_control(&Request::Unmount(token)).unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(token));
    drop(closed);
    assert!(f.service.execute_control(&Request::Status(token)).is_err());
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let observed = f.service.execute_control(&Request::Cleanup(token)).unwrap();
        if observed.reply
            == (Reply::Cleanup {
                token,
                state: CleanupObservation::Gone,
            })
        {
            break;
        }
        drop(observed);
        assert!(
            Instant::now() < deadline,
            "automatic namespace reclamation did not finish"
        );
        std::thread::yield_now();
    }
    assert!(f.service.execute_control(&Request::Status(token)).is_err());
    // Diagnostic params can never authorize a mutable routed operation.
    let wrong_route = f.owner.client().try_submit(None, Command::State);
    assert!(matches!(
        wrong_route,
        Err((OwnerError::InvalidAdmission, Command::State))
    ));
    let done = f
        .owner
        .client()
        .try_submit(
            None,
            Command::ObserveCleanup {
                namespace: token.namespace,
                incarnation: workspace.to_bytes(),
            },
        )
        .unwrap()
        .wait()
        .unwrap();
    assert!(matches!(
        done.result(),
        Ok(Response::CleanupState(layerfs_overlay::CleanupState::Gone))
    ));
    drop(done);
    f.cleanup();
}
