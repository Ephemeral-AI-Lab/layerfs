//! Actual native dispatcher -> async owner -> admitted direct Store composition.
#[path = "support/installed_store.rs"]
mod support;
use layerfs_content::filesystem::PathName;
use layerfs_daemon::{
    bootstrap::open_store, store::BindRequest, Command, NativeJob, NativeReply, Owner, OwnerConfig,
    OwnerError, Response,
};
use layerfs_fuse::{
    operations::{NativeRead, ReadFailure},
    ports::MountServices,
    Dispatch, DispatchConfig, FailureView, RequestDisposition,
};
use layerfs_history::WorkspaceId;
use layerfs_overlay::ProfileConfig;
use layerfs_workspace::{NativeReadOperation, Operation, Outcome, Position, Refusal, Time};
use std::{
    future::Future,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};
const WAIT: Duration = Duration::from_secs(3);
struct Event(mpsc::SyncSender<()>);
impl Wake for Event {
    fn wake(self: Arc<Self>) {
        let _ = self.0.try_send(());
    }
}
fn wait<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let (send, recv) = mpsc::sync_channel(1);
    let waker = Waker::from(Arc::new(Event(send)));
    let deadline = Instant::now() + WAIT;
    loop {
        match future.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(value) => return value,
            Poll::Pending => recv
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap(),
        }
    }
}
fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline, "bounded native port observation");
        std::thread::yield_now();
    }
}

