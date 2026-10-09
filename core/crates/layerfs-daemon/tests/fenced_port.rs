//! The daemon's Fuse port under a stopped fence, without a kernel mount: the
//! real owner, the real Store readers and the real dispatcher lane whose
//! `stop_service` stops the fence. Public API only; every wait is bounded and
//! no thread is spawned by the test.
#[path = "support/installed_store.rs"]
mod support;
use layerfs_content::filesystem::PathName;
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindRequest, BoundWorkspace, Store},
    Command, Completion, NativeJob, NativeReply, Owner, OwnerClient, OwnerConfig, Response,
    ServiceClass,
};
use layerfs_fuse::{
    operations::{
        create::mkdir, DirectoryStream, MutationInput, MutationRequest, NativeData, NativeMutation,
        NativeRead, ReadDataInput,
    },
    ports::{Fence, Fenced, MountServices, RequestServices, ServiceError},
    Dispatch, DispatchConfig, MountQueue, RequestDisposition,
};
use layerfs_history::WorkspaceId;
use layerfs_overlay::{NativeMount, ProfileConfig};
use layerfs_workspace::{NativeReadOperation, NativeVisitRequest, Time, VisitFacts};
use std::{
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

struct Event(mpsc::SyncSender<()>);
impl Wake for Event {
    fn wake(self: Arc<Self>) {
        let _ = self.0.try_send(());
    }
}
/// Polls with a real wakeup channel; a future that never completes fails the
/// test at its deadline instead of hanging it.
struct Stepper {
    waker: Waker,
    woken: mpsc::Receiver<()>,
}
impl Stepper {
    fn new() -> Self {
        let (send, woken) = mpsc::sync_channel(1);
        Self {
            waker: Waker::from(Arc::new(Event(send))),
            woken,
        }
    }
    fn poll<F: Future + ?Sized>(&self, future: Pin<&mut F>) -> Poll<F::Output> {
        future.poll(&mut Context::from_waker(&self.waker))
    }
    fn finish<F: Future + ?Sized>(&self, mut future: Pin<&mut F>, what: &str) -> F::Output {
        let deadline = Instant::now() + WAIT;
        loop {
            if let Poll::Ready(value) = self.poll(future.as_mut()) {
                return value;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if self.woken.recv_timeout(remaining).is_err() {
                panic!("{what} did not complete within {WAIT:?}");
            }
        }
    }
}
fn wait<T>(future: impl Future<Output = T>) -> T {
    Stepper::new().finish(std::pin::pin!(future), "port call")
}
fn until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline, "bounded observation: {what}");
        std::thread::yield_now();
    }
}
/// The first poll of an acquiring call on a stopped fence.
fn refused<T>(what: &str, future: impl Future<Output = Result<T, ServiceError>>) {
    match Stepper::new().poll(std::pin::pin!(future)) {
        Poll::Ready(Err(error)) if error.is::<Fenced>() => {}
        Poll::Ready(Err(error)) => panic!("{what}: refused for another reason: {error}"),
        Poll::Ready(Ok(_)) => panic!("{what}: attempted on a stopped fence"),
        Poll::Pending => panic!("{what}: waited on a stopped fence"),
    }
}
fn finish(client: &OwnerClient, route: layerfs_overlay::Route, command: Command) -> Completion {
    let pending = match client.try_submit(Some(route), command) {
        Ok(pending) => pending,
        Err((error, command)) => panic!("admission of {command:?}: {error:?}"),
    };
    wait(pending).unwrap()
}

