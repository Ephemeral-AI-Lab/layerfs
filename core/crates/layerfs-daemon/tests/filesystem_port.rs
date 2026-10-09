//! Actual native dispatcher -> async owner -> admitted direct Store composition.
#[path = "support/installed_store.rs"]
mod support;
use layerfs_content::filesystem::PathName;
use layerfs_daemon::{
    bootstrap::open_store, store::BindRequest, Command, NativeJob, NativeReply, Owner, OwnerConfig,
    OwnerError, Response, ServiceClass,
};
use layerfs_fuse::{
    operations::{DirectoryStep, DirectoryStream, NativeRead, ReadFailure},
    ports::{Fence, MountServices},
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
        let services = bound.request(&Fence::default()).unwrap();
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
    let services = bound.request(&Fence::default()).unwrap();
    let serial = values[0].0;
    let opened = wait(NativeRead::prepare(
        bound.request(&Fence::default()).unwrap(),
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
        bound.request(&Fence::default()).unwrap(),
        mount,
        6,
        serial,
        Some(handle),
        NativeReadOperation::Data { serial },
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
        bound.request(&Fence::default()).unwrap(),
        mount,
        7,
        serial,
        Some(handle),
        NativeReadOperation::Data { serial },
    ))
    .unwrap();
    let data = wait(read.read_file(0, layerfs_overlay::READ_WINDOW as u32)).unwrap();
    assert_eq!(data.bytes().len(), layerfs_overlay::READ_WINDOW);
    assert_eq!(&data.bytes()[..17], &support::bytes(0)[..17]);
    assert_eq!(&data.bytes()[17..], local);
    assert_eq!(store.read_work().outstanding, 0);
    wait(data.dispose()).unwrap();
    drop(wait(services.close_file(mount, serial, handle)).unwrap());

    // A definite missing-name refusal of a lookup is not confused with an
    // uncertain SQL failure, and owns nothing: the lookup was two owner
    // visits around one base read, with no source and no retained result, so
    // disposing the reply admits no job.
    let before = client.diagnostics().unwrap();
    let grants = store.read_work().grants;
    let missing = wait(NativeRead::prepare(
        bound.request(&Fence::default()).unwrap(),
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
    let visited = client.diagnostics().unwrap();
    assert_eq!(visited.outstanding, 0);
    assert_eq!(visited.admitted, before.admitted + 2);
    assert_eq!(
        visited.completed[ServiceClass::Read as usize],
        before.completed[ServiceClass::Read as usize] + 2
    );
    assert_eq!(
        visited.completed[ServiceClass::Source as usize],
        before.completed[ServiceClass::Source as usize]
    );
    assert_eq!(store.read_work().grants, grants + 1);
    wait(missing.dispose()).unwrap();
    let disposed = client.diagnostics().unwrap();
    assert_eq!(
        (disposed.outstanding, disposed.admitted),
        (0, visited.admitted)
    );
    // A definite refusal of a request that records a source still owns that
    // source and its original result until the caller disposes the reply:
    // OPEN of a directory.
    let refused = wait(NativeRead::prepare(
        bound.request(&Fence::default()).unwrap(),
        mount,
        8,
        root,
        None,
        NativeReadOperation::Open {
            serial: root,
            writable: false,
        },
    ))
    .unwrap();
    assert!(matches!(refused.value(), Err(Refusal::IsDirectory)));
    let held = client.diagnostics().unwrap();
    assert_eq!(held.outstanding, 1);
    assert_eq!(
        held.completed[ServiceClass::Source as usize],
        disposed.completed[ServiceClass::Source as usize] + 1
    );
    wait(refused.dispose()).unwrap();
    let released = client.diagnostics().unwrap();
    // Exactly the release of that source.
    assert_eq!(
        (released.outstanding, released.admitted),
        (0, held.admitted + 1)
    );
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
    owner.stop().unwrap();
    let before = client.diagnostics().unwrap().admitted;
    // A lookup's first owner job is its visit; an OPENDIR's is the
    // acquisition of its recorded source. Each is retained in its own slot.
    let requests = [
        (
            u64::MAX,
            NativeReadOperation::Lookup {
                parent: root,
                name: PathName::new("file-000000").unwrap(),
            },
        ),
        (u64::MAX - 1, NativeReadOperation::Opendir { serial: root }),
    ];
    // Both are admitted before either runs: the first retained failure makes
    // the lane terminal, which refuses later admission and not a handoff.
    let permits = requests
        .iter()
        .map(|_| queue.receive().unwrap().admit(0).unwrap())
        .collect::<Vec<_>>();
    for (slot, (permit, (request, operation))) in permits
        .into_iter()
        .zip(requests.iter().cloned())
        .enumerate()
    {
        let services = bound.request(&Fence::default()).unwrap();
        permit
            .handoff(Box::pin(async move {
                match NativeRead::prepare(services, mount, request, root, None, operation).await {
                    Err(error) => RequestDisposition::Retained(Box::new(error)),
                    Ok(_) => panic!("a stopped owner cannot run a new job"),
                }
            }))
            .unwrap();
        until(|| queue.work().unwrap().retained == slot + 1);
    }
    for (slot, (request, operation)) in requests.iter().enumerate() {
        queue
            .inspect_retained(slot, |failure| {
                let FailureView::Request(error) = failure else {
                    panic!("expected original owner error")
                };
                let error = error.downcast_ref::<ReadFailure>().unwrap();
                assert_eq!(error.request(), *request);
                assert_eq!(error.retained_source(), None);
                assert_eq!(error.retained_read(), None);
                assert_eq!(error.protected(), root);
                match (error.operation(), operation) {
                    (
                        NativeReadOperation::Lookup { parent, name },
                        NativeReadOperation::Lookup { .. },
                    ) => assert!(*parent == root && name.as_bytes() == b"file-000000"),
                    (
                        NativeReadOperation::Opendir { serial },
                        NativeReadOperation::Opendir { .. },
                    ) => assert_eq!(*serial, root),
                    other => panic!("retained another operation: {other:?}"),
                }
                let original = error.reason.downcast_ref::<OwnerError>().unwrap();
                let OwnerError::Unattempted { cause, command } = original else {
                    panic!("{original:?}")
                };
                assert!(matches!(cause.as_ref(), OwnerError::Stopped));
                // The command that was never submitted comes back unchanged.
                match (command.as_ref(), operation) {
                    (
                        Command::Native(NativeJob::ObserveVisit(visit)),
                        NativeReadOperation::Lookup { .. },
                    ) => assert_eq!(visit.mount(), mount),
                    (
                        Command::Native(NativeJob::Source {
                            mount: sourced,
                            request: sourced_request,
                            serial,
                        }),
                        NativeReadOperation::Opendir { .. },
                    ) => assert_eq!(
                        (*sourced, *sourced_request, *serial),
                        (mount, *request, root)
                    ),
                    other => panic!("unattempted another command: {other:?}"),
                }
            })
            .unwrap();
    }
    assert_eq!(client.diagnostics().unwrap().admitted, before);
    drop(pool);
    drop(queue);
    drop(bound);
    drop(store);
    f.cleanup();
}

#[test]
fn full_handoff_of_metadata_consumers_can_advance_to_data_without_more_sql_credit() {
    use std::sync::Mutex;
    struct Gate(Mutex<(bool, Vec<Option<Waker>>)>);
    struct Turn(Arc<Gate>, usize);
    impl Future for Turn {
        type Output = ();
        fn poll(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            let mut state = self.0 .0.lock().unwrap();
            if state.0 {
                Poll::Ready(())
            } else {
                state.1[self.1] = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
    let f = support::Fixture::new(1, "full-native-handoff");
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
                workspace: WorkspaceId::from_authority([110; 32]).unwrap(),
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
        Ok(Response::Native(NativeReply::Mount(mount))) => *mount,
        other => panic!("{other:?}"),
    };
    drop(done);
    let lookup = wait(NativeRead::prepare(
        bound.request(&Fence::default()).unwrap(),
        mount,
        1,
        root,
        None,
        NativeReadOperation::Lookup {
            parent: root,
            name: PathName::new("file-000000").unwrap(),
        },
    ))
    .unwrap();
    let serial = lookup.value().unwrap().stat.serial;
    wait(lookup.dispose()).unwrap();
    let mut pool = Dispatch::start(DispatchConfig {
        read_handles: 1,
        namespaces: 1,
    })
    .unwrap();
    let queue = pool.register(mount).unwrap();
    let count = layerfs_fuse::HANDOFFS;
    let gate = Arc::new(Gate(Mutex::new((false, vec![None; count]))));
    let (prepared, ready) = mpsc::channel();
    let (done, completed) = mpsc::channel();
    for index in 0..count {
        let services = bound.request(&Fence::default()).unwrap();
        let prepared = prepared.clone();
        let done = done.clone();
        let gate = gate.clone();
        queue
            .receive()
            .unwrap()
            .admit(0)
            .unwrap()
            .handoff(Box::pin(async move {
                let answer = NativeRead::prepare(
                    services,
                    mount,
                    1000 + index as u64,
                    serial,
                    None,
                    NativeReadOperation::Data { serial },
                )
                .await
                .unwrap();
                prepared.send(()).unwrap();
                Turn(gate, index).await;
                let data = answer
                    .read_file(0, layerfs_overlay::READ_WINDOW as u32)
                    .await
                    .unwrap();
                assert_eq!(data.bytes(), support::bytes(0));
                data.dispose().await.unwrap();
                done.send(()).unwrap();
                RequestDisposition::Complete
            }))
            .unwrap();
    }
    for _ in 0..count {
        ready.recv_timeout(WAIT).unwrap();
    }
    until(|| queue.work().unwrap().parked == count);
    assert_eq!(client.diagnostics().unwrap().outstanding, count);
    let wakes = {
        let mut state = gate.0.lock().unwrap();
        state.0 = true;
        std::mem::take(&mut state.1)
    };
    for wake in wakes.into_iter().flatten() {
        wake.wake();
    }
    for _ in 0..count {
        completed.recv_timeout(WAIT).unwrap_or_else(|error| panic!(
            "full native handoff cannot advance: {error}; native={:?}; owner={:?}; store={:?}; fixture={:?}",
            queue.work().unwrap(), client.diagnostics().unwrap(), store.read_work(), f.directory));
    }
    until(|| queue.work().unwrap().admitted == 0);
    assert_eq!(client.diagnostics().unwrap().outstanding, 0);
    let services = bound.request(&Fence::default()).unwrap();
    drop(wait(services.forget(mount, serial, 1)).unwrap());
    let done = wait(
        client
            .try_submit(
                Some(bound.route()),
                Command::Native(NativeJob::Revoke(mount)),
            )
            .unwrap(),
    )
    .unwrap();
    assert!(done.result().is_ok());
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
fn directory_consumer_publishes_only_accepted_names_and_survives_descriptor_close() {
    let f = support::Fixture::new(65, "native-directory-consumer");
    assert_eq!(f.count, 65);
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
                workspace: WorkspaceId::from_authority([111; 32]).unwrap(),
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
        Ok(Response::Native(NativeReply::Mount(mount))) => *mount,
        other => panic!("{other:?}"),
    };
    drop(done);
    let open = wait(NativeRead::prepare(
        bound.request(&Fence::default()).unwrap(),
        mount,
        10,
        root,
        None,
        NativeReadOperation::Opendir { serial: root },
    ))
    .unwrap();
    let directory = open.value().unwrap().directory.unwrap();
    wait(open.dispose()).unwrap();
    let attributes = wait(NativeRead::prepare(
        bound.request(&Fence::default()).unwrap(),
        mount,
        11,
        root,
        Some(directory.owner_id()),
        NativeReadOperation::Getattr { serial: root },
    ))
    .unwrap();
    assert_eq!(
        attributes.value().unwrap().stat.kind,
        layerfs_content::object::inode_leaf::InodeKind::Directory
    );
    wait(attributes.dispose()).unwrap();

    let services = bound.request(&Fence::default()).unwrap();
    // Keep every admitted request's offered page alive at once. Their original
    // SQL replies must already be consumed, so publishing any prefix still has
    // admission capacity without raising the owner's16-slot configuration.
    let mut offered = Vec::new();
    for index in 0..layerfs_fuse::HANDOFFS {
        let stream = wait(DirectoryStream::prepare(
            services.clone(),
            mount,
            100 + index as u64,
            root,
            directory.owner_id(),
            2,
        ))
        .unwrap();
        let DirectoryStep::Batch(batch) = wait(stream.next()).unwrap() else {
            panic!("missing pressure page")
        };
        offered.push(batch);
    }
    until(|| client.diagnostics().unwrap().outstanding == 0);
    for batch in offered {
        let stream = wait(batch.accept(0)).unwrap();
        wait(stream.dispose()).unwrap();
    }
    let stream = wait(DirectoryStream::prepare(
        services.clone(),
        mount,
        12,
        root,
        directory.owner_id(),
        0,
    ))
    .unwrap();
    assert_eq!(
        stream.dots().collect::<Vec<_>>(),
        vec![(".", root, 1), ("..", root, 2)]
    );
    until(|| client.diagnostics().unwrap().outstanding == 0);
    let DirectoryStep::Batch(batch) = wait(stream.next()).unwrap() else {
        panic!("missing first directory page")
    };
    let first: Vec<_> = batch
        .entries()
        .map(|(entry, cookie)| (entry.name.clone(), cookie))
        .collect();
    assert_eq!(first.len(), 64);
    until(|| client.diagnostics().unwrap().outstanding == 0);
    let last_accepted = first[2].1;
    let never_accepted = first[3].1;
    let stream = wait(batch.accept(3)).unwrap();
    let DirectoryStep::End(stream) = wait(stream.next()).unwrap() else {
        panic!("partial buffer must end this reply")
    };
    wait(stream.dispose()).unwrap();
    let invalid = match wait(DirectoryStream::prepare(
        services.clone(),
        mount,
        13,
        root,
        directory.owner_id(),
        never_accepted,
    )) {
        Err(error) => error,
        Ok(_) => panic!("reserved but unreturned cookie became visible"),
    };
    assert_eq!(invalid.retained_source(), None);
    drop(invalid);

    let stream = wait(DirectoryStream::prepare(
        services.clone(),
        mount,
        14,
        root,
        directory.owner_id(),
        last_accepted,
    ))
    .unwrap();
    assert_eq!(stream.dots().count(), 0);
    let DirectoryStep::Batch(batch) = wait(stream.next()).unwrap() else {
        panic!("missing continuation")
    };
    let remainder: Vec<_> = batch
        .entries()
        .map(|(entry, _)| entry.name.clone())
        .collect();
    assert_eq!(
        remainder,
        (3..65)
            .map(|index| format!("file-{index:06}").into_bytes())
            .collect::<Vec<_>>()
    );
    // RELEASEDIR cannot invalidate an already acquired read/cookie source.
    drop(wait(services.close_directory(directory)).unwrap());
    let closed = match wait(NativeRead::prepare(
        bound.request(&Fence::default()).unwrap(),
        mount,
        15,
        root,
        Some(directory.owner_id()),
        NativeReadOperation::Getattr { serial: root },
    )) {
        Err(error) => error,
        Ok(_) => panic!("closed directory handle acquired a new source"),
    };
    assert_eq!(closed.retained_source(), None);
    drop(closed);
    let stream = wait(batch.accept(remainder.len())).unwrap();
    let DirectoryStep::End(stream) = wait(stream.next()).unwrap() else {
        panic!("unexpected extra names")
    };
    wait(stream.dispose()).unwrap();
    until(|| client.diagnostics().unwrap().outstanding == 0);
    assert_eq!(store.read_work().outstanding, 0);
    let done = wait(
        client
            .try_submit(
                Some(bound.route()),
                Command::Native(NativeJob::Revoke(mount)),
            )
            .unwrap(),
    )
    .unwrap();
    assert!(done.result().is_ok());
    drop(done);
    drop(services);
    drop(bound);
    owner.stop().unwrap();
    drop(store);
    f.cleanup();
}
