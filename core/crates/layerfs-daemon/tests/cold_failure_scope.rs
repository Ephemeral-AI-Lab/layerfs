//! How far a failed base demand reaches, without a kernel mount: the real
//! owner, the real Store read sessions and the real dispatcher lane. A Store
//! read failure ends the one request that made the demand and leaves its
//! original cause in the mount's bounded record; an owner job's failure is
//! retained as before. The uncertain read session is staged the way
//! `store_read_service.rs` stages it, through the provider's public port with
//! the provider's own answer. Public API only; every wait is bounded and no
//! thread is spawned by the test.
#[path = "support/installed_store.rs"]
mod support;
use layerfs_content::{filesystem::PathName, ObjectId};
use layerfs_daemon::{
    store::{
        BindRequest, BoundWorkspace, PortError, ReadAdmissionError, ReadLimits, Store, StoreReader,
    },
    Command, Completion, NativeJob, NativeReply, Owner, OwnerClient, OwnerConfig, Response,
    ServiceClass,
};
use layerfs_fuse::{
    operations::{
        create::mkdir, DirectoryStream, MutationInput, MutationRequest, NativeData, NativeMutation,
        NativeRead, ReadDataInput, ReadFailure,
    },
    ports::{FailedDemands, Fence, MountServices, RequestServices},
    Dispatch, DispatchConfig, FailureView, MountQueue, RequestDisposition,
};
use layerfs_history::{HistoryCatalog, WorkspaceId};
use layerfs_overlay::{NativeMount, ProfileConfig, READ_WINDOW};
use layerfs_persistence::{Handles, StorageProvider};
use layerfs_storage::{
    location::PackInfo,
    port::{PackPersistence, PackReadChoice, PackReadPlan, PersistenceError},
    ReservationBlocks, Storage,
};
use layerfs_workspace::{NativeReadOperation, Time};
use std::{
    error::Error,
    future::Future,
    pin::Pin,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);
const NOW: Time = Time {
    seconds: 1_700_000_000,
    nanoseconds: 0,
};
type Cause = Arc<dyn Error + Send + Sync>;

struct Event(mpsc::SyncSender<()>);
impl Wake for Event {
    fn wake(self: Arc<Self>) {
        let _ = self.0.try_send(());
    }
}
/// Polls with a real wakeup channel; a future that never completes fails the
/// test at its deadline instead of hanging it.
fn wait<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let (send, woken) = mpsc::sync_channel(1);
    let waker = Waker::from(Arc::new(Event(send)));
    let deadline = Instant::now() + WAIT;
    loop {
        if let Poll::Ready(value) = Pin::as_mut(&mut future).poll(&mut Context::from_waker(&waker))
        {
            return value;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if woken.recv_timeout(remaining).is_err() {
            panic!("port call did not complete within {WAIT:?}");
        }
    }
}
fn until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline, "bounded observation: {what}");
        std::thread::yield_now();
    }
}
fn finish(client: &OwnerClient, route: layerfs_overlay::Route, command: Command) -> Completion {
    let pending = match client.try_submit(Some(route), command) {
        Ok(pending) => pending,
        Err((error, command)) => panic!("admission of {command:?}: {error:?}"),
    };
    wait(pending).unwrap()
}

struct Uncertain;
impl PackReadPlan for Uncertain {
    fn select(&mut self, _: PackInfo, _: &[u8]) -> Result<PackReadChoice, PersistenceError> {
        Err(PersistenceError::Uncertain)
    }
}
/// The provider's own uncertain answer to one real read on this session.
fn quarantine(provider: &StorageProvider, object: ObjectId) {
    let mut rows = Vec::new();
    provider.locate(&[object], &mut rows).unwrap();
    assert!(matches!(
        provider.read_scoped_pack(rows[0].location.pack_id, &mut Uncertain),
        Err(PersistenceError::Uncertain)
    ));
}
fn port_error(cause: &Cause) -> &PortError {
    cause
        .downcast_ref::<PortError>()
        .expect("the recorded cause is the Store's original PortError")
}
/// What the request driver does with a failed read after its one reply: a
/// fenced request or a failed base demand gives back what it holds and
/// completes; anything else is retained.
async fn end(failure: ReadFailure) -> RequestDisposition {
    if !failure.fenced() && failure.base_demand().is_none() {
        return RequestDisposition::Retained(Box::new(failure));
    }
    match failure.relinquish().await {
        Ok(()) => RequestDisposition::Complete,
        Err(failure) => RequestDisposition::Retained(Box::new(failure)),
    }
}

