//! A conflicted Commit's report never claims a complete written-path list it
//! does not hold (#355 F-041). The merge lists at most 4,096 paths it changed
//! in the Workspace; past that the reply says `truncated`, below it the list
//! is complete and `truncated` is false.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::{CommitControl, Reply, Request, UnmountControl, WorkspaceToken};
use layerfs_content::filesystem::PathName;
use layerfs_daemon::control::Service;
use layerfs_history::{BranchId, HistoryName, LayerStackId, WorkspaceId};
use layerfs_workspace::{Operation, Outcome, Position, Time};
use std::fs;

const LANE: &str = "lane";
/// The report's bound on written paths.
const WRITTEN_PATHS: usize = 4096;

/// Mounts a Workspace on the lane and creates `/x.txt` holding `ours`.
fn mounted(service: &Service, id: u8) -> WorkspaceToken {
    let bound = service
        .execute_control(&Request::Mount {
            workspace: WorkspaceId::from_authority([id; 32]).unwrap(),
            branch: BranchId::named(
                LayerStackId::from_authority([23; 16]),
                &HistoryName::new(LANE).unwrap(),
            ),
        })
        .unwrap();
    let Reply::Bound { token, .. } = bound.reply else {
        panic!("binding: {:?}", bound.reply);
    };
    let operation = service.operation(token).unwrap();
    let route = operation.workspace().route();
    let source = operation
        .overlay()
        .acquire_base_source(route, 9000 + u64::from(id))
        .unwrap();
    let view = operation.workspace().view_for_source(source).unwrap();
    let now = Time {
        seconds: 123,
        nanoseconds: 0,
    };
    let apply = |op| {
        let outcome = operation
            .workspace()
            .mutate(operation.overlay(), operation.ports(), &view, op, now)
            .unwrap();
        let Outcome::Applied { publication, stat } = outcome else {
            panic!("mutation: {outcome:?}");
        };
        operation.overlay().reply_attempted(publication).unwrap();
        stat
    };
    let created = apply(Operation::Create {
        parent: view.root_serial(),
        name: PathName::new("x.txt").unwrap(),
        mode: 0o644,
    })
    .expect("created inode");
    apply(Operation::Write {
        serial: created.serial,
        position: Position::At(0),
        data: b"ours\n".to_vec().into(),
    });
    drop(view);
    operation
        .overlay()
        .release_base_source(route, source)
        .unwrap();
    token
}

/// Lands `/x.txt` (theirs) and `count` other files on the lane after the
/// Workspace bound, then Commits it: the reply is the conflict report.
fn conflicted_commit(label: &str, id: u8, count: usize) -> (Vec<String>, bool) {
    let sdk = support::Fixture::new(label, |source| {
        fs::write(source.join("seed.txt"), b"seed\n").unwrap();
        1
    });
    sdk.fork(LANE);
    let service = Service::new(sdk.service.store.clone(), &sdk.owner);
    let token = mounted(&service, id);
    let mut changes = vec![support::write("/x.txt", "theirs\n")];
    changes.extend((0..count).map(|n| support::write(&format!("/f{n:05}"), "x\n")));
    sdk.apply(LANE, changes);
    let reply = service
        .execute_control(&Request::Commit(CommitControl {
            token,
            force: false,
            message: None,
        }))
        .unwrap()
        .reply;
    service
        .execute_control(&Request::Unmount(UnmountControl {
            token,
            discard: true,
        }))
        .unwrap();
    let Reply::Conflict(report) = reply else {
        panic!("expected a conflict: {reply:?}");
    };
    assert!(!report.markers);
    assert_eq!(report.conflicts.len(), 1, "{:?}", report.conflicts);
    drop(service);
    sdk.cleanup();
    (report.written, report.truncated)
}

#[test]
fn a_written_list_below_its_bound_is_complete() {
    let (written, truncated) = conflicted_commit("merge-report-small", 61, 10);
    assert!(!truncated);
    assert_eq!(written.len(), 11, "{written:?}");
    assert!(written.iter().any(|path| path == "/x.txt"));
    assert!(written.iter().any(|path| path == "/f00009"));
}

#[test]
fn a_written_list_past_its_bound_says_truncated() {
    let (written, truncated) = conflicted_commit("merge-report-large", 62, WRITTEN_PATHS + 100);
    assert!(truncated, "{} paths listed", written.len());
    assert!(written.len() <= WRITTEN_PATHS, "{}", written.len());
    assert!(!written.is_empty());
}
