//! Authenticated control over an actually installed Store and one real owner.
#[path = "support/control_fixture.rs"]
mod fixture;
#[path = "support/held_writer.rs"]
mod held_writer;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;
use layerfs_bridge::control::{Activity, ControlCode, Reply, Request, WorkspaceToken};
use layerfs_daemon::control::Service;
use layerfs_history::{
    BranchId, CommitHistoryRequest, CommitStagedOutcome, ForkRequest, ForkSource, HistoryName,
    WorkspaceId,
};
use layerfs_sdk::{control::Control, OperationCause, ProjectApi, WorkspaceApi};
use std::sync::{
    atomic::{AtomicU8, Ordering},
    mpsc, Arc,
};
use std::time::Duration;
fn identity(tag: u8) -> WorkspaceId {
    WorkspaceId::from_authority([tag; 32]).unwrap()
}
fn mount(
    client: &mut Control,
    workspace: WorkspaceId,
    branch: BranchId,
) -> (WorkspaceToken, layerfs_history::BranchSnapshot) {
    let bound = WorkspaceApi::new(client).bind(workspace, branch).unwrap();
    (bound.token, bound.binding)
}
fn committed(client: &mut Control, token: WorkspaceToken) -> layerfs_history::CommitRecord {
    match WorkspaceApi::new(client).commit(token).unwrap() {
        CommitStagedOutcome::Committed(value) => value,
        other => panic!("{other:?}"),
    }
}
fn status(client: &mut Control, token: WorkspaceToken) -> layerfs_bridge::control::WorkspaceStatus {
    WorkspaceApi::new(client).status(token).unwrap()
}
fn close(client: &mut Control, token: WorkspaceToken) {
    WorkspaceApi::new(client).unmount(token).unwrap();
}