/// One bound Workspace over a Store with two read sessions and no object
/// cache, a native mount group and a dispatcher lane, and no kernel
/// connection. The Store is composed from the public pieces `open_store`
/// composes so that the test keeps each read session's provider.
struct Rig {
    fixture: support::Fixture,
    readers: Vec<Arc<StorageProvider>>,
    object: ObjectId,
    store: Arc<Store>,
    owner: Owner,
    client: OwnerClient,
    identity: WorkspaceId,
    bound: BoundWorkspace,
    root: u64,
    mount: NativeMount,
    pool: Dispatch,
    queue: MountQueue,
}
impl Rig {
    fn new(label: &str, authority: u8) -> Self {
        let fixture = support::Fixture::new(1, label);
        assert_eq!(fixture.count, 1);
        let writer =
            Handles::open_writable(fixture.config.clone(), support::BINDING, support::CURSOR)
                .unwrap();
        let history = Arc::new(writer.history);
        let object = history
            .branch_snapshot(fixture.branch)
            .unwrap()
            .unwrap()
            .effective_root;
        let mut readers = Vec::new();
        let sessions = (0..2)
            .map(|_| {
                let read = Handles::open_read_only(
                    fixture.config.clone(),
                    support::BINDING,
                    support::CURSOR,
                )
                .unwrap();
                readers.push(read.storage.clone());
                StoreReader::new(Storage::new(read.storage).unwrap(), Arc::new(read.history))
            })
            .collect();
        let store = Arc::new(
            Store::new(
                writer.storage,
                history,
                sessions,
                0,
                ReservationBlocks::default(),
                ReadLimits::default(),
            )
            .unwrap(),
        );
        let owner = Owner::start(
            &fixture.directory.join("overlay"),
            ProfileConfig::default(),
            OwnerConfig::default(),
        )
        .unwrap();
        let client = owner.client();
        let identity = WorkspaceId::from_authority([authority; 32]).unwrap();
        let bound = store
            .bind(
                client.clone(),
                BindRequest {
                    branch: fixture.branch,
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
        let done = finish(
            &client,
            bound.route(),
            Command::Native(NativeJob::Mount { root }),
        );
        let mount = match done.result() {
            Ok(Response::Native(NativeReply::Mount(mount))) => *mount,
            other => panic!("mount: {other:?}"),
        };
        drop(done);
        let pool = Dispatch::start(DispatchConfig {
            read_handles: 2,
            namespaces: 1,
        })
        .unwrap();
        let queue = pool.register(mount).unwrap();
        Self {
            fixture,
            readers,
            object,
            store,
            owner,
            client,
            identity,
            bound,
            root,
            mount,
            pool,
            queue,
        }
    }
    fn services(&self, fence: &Fence) -> Arc<dyn RequestServices> {
        self.bound.request(fence).unwrap()
    }
    fn read(
        &self,
        services: Arc<dyn RequestServices>,
        request: u64,
        protected: u64,
        handle: Option<u64>,
        operation: NativeReadOperation,
    ) -> NativeRead {
        wait(NativeRead::prepare(
            services, self.mount, request, protected, handle, operation,
        ))
        .unwrap()
    }
    fn owner_work(&self) -> layerfs_daemon::OwnerWork {
        self.client.diagnostics().unwrap()
    }
    /// Revocation refuses while any request source or read row remains, so
    /// its success shows that every request gave back what it held.
    fn revoke_and_stop(mut self, retained: bool) {
        let done = finish(
            &self.client,
            self.bound.route(),
            Command::Native(NativeJob::Revoke(self.mount)),
        );
        assert!(
            matches!(done.result(), Ok(Response::Native(NativeReply::Done))),
            "revoke: {:?}",
            done.result()
        );
        drop(done);
        assert_eq!(self.store.read_work().outstanding, 0);
        if retained {
            // A retained request keeps its lane slot and the original owner
            // result it failed with; nothing disposes it but the lane's end.
            until("only the retained result is outstanding", || {
                self.owner_work().outstanding == 1
            });
            assert!(self.queue.finish().is_err());
            drop(self.pool);
            drop(self.queue);
        } else {
            self.queue.stop_admission().unwrap();
            self.queue.finish().unwrap();
            assert!(self.pool.stop().unwrap().clean());
        }
        until("owner credits returned", || {
            self.client.diagnostics().unwrap().outstanding == 0
        });
        drop(self.bound);
        self.owner.stop().unwrap();
        drop(self.store);
        drop(self.readers);
        self.fixture.cleanup();
    }
}

#[test]
fn a_failed_base_demand_ends_its_own_request_and_keeps_its_original_cause() {
    let rig = Rig::new("cold-failure-demand", 151);
    let (mount, root) = (rig.mount, rig.root);
    let fence = rig.queue.fence();
    let services = rig.services(&fence);
    assert_eq!(fence.failed_demands().count, 0);

    // The test leases one read session, and the other is put into its
    // uncertain state: the next cold demand is served by that one.
    let healthy = wait(rig.store.read_ticket(Some(rig.identity)).unwrap()).unwrap();
    let bad = 1 - healthy.index();
    quarantine(&rig.readers[bad], rig.object);
    assert_eq!(rig.store.read_work().quarantined, 0);

    let before = rig.owner_work();
    let (send, observed) = mpsc::channel();
    {
        let services = rig.services(&fence);
        rig.queue
            .receive()
            .unwrap()
            .admit(11)
            .unwrap()
            .handoff(Box::pin(async move {
                let prepared = NativeRead::prepare(
                    services,
                    mount,
                    1,
                    root,
                    None,
                    NativeReadOperation::Lookup {
                        parent: root,
                        name: PathName::new("file-000000").unwrap(),
                    },
                )
                .await;
                match prepared {
                    Ok(answer) => {
                        let _ = send.send(None);
                        match answer.dispose().await {
                            Ok(()) => RequestDisposition::Complete,
                            Err(failure) => RequestDisposition::Retained(Box::new(failure)),
                        }
                    }
                    Err(failure) => {
                        let _ = send.send(Some((
                            failure.fenced(),
                            failure.base_demand().map(|marker| marker.cause().clone()),
                            failure.retained_source().is_some(),
                            failure.retained_read().is_some(),
                        )));
                        end(failure).await
                    }
                }
            }))
            .unwrap();
    }
    let (fenced, cause, source, read) = observed
        .recv_timeout(WAIT)
        .unwrap()
        .expect("a lookup served by the uncertain session was answered");
    let cause = cause.expect("the failure is a failed base demand");
    // A lookup is served by owner visits that record no request source: the
    // failed demand found it holding nothing.
    assert_eq!((fenced, source, read), (false, false, false));
    until("the request left the lane", || {
        rig.queue.work().unwrap().admitted == 0
    });
    let lane = rig.queue.work().unwrap();
    assert_eq!(
        (lane.completed, lane.retained, lane.terminal),
        (1, 0, false)
    );
    assert!(!fence.stopped());
    assert_eq!(fence.terminal_replies(), 0);
    until("the request's owner results were returned", || {
        rig.owner_work().outstanding == 0
    });
    // Its whole owner work was the one undecided visit before the demand:
    // no source was acquired, so no disposal call followed the failure.
    let after = rig.owner_work();
    let class =
        |work: &layerfs_daemon::OwnerWork, class: ServiceClass| work.completed[class as usize];
    assert_eq!(after.admitted, before.admitted + 1);
    assert_eq!(
        class(&after, ServiceClass::Read),
        class(&before, ServiceClass::Read) + 1
    );
    assert_eq!(
        class(&after, ServiceClass::Source),
        class(&before, ServiceClass::Source)
    );
    assert_eq!(
        class(&after, ServiceClass::Lifecycle),
        class(&before, ServiceClass::Lifecycle)
    );

    // The session is excluded and reported with the same original failure
    // the mount's record holds: one allocation, not a copy or a text.
    let readers = rig.store.read_work();
    assert_eq!((readers.quarantined, readers.outstanding), (1, 1));
    let failures = rig.store.reader_failures();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].0, bad);
    assert!(matches!(
        failures[0].1.as_ref(),
        PortError::Storage(error) if error.is_unknown_outcome()
    ));
    let FailedDemands {
        count,
        first,
        latest,
    } = fence.failed_demands();
    let latest = latest.unwrap();
    assert_eq!(count, 1);
    assert!(Arc::ptr_eq(&latest, &cause));
    assert!(Arc::ptr_eq(&first.unwrap(), &cause));
    assert!(std::ptr::eq(port_error(&latest), failures[0].1.as_ref()));
    println!(
        "COLD-DEMAND request=(base_demand,no_source,no_read)->Complete owner_jobs=(1 visit, 0 source, 0 release) lane=(completed 1, retained 0, terminal false) quarantined=1 record=(count 1, latest={})",
        port_error(&latest)
    );

