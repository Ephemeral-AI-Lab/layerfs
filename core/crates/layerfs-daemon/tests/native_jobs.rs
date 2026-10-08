//! Native typed jobs over the real owner and a sealed, installed Store fixture.
#[path = "support/installed_store.rs"]
mod support;
use layerfs_daemon::{
    bootstrap::open_store, store::BindRequest, Command, Completion, NativeJob, NativeReply, Owner,
    OwnerClient, OwnerConfig, Pending, Response,
};
use layerfs_history::WorkspaceId;
use layerfs_overlay::{ProfileConfig, Route, StatementKind};
use layerfs_workspace::{NativeReadOperation, NativeReadStage};
use std::{
    future::Future,
    pin::Pin,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
struct Event(mpsc::SyncSender<()>);
impl Wake for Event {
    fn wake(self: Arc<Self>) {
        let _ = self.0.try_send(());
    }
}
fn finish(mut pending: Pending) -> Completion {
    let (sender, receiver) = mpsc::sync_channel(1);
    let waker = Waker::from(Arc::new(Event(sender)));
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match Pin::new(&mut pending).poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(result) => return result.unwrap(),
            Poll::Pending => receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("original owner completion did not wake"),
        }
    }
}
fn job(client: &OwnerClient, route: Route, command: Command) -> Completion {
    finish(client.try_submit(Some(route), command).unwrap())
}
#[test]
fn native_lookup_uses_actual_owner_and_preserves_original_receipt_until_disposal() {
    let f = support::Fixture::new(1, "native-jobs");
    assert_eq!(f.count, 1);
    let store = open_store(
        f.config.clone(),
        support::BINDING,
        support::CURSOR,
        2,
        0,
        Default::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &f.directory.join("native-overlay"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let bound = store
        .bind(
            client.clone(),
            BindRequest {
                branch: f.branch,
                workspace: WorkspaceId::from_authority([83; 32]).unwrap(),
            },
        )
        .unwrap()
        .workspace;
    let route = bound.route();
    let operation = bound.operation().unwrap();
    let root = operation
        .workspace()
        .base()
        .unwrap()
        .root()
        .root_inode()
        .serial();
    let done = job(&client, route, Command::Native(NativeJob::Mount { root }));
    let mount = match done.result() {
        Ok(Response::Native(NativeReply::Mount(mount))) => *mount,
        other => panic!("{other:?}"),
    };
    drop(done);
    let done = job(
        &client,
        route,
        Command::Native(NativeJob::Source {
            mount,
            request: u64::MAX,
            serial: root,
        }),
    );
    let source = match done.result() {
        Ok(Response::Native(NativeReply::Source(source))) => *source,
        other => panic!("{other:?}"),
    };
    drop(done);
    let view = operation.workspace().view_for_source(source).unwrap();
    let mut plan = view
        .native_read_plan(
            mount,
            NativeReadOperation::Lookup {
                parent: root,
                name: layerfs_content::filesystem::PathName::new("file-000000").unwrap(),
            },
        )
        .unwrap();
    let mut result = None;
    for _ in 0..4 {
        let before = store.work();
        let receipt = job(
            &client,
            route,
            Command::Native(NativeJob::Observe(Box::new(plan.job().unwrap().clone()))),
        );
        assert_eq!(store.work(), before, "SQL owner made a Store demand");
        let original = match receipt.result() {
            Ok(Response::Native(NativeReply::Observed(original))) => original.clone(),
            other => panic!("{other:?}"),
        };
        if let Some(value) = plan.accept(original).unwrap() {
            result = Some((value, receipt));
            break;
        }
        drop(receipt);
        assert_eq!(plan.stage(), NativeReadStage::Base);
        // This component test's caller is a constructor/control thread. Native
        // Fuse integration must supply these facts through admitted read steps.
        plan.supply(&view).unwrap();
    }
    let (value, receipt) = result.expect("finite native fact rounds");
    assert_eq!(value.stat.logical_len, support::bytes(0).len() as u64);
    assert_eq!(
        receipt
            .work()
            .sql
            .family(StatementKind::Begin)
            .unwrap()
            .executions,
        1
    );
    assert_eq!(
        receipt
            .work()
            .sql
            .family(StatementKind::Commit)
            .unwrap()
            .executions,
        1
    );
    assert!(client.diagnostics().unwrap().credited_bytes > 0);
    assert!(
        job(&client, route, Command::Native(NativeJob::Revoke(mount)))
            .result()
            .is_err()
    );
    let serial = value.stat.serial;
    let read = value.read;
    drop(value);
    assert!(job(&client, route, Command::ReleaseFileRead(read))
        .result()
        .is_ok());
    assert!(job(&client, route, Command::ReleaseBaseSource(source))
        .result()
        .is_ok());
    drop(receipt);
    let done = job(
        &client,
        route,
        Command::Native(NativeJob::Source {
            mount,
            request: u64::MAX - 1,
            serial,
        }),
    );
    let source = match done.result() {
        Ok(Response::Native(NativeReply::Source(source))) => *source,
        other => panic!("{other:?}"),
    };
    drop(done);
    let view = operation.workspace().view_for_source(source).unwrap();
    let mut plan = view
        .native_read_plan(
            mount,
            NativeReadOperation::Open {
                serial,
                writable: false,
            },
        )
        .unwrap();
    let mut opened = None;
    for _ in 0..4 {
        let receipt = job(
            &client,
            route,
            Command::Native(NativeJob::Observe(Box::new(plan.job().unwrap().clone()))),
        );
        let original = match receipt.result() {
            Ok(Response::Native(NativeReply::Observed(original))) => original.clone(),
            other => panic!("{other:?}"),
        };
        if let Some(value) = plan.accept(original).unwrap() {
            opened = Some((value, receipt));
            break;
        }
        drop(receipt);
        plan.supply(&view).unwrap();
    }
    let (value, receipt) = opened.expect("bounded open fact rounds");
    let file = value.file.unwrap();
    assert!(!file.writable());
    assert_eq!(
        receipt
            .work()
            .sql
            .family(StatementKind::Begin)
            .unwrap()
            .executions,
        1
    );
    assert_eq!(
        receipt
            .work()
            .sql
            .family(StatementKind::Commit)
            .unwrap()
            .executions,
        1
    );
    let done = job(
        &client,
        route,
        Command::Native(NativeJob::RetainedFile {
            mount,
            request: u64::MAX - 1,
        }),
    );
    assert!(
        matches!(done.result(), Ok(Response::Native(NativeReply::RetainedFile(Some(retained)))) if *retained == file)
    );
    drop(done);
    assert!(job(&client, route, Command::ReleaseFileRead(value.read))
        .result()
        .is_ok());
    assert!(job(&client, route, Command::ReleaseBaseSource(source))
        .result()
        .is_ok());
    drop((value, receipt));
    assert!(job(
        &client,
        route,
        Command::Native(NativeJob::Forget {
            mount,
            serial,
            count: 1
        })
    )
    .result()
    .is_ok());
    assert!(
        job(&client, route, Command::Native(NativeJob::Revoke(mount)))
            .result()
            .is_err()
    );
    let done = job(
        &client,
        route,
        Command::Native(NativeJob::FileSource {
            mount,
            request: u64::MAX - 2,
            serial,
            handle: file.owner_id(),
        }),
    );
    let processing = match done.result() {
        Ok(Response::Native(NativeReply::Source(source))) => *source,
        other => panic!("{other:?}"),
    };
    drop(done);
    assert!(job(
        &client,
        route,
        Command::Native(NativeJob::CloseFile {
            mount,
            serial,
            handle: file.owner_id(),
        })
    )
    .result()
    .is_ok());
    assert!(job(
        &client,
        route,
        Command::Native(NativeJob::File {
            mount,
            serial,
            handle: file.owner_id(),
        })
    )
    .result()
    .is_err());
    assert!(
        job(&client, route, Command::Native(NativeJob::Revoke(mount)))
            .result()
            .is_err()
    );
    assert!(job(&client, route, Command::ReleaseBaseSource(processing))
        .result()
        .is_ok());
    assert!(
        job(&client, route, Command::Native(NativeJob::Revoke(mount)))
            .result()
            .is_ok()
    );
    assert!(job(&client, route, Command::Close).result().is_ok());
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let work = client.diagnostics().unwrap();
        if work.closed_namespaces == 1 && work.outstanding == 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "automatic native/namespace retirement deadline"
        );
        std::thread::yield_now();
    }
    assert!(client.maintenance_failure().unwrap().is_none());
    assert_eq!(client.diagnostics().unwrap().receipt_overruns, 0);
    drop((plan, view, operation, bound, store));
    owner.stop().unwrap();
    f.cleanup();
}