fn serve(
    service: Arc<Service>,
    tag: Arc<AtomicU8>,
    last: WorkspaceId,
    policy: layerfs_content::ConstructionPolicy,
) -> (Control, support::Worker<usize>) {
    let (channel, worker) = support::pair(move |connection| {
        let mut count = 0;
        loop {
            let served = service
                .serve_one(connection, |save, _, snapshot| {
                    fixture::construct(save, snapshot, tag.load(Ordering::Acquire), policy)
                })
                .unwrap();
            count += 1;
            if let Ok(success) = &served.outcome {
                if let Some(commit) = &success.commit {
                    println!(
                        "CONTROL_COMMIT writes={} initial={} refills={} publication={} history=1",
                        commit.storage.reserve + commit.storage.publish + 1,
                        commit.storage.initial_reservations,
                        commit.storage.reservation_refills,
                        commit.storage.publish
                    );
                }
            }
            let done = matches!(served.call.request,Request::Unmount(token) if token.workspace==last)
                && served.outcome.is_ok();
            drop(served);
            if done {
                return count;
            }
        }
    });
    (Control::new(channel), worker)
}
#[test]
fn native_mount_commit_status_fork_history_and_terminal_unmount() {
    let f = fixture::Fixture::new("control-flow");
    let branch = f.native.project.branch.branch.id;
    let tag = Arc::new(AtomicU8::new(b'A'));
    let last = identity(79);
    let (mut client, worker) = serve(
        f.service.clone(),
        tag.clone(),
        last,
        f.installed.opened.store.policy().construction(),
    );
    let (a, original) = mount(&mut client, identity(71), branch);
    let before = f.installed.opened.store.work();
    let (b, _) = mount(&mut client, identity(72), branch);
    let after = f.installed.opened.store.work();
    assert_eq!(
        after.object_batches, before.object_batches,
        "warm mount has no object demands; history snapshot remains paid"
    );
    let sql = f.statements();
    let observed = status(&mut client, a);
    assert_eq!(observed.activity, Activity::Idle);
    assert_eq!(f.statements(), sql, "Status must issue zero Store SQL");
    assert_eq!(observed.binding, original);
    fixture::write(&f.service, a, b'A');
    fixture::write(&f.service, b, b'B');
    let first = committed(&mut client, a);
    assert!(
        matches!(WorkspaceApi::new(&mut client).commit(a).unwrap(),CommitStagedOutcome::UpToDate{head:Some(head),root} if head==first.id&&root==first.root)
    );
    let sql = f.statements();
    let stale = status(&mut client, b);
    assert_eq!(stale.binding, original);
    assert_eq!(
        f.statements(),
        sql,
        "Status never refreshes the moved Branch"
    );
    tag.store(b'B', Ordering::Release);
    let overwritten = committed(&mut client, b);
    assert_eq!(overwritten.parent, original.branch.head_commit);
    assert_ne!(overwritten.root, first.root);
    let fork = BranchId::from_authority([75; 16]);
    let forked = ProjectApi::new()
        .fork(
            &mut client,
            ForkRequest {
                stack: original.branch.stack,
                branch: fork,
                name: HistoryName::new("fork").unwrap(),
                source: ForkSource::Commit {
                    branch,
                    commit: overwritten.id,
                },
            },
        )
        .unwrap();
    assert_eq!(forked.effective_root, overwritten.root);
    let (c, binding) = mount(&mut client, identity(73), fork);
    assert_eq!(binding.effective_root, overwritten.root);
    fixture::write(&f.service, a, b'C');
    tag.store(b'C', Ordering::Release);
    let second = committed(&mut client, a);
    assert_eq!(second.parent, Some(first.id));
    let page = ProjectApi::new()
        .history(
            &mut client,
            CommitHistoryRequest {
                branch,
                start: None,
                cursor: None,
                limit: 1,
            },
        )
        .unwrap();
    assert_eq!(page.records, vec![second.clone()]);
    let cursor = page.continuation.unwrap();
    fixture::write(&f.service, a, b'D');
    tag.store(b'D', Ordering::Release);
    let third = committed(&mut client, a);
    assert_eq!(third.parent, Some(second.id));
    let page = ProjectApi::new()
        .history(
            &mut client,
            CommitHistoryRequest {
                branch,
                start: None,
                cursor: Some(cursor),
                limit: 1,
            },
        )
        .unwrap();
    assert_eq!(page.records, vec![first]);
    assert!(page.continuation.is_none());
    let stale_token = WorkspaceToken {
        namespace: a.namespace + 100,
        ..a
    };
    assert!(
        matches!(client.call(Request::Status(stale_token)).unwrap(),Reply::Refused(value) if value.code==ControlCode::Invalid)
    );
    close(&mut client, a);
    assert!(
        matches!(client.call(Request::Status(a)).unwrap(),Reply::Refused(value) if value.code==ControlCode::Missing)
    );
    close(&mut client, b);
    close(&mut client, c);
    let (end, _) = mount(&mut client, last, branch);
    close(&mut client, end);
    let count = worker.join();
    assert_eq!(count, 20);
    println!("CONTROL_FLOW overwrite_last_effect=true captured_parent_preserved=true calls={count} Store_SQL_for_status=0 history_anchor_stable=true stale_namespace_fenced=true");
    f.cleanup();
}
#[test]
fn control_busy_keeps_the_channel_healthy_for_a_later_explicit_call() {
    let f = fixture::Fixture::new("control-busy");
    let branch = f.native.project.branch.branch.id;
    let last = identity(81);
    let tag = Arc::new(AtomicU8::new(b'Q'));
    let (mut client, worker) = serve(
        f.service.clone(),
        tag,
        last,
        f.installed.opened.store.policy().construction(),
    );
    let (token, _) = mount(&mut client, last, branch);
    fixture::write(&f.service, token, b'Q');
    let writer =
        held_writer::HeldWriter::acquire(&std::path::PathBuf::from(&f.installed.manifest.locator));
    assert!(
        matches!(WorkspaceApi::new(&mut client).commit(token).unwrap_err().cause, OperationCause::Remote(value) if value.code==ControlCode::Busy)
    );
    writer.release();
    let observed = status(&mut client, token);
    assert_eq!(observed.activity, Activity::Idle);
    assert!(observed.local.as_ref().unwrap().dirty_inodes > 0);
    committed(&mut client, token);
    close(&mut client, token);
    assert_eq!(worker.join(), 5);
    f.cleanup();
}
#[test]
fn original_lost_control_reply_is_not_replayed_or_settled_by_an_observer() {
    let f = fixture::Fixture::new("control-lost");
    let branch = f.native.project.branch.branch.id;
    let last = identity(85);
    let service = f.service.clone();
    let (send_entered, entered) = mpsc::sync_channel(1);
    let (send_release, release) = mpsc::sync_channel(1);
    let (send_fence, fence) = mpsc::sync_channel(1);
    let policy = f.installed.opened.store.policy().construction();
    let (channel, worker) = support::pair(move |connection| {
        send_fence.send(connection.close_handle().unwrap()).unwrap();
        let mounted = service
            .serve_one(connection, |_, _, _| unreachable!())
            .unwrap();
        assert!(mounted.outcome.is_ok());
        drop(mounted);
        service.serve_one(connection, |save, _, snapshot| {
            send_entered.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(3)).unwrap();
            fixture::construct(save, snapshot, b'L', policy)
        })
    });
    let mut client = Control::new(channel);
    let (token, _) = mount(&mut client, last, branch);
    fixture::write(&f.service, token, b'L');
    let server_fence = fence.recv_timeout(Duration::from_secs(3)).unwrap();
    let calling = std::thread::spawn(move || WorkspaceApi::new(&mut client).commit(token));
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    server_fence.close().unwrap();
    send_release.send(()).unwrap();
    let failure = calling.join().unwrap().unwrap_err();
    assert_eq!(failure.request, Request::Commit(token));
    let OperationCause::Exchange(original) = &failure.cause else {
        panic!("expected original exchange custody: {failure:?}");
    };
    assert_eq!(original.call.request, failure.request);
    assert!(original.attempted && original.received.is_none());
    let delivery = worker.join().unwrap_err();
    let served = delivery.original.as_ref().unwrap();
    let success = served.outcome.as_ref().unwrap();
    let published = success.commit.as_ref().unwrap();
    assert!(matches!(
        published.history,
        CommitStagedOutcome::Committed(_)
    ));
    let observer = f
        .installed
        .opened
        .store
        .history()
        .branch_snapshot(branch)
        .unwrap()
        .unwrap();
    assert_eq!(
        observer.effective_root,
        match &published.history {
            CommitStagedOutcome::Committed(record) => record.root,
            _ => unreachable!(),
        }
    );
    assert!(
        original.received.is_none(),
        "observer cannot settle original lost reply"
    );
    println!(
        "CONTROL_LOST_ORIGINAL client={original:?} daemon_reply={:?} capture={:?}",
        success.reply, published.capture
    );
    println!("CONTROL_LOST_REPLY client_original_unknown=true daemon_original_known_commit=true replay=false observer_does_not_settle=true");
    drop((delivery, failure));
    f.cleanup();
}

