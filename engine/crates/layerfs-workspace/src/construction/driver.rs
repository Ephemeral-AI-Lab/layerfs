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
        statistics::LogicalChanges, update::update_filesystem_streamed_backed_counted,
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
    inode_read_ahead: bool,
    logical_transitions: bool,
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
    inode_read_ahead: bool,
    logical_transitions: bool,
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
            inode_read_ahead: false,
            logical_transitions: false,
        }
    }
    /// Records each fresh file's whole-content digest, when the overlay
    /// kept one, in the Store through `digests` (#364).
    pub fn with_digests(mut self, digests: &'a dyn FileDigests) -> Self {
        self.digests = Some(digests);
        self
    }
    /// Enables bounded inode-leaf demands for a remote Store's Commit.
    pub fn with_inode_read_ahead(mut self, enabled: bool) -> Self {
        self.inode_read_ahead = enabled;
        self
    }

    /// Retains the canonical inode transitions for the caller's later logical
    /// count. The operation owner must stay held through that count.
    pub fn with_logical_transitions(mut self, enabled: bool) -> Self {
        self.logical_transitions = enabled;
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
            inode_read_ahead,
            logical_transitions,
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
            inode_read_ahead,
            logical_transitions,
        };
        let (result, logical_changes) =
            match scope.run(|timing| attempt.run(base, consumer, timing)) {
                Ok((built, logical)) => (Ok(built), logical),
                Err(original) => {
                    shared.borrow_mut().content(original.clone());
                    (Err(original), None)
                }
            };
        let state = shared.into_inner();
        CapturedNamespaceAttempt {
            result,
            logical_changes,
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
    ) -> ContentResult<(FilesystemResult, Option<LogicalChanges>)> {
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
        let (headers, same) = names(self.provider, shared, facts, &base)?;
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
        let totals = values(&builder, &mut timed, timing, headers, &same);
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
        let base_cache = base.client();
        let old_pages = layerfs_content::filesystem::objects::BasePageReader::new(
            base_cache.as_ref(),
            self.objects,
        );
        let many_rows =
            totals.headers.saturating_add(totals.values) > layerfs_overlay::RECORD_ROWS as u64;
        let cached =
            (self.inode_read_ahead && many_rows).then_some(&old_pages as &dyn AuthenticatedObjects);
        let mut output =
            FilesystemObjects::new_with_accepted(self.objects, &mut timed, self.objects)
                .with_inode_read_ahead(self.inode_read_ahead)
                .with_base_reader(cached);
        let recorded = shared.borrow().record_ns;
        let started = Instant::now();
        let result = timing.child("update").run(|_| {
            let mut backing = Backing(shared);
            if self.logical_transitions && many_rows {
                update_filesystem_streamed_backed_counted(&mut output, &input, &mut backing)
                    .map(|(built, logical)| (built, Some(logical)))
            } else {
                update_filesystem_streamed_backed(&mut output, &input, &mut backing, None)
                    .map(|built| (built, None))
            }
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
        if let Ok((built, _)) = &result {
            let _ = base.remember_constructed_counts(self.objects, built.root, &directories);
        }
        result
    }
}