#[test]
fn real_native_steps_park_for_readers_and_release_original_consumers() {
    let f = support::Fixture::new(1, "filesystem-port");
    assert_eq!(f.count, 1);
    let store = open_store(
        f.config.clone(),
        support::BINDING,
        support::CURSOR,
        1,
        0,
        Default::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &f.directory.join("overlay"),
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
    .unwrap();
    let client = owner.client();
    let identity = WorkspaceId::from_authority([108; 32]).unwrap();
    let bound = store
        .bind(
            client.clone(),
            BindRequest {
                branch: f.branch,
                workspace: identity,
            },
        )
        .unwrap()
        .workspace;
    let root = bound
        .operation()
        .unwrap()
        .workspace()
        .base()
        .unwrap()
        .root()
        .root_inode()
        .serial();
    let done = wait(
        client
            .try_submit(
                Some(bound.route()),
                Command::Native(NativeJob::Mount { root }),
            )
            .unwrap(),
    )
    .unwrap();
    let mount = match done.result() {
        Ok(Response::Native(NativeReply::Mount(value))) => *value,
        value => panic!("{value:?}"),
    };
    drop(done);
    let mut pool = Dispatch::start(DispatchConfig {
        read_handles: 1,
        namespaces: 1,
    })
    .unwrap();
    let queue = pool.register(mount).unwrap();
    // Exhaust the actual provider, then park all K requests. A fourth runnable
    // continuation must execute while those original reader tickets wait.
    let held = wait(store.read_ticket(Some(identity)).unwrap()).unwrap();
    let (send, recv) = mpsc::channel();
    for request in 1..=3 {
        let services = bound.request().unwrap();
        let send = send.clone();
        queue
            .receive()
            .unwrap()
            .admit(11)
            .unwrap()
            .handoff(Box::pin(async move {
                let answer = NativeRead::prepare(
                    services,
                    mount,
                    request,
                    root,
                    None,
                    NativeReadOperation::Lookup {
                        parent: root,
                        name: PathName::new("file-000000").unwrap(),
                    },
                )
                .await
                .unwrap();
                let value = answer.value().unwrap();
                let observation = (value.stat.serial, value.stat.logical_len);
                answer.dispose().await.unwrap();
                send.send(observation).unwrap();
                RequestDisposition::Complete
            }))
            .unwrap();
    }
    until(|| store.read_work().waiting == 3 && queue.work().unwrap().parked == 3);
    assert_eq!(
        client.diagnostics().unwrap().outstanding,
        0,
        "fact rounds released SQL completions before reader wait"
    );
    let (progress, observed) = mpsc::sync_channel(1);
    queue
        .receive()
        .unwrap()
        .admit(0)
        .unwrap()
        .handoff(Box::pin(async move {
            progress.send(()).unwrap();
            RequestDisposition::Complete
        }))
        .unwrap();
    observed.recv_timeout(WAIT).unwrap();
    assert_eq!(store.read_work().waiting, 3);
    drop(held); // Original reader-release event, with no subsequent request.
    let values: Vec<_> = (0..3).map(|_| recv.recv_timeout(WAIT).unwrap()).collect();
    assert!(values.iter().all(|value| *value == values[0]));
    assert_eq!(values[0].1, support::bytes(0).len() as u64);
    until(|| queue.work().unwrap().admitted == 0);
    assert_eq!(store.read_work().outstanding, 0);
    assert_eq!(client.diagnostics().unwrap().outstanding, 0);
    let services = bound.request().unwrap();
    let serial = values[0].0;
    let opened = wait(NativeRead::prepare(
        bound.request().unwrap(),
        mount,
        5,
        serial,
        None,
        NativeReadOperation::Open {
            serial,
            writable: false,
        },
    ))
    .unwrap();
    let handle = opened.value().unwrap().file.unwrap().owner_id();
    wait(opened.dispose()).unwrap();
    drop(wait(services.forget(mount, values[0].0, 3)).unwrap());
    let read = wait(NativeRead::prepare(
        bound.request().unwrap(),
        mount,
        6,
        serial,
        Some(handle),
        NativeReadOperation::Getattr { serial },
    ))
    .unwrap();
    let data = wait(read.read_file(0, layerfs_overlay::READ_WINDOW as u32)).unwrap();
    assert_eq!(data.bytes(), support::bytes(0));
    assert_eq!(
        store.read_work().outstanding,
        0,
        "provider returned before reply consumer"
    );
    wait(data.dispose()).unwrap();
    // Prepare an actual mixed inherited/local 128KiB window through the normal
    // Workspace mutation API; the native data consumer keeps the original SQL
    // read payload borrowed and composes the same retained source afterward.
    let operation = bound.operation().unwrap();
    let acquired = wait(
        client
            .try_submit(
                Some(bound.route()),
                Command::AcquireBaseSource { owner: 991 },
            )
            .unwrap(),
    )
    .unwrap();
    let source = match acquired.result() {
        Ok(Response::BaseSource(source)) => *source,
        other => panic!("{other:?}"),
    };
    drop(acquired);
    let view = operation.workspace().view_for_source(source).unwrap();
    let local = vec![0x6e; layerfs_overlay::READ_WINDOW - 17];
    let outcome = operation
        .workspace()
        .mutate(
            &client,
            operation.ports(),
            &view,
            Operation::Write {
                serial,
                position: Position::At(17),
                data: local.as_slice().into(),
            },
            Time {
                seconds: 5,
                nanoseconds: 7,
            },
        )
        .unwrap();
    let Outcome::Applied { publication, .. } = outcome else {
        panic!("local window was not published")
    };
    let disposed = wait(
        client
            .try_submit(Some(bound.route()), Command::ReplyAttempted(publication))
            .unwrap(),
    )
    .unwrap();
    assert!(disposed.result().is_ok());
    drop(disposed);
    drop(view);
    drop(operation);
    let released = wait(
        client
            .try_submit(Some(bound.route()), Command::ReleaseBaseSource(source))
            .unwrap(),
    )
    .unwrap();
    assert!(released.result().is_ok());
    drop(released);
    let read = wait(NativeRead::prepare(
        bound.request().unwrap(),
        mount,
        7,
        serial,
        Some(handle),
        NativeReadOperation::Getattr { serial },
    ))
    .unwrap();
    let data = wait(read.read_file(0, layerfs_overlay::READ_WINDOW as u32)).unwrap();
    assert_eq!(data.bytes().len(), layerfs_overlay::READ_WINDOW);
    assert_eq!(&data.bytes()[..17], &support::bytes(0)[..17]);
    assert_eq!(&data.bytes()[17..], local);
    assert_eq!(store.read_work().outstanding, 0);
    wait(data.dispose()).unwrap();
    drop(wait(services.close_file(mount, serial, handle)).unwrap());

    // A definite missing-name refusal still owns its source until the caller
    // disposes the reply; it is not confused with an uncertain SQL failure.
    let missing = wait(NativeRead::prepare(
        bound.request().unwrap(),
        mount,
        4,
        root,
        None,
        NativeReadOperation::Lookup {
            parent: root,
            name: PathName::new("absent").unwrap(),
        },
    ))
    .unwrap();
    assert!(matches!(missing.value(), Err(Refusal::Missing)));
    assert_eq!(client.diagnostics().unwrap().outstanding, 1);
    wait(missing.dispose()).unwrap();
    assert_eq!(client.diagnostics().unwrap().outstanding, 0);
    let done = wait(
        client
            .try_submit(
                Some(bound.route()),
                Command::Native(NativeJob::Revoke(mount)),
            )
            .unwrap(),
    )
    .unwrap();
    assert!(matches!(
        done.result(),
        Ok(Response::Native(NativeReply::Done))
    ));
    drop(done);
    queue.stop_admission().unwrap();
    queue.finish().unwrap();
    assert!(pool.stop().unwrap().clean());
    drop(services);
    drop(bound);
    owner.stop().unwrap();
    drop(store);
    f.cleanup();
}

#[test]
fn terminal_owner_failure_retains_unattempted_input_without_replay() {
    let f = support::Fixture::new(1, "filesystem-port-stopped");
    assert_eq!(f.count, 1);
    let store = open_store(
        f.config.clone(),
        support::BINDING,
        support::CURSOR,
        1,
        0,
        Default::default(),
    )
    .unwrap();
    let owner = Owner::start(
        &f.directory.join("overlay"),
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
                workspace: WorkspaceId::from_authority([109; 32]).unwrap(),
            },
        )
        .unwrap()
        .workspace;
    let root = bound
        .operation()
        .unwrap()
        .workspace()
        .base()
        .unwrap()
        .root()
        .root_inode()
        .serial();
    let done = wait(
        client
            .try_submit(
                Some(bound.route()),
                Command::Native(NativeJob::Mount { root }),
            )
            .unwrap(),
    )
    .unwrap();
    let mount = match done.result() {
        Ok(Response::Native(NativeReply::Mount(value))) => *value,
        value => panic!("{value:?}"),
    };
    drop(done);
    let pool = Dispatch::start(DispatchConfig {
        read_handles: 1,
        namespaces: 1,
    })
    .unwrap();
    let queue = pool.register(mount).unwrap();
    let services = bound.request().unwrap();
    owner.stop().unwrap();
    let before = client.diagnostics().unwrap().admitted;
    queue
        .receive()
        .unwrap()
        .admit(0)
        .unwrap()
        .handoff(Box::pin(async move {
            match NativeRead::prepare(
                services,
                mount,
                u64::MAX,
                root,
                None,
                NativeReadOperation::Lookup {
                    parent: root,
                    name: PathName::new("file-000000").unwrap(),
                },
            )
            .await
            {
                Err(error) => RequestDisposition::Retained(Box::new(error)),
                Ok(_) => panic!("stopped owner cannot publish a new source"),
            }
        }))
        .unwrap();
    until(|| queue.work().unwrap().retained == 1);
    queue
        .inspect_retained(0, |failure| {
            let FailureView::Request(error) = failure else {
                panic!("expected original owner error")
            };
            let error = error.downcast_ref::<ReadFailure>().unwrap();
            assert_eq!(error.request(), u64::MAX);
            assert_eq!(error.retained_source(), None);
            assert_eq!(error.protected(), root);
            assert!(
                matches!(error.operation(), NativeReadOperation::Lookup { parent, name }
                if *parent == root && name.as_bytes() == b"file-000000")
            );
            let original = error.reason.downcast_ref::<OwnerError>().unwrap();
            let OwnerError::Unattempted { cause, command } = original else {
                panic!("{original:?}")
            };
            assert!(matches!(cause.as_ref(), OwnerError::Stopped));
            assert!(matches!(
                command.as_ref(),
                Command::Native(NativeJob::Source {
                    request: u64::MAX,
                    ..
                })
            ));
        })
        .unwrap();
    assert_eq!(client.diagnostics().unwrap().admitted, before);
    drop(pool);
    drop(queue);
    drop(bound);
    drop(store);
    f.cleanup();
}