#[test]
fn another_control_channel_observes_running_commit_and_refuses_unmount_before_effect() {
    let f = fixture::Fixture::new("control-concurrent");
    let branch = f.native.project.branch.branch.id;
    let service = f.service.clone();
    let policy = f.installed.opened.store.policy().construction();
    let (send_entered, entered) = mpsc::sync_channel(1);
    let (send_release, release) = mpsc::sync_channel(1);
    let (channel, worker) = support::pair(move |connection| {
        let mounted = service
            .serve_one(connection, |_, _, _| unreachable!())
            .unwrap();
        drop(mounted);
        service.serve_one(connection, |save, _, snapshot| {
            send_entered.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(3)).unwrap();
            fixture::construct(save, snapshot, b'R', policy)
        })
    });
    let mut client = Control::new(channel);
    let (token, original) = mount(&mut client, identity(88), branch);
    fixture::write(&f.service, token, b'R');
    let calling = std::thread::spawn(move || client.call(Request::Commit(token)));
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    let service = f.service.clone();
    let (channel, observing) = support::pair(move |connection| {
        for _ in 0..2 {
            let served = service
                .serve_one(connection, |_, _, _| unreachable!())
                .unwrap();
            drop(served);
        }
    });
    let mut observer = Control::new(channel);
    let sql = f.statements();
    let observed = status(&mut observer, token);
    assert_eq!(observed.activity, Activity::Committing);
    assert_eq!(observed.binding, original);
    assert!(observed.local.as_ref().unwrap().captured.is_some());
    assert_eq!(f.statements(), sql);
    assert!(
        matches!(observer.call(Request::Unmount(token)).unwrap(),Reply::Refused(value) if value.code==ControlCode::Busy)
    );
    observing.join();
    send_release.send(()).unwrap();
    assert!(matches!(
        calling.join().unwrap().unwrap(),
        Reply::Committed(CommitStagedOutcome::Committed(_))
    ));
    let completed = worker.join().unwrap();
    assert!(completed.outcome.is_ok());
    drop(completed);
    println!("CONTROL_CONCURRENT status_while_constructing=true Store_SQL_for_status=0 unmount=Busy_before_effect later_commit=Committed");
    f.cleanup();
}

