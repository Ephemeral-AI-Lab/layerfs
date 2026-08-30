use super::CliSession;
use crate::{
    BranchId, BranchRelation, CliEvent, DiffRequest, FinishedStatus, PageRequest, RemotePlacement,
    RouteTarget, ViewQuery, ViewSnapshot,
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
        "workspace exec W14 -- /bin/bash -lc 'printf ok'",
        "workspace exec W31 -- /bin/bash -lc 'printf ok'",
        "workspace exec W44 -- /bin/bash -lc 'printf ok'",
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
        finish(&session, "workspace exec W9 -- /bin/bash -lc 'printf ok'",),
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
fn real_diff_and_fork_receipt_is_zero_copy() {
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
    assert_eq!(first.entries.items.len(), 1);
    assert_eq!(first.from, "B-search-a/C-B-search-a-05");
    assert_eq!(first.to, "B-search-a/C-B-search-a-08");
    assert_eq!(first.summary.modified, 1);
    assert_eq!(first.entries.next, None);
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
fn files_and_changes_pin_real_layer_branch_and_commit_trees() {
    let session = CliSession::open("mock").unwrap();

    let first = session
        .snapshot(ViewQuery::Files {
            target: RouteTarget::Layer("L-A-02".into()),
            page: PageRequest::first(2),
        })
        .unwrap();
    let ViewSnapshot::Files(first) = first else {
        panic!("Files")
    };
    assert_eq!(first.target, RouteTarget::Layer("L-A-02".into()));
    assert_eq!(first.resolved, first.target);
    assert_eq!(first.files.items.len(), 2);
    assert_eq!(first.files.items[0].path, "package.json");
    assert!(first
        .files
        .items
        .iter()
        .all(|file| file.allocated_bytes.is_none()));
    let next = session
        .snapshot(ViewQuery::Files {
            target: first.target.clone(),
            page: PageRequest {
                after: first.files.next,
                limit: 8,
            },
        })
        .unwrap();
    let ViewSnapshot::Files(next) = next else {
        panic!("Files")
    };
    assert_eq!(next.files.items.len(), 3);

    let layer = session
        .snapshot(ViewQuery::Changes {
            target: RouteTarget::Layer("L-A-02".into()),
            page: PageRequest::first(128),
        })
        .unwrap();
    let ViewSnapshot::Changes(layer) = layer else {
        panic!("Changes")
    };
    assert_eq!(layer.from_target, Some(RouteTarget::Layer("L-A-01".into())));
    assert_eq!(layer.to_target, RouteTarget::Layer("L-A-02".into()));
    assert_eq!(layer.summary.modified, 1);
    assert_eq!(layer.entries.items.len(), 1);

    let branch = session
        .snapshot(ViewQuery::Changes {
            target: RouteTarget::Branch("B-search-a".into()),
            page: PageRequest::first(128),
        })
        .unwrap();
    let ViewSnapshot::Changes(branch) = branch else {
        panic!("Changes")
    };
    assert_eq!(branch.target, RouteTarget::Branch("B-search-a".into()));
    assert_eq!(
        branch.from_target,
        Some(RouteTarget::Commit("B-main".into(), "C-B-main-35".into()))
    );
    assert_eq!(
        branch.to_target,
        RouteTarget::Commit("B-search-a".into(), "C-B-search-a-08".into())
    );
    assert_eq!(branch.summary.modified, 1);

    let branch_files = session
        .snapshot(ViewQuery::Files {
            target: RouteTarget::Branch("B-search-a".into()),
            page: PageRequest::first(128),
        })
        .unwrap();
    let ViewSnapshot::Files(branch_files) = branch_files else {
        panic!("Files")
    };
    assert_eq!(branch_files.resolved, branch.to_target);
    assert_eq!(branch_files.root, branch.to_root);

    let commit = session
        .snapshot(ViewQuery::Changes {
            target: RouteTarget::Commit("B-search-a".into(), "C-B-search-a-08".into()),
            page: PageRequest::first(128),
        })
        .unwrap();
    let ViewSnapshot::Changes(commit) = commit else {
        panic!("Changes")
    };
    assert_eq!(
        commit.from_target,
        Some(RouteTarget::Commit(
            "B-search-a".into(),
            "C-B-search-a-07".into()
        ))
    );
    assert_eq!(commit.summary.modified, 1);
    assert_ne!(commit.from_root, Some(commit.to_root));

    let first_commit = session
        .snapshot(ViewQuery::Changes {
            target: RouteTarget::Commit("B-search-a".into(), "C-B-search-a-01".into()),
            page: PageRequest::first(128),
        })
        .unwrap();
    let ViewSnapshot::Changes(first_commit) = first_commit else {
        panic!("Changes")
    };
    assert_eq!(
        first_commit.from_target,
        Some(RouteTarget::Commit("B-main".into(), "C-B-main-35".into()))
    );
}

#[test]
fn genesis_and_workspace_changes_share_the_final_tree_diff() {
    let session = CliSession::open("mock").unwrap();
    let genesis = session
        .snapshot(ViewQuery::Changes {
            target: RouteTarget::Layer("L-A-01".into()),
            page: PageRequest::first(2),
        })
        .unwrap();
    let ViewSnapshot::Changes(genesis) = genesis else {
        panic!("Changes")
    };
    assert_eq!(genesis.from_target, None);
    assert_eq!(genesis.from_root, None);
    assert_eq!(genesis.summary.added, 5);
    assert_eq!(genesis.entries.items.len(), 2);
    assert!(genesis.entries.next.is_some());

    let files = session
        .snapshot(ViewQuery::Files {
            target: RouteTarget::Workspace("W9".into()),
            page: PageRequest::first(128),
        })
        .unwrap();
    let ViewSnapshot::Files(files) = files else {
        panic!("Files")
    };
    assert_eq!(files.generation, Some(1));
    assert!(files
        .files
        .items
        .iter()
        .all(|file| file.allocated_bytes.is_some()));
    assert!(files
        .files
        .items
        .iter()
        .any(|file| file.path == "src/workspace-change.js"));

    let changes = session
        .snapshot(ViewQuery::Changes {
            target: RouteTarget::Workspace("W9".into()),
            page: PageRequest::first(128),
        })
        .unwrap();
    let ViewSnapshot::Changes(changes) = changes else {
        panic!("Changes")
    };
    assert_eq!(
        changes.from_target,
        Some(RouteTarget::Commit(
            "B-search-a".into(),
            "C-B-search-a-08".into()
        ))
    );
    assert_eq!(changes.to_target, RouteTarget::Workspace("W9".into()));
    assert_eq!(changes.summary.added, 1);
    assert_eq!(changes.entries.items, workspace(&session, "W9").changes);
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
    let add = CliSession::parse_line("layerstack add B-local-sync").unwrap();
    assert!(matches!(
        session.execute(add),
        Err(crate::CliError::NotPulled(_))
    ));
    assert!(matches!(
        finish(&session, "layerstack pull --through L-A-19 --reference"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
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

#[test]
fn real_db_context_use_show_and_parent_validation_are_atomic() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-mock-context-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let context = root.join("context");
    let layerstack = root.join("layerstack.sqlite");
    let other = root.join("other.sqlite");
    let branch = root.join("branch.sqlite");
    let session = CliSession::open(&context).unwrap();

    for command in [
        format!("db create layerstack {}", layerstack.display()),
        format!("db create layerstack {}", other.display()),
        format!(
            "db create branch {} --parent {}",
            branch.display(),
            layerstack.display()
        ),
        format!("db connect layerstack {}", layerstack.display()),
        format!(
            "db connect branch {} --parent {}",
            branch.display(),
            layerstack.display()
        ),
    ] {
        assert!(matches!(
            finish(&session, &command),
            CliEvent::Finished {
                status: FinishedStatus::Succeeded,
                ..
            }
        ));
    }

    let select = format!(
        "context use --layerstack {} --branch {}",
        layerstack.display(),
        branch.display()
    );
    assert_context(&session, &select, &layerstack, &branch);
    assert_context(&session, "context show", &layerstack, &branch);

    let mismatch = format!(
        "context use --layerstack {} --branch {}",
        other.display(),
        branch.display()
    );
    assert!(matches!(
        finish(&session, &mismatch),
        CliEvent::Finished {
            status: FinishedStatus::Failed,
            result: Err(crate::CliError::Integrity(_)),
            ..
        }
    ));
    assert_context(&session, "context show", &layerstack, &branch);

    assert!(matches!(
        finish(&session, "layerstack init --name real-schema --empty"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    let connection = rusqlite::Connection::open(&layerstack).unwrap();
    let projects: i64 = connection
        .query_row("SELECT count(*) FROM layer_stacks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(projects, 1);
    drop(connection);
    std::fs::rename(&layerstack, root.join("layerstack.offline")).unwrap();
    std::fs::rename(&branch, root.join("branch.offline")).unwrap();
    let offline = CliSession::open(&context).unwrap();
    assert_context(&offline, "context show", &layerstack, &branch);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn persisted_store_pair_reopens_and_accepts_more_work() {
    let root = std::env::temp_dir().join(format!(
        "layerfs-persisted-context-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let context = root.join("context");
    let layerstack = root.join("layerstack.sqlite");
    let branch_store = root.join("branch.sqlite");
    let first_workspace = root.join("W90");
    let session = CliSession::open(&context).unwrap();
    for command in [
        format!("db create layerstack {}", layerstack.display()),
        format!(
            "db create branch {} --parent {}",
            branch_store.display(),
            layerstack.display()
        ),
        format!(
            "context use --layerstack {} --branch {}",
            layerstack.display(),
            branch_store.display()
        ),
        "layerstack init --name persisted --empty".into(),
        "layerstack pull --through L-S-MOCK-1-01 --replica".into(),
        "branch fork --name main --layer L-S-MOCK-1-01".into(),
        format!(
            "workspace create --branch B-local-90 --initial-layer L-S-MOCK-1-01 --at {}",
            first_workspace.display()
        ),
        "workspace exec W90 -- /bin/bash -lc 'printf persisted > value.txt'".into(),
        "workspace commit W90".into(),
        "workspace end W90".into(),
        "branch push B-local-90".into(),
        "layerstack add B-local-90".into(),
    ] {
        assert!(matches!(
            finish(&session, &command),
            CliEvent::Finished {
                status: FinishedStatus::Succeeded,
                ..
            }
        ));
    }
    drop(session);

    let reopened = CliSession::open(&context).unwrap();
    let projects = reopened
        .snapshot(ViewQuery::Projects(PageRequest::first(16)))
        .unwrap();
    let ViewSnapshot::Projects(projects) = projects else {
        panic!("Projects")
    };
    let project = projects
        .items
        .iter()
        .find(|project| project.name.as_str() == "persisted")
        .unwrap();
    assert_eq!(project.authority_number, 2);
    assert_eq!(project.work_number, Some(1));
    assert!(project.id.as_str().starts_with("S~"));
    let snapshot = reopened
        .snapshot(ViewQuery::Project {
            id: project.id.clone(),
            page: PageRequest::first(64),
        })
        .unwrap();
    let ViewSnapshot::Project(snapshot) = snapshot else {
        panic!("Project")
    };
    let main = snapshot
        .branches
        .items
        .iter()
        .find(|branch| branch.name.as_str() == "main")
        .unwrap();
    assert_eq!(main.commits.len(), 1);
    assert_eq!(main.work_number, Some(1));
    assert_eq!(main.authority_number, Some(1));
    assert!(main.id.as_str().starts_with("B~"));
    let accepted = snapshot
        .layers
        .items
        .iter()
        .find(|layer| layer.number == 2)
        .unwrap();
    assert_eq!(accepted.root, main.commits[0].root);
    assert_eq!(
        accepted.source.as_ref().map(|source| &source.0),
        Some(&main.id)
    );
    let workspaces = reopened
        .snapshot(ViewQuery::Workspaces {
            project: None,
            page: PageRequest::first(16),
        })
        .unwrap();
    let ViewSnapshot::Workspaces(workspaces) = workspaces else {
        panic!("Workspaces")
    };
    assert!(workspaces.workspaces.items.is_empty());

    let layer = accepted.id.clone();
    assert!(matches!(
        finish(
            &reopened,
            &format!("layerstack pull --through {layer} --replica")
        ),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    let next = match finish(
        &reopened,
        &format!("branch fork --name next --layer {layer}"),
    ) {
        CliEvent::Finished {
            result: Ok(crate::CommandResult::Fork { branch_id, .. }),
            ..
        } => branch_id,
        event => panic!("unexpected Fork result: {event:?}"),
    };
    assert_eq!(next.as_str(), "B-local-91");
    drop(reopened);

    let reopened_again = CliSession::open(&context).unwrap();
    let snapshot = reopened_again
        .snapshot(ViewQuery::Project {
            id: project.id.clone(),
            page: PageRequest::first(64),
        })
        .unwrap();
    let ViewSnapshot::Project(snapshot) = snapshot else {
        panic!("Project")
    };
    assert!(snapshot
        .branches
        .items
        .iter()
        .any(|branch| branch.name.as_str() == "next"));
    assert_eq!(snapshot.project.work_number, Some(2));
    std::fs::remove_dir_all(root).unwrap();
}

fn assert_context(
    session: &CliSession,
    command: &str,
    layerstack: &std::path::Path,
    branch: &std::path::Path,
) {
    match finish(session, command) {
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            result: Ok(crate::CommandResult::Context(profile)),
            ..
        } => {
            assert_eq!(profile.layerstack, layerstack);
            assert_eq!(profile.branch, branch);
        }
        event => panic!("unexpected context result: {event:?}"),
    }
}

fn temporary_workspace(label: &str) -> String {
    let path = std::env::temp_dir().join(format!(
        "layerfs-cli-test-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    path.display().to_string()
}

fn fork_from_layer(session: &CliSession, name: &str, layer: &str) -> BranchId {
    match finish(
        session,
        &format!("branch fork --name {name} --layer {layer}"),
    ) {
        CliEvent::Finished {
            result: Ok(crate::CommandResult::Fork { branch_id, .. }),
            ..
        } => branch_id,
        event => panic!("unexpected Fork result: {event:?}"),
    }
}

fn workspace(session: &CliSession, id: &str) -> crate::WorkspaceView {
    let snapshot = session
        .snapshot(ViewQuery::Workspaces {
            project: None,
            page: PageRequest::first(128),
        })
        .unwrap();
    let ViewSnapshot::Workspaces(snapshot) = snapshot else {
        panic!("Workspaces")
    };
    snapshot
        .workspaces
        .items
        .into_iter()
        .find(|workspace| workspace.id.as_str() == id)
        .unwrap()
}

fn committed_root(script: &str, label: &str) -> crate::ObjectId {
    let session = CliSession::open("mock").unwrap();
    let branch = fork_from_layer(&session, label, "L-A-18");
    let mount = temporary_workspace(label);
    assert!(matches!(
        finish(
            &session,
            &format!("workspace create --branch {branch} --initial-layer L-A-18 --at {mount}"),
        ),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    assert!(matches!(
        finish(
            &session,
            &format!("workspace exec W90 -- /bin/bash -lc '{script}'"),
        ),
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
    let view = workspace(&session, "W90");
    assert!(view.files.iter().all(|file| file.path != "temporary.txt"));
    assert_eq!(view.changes.len(), 1);
    let root = view.published_root.unwrap();
    assert!(matches!(
        finish(&session, "workspace end W90"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    assert!(!std::path::Path::new(&mount).exists());
    root
}

#[test]
fn final_root_ignores_bash_operation_history_and_create_delete_undo() {
    let first = committed_root(
        "printf final > result.txt; printf temporary > temporary.txt; rm temporary.txt",
        "history-a",
    );
    let second = committed_root(
        "touch temporary.txt; rm temporary.txt; printf final > result.txt",
        "history-b",
    );
    assert_eq!(first, second);
}

#[test]
fn real_bash_failure_keeps_effects_and_bounds_output() {
    let session = CliSession::open("mock").unwrap();
    let branch = fork_from_layer(&session, "bash-failure", "L-A-18");
    let mount = temporary_workspace("bash-failure");
    assert!(matches!(
        finish(
            &session,
            &format!("workspace create --branch {branch} --initial-layer L-A-18 --at {mount}"),
        ),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    let event = finish(
        &session,
        "workspace exec W90 -- /bin/bash -lc 'printf kept > failure.txt; yes x | head -c 700000; exit 7'",
    );
    assert!(matches!(
        event,
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            result: Ok(crate::CommandResult::Workspace(ref value)),
            ..
        } if value.contains("exited 7")
    ));
    let view = workspace(&session, "W90");
    assert_eq!(view.runs.last().unwrap().exit_code, 7);
    assert!(view.runs.last().unwrap().output_bytes <= 1024 * 1024);
    assert!(view.files.iter().any(|file| file.path == "failure.txt"));
    assert!(view.timing.bash_last_micros > 0);
    assert!(matches!(
        finish(&session, "workspace end W90 --discard"),
        CliEvent::Finished {
            status: FinishedStatus::Succeeded,
            ..
        }
    ));
    assert!(!std::path::Path::new(&mount).exists());
}

#[test]
fn commit_dedup_equations_and_add_reuse_the_exact_root() {
    let session = CliSession::open("mock").unwrap();
    let first = fork_from_layer(&session, "dedup-a", "L-SW-11");
    let first_mount = temporary_workspace("dedup-a");
    finish(
        &session,
        &format!("workspace create --branch {first} --initial-layer L-SW-11 --at {first_mount}"),
    );
    finish(
        &session,
        "workspace exec W90 -- /bin/bash -lc 'printf shared > shared.txt'",
    );
    finish(&session, "workspace commit W90");
    let first_root = workspace(&session, "W90").published_root.unwrap();
    finish(&session, "workspace end W90");

    let second = fork_from_layer(&session, "dedup-b", "L-SW-11");
    let second_mount = temporary_workspace("dedup-b");
    finish(
        &session,
        &format!("workspace create --branch {second} --initial-layer L-SW-11 --at {second_mount}"),
    );
    finish(
        &session,
        "workspace exec W91 -- /bin/bash -lc 'printf shared > shared.txt'",
    );
    finish(&session, "workspace commit W91");
    let second_view = workspace(&session, "W91");
    let receipt = second_view.commit_receipt.as_ref().unwrap();
    assert_eq!(second_view.published_root.as_ref(), Some(&first_root));
    assert_eq!(
        receipt.candidate_objects,
        receipt.inserted_objects + receipt.reused_objects
    );
    assert_eq!(
        receipt.candidate_bytes,
        receipt.inserted_bytes + receipt.reused_bytes
    );
    assert!(receipt.reused_objects > 0);

    finish(&session, &format!("branch push {second}"));
    finish(&session, &format!("layerstack add {second}"));
    let snapshot = session
        .snapshot(ViewQuery::Project {
            id: "SW-20".into(),
            page: PageRequest::first(64),
        })
        .unwrap();
    let ViewSnapshot::Project(snapshot) = snapshot else {
        panic!("Project")
    };
    let added = snapshot.layers.items.last().unwrap();
    assert_eq!(added.root, first_root);
    assert_eq!(added.source.as_ref().map(|source| &source.0), Some(&second));
    finish(&session, "workspace end W91");
    assert!(!std::path::Path::new(&second_mount).exists());
}

#[test]
fn workspace_rejects_relative_mounts_and_symlink_capture() {
    let session = CliSession::open("mock").unwrap();
    let branch = fork_from_layer(&session, "bounded", "L-A-18");
    assert!(matches!(
        finish(
            &session,
            &format!("workspace create --branch {branch} --initial-layer L-A-18 --at relative"),
        ),
        CliEvent::Finished {
            status: FinishedStatus::Failed,
            result: Err(crate::CliError::Parse(_)),
            ..
        }
    ));
    let mount = temporary_workspace("bounded");
    finish(
        &session,
        &format!("workspace create --branch {branch} --initial-layer L-A-18 --at {mount}"),
    );
    assert!(matches!(
        finish(
            &session,
            "workspace exec W90 -- /bin/bash -lc 'ln -s /tmp escaped-link'",
        ),
        CliEvent::Finished {
            status: FinishedStatus::Failed,
            result: Err(crate::CliError::Integrity(_)),
            ..
        }
    ));
    finish(&session, "workspace end W90 --discard");
    assert!(!std::path::Path::new(&mount).exists());
}
