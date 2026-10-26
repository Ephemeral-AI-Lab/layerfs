//! Public control lifetime and backing-file custody proofs for private overlay files.
#[allow(dead_code)]
#[path = "support/control_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/host_install.rs"]
mod support;
use layerfs_bridge::control::{CleanupObservation, Reply, Request, WorkspaceToken};
use layerfs_history::{BranchId, WorkspaceId};

fn token(reply: &Reply) -> WorkspaceToken {
    match reply {
        Reply::Bound { token, .. } => *token,
        other => panic!("{other:?}"),
    }
}

#[test]
fn terminal_unmount_deletes_only_its_file_after_retained_requests_release() {
    let f = fixture::Fixture::new("issue321-files");
    let branch = BranchId::from_bytes(f.host.project.manifest.branch).unwrap();
    let template = f.host.directory.join("overlay.sqlite");
    let pristine = std::fs::read(&template).unwrap();
    let mount = |tag| {
        f.service
            .execute_control(&Request::Mount {
                workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
                branch,
            })
            .unwrap()
    };
    let a = mount(111);
    let b = mount(112);
    let (a_token, b_token) = (token(&a.reply), token(&b.reply));
    assert_eq!((a_token.namespace, b_token.namespace), (1, 2));
    let held_a = f.service.operation(a_token).unwrap();
    let a_file = f.host.directory.join("1.sqlite");
    let b_file = f.host.directory.join("2.sqlite");
    assert!(a_file.is_file() && b_file.is_file());
    fixture::write(&f.service, a_token, 87);
    fixture::write(&f.service, b_token, 88);
    let closed = f
        .service
        .execute_control(&Request::Unmount(layerfs_bridge::control::UnmountControl {
            token: a_token,
            discard: true,
        }))
        .unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(a_token));
    drop(closed);
    assert!(a_file.exists(), "retained request owns the file");
    let queued = f
        .service
        .execute_control(&Request::Cleanup(a_token))
        .unwrap();
    assert_eq!(
        queued.reply,
        Reply::Cleanup {
            token: a_token,
            state: CleanupObservation::Queued
        }
    );
    drop(queued);

    drop((a, held_a));
    assert!(!a_file.exists(), "last request releases the closed file");
    assert!(b_file.exists());
    assert!(f.service.execute_control(&Request::Status(b_token)).is_ok());
    let c = mount(113);
    let c_token = token(&c.reply);
    assert_eq!(c_token.namespace, 3);
    assert_eq!(std::fs::read(&template).unwrap(), pristine);
    drop(
        f.service
            .execute_control(&Request::Unmount(layerfs_bridge::control::UnmountControl {
                token: b_token,
                discard: true,
            }))
            .unwrap(),
    );
    drop(b);
    drop(
        f.service
            .execute_control(&Request::Unmount(layerfs_bridge::control::UnmountControl {
                token: c_token,
                discard: true,
            }))
            .unwrap(),
    );
    drop(c);
    assert!(!b_file.exists());
    assert!(!f.host.directory.join("3.sqlite").exists());
    f.cleanup();
}

#[test]
fn a_replaced_terminal_path_is_retained_and_never_deleted_on_a_guess() {
    let f = fixture::Fixture::new("issue321-delete-custody");
    let branch = BranchId::from_bytes(f.host.project.manifest.branch).unwrap();
    let bound = f
        .service
        .execute_control(&Request::Mount {
            workspace: WorkspaceId::from_authority([117; 32]).unwrap(),
            branch,
        })
        .unwrap();
    let selected = token(&bound.reply);
    let held = f.service.operation(selected).unwrap();
    let path = f.host.directory.join("1.sqlite");
    drop(
        f.service
            .execute_control(&Request::Unmount(layerfs_bridge::control::UnmountControl {
                token: selected,
                discard: true,
            }))
            .unwrap(),
    );
    let original = std::fs::read(&path).unwrap();
    let retained = f.host.directory.join("retained.sqlite");
    std::fs::rename(&path, &retained).unwrap();
    std::fs::write(&path, b"replacement custody").unwrap();
    drop((bound, held));
    assert_eq!(std::fs::read(&path).unwrap(), b"replacement custody");
    assert_eq!(std::fs::read(&retained).unwrap(), original);
    assert!(f
        .service
        .execute_control(&Request::Cleanup(selected))
        .is_err());
    let other = f
        .service
        .execute_control(&Request::Mount {
            workspace: WorkspaceId::from_authority([118; 32]).unwrap(),
            branch,
        })
        .unwrap();
    let other_token = token(&other.reply);
    assert_eq!(other_token.namespace, 2);
    fixture::write(&f.service, other_token, 89);
    drop(
        f.service
            .execute_control(&Request::Unmount(layerfs_bridge::control::UnmountControl {
                token: other_token,
                discard: true,
            }))
            .unwrap(),
    );
    drop(other);
    assert_eq!(std::fs::read(&path).unwrap(), b"replacement custody");
    f.cleanup();
}