    // Later requests on the same lane are served by the healthy session.
    drop(healthy);
    let grants = rig.store.read_work().grants;
    let (send, found) = mpsc::channel();
    {
        let services = rig.services(&fence);
        rig.queue
            .receive()
            .unwrap()
            .admit(11)
            .unwrap()
            .handoff(Box::pin(async move {
                let answer = match NativeRead::prepare(
                    services,
                    mount,
                    2,
                    root,
                    None,
                    NativeReadOperation::Lookup {
                        parent: root,
                        name: PathName::new("file-000000").unwrap(),
                    },
                )
                .await
                {
                    Ok(answer) => answer,
                    Err(failure) => return end(failure).await,
                };
                let _ = send.send(answer.value().ok().map(|value| value.stat.serial));
                match answer.dispose().await {
                    Ok(()) => RequestDisposition::Complete,
                    Err(failure) => RequestDisposition::Retained(Box::new(failure)),
                }
            }))
            .unwrap();
    }
    let serial = found
        .recv_timeout(WAIT)
        .expect("the later lookup was answered")
        .expect("the later lookup found the file");
    until("the later request left the lane", || {
        rig.queue.work().unwrap().admitted == 0
    });
    let lane = rig.queue.work().unwrap();
    assert_eq!((lane.completed, lane.retained), (2, 0));
    let opened = rig.read(
        services.clone(),
        3,
        serial,
        None,
        NativeReadOperation::Open {
            serial,
            writable: false,
        },
    );
    let handle = opened.value().unwrap().file.unwrap().owner_id();
    wait(opened.dispose()).unwrap();
    let window = ReadDataInput::File {
        offset: 0,
        length: READ_WINDOW as u32,
    };
    let data = wait(NativeData::read(
        services.clone(),
        mount,
        4,
        serial,
        Some(handle),
        window,
    ))
    .unwrap()
    .unwrap();
    assert_eq!(data.bytes(), support::bytes(0));
    drop(data);
    let readers = rig.store.read_work();
    assert!(readers.grants > grants);
    assert_eq!((readers.quarantined, readers.outstanding), (1, 0));
    assert_eq!(rig.store.reader_failures().len(), 1);
    assert_eq!(fence.failed_demands().count, 1);

