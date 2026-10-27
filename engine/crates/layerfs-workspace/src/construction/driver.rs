//! One consuming captured namespace attempt; original custody never releases on Drop.
use super::namespace::{
    begin, names, seal, values, Backing, Builder, Cursor, Facts, Shared, State,
};
use super::outcome::{
    nanos, CapturedNamespaceAttempt, CapturedNamespaceCustody, CapturedNamespaceWork,
};
use crate::Route;
use crate::{
    BaseView, FileDigests, IndexedConstructionRecords, OverlayCapturedNamespace,
    OverlayOperationRecords, Workspace, WorkspaceResult,
};
use layerfs_content::{
    filesystem::{
        update_filesystem_streamed_backed, FilesystemObjects, FilesystemResources,
        FilesystemRootId, StreamedFilesystemInput,
    },
    AuthenticatedObjects, ConstructionCapacities, ConstructionPolicy, ContentError, ContentResult,
    FilesystemResult, FinalizedConsumer, FinalizedObject, ObjectId,
};
use layerfs_overlay::{CapturedReader, IndexedOperationRecordScope, OperationOwner, OverlayError};
use layerfs_telemetry::timer::{Active, Timing, TimingScope};
use std::time::Instant;

/// Adds one phase's wall time to its observation counter.
fn elapsed<P: OverlayOperationRecords + ?Sized>(
    shared: &Shared<'_, P>,
    started: Instant,
    counter: impl FnOnce(&mut CapturedNamespaceWork) -> &mut u64,
) {
    let ns = nanos(started);
    let mut owner = shared.borrow_mut();
    let counter = counter(&mut owner.work);
    *counter = counter.saturating_add(ns);
}
/// The caller's consumer, with the time it takes to accept each object.
struct Timed<'c> {
    consumer: &'c mut dyn FinalizedConsumer,
    ns: u64,
}
impl FinalizedConsumer for Timed<'_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        let started = Instant::now();
        let accepted = self.consumer.accept(object);
        self.ns = self.ns.saturating_add(nanos(started));
        accepted
    }
}