#[test]
fn an_original_unknown_blocks_another_commit_and_normal_unmount() {
    let f = fixture::Fixture::new("control-unknown");
    let branch = f.native.project.branch.branch.id;
    let service = f.service.clone();
    let (channel, worker) = support::pair(move |connection| {
        drop(
            service
                .serve_one(connection, |_, _, _| unreachable!())
                .unwrap(),
        );
        let original = service
            .serve_one(connection, |_, _, _| {
                Err(layerfs_daemon::store::CommitError::Storage(
                    layerfs_storage::StorageError::UnknownOutcome {
                        original: Box::new(layerfs_storage::StorageError::Integrity(
                            "original producer uncertainty",
                        )),
                    },
                ))
            })
            .unwrap();
        for _ in 0..3 {
            drop(
                service
                    .serve_one(connection, |_, _, _| {
                        unreachable!("unresolved constructor cannot enter")
                    })
                    .unwrap(),
            );
        }
        original
    });
    let mut client = Control::new(channel);
    let (token, binding) = mount(&mut client, identity(89), branch);
    fixture::write(&f.service, token, b'U');
    assert!(
        matches!(client.call(Request::Commit(token)).unwrap(),Reply::Refused(value) if value.code==ControlCode::Unknown)
    );
    let observed = status(&mut client, token);
    assert_eq!(observed.activity, Activity::Uncertain);
    assert_eq!(observed.binding, binding);
    assert!(observed.published.is_none());
    assert!(observed.local.unwrap().captured.is_some());
    assert!(matches!(f.service.execute_control(&Request::Commit(token)),
        Err(layerfs_daemon::control::Failure::Custody(value)) if value.code == ControlCode::Unknown && value.published.is_none()));
    assert!(
        matches!(client.call(Request::Unmount(token)).unwrap(),Reply::Refused(value) if value.code==ControlCode::Unknown)
    );
    assert!(
        matches!(client.call(Request::Commit(token)).unwrap(),Reply::Refused(value) if value.code==ControlCode::Unknown)
    );
    let retained = worker.join();
    assert!(retained.outcome.is_err());
    println!("CONTROL_UNKNOWN producer_original_retained=true normal_unmount=Unknown another_commit=Unknown no_reentry=true");
    drop(retained);
    f.cleanup();
}
#[test]
fn status_retains_known_publication_when_the_engine_is_unavailable() {
    let f = fixture::Fixture::new("control-local-failure");
    let branch = f.native.project.branch.branch.id;
    let service = f.service.clone();
    let policy = f.installed.opened.store.policy().construction();
    let (send_entered, entered) = mpsc::sync_channel(1);
    let (send_release, release) = mpsc::sync_channel(1);
    let (channel, worker) = support::pair(move |connection| {
        drop(
            service
                .serve_one(connection, |_, _, _| unreachable!())
                .unwrap(),
        );
        let original = service
            .serve_one(connection, |save, _, snapshot| {
                let root = fixture::construct(save, snapshot, b'V', policy)?;
                send_entered.send(()).unwrap();
                release.recv_timeout(Duration::from_secs(3)).unwrap();
                Ok(root)
            })
            .unwrap();
        let status = service
            .serve_one(connection, |_, _, _| unreachable!())
            .unwrap();
        assert!(status
            .outcome
            .as_ref()
            .unwrap()
            .observation_failure
            .is_some());
        drop(status);
        drop(
            service
                .serve_one(connection, |_, _, _| unreachable!())
                .unwrap(),
        );
        original
    });
    let mut client = Control::new(channel);
    let (token, binding) = mount(&mut client, identity(90), branch);
    fixture::write(&f.service, token, b'V');
    let calling = std::thread::spawn(move || {
        let reply = client.call(Request::Commit(token));
        (client, reply)
    });
    entered.recv_timeout(Duration::from_secs(3)).unwrap();
    f.owner.stop().unwrap();
    send_release.send(()).unwrap();
    let (mut client, reply) = calling.join().unwrap();
    let publication = match reply.unwrap() {
        Reply::Refused(value) => value.published.expect("original known history"),
        other => panic!("{other:?}"),
    };
    let before = f.installed.opened.diagnostics().unwrap().writer.statements;
    let observed = status(&mut client, token);
    assert_eq!(observed.activity, Activity::LocalFailure);
    assert_eq!(observed.binding, binding);
    assert_eq!(observed.published, Some(publication));
    assert!(observed.local.is_none() && observed.local_failure.is_some());
    assert_eq!(
        f.installed.opened.diagnostics().unwrap().writer.statements,
        before
    );
    assert!(
        matches!(client.call(Request::Unmount(token)).unwrap(),Reply::Refused(value) if value.code==ControlCode::Unknown)
    );
    let original = worker.join();
    assert!(original.outcome.is_err());
    println!("CONTROL_LOCAL_FAILURE known_publication=retained engine=unavailable Store_SQL_for_status=0 normal_unmount=Unknown");
    drop((original, f.service, f.installed));
    f.native.cleanup();
}