/// One bound Workspace with a native mount group and a dispatcher lane, and
/// no kernel connection. The lane exists so that the product's own
/// `stop_service` stops the fence the port observes.
struct Rig {
    fixture: support::Fixture,
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
        let store = open_store(
            fixture.config.clone(),
            support::BINDING,
            support::CURSOR,
            1,
            0,
            Default::default(),
        )
        .unwrap();
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
            read_handles: 1,
            namespaces: 1,
        })
        .unwrap();
        let queue = pool.register(mount).unwrap();
        Self {
            fixture,
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
    fn lookup(
        &self,
        services: Arc<dyn RequestServices>,
        request: u64,
        parent: u64,
        name: &str,
    ) -> NativeRead {
        wait(NativeRead::prepare(
            services,
            self.mount,
            request,
            parent,
            None,
            NativeReadOperation::Lookup {
                parent,
                name: PathName::new(name).unwrap(),
            },
        ))
        .unwrap()
    }
    fn make_directory(&self, request: u64, parent: u64, name: &str) -> MutationRequest {
        MutationRequest {
            mount: self.mount,
            request,
            protected: parent,
            handle: None,
            input: MutationInput::Named(mkdir(parent, PathName::new(name).unwrap(), 0o755)),
            open: None,
            now: NOW,
        }
    }
    fn owner_work(&self) -> layerfs_daemon::OwnerWork {
        self.client.diagnostics().unwrap()
    }
    /// Reply tickets still owed in the Workspace: one Read-class owner job
    /// of the test, submitted beside the port and not through its fence.
    fn pending_publications(
        &self,
        route: layerfs_overlay::Route,
    ) -> Vec<layerfs_overlay::Publication> {
        let done = finish(
            &self.client,
            route,
            Command::PendingPublications { after: 0 },
        );
        match done.result() {
            Ok(Response::Publications(pending)) => pending.clone(),
            other => panic!("pending publications: {other:?}"),
        }
    }
    /// Revocation refuses while any request source or read row remains, so
    /// its success shows that every request gave back what it held.
    fn revoke_and_stop(mut self) {
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
        until("owner credits returned", || {
            self.owner_work().outstanding == 0
        });
        assert_eq!(self.store.read_work().outstanding, 0);
        self.queue.finish().unwrap();
        assert!(self.pool.stop().unwrap().clean());
        drop(self.bound);
        self.owner.stop().unwrap();
        drop(self.store);
        self.fixture.cleanup();
    }
}

