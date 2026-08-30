use super::CliSession;
use crate::{
    BranchId, BranchRelation, CliEvent, DiffRequest, FinishedStatus, PageRequest, RemotePlacement,
    ViewQuery, ViewSnapshot,
};

fn finish(session: &CliSession, command: &str) -> CliEvent {
    let command = CliSession::parse_line(command).unwrap();
    let mut handle = session.execute(command).unwrap();
    let mut terminal = None;
    while let Some(event) = handle.next_event().unwrap() {
        if matches!(event, CliEvent::Finished { .. }) {
            terminal = Some(event);
        }
    }
    terminal.expect("Finished")
}

fn main_branch(session: &CliSession) -> crate::BranchView {
    let snapshot = session
        .snapshot(ViewQuery::Branch {
            id: BranchId::from("B-main"),
            page: PageRequest::first(64),
        })
        .unwrap();
    let ViewSnapshot::Branch(snapshot, _) = snapshot else {
        panic!("Branch")
    };
    snapshot
        .branches
        .items
        .into_iter()
        .find(|branch| branch.id == BranchId::from("B-main"))
        .unwrap()
}

#[test]
fn operation_is_incremental_and_applies_at_finished() {
    let session = CliSession::open("mock").unwrap();
    let command =
        CliSession::parse_line("branch pull B-main --through C-B-main-42 --replica").unwrap();
    let mut handle = session.execute(command).unwrap();
    let before = session
        .snapshot(ViewQuery::Project {
            id: "SA-91".into(),
            page: PageRequest::first(64),
        })
        .unwrap();
    let ViewSnapshot::Project(before) = before else {
        panic!("Project")
    };
    let main = before
        .branches
        .items
        .iter()
        .find(|branch| branch.id == BranchId::from("B-main"))
        .unwrap();
    assert_eq!(main.work_number, Some(37));
    assert!(matches!(
        handle.try_next_event().unwrap(),
        Some(CliEvent::Started { .. })
    ));
    assert!(handle.try_next_event().unwrap().is_none());
    let mut finished = 0;
    for _ in 0..32 {
        if let Some(event) = handle.next_event().unwrap() {
            if matches!(
                event,
                CliEvent::Finished {
                    status: FinishedStatus::Succeeded,
                    ..
                }
            ) {
                finished += 1;
            }
        }
    }
    assert_eq!(finished, 1);
    assert_eq!(main_branch(&session).work_number, Some(42));
}

#[test]
fn interrupt_finishes_once_without_mutation() {
    let session = CliSession::open("mock").unwrap();
    let command = CliSession::parse_line("layerstack pull --through L-A-19 --replica").unwrap();
    let mut handle = session.execute(command).unwrap();
    handle.interrupt().unwrap();
    assert!(matches!(
        handle.try_next_event().unwrap(),
        Some(CliEvent::Finished {
            status: FinishedStatus::Interrupted,
            ..
        })
    ));
    assert!(handle.try_next_event().unwrap().is_none());
}

#[test]
fn fork_never_hides_pull() {
    let session = CliSession::open("mock").unwrap();
    for command in [
        "branch fork --name bad-commit --branch B-main --commit C-B-main-42",
        "branch fork --name bad-layer --layer L-A-19",
    ] {
        let command = CliSession::parse_line(command).unwrap();
        assert!(session.execute(command).is_err());
    }
    let boundary = CliSession::parse_line(
        "branch fork --name bad-boundary --branch B-remote-new --commit C-B-main-40",
    )
    .unwrap();
    assert!(session.execute(boundary).is_err());
}

#[test]
fn branch_pull_is_monotonic_and_retains_replica_coverage() {
    let session = CliSession::open("mock").unwrap();
    let event = finish(
        &session,
        "branch pull B-main --through C-B-main-20 --replica",
    );
    assert!(matches!(
        event,
        CliEvent::Finished {
            result: Ok(crate::CommandResult::Pull(ref value)),
            receipt: crate::OperationReceipt {
                facts_inserted: 0,
                objects_sent: 0,
                ..
            },
            ..
        } if value.contains("AlreadyContained")
    ));
    assert_eq!(main_branch(&session).work_number, Some(37));
    let event = finish(
        &session,
        "branch pull B-main --through C-B-main-37 --reference",
    );
    assert!(matches!(
        event,
        CliEvent::Finished {
            result: Ok(crate::CommandResult::Pull(ref value)),
            receipt: crate::OperationReceipt { objects_sent: 0, .. },
            ..
        } if value.contains("ModeChanged")
    ));
    let main = main_branch(&session);
    assert!(matches!(
        main.relation,
        BranchRelation::RemotePullBehind {
            mode: RemotePlacement::Reference,
            ..
        }
    ));
    assert_eq!(
        main.remote_complete_through
            .as_ref()
            .map(ToString::to_string),
        Some("C-B-main-37".into())
    );
}