    // A base demand that fails without quarantining anything: read admission
    // is stopped, so each driver's next demand is refused a reader. Whatever
    // the prepared requests hold was acquired while readers were still
    // granted; a request that starts afterwards still gets its source from
    // the owner, which stopped reads do not touch.
    let listed = rig.read(
        services.clone(),
        6,
        root,
        None,
        NativeReadOperation::Opendir { serial: root },
    );
    let directory = listed.value().unwrap().directory.unwrap();
    wait(listed.dispose()).unwrap();
    let stream = wait(DirectoryStream::prepare(
        services.clone(),
        mount,
        7,
        root,
        directory.owner_id(),
        0,
    ))
    .unwrap();
    rig.store.stop_reads();
    let stopped = |cause: &Cause| {
        assert!(
            matches!(
                port_error(cause),
                PortError::ReadAdmission(ReadAdmissionError::Stopped)
            ),
            "{cause}"
        );
    };

    // A READ of base bytes is one owner visit and then the one base read it
    // is refused a reader for. It recorded nothing: the failure holds no
    // source and no read, and giving it up admits no owner job.
    let before = rig.owner_work();
    let failure = match wait(NativeData::read(
        services.clone(),
        mount,
        5,
        serial,
        Some(handle),
        window,
    )) {
        Err(failure) => failure,
        Ok(_) => panic!("file data was read without a reader"),
    };
    stopped(failure.base_demand().unwrap().cause());
    assert!(!failure.fenced());
    assert!(failure.retained_source().is_none() && failure.retained_read().is_none());
    assert_eq!(failure.data_input(), Some(window));
    wait(failure.relinquish()).unwrap();
    let after = rig.owner_work();
    assert_eq!(after.admitted, before.admitted + 1);
    assert_eq!(
        after.completed[ServiceClass::Read as usize],
        before.completed[ServiceClass::Read as usize] + 1
    );

