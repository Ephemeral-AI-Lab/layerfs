//! #64 option B: a named mount (`Request::MountNamed`) binds the Branch its
//! project and Branch names select, found by the daemon on the Workspace's
//! own reader, so its caller reads no Store. A missing project or Branch is
//! the Store service lookup's refusal, code and message unchanged, with the
//! admission withdrawn: the same Workspace identity then mounts. An invalid
//! project name, an invalid Branch name and `main` refuse as the lookup
//! does. On the installed SQLite Store; the PostgreSQL statement is counted
//! by store-service `postgresql_open_cost` and `postgresql_phase_attribution`.
#[allow(dead_code)]
#[path = "support/control_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "support/host_install.rs"]
mod support;
use layerfs_bridge::control::{Reply, Request};
use layerfs_history::{BranchId, ForkRequest, ForkSource, HistoryName, LayerStackId, WorkspaceId};

fn named(f: &fixture::Fixture, tag: u8, project: &str, branch: &str) -> Reply {
    f.service
        .execute_control(&Request::MountNamed {
            workspace: WorkspaceId::from_authority([tag; 32]).unwrap(),
            project: project.to_owned(),
            branch: branch.to_owned(),
        })
        .unwrap()
        .reply
}

fn refused(reply: Reply) -> (String, String) {
    match reply {
        Reply::SdkRefused(refusal) => (refusal.code, refusal.message),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_named_mount_binds_its_branch_and_refuses_as_the_lookup_does() {
    let f = fixture::Fixture::new("issue64-named");
    let store = &f.installed.opened.store;
    let stack = LayerStackId::from_authority([54; 16]);
    let base = store
        .history()
        .layer_stack(stack)
        .unwrap()
        .unwrap()
        .head_layer;
    let work = BranchId::from_authority([64; 16]);
    store
        .history()
        .fork(&ForkRequest {
            stack,
            branch: work,
            name: HistoryName::new("work").unwrap(),
            source: ForkSource::Layer(base),
        })
        .unwrap();

    let refusals = [
        (
            "ghost",
            "work",
            "NoSuchProject",
            "project ghost does not exist",
        ),
        (
            "Not A Name",
            "work",
            "NoSuchProject",
            "project Not A Name does not exist",
        ),
        (
            "project",
            "ghost",
            "NoSuchBranch",
            "Branch ghost does not exist",
        ),
        (
            "project",
            "main",
            "NoSuchBranch",
            "Branch main does not exist",
        ),
        (
            "project",
            "Not A Name",
            "NoSuchBranch",
            "Branch Not A Name does not exist",
        ),
        (
            "ghost",
            "Not A Name",
            "NoSuchProject",
            "project ghost does not exist",
        ),
    ];
    for (project, branch, code, message) in refusals {
        // the same identity each time: a refusal leaves no admission
        let (got_code, got_message) = refused(named(&f, 7, project, branch));
        assert_eq!((got_code.as_str(), got_message.as_str()), (code, message));
    }

    // a SQLite Store keeps no idle Workspace readers: no eager reader
    assert!(store.prefill_reader().is_none());
    let token = match named(&f, 7, "project", "work") {
        Reply::Bound { token, binding } => {
            assert_eq!(
                token.workspace,
                WorkspaceId::from_authority([7; 32]).unwrap()
            );
            assert_eq!(binding.branch.id, work);
            assert_eq!(binding.branch.name.as_str(), "work");
            token
        }
        other => panic!("{other:?}"),
    };
    // the bound identity is admitted now: a second mount of it is refused
    let again = f.service.execute_control(&Request::MountNamed {
        workspace: WorkspaceId::from_authority([7; 32]).unwrap(),
        project: "project".to_owned(),
        branch: "work".to_owned(),
    });
    assert!(again.is_err(), "an admitted identity is not bound twice");
    drop(again);
    let closed = f
        .service
        .execute_control(&Request::Unmount(layerfs_bridge::control::UnmountControl {
            token,
            discard: true,
        }))
        .unwrap();
    assert_eq!(closed.reply, Reply::Unmounted(token));
    drop(closed);
    f.cleanup();
}