#[test]
fn workspace_process_ids_and_retained_delta_rules_are_enforced() {
    let session = CliSession::open("mock").unwrap();
    assert!(matches!(
        finish(&session, "workspace output E-W14"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    assert!(matches!(
        finish(&session, "workspace commit W31"),
        CliEvent::Finished {
            status: FinishedStatus::Failed,
            result: Err(crate::CliError::HeadMoved(_)),
            ..
        }
    ));
    for command in [
        "workspace exec W14 -- cargo test",
        "workspace exec W31 -- cargo test",
        "workspace exec W44 -- cargo test",
    ] {
        assert!(matches!(
            finish(&session, command),
            CliEvent::Finished {
                status: FinishedStatus::Failed,
                ..
            }
        ));
    }
    assert!(matches!(
        finish(&session, "workspace commit W9"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    assert!(matches!(
        finish(&session, "workspace exec W9 -- cargo test"),
        CliEvent::Finished {
            status: FinishedStatus::Failed,
            result: Err(crate::CliError::ReadOnly(_)),
            ..
        }
    ));
    assert!(matches!(
        finish(&session, "workspace end W50"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    let wrong = CliSession::parse_line(
        "workspace create --branch B-empty --initial-layer L-A-17 --at /tmp/wrong",
    )
    .unwrap();
    assert!(session.execute(wrong).is_err());
}

#[test]
fn diff_cursor_is_consumable_and_fork_receipt_is_zero_copy() {
    let session = CliSession::open("mock").unwrap();
    let request = DiffRequest::BranchCommits {
        branch: "B-search-a".into(),
        from: "C-B-search-a-05".into(),
        to: "C-B-search-a-08".into(),
    };
    let first = session
        .snapshot(ViewQuery::Diff {
            request: request.clone(),
            page: PageRequest::first(2),
        })
        .unwrap();
    let ViewSnapshot::Diff(first) = first else {
        panic!("Diff")
    };
    assert_eq!(first.entries.items.len(), 2);
    let second = session
        .snapshot(ViewQuery::Diff {
            request,
            page: PageRequest {
                after: first.entries.next,
                limit: 2,
            },
        })
        .unwrap();
    let ViewSnapshot::Diff(second) = second else {
        panic!("Diff")
    };
    assert_eq!(second.entries.items.len(), 2);
    assert!(matches!(
        finish(
            &session,
            "branch fork --name zero-copy --branch B-main --commit C-B-main-35"
        ),
        CliEvent::Finished {
            receipt: crate::OperationReceipt {
                objects_announced: 0,
                objects_sent: 0,
                ..
            },
            ..
        }
    ));
}

#[test]
fn forked_branch_pull_preserves_exact_boundary_and_visible_ancestry() {
    let session = CliSession::open("mock").unwrap();
    assert!(matches!(
        finish(
            &session,
            "branch pull B-remote-new --through C-B-remote-new-03 --replica"
        ),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    let snapshot = session
        .snapshot(ViewQuery::Branch {
            id: BranchId::from("B-remote-new"),
            page: PageRequest::first(64),
        })
        .unwrap();
    let ViewSnapshot::Branch(snapshot, _) = snapshot else {
        panic!("Branch")
    };
    let branch = snapshot
        .branches
        .items
        .iter()
        .find(|branch| branch.id == BranchId::from("B-remote-new"))
        .unwrap();
    assert_eq!(branch.work_number, Some(3));
    assert!(branch.visible_roots_complete);
    assert!(branch
        .commits
        .iter()
        .any(|commit| { commit.id.as_str() == "C-B-main-40" && commit.inherited && commit.work }));
    let event = finish(
        &session,
        "branch fork --name exact-boundary --branch B-remote-new --commit C-B-main-40",
    );
    assert!(matches!(
        event,
        CliEvent::Finished {
            result: Ok(crate::CommandResult::Fork { ref origin, .. }),
            ..
        } if origin == "B-remote-new/C-B-main-40"
    ));
}

#[test]
fn add_is_idempotent_and_rejects_stale_base() {
    let session = CliSession::open("mock").unwrap();
    assert!(matches!(
        finish(&session, "layerstack add B-local-sync"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            result: Ok(crate::CommandResult::NeedsResolution {
                ref workspace_id,
                ref old_base,
                ref current_layer,
                conflict_count: 2,
            }),
            receipt: crate::OperationReceipt {
                facts_inserted: 0,
                objects_sent: 0,
                ..
            },
            ..
        } if workspace_id.as_str() == "W90"
            && old_base.as_str() == "L-A-15"
            && current_layer.as_str() == "L-A-19"
    ));
    assert!(matches!(
        finish(&session, "workspace commit W90"),
        CliEvent::Finished {
            status: FinishedStatus::Failed,
            ..
        }
    ));
    assert!(matches!(
        finish(&session, "workspace resolve W90 conflict-1 --working-tree"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    assert!(matches!(
        finish(&session, "workspace commit W90"),
        CliEvent::Finished {
            status: FinishedStatus::Failed,
            ..
        }
    ));
    assert!(matches!(
        finish(&session, "workspace resolve W90 conflict-1 --working-tree"),
        CliEvent::Finished {
            status: FinishedStatus::Failed,
            result: Err(crate::CliError::NotFound(_)),
            ..
        }
    ));
    assert!(matches!(
        finish(&session, "workspace resolve W90 conflict-2 --layer"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    assert!(matches!(
        finish(&session, "workspace commit W90"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    assert!(matches!(
        finish(&session, "branch push B-local-sync"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    assert!(matches!(
        finish(&session, "layerstack add B-local-sync"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            result: Ok(crate::CommandResult::Add(ref value)),
            ..
        } if value.contains("Added L-A-20")
    ));
    assert!(matches!(
        finish(&session, "layerstack add B-rollout-4"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            result: Ok(crate::CommandResult::Add(ref value)),
            ..
        } if value.contains("AlreadyAccepted L-A-16")
    ));
}

#[test]
fn unavailable_authority_blocks_pull() {
    let session = CliSession::open("mock").unwrap();
    for command in [
        "layerstack pull --through L-SE-07 --replica",
        "branch pull BE-release --through C-BE-release-05 --replica",
    ] {
        let command = CliSession::parse_line(command).unwrap();
        assert!(matches!(
            session.execute(command),
            Err(crate::CliError::AuthorityUnavailable(_))
        ));
    }
}