    // OPENDIR is a visit too: it needs the directory's base inode, is
    // refused a reader, and holds nothing.
    let failure = match wait(NativeRead::prepare(
        services.clone(),
        mount,
        9,
        root,
        None,
        NativeReadOperation::Opendir { serial: root },
    )) {
        Err(failure) => failure,
        Ok(_) => panic!("a directory was opened without its base fact"),
    };
    stopped(failure.base_demand().unwrap().cause());
    assert!(!failure.fenced());
    assert!(failure.retained_source().is_none() && failure.retained_read().is_none());
    let admitted = rig.owner_work().admitted;
    wait(failure.relinquish()).unwrap();
    // Giving it up admits no owner job.
    assert_eq!(rig.owner_work().admitted, admitted);

    let before = rig.owner_work();
    let failure = match wait(NativeMutation::perform(
        services.clone(),
        MutationRequest {
            mount,
            request: 8,
            protected: root,
            handle: None,
            input: MutationInput::Named(mkdir(root, PathName::new("never-made").unwrap(), 0o755)),
            open: None,
            now: NOW,
        },
    )) {
        Err(failure) => failure,
        Ok(_) => panic!("a mutation was decided without its base fact"),
    };
    stopped(failure.base_demand().unwrap().cause());
    assert!(!failure.fenced());
    // A mutation records no request source, and this one published nothing:
    // it holds nothing, and giving it up admits no owner job.
    assert!(failure.publication().is_none());
    wait(failure.relinquish()).unwrap();
    let after = rig.owner_work();
    assert_eq!(after.admitted, before.admitted + 1);
    assert_eq!(
        after.completed[ServiceClass::Mutation as usize],
        before.completed[ServiceClass::Mutation as usize] + 1
    );

    let failure = match wait(stream.next()) {
        Err(failure) => failure,
        Ok(_) => panic!("a directory page was listed without a reader"),
    };
    stopped(failure.base_demand().unwrap().cause());
    assert!(!failure.fenced() && failure.offered().is_none());
    wait(failure.relinquish()).unwrap();