#[test]
fn product_control_commit_captures_the_frontier_releases_its_owners_and_is_then_up_to_date() {
    let f = fixture::Fixture::new("product-producer");
    let branch = f.native.project.branch.branch.id;
    let success = f
        .service
        .execute_control(&Request::Mount {
            workspace: identity(87),
            branch,
        })
        .unwrap();
    let (token, bound) = match &success.reply {
        Reply::Bound { token, binding } => (*token, binding.clone()),
        other => panic!("{other:?}"),
    };
    drop(success);
    fixture::write(&f.service, token, b'Q');
    let status = |token| match f
        .service
        .execute_control(&Request::Status(token))
        .unwrap()
        .reply
    {
        Reply::Status(v) => v,
        other => panic!("{other:?}"),
    };
    let before = status(token);
    // A stale namespace is refused before admission, capture or Save.
    let sql = f.statements();
    assert!(matches!(
        f.service.execute_control(&Request::Commit(WorkspaceToken {
            namespace: token.namespace + 1,
            ..token
        })),
        Err(layerfs_daemon::control::Failure::Rejected(
            ControlCode::Invalid,
            _
        ))
    ));
    assert_eq!(status(token), before, "no epoch, capture or install effect");
    assert_eq!(f.statements(), sql, "no Store SQL for a refused token");
    // The product route: no caller constructor exists on this path.
    let done = f.service.execute_control(&Request::Commit(token)).unwrap();
    let record = match &done.reply {
        Reply::Committed(CommitStagedOutcome::Committed(record)) => record.clone(),
        other => panic!("{other:?}"),
    };
    assert_ne!(record.root, bound.effective_root);
    assert_eq!(record.parent, bound.branch.head_commit);
    let commit = done.commit.as_ref().expect("original Commit receipt");
    let namespace = commit.namespace.as_ref().expect("product constructor ran");
    assert!(!namespace.retained(), "{namespace:?}");
    assert_eq!(namespace.released.len(), 2, "reader, then operation owner");
    assert!(namespace.release_error.is_none() && namespace.custody.is_none());
    let work = namespace.work.expect("producer work");
    assert!(
        work.inode_rows > 0 && work.files_constructed > 0,
        "{work:?}"
    );
    assert!(namespace.counters.is_some());
    println!(
        "PRODUCT_COMMIT inode_rows={} entry_rows={} files={} record_jobs={} released=2",
        work.inode_rows, work.entry_rows, work.files_constructed, work.record_jobs
    );
    drop(done);
    let after = status(token);
    assert_eq!(after.activity, Activity::Idle);
    assert_eq!(after.binding.effective_root, record.root);
    assert_eq!(after.binding.branch.head_commit, Some(record.id));
    // Unchanged: the same head and root, no new Commit record.
    let again = f.service.execute_control(&Request::Commit(token)).unwrap();
    assert!(
        matches!(&again.reply, Reply::Committed(CommitStagedOutcome::UpToDate { head: Some(head), root }) if *head == record.id && *root == record.root),
        "{:?}",
        again.reply
    );
    let unchanged = again.commit.as_ref().unwrap().namespace.as_ref().unwrap();
    assert!(!unchanged.retained() && unchanged.released.len() == 2);
    drop(again);
    // A new Workspace binds the published root. Only the root identity is
    // compared here; the committed bytes are read back in product_commit.
    let other = match f
        .service
        .execute_control(&Request::Mount {
            workspace: identity(88),
            branch,
        })
        .unwrap()
        .reply
    {
        Reply::Bound { token, binding } => {
            assert_eq!(binding.effective_root, record.root);
            token
        }
        other => panic!("{other:?}"),
    };
    f.service.execute_control(&Request::Unmount(other)).unwrap();
    f.service.execute_control(&Request::Unmount(token)).unwrap();
    f.cleanup();
}
