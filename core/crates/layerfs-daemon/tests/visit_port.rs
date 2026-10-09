//! LOOKUP, GETATTR and mutations through the daemon's Fuse port, counted as
//! owner jobs and Store reader grants: the real owner and the real Store with
//! its shared canonical cache, without a kernel mount. These requests are
//! served by owner visits that record no request source. Public API only;
//! every wait is bounded and no thread is spawned by the test.
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
    /// base-reader count, its base-source rows and this request's own source.
    /// These observations are owner jobs themselves.
    fn recorded(&self, request: u64) -> (u64, u64, bool) {
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
        let done = finish(
            &self.client,
            route,
            Command::Native(NativeJob::RetainedSource {
                mount: self.mount,
                request,
            }),
        );
        let source = match done.result() {
            Ok(Response::Native(NativeReply::RetainedSource(source))) => source.is_some(),
            other => panic!("retained source: {other:?}"),
        };
        drop(done);
        (readers, sources, source)
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
    assert_eq!(rig.recorded(1), (0, 0, false));

    // Whatever binding the Workspace left unread of the root directory is
    // read by this first miss, through the Store's shared canonical cache.
    let warm = rig.lookup(1, "absent-warm");
    assert!(matches!(warm.value(), Err(Refusal::Missing)));
    wait(warm.dispose()).unwrap();
    assert_eq!(rig.recorded(1), (0, 0, false));

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
    assert_eq!(rig.recorded(2), (0, 0, false));

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
    assert_eq!(rig.recorded(3), (0, 0, false));

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
    assert_eq!(rig.recorded(4), (0, 0, false));

    println!(
        "VISIT-LOOKUP resident_miss=(1 Read job, 0 grants) resident_getattr=(1 Read job, 0 grants) base_file=(2 Read jobs, 1 grant) sources=0 base_readers=0 release_jobs=0"
    );
    drop(wait(rig.services.forget(mount, serial, 1)).unwrap());
    rig.revoke_and_stop();
}

#[test]
fn a_create_is_one_mutation_job_and_one_reply_attempt_job() {
    let rig = Rig::new("visit-port-create", 162);
    let (mount, root) = (rig.mount, rig.root);
    // Make the parent directory's objects resident.
    let warm = rig.lookup(1, "absent-warm");
    assert!(matches!(warm.value(), Err(Refusal::Missing)));
    wait(warm.dispose()).unwrap();

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

    // After the one reply attempt the only release is the ticket's: one
    // Lifecycle job.
    wait(created.replied()).unwrap();
    expected = expected.and(&[(ServiceClass::Lifecycle, 1)], 0);
    assert_eq!(rig.counted(), expected);
    assert_eq!(rig.recorded(2), (0, 0, false));

    // The published file is found by one visit over its local row.
    let before = rig.counted();
    let found = rig.lookup(3, "created");
    assert_eq!(found.value().unwrap().stat.serial, serial);
    wait(found.dispose()).unwrap();
    assert_eq!(rig.counted(), before.and(&[(ServiceClass::Read, 1)], 0));
    assert_eq!(rig.recorded(3), (0, 0, false));

    println!(
        "VISIT-CREATE jobs=(1 Mutation, 1 Lifecycle reply attempt) sources=0 reader_grants=0 base_readers=0"
    );
    drop(wait(rig.services.close_file(mount, serial, file.owner_id())).unwrap());
    // The create's reply and the lookup each handed the kernel one reference.
    drop(wait(rig.services.forget(mount, serial, 2)).unwrap());
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
    assert_eq!(rig.recorded(1), (0, 0, false));
    assert_eq!(rig.recorded(2), (0, 0, false));
    let revision = rig.revision();

    // Revocation is fenced by recorded request sources only: it is not
    // refused while these two requests wait between their visits.
    rig.revoke();

    // The reader returns; each request reads its facts and visits again. The
    // visit re-reads the mount and is refused, having decided nothing.
    drop(reader);
    let failure = match stepper.finish(looking.as_mut(), "lookup") {
        Err(failure) => failure,
        Ok(_) => panic!("a lookup was answered on a revoked mount"),
    };
    assert!(!failure.fenced() && failure.base_demand().is_none());
    assert!(failure.retained_source().is_none() && failure.retained_read().is_none());
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