/// One complete canonical filesystem root from one exact captured reader.
/// Every read goes through that reader, this attempt's sealed records or the
/// immutable base bound to the reader's root; current active state is never
/// consulted. Consuming construct prevents recovery of a failed attempt.
pub struct CapturedNamespace<'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> {
    provider: &'a P,
    shared: Shared<'a, P>,
    route: Route,
    base: WorkspaceResult<BaseView>,
    reader: CapturedReader,
    operation: OperationOwner,
    digests: Option<&'a dyn FileDigests>,
}
/// The borrowed inputs of the single running attempt.
struct Attempt<'s, 'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> {
    provider: &'a P,
    shared: &'s Shared<'a, P>,
    route: Route,
    reader: CapturedReader,
    operation: OperationOwner,
    policy: ConstructionPolicy,
    capacities: &'s ConstructionCapacities,
    objects: &'s dyn AuthenticatedObjects,
    digests: Option<&'a dyn FileDigests>,
}
impl<'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> CapturedNamespace<'a, P> {
    /// Binds exact inputs. Performs no I/O: the Workspace's selected binding
    /// and route are retained as they are, and checked by the one attempt.
    /// The namespace's records use the operation's file scope 0; each changed
    /// file uses its own serial as file scope under the same owner.
    pub fn new(
        workspace: &Workspace,
        provider: &'a P,
        reader: CapturedReader,
        operation: OperationOwner,
    ) -> Self {
        Self {
            provider,
            shared: Shared::new(State {
                records: IndexedConstructionRecords::new(
                    provider,
                    IndexedOperationRecordScope {
                        owner: operation,
                        file_scope: 0,
                    },
                ),
                failure: None,
                file: None,
                work: CapturedNamespaceWork::default(),
                directories: Vec::new(),
                record_ns: 0,
            }),
            route: workspace.route(),
            base: workspace.base(),
            reader,
            operation,
            digests: None,
        }
    }
    /// Records each fresh file's whole-content digest, when the overlay
    /// kept one, in the Store through `digests` (#364).
    pub fn with_digests(mut self, digests: &'a dyn FileDigests) -> Self {
        self.digests = Some(digests);
        self
    }
    /// One attempt: normalize, construct changed files and metadata, then the
    /// canonical update, with the caller's actual Store-derived file policy and
    /// consumer. `objects` serves base reads and same-Save reads. It neither
    /// saves nor publishes history and releases nothing.
    pub fn construct(
        self,
        policy: ConstructionPolicy,
        capacities: &ConstructionCapacities,
        objects: &dyn AuthenticatedObjects,
        consumer: &mut dyn FinalizedConsumer,
        scope: TimingScope<'_>,
    ) -> CapturedNamespaceAttempt {
        let Self {
            provider,
            shared,
            route,
            base,
            reader,
            operation,
            digests,
        } = self;
        let attempt = Attempt {
            provider,
            shared: &shared,
            route,
            reader,
            operation,
            policy,
            capacities,
            objects,
            digests,
        };
        let result = scope.run(|timing| attempt.run(base, consumer, timing));
        if let Err(original) = &result {
            shared.borrow_mut().content(original.clone());
        }
        let state = shared.into_inner();
        CapturedNamespaceAttempt {
            result,
            custody: CapturedNamespaceCustody {
                reader,
                operation,
                records: state.records.into_custody(),
                file: state.file,
                failure: state.failure,
                work: state.work,
            },
        }
    }
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> CapturedNamespace<'_, P> {
    /// The same one attempt with no clocks, nodes or timing output, for a
    /// caller that records none.
    pub fn construct_untimed(
        self,
        policy: ConstructionPolicy,
        capacities: &ConstructionCapacities,
        objects: &dyn AuthenticatedObjects,
        consumer: &mut dyn FinalizedConsumer,
    ) -> CapturedNamespaceAttempt {
        let (attempt, _) = Timing::disabled("captured.namespace", |scope| {
            Ok::<_, std::convert::Infallible>(self.construct(
                policy,
                capacities,
                objects,
                consumer,
                scope.child("construct"),
            ))
        });
        match attempt {
            Ok(attempt) => attempt,
            Err(never) => match never {},
        }
    }
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> Attempt<'_, '_, P> {
    /// The reader's exact root, never the Workspace's possibly later binding.
    fn bind(&self, base: WorkspaceResult<BaseView>) -> ContentResult<BaseView> {
        if self.route.engine() != self.reader.capture().engine()
            || self.operation.engine() != self.route.engine()
        {
            return Err(self
                .shared
                .borrow_mut()
                .original(OverlayError::Stale.into()));
        }
        let base = base.map_err(|original| self.shared.borrow_mut().original(original))?;
        if base.identity().0.to_bytes() == self.reader.root() {
            return Ok(base);
        }
        base.rebind(FilesystemRootId(ObjectId::from_bytes(&self.reader.root())?))
    }
    fn run(
        &self,
        base: WorkspaceResult<BaseView>,
        consumer: &mut dyn FinalizedConsumer,
        timing: &TimingScope<'_, Active>,
    ) -> ContentResult<FilesystemResult> {
        let shared = self.shared;
        self.policy.validated()?;
        if *self.capacities != self.policy.capacities() {
            return Err(ContentError::InvalidRecord(
                "captured namespace policy capacities",
            ));
        }
        // Bind: exact base, scope and root serial, then the unsealed context.
        let base = self.bind(base)?;
        let facts = Facts {
            reader: self.reader,
            operation: self.operation,
            scope: base.root().scope(),
            profile: base.root().profile(),
            root: base.identity(),
            root_serial: base.root().root_inode().serial(),
        };
        let initial = begin(shared, facts)?;
        // Normalize names: one header per changed parent that survives.
        let started = Instant::now();
        let headers = names(self.provider, shared, facts)?;
        elapsed(shared, started, |work| &mut work.normalize_ns);
        // Normalize inodes: typed values and dense fresh ranks, one file at a
        // time through the existing changed-file constructor.
        let workspace = Workspace::bind(self.route, base.clone());
        let builder = Builder {
            provider: self.provider,
            shared,
            facts,
            base: &base,
            workspace: &workspace,
            policy: self.policy,
            capacities: self.capacities,
            objects: self.objects,
            digests: self.digests,
        };
        let started = Instant::now();
        let mut timed = Timed { consumer, ns: 0 };
        let recorded = shared.borrow().record_ns;
        let totals = values(&builder, &mut timed, timing, headers);
        elapsed(shared, started, |work| &mut work.values_ns);
        {
            let mut owner = shared.borrow_mut();
            owner.work.values_emit_ns = timed.ns;
            owner.work.values_record_ns = owner.record_ns.saturating_sub(recorded);
        }
        let totals = totals?;
        // Seal: the context now binds the reader identity and declared totals.
        let sealed = seal(shared, facts, totals, initial)?;
        // Update: Content's canonical algorithm over the sealed rows. No Store
        // policy supplies these ceilings, so they are Content's defaults.
        let rows = Cursor::new(self.provider, shared, self.reader, &sealed, totals)?;
        let input = StreamedFilesystemInput {
            base: Some(facts.root),
            scope: facts.scope,
            root_serial: facts.root_serial,
            resources: FilesystemResources::default(),
            rows: &rows,
        };
        let mut timed = Timed { consumer, ns: 0 };
        let mut output =
            FilesystemObjects::new_with_accepted(self.objects, &mut timed, self.objects);
        let recorded = shared.borrow().record_ns;
        let started = Instant::now();
        let result = timing.child("update").run(|_| {
            update_filesystem_streamed_backed(&mut output, &input, &mut Backing(shared), None)
        });
        elapsed(shared, started, |work| &mut work.update_ns);
        let mut owner = shared.borrow_mut();
        owner.work.update_emit_ns = timed.ns;
        owner.work.update_record_ns = owner.record_ns.saturating_sub(recorded);
        let directories = std::mem::take(&mut owner.directories);
        drop(owner);
        // Each changed directory's new root gets the count its row holds, so
        // the next change there need not list it again (#367). A cache fill:
        // it never changes this attempt's result.
        if let Ok(built) = &result {
            let _ = base.remember_constructed_counts(self.objects, built.root, &directories);
        }
        result
    }
}
