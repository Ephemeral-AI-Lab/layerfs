//! A checked Unmount refuses only changes a Commit would keep, and names them.
#[allow(dead_code)]
#[path = "../../layerfs-daemon/tests/support/control_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "../../layerfs-daemon/tests/support/host_install.rs"]
mod support;
use layerfs_api::{internal::Control, Unmounted};
use layerfs_content::filesystem::PathName;
use layerfs_daemon::control::Service;
use layerfs_history::WorkspaceId;
use layerfs_workspace::{Operation, Outcome, Time};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

const NOW: Time = Time {
    seconds: 1_800_000_000,
    nanoseconds: 0,
};

fn serve(
    service: Arc<Service>,
    policy: layerfs_content::ConstructionPolicy,
) -> (Control, support::Worker<()>) {
    let (channel, worker) = support::pair(move |connection| {
        while let Ok(served) = service.serve_one(connection, |save, _, snapshot| {
            fixture::construct(save, snapshot, b'D', policy)
        }) {
            drop(served);
        }
    });
    (Control::new(channel), worker)
}

fn name(value: &str) -> PathName {
    PathName::new(value).unwrap()
}

/// Base serials of `/`, `/.git` and `/.git/index`, which has one name.
#[derive(Clone, Copy)]
struct Serials {
    root: u64,
    git: u64,
    index: u64,
}

/// Applies each operation as its own mutation, then asks for a checked
/// Unmount without `discard`.
fn refusal(tag: u8, operations: impl Fn(Serials) -> Vec<Operation>) -> Unmounted {
    static NEXT: AtomicU64 = AtomicU64::new(700);
    let f = fixture::Fixture::new(&format!("unmount-dirty-{tag}"));
    let branch = f.host.project.branch.branch.id;
    let (mut client, worker) = serve(
        f.service.clone(),
        f.installed.opened.store.policy().construction(),
    );
    let workspace = WorkspaceId::from_authority([tag; 32]).unwrap();
    let token = layerfs_api::internal::workspace(&mut client)
        .bind(workspace, branch)
        .unwrap()
        .token;
    let operation = f.service.operation(token).unwrap();
    let route = operation.workspace().route();
    let source = operation
        .overlay()
        .acquire_base_source(route, NEXT.fetch_add(1, Ordering::Relaxed))
        .unwrap();
    let view = operation.workspace().view_for_source(source).unwrap();
    let root = view.root_serial();
    let git = view
        .lookup(operation.overlay(), root, &name(".git"))
        .unwrap()
        .serial;
    let index = view
        .lookup(operation.overlay(), git, &name("index"))
        .unwrap()
        .serial;
    for change in operations(Serials { root, git, index }) {
        let changed = operation
            .workspace()
            .mutate(operation.overlay(), operation.ports(), &view, change, NOW)
            .unwrap();
        match changed {
            Outcome::Applied { publication, .. } => {
                operation.overlay().reply_attempted(publication).unwrap()
            }
            other => panic!("{other:?}"),
        }
    }
    drop(view);
    operation
        .overlay()
        .release_base_source(route, source)
        .unwrap();
    drop(operation);
    let refused = layerfs_api::internal::workspace(&mut client)
        .unmount_checked(token, false)
        .unwrap();
    if matches!(refused, Unmounted::Dirty(_)) {
        let closed = layerfs_api::internal::workspace(&mut client)
            .unmount_checked(token, true)
            .unwrap();
        assert!(matches!(closed, Unmounted::Closed), "{closed:?}");
    }
    drop(client);
    worker.join();
    f.cleanup();
    refused
}

/// The operations of one case, given the base serials.
type Change = dyn Fn(Serials) -> Vec<Operation>;

fn named(refused: Unmounted) -> (Vec<String>, bool) {
    match refused {
        Unmounted::Dirty(report) => (report.paths.clone(), report.truncated),
        other => panic!("expected a Dirty refusal, got {other:?}"),
    }
}

fn chmod(serial: u64) -> Operation {
    Operation::SetAttributes {
        serial,
        mode: Some(0o711),
        mtime: None,
        size: None,
    }
}

fn mtime(serial: u64) -> Operation {
    Operation::SetAttributes {
        serial,
        mode: None,
        mtime: Some(Time {
            seconds: NOW.seconds + 1,
            nanoseconds: 0,
        }),
        size: None,
    }
}

#[test]
fn changes_a_commit_drops_do_not_refuse_an_unmount() {
    let cases: [(u8, &str, &Change); 4] = [
        (41, "touch .", &|at| vec![mtime(at.root)]),
        (42, "touch .git", &|at| vec![mtime(at.git)]),
        (43, "create and unlink", &|Serials { root, .. }| {
            vec![
                Operation::Create {
                    parent: root,
                    name: name(".probe"),
                    mode: 0o644,
                },
                Operation::Unlink {
                    parent: root,
                    name: name(".probe"),
                },
            ]
        }),
        (44, "mkdir and rmdir", &|Serials { root, .. }| {
            vec![
                Operation::Mkdir {
                    parent: root,
                    name: name(".d"),
                    mode: 0o755,
                },
                Operation::Rmdir {
                    parent: root,
                    name: name(".d"),
                },
            ]
        }),
    ];
    for (tag, label, operations) in cases {
        let refused = refusal(tag, operations);
        assert!(matches!(refused, Unmounted::Closed), "{label}: {refused:?}");
    }
}

#[test]
fn a_refusal_names_every_change_it_counts() {
    let root = refusal(51, |at| vec![chmod(at.root)]);
    assert_eq!(named(root), (vec!["/".to_owned()], false));
    let git = refusal(52, |at| vec![chmod(at.git)]);
    assert_eq!(named(git), (vec!["/.git".to_owned()], false));
    let kept = refusal(53, |at| {
        vec![Operation::Create {
            parent: at.root,
            name: name("kept"),
            mode: 0o644,
        }]
    });
    assert_eq!(named(kept), (vec!["/kept".to_owned()], false));
    let touched = refusal(54, |at| vec![mtime(at.index)]);
    assert_eq!(named(touched), (vec!["/.git/index".to_owned()], false));
}