    // Fixed slots: the count grew by four; the first original cause is
    // still the quarantined reader's failure and the latest is the last one.
    let FailedDemands {
        count,
        first,
        latest,
    } = fence.failed_demands();
    assert_eq!(count, 5);
    assert!(std::ptr::eq(
        port_error(&first.unwrap()),
        rig.store.reader_failures()[0].1.as_ref()
    ));
    stopped(&latest.unwrap());
    let readers = rig.store.read_work();
    assert_eq!((readers.quarantined, readers.outstanding), (1, 0));
    assert_eq!(rig.store.reader_failures().len(), 1);
    assert!(!fence.stopped());
    assert_eq!(fence.terminal_replies(), 0);
    println!(
        "COLD-ADMISSION read=(base_demand,no_source,no_read; 1 visit, 0 release) opendir=(base_demand,source_held,no_read)->released mutation=(base_demand,no_source,no_ticket)->nothing held directory=(base_demand,source_held)->released quarantined=1 record=(count 5, first=quarantined reader failure, latest=ReadAdmission(Stopped))"
    );

    drop(wait(services.close_file(mount, serial, handle)).unwrap());
    drop(wait(services.close_directory(mount, directory.serial(), directory.owner_id())).unwrap());
    drop(wait(services.forget(mount, serial, 1)).unwrap());
    drop(services);
    rig.revoke_and_stop(false);
}

#[test]
fn an_owner_job_failure_is_not_a_base_demand_and_is_still_retained() {
    let rig = Rig::new("cold-failure-owner", 152);
    let (mount, root) = (rig.mount, rig.root);
    let fence = rig.queue.fence();
    let services = rig.services(&fence);

    // A closed directory handle: the owner refuses the source acquisition.
    let listed = rig.read(
        services.clone(),
        1,
        root,
        None,
        NativeReadOperation::Opendir { serial: root },
    );
    let directory = listed.value().unwrap().directory.unwrap();
    wait(listed.dispose()).unwrap();
    drop(wait(services.close_directory(mount, directory.serial(), directory.owner_id())).unwrap());
    let through_closed = |request| {
        NativeRead::prepare(
            rig.services(&fence),
            mount,
            request,
            root,
            Some(directory.owner_id()),
            NativeReadOperation::Getattr { serial: root },
        )
    };

    let request = through_closed(2);
    rig.queue
        .receive()
        .unwrap()
        .admit(11)
        .unwrap()
        .handoff(Box::pin(async move {
            match request.await {
                Err(failure) => end(failure).await,
                Ok(_) => panic!("a closed directory handle acquired a source"),
            }
        }))
        .unwrap();
    until("the owner failure was retained", || {
        rig.queue.work().unwrap().retained == 1
    });
    let reason = rig
        .queue
        .inspect_retained(0, |failure| {
            let FailureView::Request(error) = failure else {
                panic!("expected the request's original failure")
            };
            let error = error.downcast_ref::<ReadFailure>().unwrap();
            assert_eq!(error.request(), 2);
            assert!(!error.fenced() && error.base_demand().is_none());
            assert!(error.reason.downcast_ref::<PortError>().is_none());
            error.reason.to_string()
        })
        .unwrap();
    let lane = rig.queue.work().unwrap();
    assert_eq!((lane.completed, lane.retained, lane.admitted), (0, 1, 1));

    // The relinquisher itself returns such a failure unchanged.
    let failure = match wait(through_closed(3)) {
        Err(failure) => failure,
        Ok(_) => panic!("a closed directory handle acquired a source"),
    };
    assert!(failure.base_demand().is_none());
    let admitted = rig.owner_work().admitted;
    let failure = wait(failure.relinquish()).unwrap_err();
    assert_eq!(failure.request(), 3);
    assert_eq!(rig.owner_work().admitted, admitted);
    drop(failure);

    // Nothing was recorded as a failed base demand, and no reader was touched.
    assert_eq!(fence.failed_demands().count, 0);
    assert!(fence.failed_demands().latest.is_none());
    assert_eq!(rig.store.read_work().quarantined, 0);
    println!(
        "OWNER-FAILURE request=(not base_demand)->Retained lane=(completed 0, retained 1) relinquish=unchanged record=(count 0) reason={reason}"
    );
    drop(services);
    rig.revoke_and_stop(true);
}