#[test]
fn a_stopped_fence_refuses_every_acquiring_call_and_no_disposal_call() {
    let rig = Rig::new("fenced-port-calls", 141);
    let (mount, root) = (rig.mount, rig.root);
    let fence = rig.queue.fence();
    let services = rig.services(&fence);

    // Everything a later call needs is acquired while the mount still serves.
    let found = rig.lookup(services.clone(), 1, root, "file-000000");
    let serial = found.value().unwrap().stat.serial;
    wait(found.dispose()).unwrap();
    let opened = wait(NativeRead::prepare(
        services.clone(),
        mount,
        2,
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
    let listed = wait(NativeRead::prepare(
        services.clone(),
        mount,
        3,
        root,
        None,
        NativeReadOperation::Opendir { serial: root },
    ))
    .unwrap();
    let directory = listed.value().unwrap().directory.unwrap();
    wait(listed.dispose()).unwrap();
    // A READDIR reading visit made before the stop: it holds nothing, and
    // its offer of offsets is what a publishing visit would be given.
    let reply = wait(services.directory_visit(mount, root, directory.owner_id(), 2, None)).unwrap();
    let offer = reply.get().offer.clone().expect("offsets for the reply");
    drop(reply);
    // A mutation as an owner visit, which records no source.
    let fresh = services.reserve_serial().unwrap().unwrap();
    let visit = NativeVisitRequest {
        mount,
        request: 8,
        serial: root,
        handle: None,
        input: MutationInput::Named(mkdir(root, PathName::new("never-visited").unwrap(), 0o755)),
        open: None,
        now: NOW,
        fresh: Some(fresh),
        facts: Arc::new(VisitFacts::default()),
    };
    // A published mutation whose reply attempt is still owed.
    let published = wait(NativeMutation::perform(
        services.clone(),
        rig.make_directory(7, root, "made-before"),
    ))
    .unwrap();
    assert!(published.value().unwrap().changed);

    let admitted = rig.owner_work().admitted;
    let grants = rig.store.read_work().grants;
    rig.queue.stop_service().unwrap();
    assert!(fence.stopped());

    refused(
        "observe_visit",
        services.observe_visit(
            mount,
            14,
            root,
            None,
            NativeReadOperation::Getattr { serial: root },
            Arc::new(VisitFacts::default()),
        ),
    );
    refused(
        "observe_visit of an OPEN",
        services.observe_visit(
            mount,
            15,
            serial,
            None,
            NativeReadOperation::Open {
                serial,
                writable: false,
            },
            Arc::new(VisitFacts::default()),
        ),
    );
    refused("mutate_visit", services.mutate_visit(visit));
    refused("base", services.base());
    refused(
        "read_visit",
        services.read_visit(mount, serial, Some(handle), 0, 1),
    );
    refused(
        "directory_visit",
        services.directory_visit(mount, root, directory.owner_id(), 2, None),
    );
    refused(
        "publish_cookies",
        services.publish_cookies(offer, vec![b"file-000000".to_vec()]),
    );
    match services.reserve_serial() {
        Err(error) if error.is::<Fenced>() => {}
        other => panic!("reserve_serial on a stopped fence: {other:?}"),
    }
    // Refused before the attempt: no owner job was admitted, no reader granted.
    assert_eq!(rig.owner_work().admitted, admitted);
    assert_eq!(rig.store.read_work().grants, grants);
    assert_eq!(rig.store.read_work().outstanding, 0);

    // Every disposal call still runs: ticket, handles, lookup.
    // A replied mutation holds no source, and its reply attempt gives back a
    // ticket held in the engine's memory: nothing waits for this Workspace's
    // tickets, so the attempt is recorded with no owner job at all.
    let route = rig.bound.route();
    assert_eq!(rig.pending_publications(route).len(), 1);
    let admitted = rig.owner_work().admitted;
    wait(published.replied()).unwrap();
    assert_eq!(rig.owner_work().admitted, admitted);
    drop(wait(services.close_file(mount, serial, handle)).unwrap());
    drop(wait(services.close_directory(mount, directory.serial(), directory.owner_id())).unwrap());
    drop(wait(services.forget(mount, serial, 1)).unwrap());
    // A READ or READLINK served before the stop left nothing to dispose.
    assert_eq!(rig.owner_work().admitted, admitted + 3);
    // The stopped fence did not keep the ticket: it was returned.
    assert!(rig.pending_publications(route).is_empty());
    // The port itself replies to nothing.
    assert_eq!(fence.terminal_replies(), 0);
    println!(
        "FENCED-CALLS refused=8 owner_admitted_during_refusals=0 reader_grants_during_refusals=0 reply_attempt_jobs_after_stop=0 disposal_jobs_after_stop=3"
    );
    drop(services);
    rig.revoke_and_stop();
}

#[test]
fn a_wait_for_admission_or_for_a_reader_ends_at_the_stop_with_nothing_attempted() {
    let rig = Rig::new("fenced-port-waits", 142);
    let (mount, root) = (rig.mount, rig.root);
    let fence = rig.queue.fence();
    let services = rig.services(&fence);
    let stepper = Stepper::new();

    // A file opened while readers are still granted, for the READ below.
    let found = rig.lookup(services.clone(), 10, root, "file-000000");
    let serial = found.value().unwrap().stat.serial;
    wait(found.dispose()).unwrap();
    let opened = wait(NativeRead::prepare(
        services.clone(),
        mount,
        11,
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

    // Store readers: the only reader is leased to the test, so three cold
    // requests handed to the dispatcher park before their provider demand. A
    // lookup is served by owner visits and parks holding nothing; a READ of
    // base bytes made its one visit, recorded nothing and parks holding
    // nothing; an OPENDIR is an owner visit too and parks holding nothing.
    let reader = wait(rig.store.read_ticket(Some(rig.identity)).unwrap()).unwrap();
    let (send, observed) = mpsc::channel();
    {
        let services = rig.services(&fence);
        let send = send.clone();
        rig.queue
            .receive()
            .unwrap()
            .admit(0)
            .unwrap()
            .handoff(Box::pin(async move {
                let window = ReadDataInput::File {
                    offset: 0,
                    length: 4096,
                };
                match NativeData::read(services, mount, 302, serial, Some(handle), window).await {
                    Ok(_) => {
                        let _ = send.send((302, None));
                        RequestDisposition::Complete
                    }
                    Err(failure) => {
                        let _ = send.send((302, Some(failure.fenced())));
                        match failure.relinquish().await {
                            Ok(()) => RequestDisposition::Complete,
                            Err(failure) => RequestDisposition::Retained(Box::new(failure)),
                        }
                    }
                }
            }))
            .unwrap();
    }
    for (request, operation) in [
        (
            300,
            NativeReadOperation::Lookup {
                parent: root,
                name: PathName::new("file-000000").unwrap(),
            },
        ),
        (301, NativeReadOperation::Opendir { serial: root }),
    ] {
        let services = rig.services(&fence);
        let send = send.clone();
        rig.queue
            .receive()
            .unwrap()
            .admit(11)
            .unwrap()
            .handoff(Box::pin(async move {
                let prepared =
                    NativeRead::prepare(services, mount, request, root, None, operation).await;
                match prepared {
                    Ok(answer) => {
                        let _ = send.send((request, None));
                        match answer.dispose().await {
                            Ok(()) => RequestDisposition::Complete,
                            Err(failure) => RequestDisposition::Retained(Box::new(failure)),
                        }
                    }
                    Err(failure) => {
                        let _ = send.send((request, Some(failure.fenced())));
                        // What the request driver does after its terminal reply.
                        match failure.relinquish().await {
                            Ok(()) => RequestDisposition::Complete,
                            Err(failure) => RequestDisposition::Retained(Box::new(failure)),
                        }
                    }
                }
            }))
            .unwrap();
    }
    until("the three cold requests parked for a reader", || {
        rig.store.read_work().waiting == 3 && rig.queue.work().unwrap().parked == 3
    });
    until("their owner results were returned", || {
        rig.owner_work().outstanding == 0
    });

    // Owner admission: the namespace's whole job bound is held by results
    // the test keeps, so one more acquisition waits before it is submitted.
    let bound = OwnerConfig::default().jobs_per_namespace;
    let getattr = |request: u64| {
        services.observe_visit(
            mount,
            request,
            root,
            None,
            NativeReadOperation::Getattr { serial: root },
            Arc::new(VisitFacts::default()),
        )
    };
    let holders: Vec<_> = (0..bound)
        .map(|index| wait(getattr(100 + index as u64)).unwrap())
        .collect();
    assert_eq!(rig.owner_work().outstanding, bound);
    let admitted = rig.owner_work().admitted;
    let mut blocked = getattr(200);
    assert!(stepper.poll(blocked.as_mut()).is_pending());
    assert!(stepper.poll(blocked.as_mut()).is_pending());
    assert_eq!(rig.owner_work().admitted, admitted);
    // One direct reader wait on the port, beside the parked requests': for
    // the Workspace's current base.
    let mut cold_base = services.base();
    assert!(stepper.poll(cold_base.as_mut()).is_pending());
    until("all four reader waits parked", || {
        rig.store.read_work().waiting == 4 && rig.queue.work().unwrap().parked == 3
    });
    let grants = rig.store.read_work().grants;
    let before = rig.owner_work();

    rig.queue.stop_service().unwrap();

    // The parked requests needed no wakeup of their own: the stop ran them and
    // each failed before its demand. The lookup, the OPENDIR and the READ
    // held nothing.
    let mut seen = [
        observed.recv_timeout(WAIT).unwrap(),
        observed.recv_timeout(WAIT).unwrap(),
        observed.recv_timeout(WAIT).unwrap(),
    ];
    seen.sort();
    assert_eq!(
        seen,
        [(300, Some(true)), (301, Some(true)), (302, Some(true))]
    );
    until("the fenced requests left the lane", || {
        rig.queue.work().unwrap().admitted == 0
    });
    let lane = rig.queue.work().unwrap();
    assert_eq!((lane.completed, lane.retained), (3, 0));
    // The two waits on the port end as `Fenced`, having attempted nothing.
    match stepper.poll(cold_base.as_mut()) {
        Poll::Ready(Err(error)) if error.is::<Fenced>() => {}
        _ => panic!("base reader wait survived the stop"),
    }
    match stepper.poll(blocked.as_mut()) {
        Poll::Ready(Err(error)) if error.is::<Fenced>() => {}
        _ => panic!("admission wait survived the stop"),
    }
    drop((cold_base, blocked));
    let readers = rig.store.read_work();
    assert_eq!(
        (readers.waiting, readers.outstanding, readers.grants),
        (0, 1, grants)
    );
    drop(reader);
    // Nothing was admitted since the stop: the fenced lookup, the fenced
    // OPENDIR and the fenced READ had nothing to release, and the blocked
    // acquisition never was admitted.
    until("only the held results keep their credits", || {
        rig.owner_work().outstanding == bound
    });
    let after = rig.owner_work();
    assert_eq!(after.admitted, before.admitted);
    assert_eq!(
        after.completed[ServiceClass::Source as usize],
        before.completed[ServiceClass::Source as usize]
    );
    println!(
        "FENCED-WAITS admission_wait=Fenced base_reader_wait=Fenced parked_lookup=(fenced,no_source,no_read)->Complete parked_read=(fenced,no_source,no_read)->Complete parked_opendir=(fenced,no_source,no_read)->Complete owner_jobs_after_stop=0 source_jobs_after_stop=0 reader_grants_after_stop=0"
    );

    // The results the test held return their credits; they hold no row.
    drop(holders);
    drop(wait(services.close_file(mount, serial, handle)).unwrap());
    drop(wait(services.forget(mount, serial, 1)).unwrap());
    drop(services);
    rig.revoke_and_stop();
}

#[test]
fn a_job_submitted_before_the_stop_is_awaited_to_its_original_result() {
    let rig = Rig::new("fenced-port-submitted", 143);
    let (mount, root, route) = (rig.mount, rig.root, rig.bound.route());
    let fence = rig.queue.fence();
    let stepper = Stepper::new();

    // A published directory whose reply attempt is owed keeps a capture
    // parked, and a parked capture holds back every later mutation job.
    let first = wait(NativeMutation::perform(
        rig.services(&fence),
        rig.make_directory(1, root, "first"),
    ))
    .unwrap();
    let parent = first.value().unwrap().stat.as_ref().unwrap().serial;
    let unparked = rig.owner_work().sql_foreground.total().attempts;
    let capture = rig
        .client
        .try_submit(Some(route), Command::Capture)
        .unwrap();
    // Queued again after its readiness turn, which observed the ticket
    // pending: that turn's read is published before the job is requeued.
    until("capture parked behind the ticket", || {
        let work = rig.owner_work();
        work.queued == 1 && work.sql_foreground.total().attempts > unparked
    });
    let sources = rig.owner_work().completed[ServiceClass::Source as usize];
    let mutations = rig.owner_work().completed[ServiceClass::Mutation as usize];
    // Inside the new local directory nothing has to be read from the base, so
    // this mutation's first owner job is the one that publishes. It is also
    // its only one: a mutation is an owner visit that acquires no source.
    let mut second = std::pin::pin!(NativeMutation::perform(
        rig.services(&fence),
        rig.make_directory(2, parent, "second"),
    ));
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(stepper.poll(second.as_mut()).is_pending());
        let work = rig.owner_work();
        assert_eq!(work.completed[ServiceClass::Source as usize], sources);
        assert_eq!(work.completed[ServiceClass::Mutation as usize], mutations);
        if work.queued == 2 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "mutation never queued behind the capture: {work:?}"
        );
        let _ = stepper.woken.recv_timeout(Duration::from_millis(20));
    }
    assert!(capture.try_complete().unwrap().is_none());
    let admitted = rig.owner_work().admitted;

    rig.queue.stop_service().unwrap();
    assert!(fence.stopped());
    // Submitted and not yet run: the stop neither fails nor cancels it.
    assert!(stepper.poll(second.as_mut()).is_pending());
    assert_eq!(rig.owner_work().queued, 2);
    // A mutation that starts now is refused before it acquires anything.
    let late = match wait(NativeMutation::perform(
        rig.services(&fence),
        rig.make_directory(3, root, "late"),
    )) {
        Err(failure) => failure,
        Ok(_) => panic!("a mutation started on a stopped fence"),
    };
    assert!(late.fenced());
    // Refused before its first visit: nothing published, nothing held, and
    // neither the attempt nor giving it up admitted an owner job.
    assert!(late.publication().is_none());
    wait(late.relinquish()).unwrap();
    assert_eq!(rig.owner_work().admitted, admitted);

    // The owed reply attempt is a disposal call: it runs after the stop.
    // The attempt itself is recorded with no owner job; because the parked
    // capture observed this last ticket pending, it owes the owner exactly
    // one settling job. The capture then completes and the held-back job
    // runs to its own result.
    let lifecycle = rig.owner_work().completed[ServiceClass::Lifecycle as usize];
    wait(first.replied()).unwrap();
    let work = rig.owner_work();
    assert_eq!(work.admitted, admitted + 1);
    assert_eq!(
        work.completed[ServiceClass::Lifecycle as usize],
        lifecycle + 1
    );
    let captured = wait(capture).unwrap();
    assert!(matches!(captured.result(), Ok(Response::Captured(_))));
    assert!(captured.work().parked_turns >= 1);
    drop(captured);
    let second = stepper
        .finish(second.as_mut(), "submitted mutation")
        .unwrap_or_else(|failure| panic!("submitted job lost its result: {failure:?}"));
    let made = second.value().unwrap();
    assert!(made.changed);
    let serial = made.stat.as_ref().unwrap().serial;
    // Nothing waits for the second publication's ticket: no owner job.
    wait(second.replied()).unwrap();
    let work = rig.owner_work();
    assert_eq!(work.admitted, admitted + 1);
    assert_eq!(
        work.completed[ServiceClass::Lifecycle as usize],
        lifecycle + 1
    );
    assert_eq!(fence.terminal_replies(), 0);

    // Read back through a fence that was never stopped: the second directory
    // exists and the refused one does not.
    let reader = rig.services(&Fence::default());
    let found = rig.lookup(reader.clone(), 10, parent, "second");
    assert_eq!(found.value().unwrap().stat.serial, serial);
    wait(found.dispose()).unwrap();
    let absent = wait(NativeRead::prepare(
        reader,
        mount,
        11,
        root,
        None,
        NativeReadOperation::Lookup {
            parent: root,
            name: PathName::new("late").unwrap(),
        },
    ))
    .unwrap();
    assert!(absent.value().is_err());
    wait(absent.dispose()).unwrap();
    println!(
        "FENCED-SUBMITTED held_job=Mutation result_after_stop=Published(changed) late_mutation=Fenced(no owner job, no ticket)"
    );
    rig.revoke_and_stop();
}

#[test]
fn a_fenced_mutation_holds_nothing_and_fenced_source_requests_give_back_what_they_hold() {
    let rig = Rig::new("fenced-port-drivers", 144);
    let (mount, root) = (rig.mount, rig.root);
    let fence = rig.queue.fence();
    let services = rig.services(&fence);
    let stepper = Stepper::new();

    let listed = wait(NativeRead::prepare(
        services.clone(),
        mount,
        1,
        root,
        None,
        NativeReadOperation::Opendir { serial: root },
    ))
    .unwrap();
    let directory = listed.value().unwrap().directory.unwrap();
    wait(listed.dispose()).unwrap();
    // An enumeration whose reading visit is done and that has not merged its
    // window yet: it holds nothing.
    let stream = wait(DirectoryStream::prepare(
        services.clone(),
        mount,
        2,
        root,
        directory.owner_id(),
        0,
    ))
    .unwrap();
    // A mutation whose first visit was undecided and wrote nothing. It waits
    // for the only Store reader, which the test has leased, holding nothing:
    // its base facts were never demanded.
    let reader = wait(rig.store.read_ticket(Some(rig.identity)).unwrap()).unwrap();
    let mut making = std::pin::pin!(NativeMutation::perform(
        services.clone(),
        rig.make_directory(3, root, "never-made"),
    ));
    let deadline = Instant::now() + WAIT;
    while rig.store.read_work().waiting != 1 {
        assert!(
            stepper.poll(making.as_mut()).is_pending(),
            "the mutation decided without a base fact"
        );
        assert!(
            Instant::now() < deadline,
            "mutation never waited for a reader"
        );
        let _ = stepper.woken.recv_timeout(Duration::from_millis(20));
    }
    // A second OPENDIR waits for the same reader while its base fact is not
    // yet read. It is a visit and holds nothing.
    let mut opening = std::pin::pin!(NativeRead::prepare(
        services.clone(),
        mount,
        5,
        root,
        None,
        NativeReadOperation::Opendir { serial: root },
    ));
    let deadline = Instant::now() + WAIT;
    while rig.store.read_work().waiting != 2 {
        assert!(
            stepper.poll(opening.as_mut()).is_pending(),
            "the directory was opened without a base fact"
        );
        assert!(
            Instant::now() < deadline,
            "OPENDIR never waited for a reader"
        );
        let _ = stepper.woken.recv_timeout(Duration::from_millis(20));
    }
    let admitted = rig.owner_work().admitted;

    rig.queue.stop_service().unwrap();

    let failure = match stepper.poll(making.as_mut()) {
        Poll::Ready(Err(failure)) => failure,
        Poll::Ready(Ok(_)) => panic!("a mutation was decided on a stopped fence"),
        Poll::Pending => panic!("the reader wait survived the stop"),
    };
    assert!(failure.fenced());
    // A mutation records no request source and this one published nothing:
    // it holds nothing, and giving it up admits no owner job.
    assert!(failure.publication().is_none());
    wait(failure.relinquish()).unwrap();
    assert_eq!(rig.owner_work().admitted, admitted);

    let failure = match stepper.poll(opening.as_mut()) {
        Poll::Ready(Err(failure)) => failure,
        Poll::Ready(Ok(_)) => panic!("a directory was opened on a stopped fence"),
        Poll::Pending => panic!("the OPENDIR reader wait survived the stop"),
    };
    assert!(failure.fenced());
    wait(failure.relinquish()).unwrap();
    assert_eq!(rig.owner_work().admitted, admitted);
    assert_eq!(rig.store.read_work().waiting, 0);

    let failure = match wait(stream.next()) {
        Err(failure) => failure,
        Ok(_) => panic!("a directory page was read on a stopped fence"),
    };
    assert!(failure.fenced() && failure.offered().is_none());
    wait(failure.relinquish()).unwrap();
    // An enumeration that starts now is refused holding nothing too.
    let failure = match wait(DirectoryStream::prepare(
        services.clone(),
        mount,
        4,
        root,
        directory.owner_id(),
        0,
    )) {
        Err(failure) => failure,
        Ok(_) => panic!("an enumeration started on a stopped fence"),
    };
    assert!(failure.fenced() && failure.offered().is_none());
    wait(failure.relinquish()).unwrap();
    // Nothing was held, so nothing was released: no owner job was admitted.
    assert_eq!(rig.owner_work().admitted, admitted);
    drop(reader);
    drop(wait(services.close_directory(mount, directory.serial(), directory.owner_id())).unwrap());

    let absent = wait(NativeRead::prepare(
        rig.services(&Fence::default()),
        mount,
        6,
        root,
        None,
        NativeReadOperation::Lookup {
            parent: root,
            name: PathName::new("never-made").unwrap(),
        },
    ))
    .unwrap();
    assert!(absent.value().is_err());
    wait(absent.dispose()).unwrap();
    println!(
        "FENCED-DRIVERS mutation=(fenced,no_source,no_ticket)->nothing held opendir=(fenced,no_source,no_read)->nothing held directory=(fenced,nothing) late_directory=(fenced,nothing) owner_jobs_after_stop=0"
    );
    drop(services);
    rig.revoke_and_stop();
}
