//! LOOKUP, GETATTR and mutations through the daemon's Fuse port, counted as
//! owner jobs and Store reader grants: the real owner and the real Store with
//! its shared canonical cache, without a kernel mount. These requests are
//! served by owner visits that record no request source, and a published
//! mutation's reply attempt is recorded in the engine's memory from the
//! replying thread: it is an owner job only when a capture waited for it.
//! Public API only; every wait is bounded and no thread is spawned by the
//! test.
#[path = "support/installed_store.rs"]
mod support;
use layerfs_content::filesystem::PathName;
use layerfs_daemon::{
    bootstrap::open_store,
    store::{BindRequest, BoundWorkspace, Store},
    Command, Completion, NativeJob, NativeReply, Owner, OwnerClient, OwnerConfig, OwnerError,
    Response, ServiceClass,
};
use layerfs_fuse::{
    operations::{
        create::{create, mkdir},
        MutationInput, MutationRequest, NativeMutation, NativeRead,
    },
    ports::{Fence, MountServices, RequestServices},
};
use layerfs_history::WorkspaceId;
use layerfs_overlay::{NativeMount, OverlayError, ProfileConfig};
use layerfs_workspace::{NativeReadOperation, Refusal, Time, WorkspaceError};
use std::{
    future::Future,
    pin::Pin,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(5);
const CACHE: usize = 4 * 1024 * 1024;
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
    /// Polls a request that must stay pending until `reached` holds.
    fn park<F: Future + ?Sized>(
        &self,
        mut future: Pin<&mut F>,
        what: &str,
        reached: impl Fn() -> bool,
    ) {
        let deadline = Instant::now() + WAIT;
        while !reached() {
            assert!(self.poll(future.as_mut()).is_pending(), "{what}: decided");
            assert!(Instant::now() < deadline, "{what}: never parked");
            let _ = self.woken.recv_timeout(Duration::from_millis(20));
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
fn finish(client: &OwnerClient, route: layerfs_overlay::Route, command: Command) -> Completion {
    let pending = match client.try_submit(Some(route), command) {
        Ok(pending) => pending,
        Err((error, command)) => panic!("admission of {command:?}: {error:?}"),
    };
    wait(pending).unwrap()
}

/// Owner jobs completed per class, jobs admitted, and Store reader grants.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Counted {
    completed: [u64; 6],
    admitted: u64,
    grants: u64,
}
impl Counted {
    /// The counts after `jobs` more completed jobs of each named class and
    /// `grants` more reader grants, and nothing else.
    fn and(mut self, jobs: &[(ServiceClass, u64)], grants: u64) -> Self {
        for (class, count) in jobs {
            self.completed[*class as usize] += count;
            self.admitted += count;
        }
        self.grants += grants;
        self
    }
}

/// One bound Workspace over a Store with one reader and a canonical cache,
/// and a native mount group. No kernel connection and no dispatcher lane.
struct Rig {
    fixture: support::Fixture,
    store: Arc<Store>,
    owner: Owner,
    client: OwnerClient,
    identity: WorkspaceId,
    bound: BoundWorkspace,
    root: u64,
    mount: NativeMount,
    services: Arc<dyn RequestServices>,
}
impl Rig {
    fn new(label: &str, authority: u8) -> Self {
        Self::with_cache(label, authority, CACHE)
    }
    fn with_cache(label: &str, authority: u8, cache: usize) -> Self {
        let fixture = support::Fixture::new(1, label);
        assert_eq!(fixture.count, 1);
        let store = open_store(
            fixture.config.clone(),
            support::BINDING,
            support::CURSOR,
            1,
            cache,
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
        let services = bound.request(&Fence::default()).unwrap();
        Self {
            fixture,
            store,
            owner,
            client,
            identity,
            bound,
            root,
            mount,
            services,
        }
    }
    /// Taken once every result of the request under observation is returned.
    fn counted(&self) -> Counted {
        until("owner results returned", || {
            self.client.diagnostics().unwrap().outstanding == 0
        });
        let work = self.client.diagnostics().unwrap();
        let readers = self.store.read_work();
        assert_eq!((readers.outstanding, readers.waiting), (0, 0));
        Counted {
            completed: work.completed,
            admitted: work.admitted,
            grants: readers.grants,
        }
    }
    fn lookup(&self, request: u64, child: &str) -> NativeRead {
        wait(NativeRead::prepare(
            self.services.clone(),
            self.mount,
            request,
            self.root,
            None,
            NativeReadOperation::Lookup {
                parent: self.root,
                name: PathName::new(child).unwrap(),
            },
        ))
        .unwrap()
    }
    /// What a request may have recorded in the owner: the Workspace's
    /// base-reader count and its base-source rows. These observations are
    /// owner jobs themselves.
    fn recorded(&self) -> (u64, u64) {
        let route = self.bound.route();
        let done = finish(&self.client, route, Command::State);
        let readers = match done.result() {
            Ok(Response::State(state)) => state.base_readers,
            other => panic!("state: {other:?}"),
        };
        drop(done);
        let done = finish(&self.client, route, Command::Resources { global: false });
        let sources = match done.result() {
            Ok(Response::Resources(resources)) => resources.counts.source_rows,
            other => panic!("resources: {other:?}"),
        };
        drop(done);
        (readers, sources)
    }
    fn revoke(&self) {
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
    }
    /// Reply tickets still owed in the Workspace: one Read-class owner job.
    fn pending_publications(&self) -> Vec<layerfs_overlay::Publication> {
        let done = finish(
            &self.client,
            self.bound.route(),
            Command::PendingPublications { after: 0 },
        );
        match done.result() {
            Ok(Response::Publications(pending)) => pending.clone(),
            other => panic!("pending publications: {other:?}"),
        }
    }
    fn make_directory(&self, request: u64, name: &str) -> MutationRequest {
        MutationRequest {
            mount: self.mount,
            request,
            protected: self.root,
            handle: None,
            input: MutationInput::Named(mkdir(self.root, PathName::new(name).unwrap(), 0o755)),
            open: None,
            now: NOW,
        }
    }
    fn revision(&self) -> i64 {
        let done = finish(&self.client, self.bound.route(), Command::State);
        match done.result() {
            Ok(Response::State(state)) => state.revision,
            other => panic!("state: {other:?}"),
        }
    }
    fn revoke_and_stop(self) {
        self.revoke();
        self.stop();
    }
    fn stop(self) {
        let Self {
            fixture,
            store,
            owner,
            client,
            bound,
            services,
            ..
        } = self;
        until("owner credits returned", || {
            client.diagnostics().unwrap().outstanding == 0
        });
        assert_eq!(store.read_work().outstanding, 0);
        drop(services);
        drop(bound);
        owner.stop().unwrap();
        drop(store);
        fixture.cleanup();
    }
}

#[test]
fn a_lookup_over_resident_objects_is_one_read_job_and_no_reader_grant() {
    let rig = Rig::new("visit-port-lookup", 161);
    let (mount, root) = (rig.mount, rig.root);
    assert_eq!(rig.recorded(), (0, 0));

    // Whatever binding the Workspace left unread of the root directory is
    // read by this first miss, through the Store's shared canonical cache.
    let warm = rig.lookup(1, "absent-warm");
    assert!(matches!(warm.value(), Err(Refusal::Missing)));
    wait(warm.dispose()).unwrap();
    assert_eq!(rig.recorded(), (0, 0));

    // The directory's objects are resident: a miss of another name in it is
    // one owner job of class Read, with no reader grant. The request recorded
    // no source, so its disposal is no owner job either.
    let before = rig.counted();
    let missing = rig.lookup(2, "absent");
    assert!(matches!(missing.value(), Err(Refusal::Missing)));
    let visited = rig.counted();
    assert_eq!(visited, before.and(&[(ServiceClass::Read, 1)], 0));
    wait(missing.dispose()).unwrap();
    assert_eq!(rig.counted(), visited);
    assert_eq!(rig.recorded(), (0, 0));

    // GETATTR of that directory is the same single job.
    let before = rig.counted();
    let attributes = wait(NativeRead::prepare(
        rig.services.clone(),
        mount,
        3,
        root,
        None,
        NativeReadOperation::Getattr { serial: root },
    ))
    .unwrap();
    assert_eq!(attributes.value().unwrap().stat.serial, root);
    wait(attributes.dispose()).unwrap();
    assert_eq!(rig.counted(), before.and(&[(ServiceClass::Read, 1)], 0));
    assert_eq!(rig.recorded(), (0, 0));

    // A regular file's length is the Store's answer, not a canonical object:
    // the lookup of a base file is undecided once and takes one reader, and
    // still records no source.
    let before = rig.counted();
    let found = rig.lookup(4, "file-000000");
    let stat = &found.value().unwrap().stat;
    let serial = stat.serial;
    assert_eq!(stat.logical_len, support::bytes(0).len() as u64);
    wait(found.dispose()).unwrap();
    assert_eq!(rig.counted(), before.and(&[(ServiceClass::Read, 2)], 1));
    assert_eq!(rig.recorded(), (0, 0));

    println!(
        "VISIT-LOOKUP resident_miss=(1 Read job, 0 grants) resident_getattr=(1 Read job, 0 grants) base_file=(2 Read jobs, 1 grant) sources=0 base_readers=0 release_jobs=0"
    );
    drop(wait(rig.services.forget(mount, serial, 1)).unwrap());
    rig.revoke_and_stop();
}

#[test]
fn a_create_is_one_mutation_job_and_its_reply_attempt_is_no_owner_job() {
    let rig = Rig::new("visit-port-create", 162);
    let (mount, root) = (rig.mount, rig.root);
    // Make the parent directory's objects resident.
    let warm = rig.lookup(1, "absent-warm");
    assert!(matches!(warm.value(), Err(Refusal::Missing)));
    wait(warm.dispose()).unwrap();
    assert!(rig.pending_publications().is_empty());

    // CREATE with an open descriptor: decided and published by one owner job
    // of class Mutation, with no source acquisition and no reader grant.
    let before = rig.counted();
    let created = wait(NativeMutation::perform(
        rig.services.clone(),
        MutationRequest {
            mount,
            request: 2,
            protected: root,
            handle: None,
            input: MutationInput::Named(create(root, PathName::new("created").unwrap(), 0o644)),
            open: Some(true),
            now: NOW,
        },
    ))
    .unwrap_or_else(|failure| panic!("create: {failure:?}"));
    let published = created.value().unwrap();
    assert!(published.changed);
    let serial = published.stat.as_ref().unwrap().serial;
    let file = published.file.expect("the created file is open");
    assert_eq!((file.serial(), file.writable()), (serial, true));
    // The mutation's original result is still held for its reply.
    let work = rig.client.diagnostics().unwrap();
    assert_eq!(work.outstanding, 1);
    let mut expected = before.and(&[(ServiceClass::Mutation, 1)], 0);
    assert_eq!(
        (work.completed, work.admitted, rig.store.read_work().grants),
        (expected.completed, expected.admitted, expected.grants)
    );
    // Its one reply ticket is owed. This observation is the test's own
    // Read job.
    let owed = rig.pending_publications();
    assert_eq!(owed.len(), 1);
    assert_eq!(owed[0].revision(), rig.revision());
    expected = expected.and(&[(ServiceClass::Read, 1), (ServiceClass::Lifecycle, 1)], 0);
    until("the observations' results returned", || {
        rig.client.diagnostics().unwrap().outstanding == 1
    });
    let work = rig.client.diagnostics().unwrap();
    assert_eq!(
        (work.completed, work.admitted),
        (expected.completed, expected.admitted)
    );

    // The one reply attempt returns the ticket from this thread. Nothing
    // waits for this Workspace's tickets, so it is no owner job: nothing is
    // admitted and nothing completes, of class Lifecycle or any other.
    wait(created.replied()).unwrap();
    assert_eq!(rig.counted(), expected);
    // The ticket was returned, and exactly once.
    assert!(rig.pending_publications().is_empty());
    assert!(matches!(
        rig.client.reply_attempted(owed[0]),
        Err(OwnerError::Overlay(OverlayError::Stale))
    ));
    assert_eq!(rig.recorded(), (0, 0));

    // The published file is found by one visit over its local row.
    let before = rig.counted();
    let found = rig.lookup(3, "created");
    assert_eq!(found.value().unwrap().stat.serial, serial);
    wait(found.dispose()).unwrap();
    assert_eq!(rig.counted(), before.and(&[(ServiceClass::Read, 1)], 0));
    assert_eq!(rig.recorded(), (0, 0));

    println!(
        "VISIT-CREATE jobs=(1 Mutation, 0 for its reply attempt) sources=0 reader_grants=0 base_readers=0"
    );
    drop(wait(rig.services.close_file(mount, serial, file.owner_id())).unwrap());
    // The create's reply and the lookup each handed the kernel one reference.
    drop(wait(rig.services.forget(mount, serial, 2)).unwrap());
    rig.revoke_and_stop();
}

#[test]
fn a_capture_parked_behind_the_ticket_is_owed_exactly_one_settling_job() {
    let rig = Rig::new("visit-port-settled", 164);
    let (mount, route) = (rig.mount, rig.bound.route());
    let warm = rig.lookup(1, "absent-warm");
    assert!(matches!(warm.value(), Err(Refusal::Missing)));
    wait(warm.dispose()).unwrap();

    // Two published mutations whose reply attempts are owed.
    let first = wait(NativeMutation::perform(
        rig.services.clone(),
        rig.make_directory(2, "first"),
    ))
    .unwrap_or_else(|failure| panic!("first: {failure:?}"));
    let second = wait(NativeMutation::perform(
        rig.services.clone(),
        rig.make_directory(3, "second"),
    ))
    .unwrap_or_else(|failure| panic!("second: {failure:?}"));
    let made: Vec<u64> = [&first, &second]
        .map(|done| done.value().unwrap().stat.as_ref().unwrap().serial)
        .to_vec();
    let owed = rig.pending_publications();
    assert_eq!(owed.len(), 2);
    let revision = rig.revision();
    assert_eq!(owed[1].revision(), revision);
    until("the observations' results returned", || {
        rig.client.diagnostics().unwrap().outstanding == 2
    });

    // A Capture is admitted and parks: its readiness turn observes the
    // tickets pending, which is published before the job is requeued.
    let unparked = rig.client.diagnostics().unwrap();
    let mut capture = Box::pin(
        rig.client
            .try_submit(Some(route), Command::Capture)
            .unwrap(),
    );
    until("capture parked behind the tickets", || {
        let work = rig.client.diagnostics().unwrap();
        work.queued == 1
            && work.sql_foreground.total().attempts > unparked.sql_foreground.total().attempts
    });
    let stepper = Stepper::new();
    assert!(stepper.poll(capture.as_mut()).is_pending());
    let parked = rig.client.diagnostics().unwrap();
    assert_eq!(parked.admitted, unparked.admitted + 1);
    assert_eq!(parked.completed, unparked.completed);
    // Both mutation results and the capture's admission are still held.
    assert_eq!((parked.queued, parked.outstanding), (1, 3));

    // The first reply attempt leaves a ticket pending: no owner job, and
    // the capture stays parked.
    wait(first.replied()).unwrap();
    let work = rig.client.diagnostics().unwrap();
    assert_eq!(
        (work.admitted, work.completed, work.queued),
        (parked.admitted, parked.completed, 1)
    );
    assert!(stepper.poll(capture.as_mut()).is_pending());

    // The last one is what the capture waited for: exactly one settling
    // job of class Lifecycle, after which the capture runs and completes.
    wait(second.replied()).unwrap();
    let captured = stepper.finish(capture.as_mut(), "parked capture").unwrap();
    let sealed = match captured.result() {
        Ok(Response::Captured(capture)) => *capture,
        other => panic!("capture: {other:?}"),
    };
    assert!(captured.work().parked_turns >= 1);
    // The result's credit is held by its completion and by its handle.
    drop((captured, capture));
    until("owner results returned", || {
        rig.client.diagnostics().unwrap().outstanding == 0
    });
    let work = rig.client.diagnostics().unwrap();
    let mut completed = parked.completed;
    completed[ServiceClass::Lifecycle as usize] += 1;
    completed[ServiceClass::Capture as usize] += 1;
    assert_eq!(
        (work.admitted, work.completed, work.queued),
        (parked.admitted + 1, completed, 0)
    );
    // It sealed both publications, and no ticket is owed.
    assert_eq!(sealed.revision, revision);
    assert!(rig.pending_publications().is_empty());

    // With the capture retained and nothing waiting, a third mutation's
    // reply attempt is again no owner job.
    let third = wait(NativeMutation::perform(
        rig.services.clone(),
        rig.make_directory(4, "third"),
    ))
    .unwrap_or_else(|failure| panic!("third: {failure:?}"));
    let last = third.value().unwrap().stat.as_ref().unwrap().serial;
    let before = rig.client.diagnostics().unwrap();
    wait(third.replied()).unwrap();
    let work = rig.client.diagnostics().unwrap();
    assert_eq!(
        (work.admitted, work.completed),
        (before.admitted, before.completed)
    );
    assert!(rig.pending_publications().is_empty());

    println!(
        "VISIT-SETTLED parked_capture=(2 tickets owed) first_attempt=(0 jobs, capture parked) last_attempt=(1 Lifecycle ReplySettled, capture completes) later_attempt=(0 jobs)"
    );
    for serial in made.into_iter().chain([last]) {
        drop(wait(rig.services.forget(mount, serial, 1)).unwrap());
    }
    rig.revoke_and_stop();
}

#[test]
fn a_request_between_two_visits_fences_no_revocation_and_its_next_visit_is_refused() {
    // No canonical cache: every base fact is read through the one Store
    // reader, which the test leases so that each request stops after its
    // first, undecided visit.
    let rig = Rig::with_cache("visit-port-revoked", 163, 0);
    let (mount, root) = (rig.mount, rig.root);
    let stepper = Stepper::new();
    let reader = wait(rig.store.read_ticket(Some(rig.identity)).unwrap()).unwrap();
    let mut looking = std::pin::pin!(NativeRead::prepare(
        rig.services.clone(),
        mount,
        1,
        root,
        None,
        NativeReadOperation::Lookup {
            parent: root,
            name: PathName::new("file-000000").unwrap(),
        },
    ));
    stepper.park(looking.as_mut(), "lookup", || {
        rig.store.read_work().waiting == 1
    });
    let mut making = std::pin::pin!(NativeMutation::perform(
        rig.services.clone(),
        MutationRequest {
            mount,
            request: 2,
            protected: root,
            handle: None,
            input: MutationInput::Named(mkdir(root, PathName::new("never-made").unwrap(), 0o755)),
            open: None,
            now: NOW,
        },
    ));
    stepper.park(making.as_mut(), "mutation", || {
        rig.store.read_work().waiting == 2
    });
    until("the undecided visits returned their results", || {
        rig.client.diagnostics().unwrap().outstanding == 0
    });
    // Each made one visit and wrote nothing: no source, no reader count.
    assert_eq!(rig.recorded(), (0, 0));
    assert_eq!(rig.recorded(), (0, 0));
    let revision = rig.revision();

    // Revocation waits for no request: it is not refused while these two
    // requests wait between their visits.
    rig.revoke();

    // The reader returns; each request reads its facts and visits again. The
    // visit re-reads the mount and is refused, having decided nothing.
    drop(reader);
    let failure = match stepper.finish(looking.as_mut(), "lookup") {
        Err(failure) => failure,
        Ok(_) => panic!("a lookup was answered on a revoked mount"),
    };
    assert!(!failure.fenced() && failure.base_demand().is_none());
    let observation = failure.observation().expect("the refused visit's result");
    assert!(matches!(observation.result, Err(OverlayError::Stale)));
    assert!(observation.decision.is_none());
    // It is an owner failure, not a fence or a base demand: retained as it is.
    let failure = wait(failure.relinquish()).unwrap_err();
    assert_eq!(failure.request(), 1);
    drop(failure);

    let failure = match stepper.finish(making.as_mut(), "mutation") {
        Err(failure) => failure,
        Ok(_) => panic!("a mutation was decided on a revoked mount"),
    };
    assert!(!failure.fenced() && failure.base_demand().is_none());
    assert!(failure.publication().is_none());
    let outcome = failure.outcome().expect("the refused visit's result");
    assert!(matches!(
        outcome.result,
        Err(WorkspaceError::Overlay(OverlayError::Stale))
    ));
    assert_eq!(outcome.file, None);
    let failure = wait(failure.relinquish()).unwrap_err();
    assert_eq!(failure.request().request, 2);
    drop(failure);

    // Nothing was published, and both requests used the reader exactly once.
    assert_eq!(rig.revision(), revision);
    let readers = rig.store.read_work();
    assert_eq!((readers.waiting, readers.outstanding), (0, 0));
    println!(
        "VISIT-REVOKED between_visits=(lookup, mkdir) base_readers=0 sources=0 revoke=Done next_visit=Stale published=0"
    );
    rig.stop();
}
