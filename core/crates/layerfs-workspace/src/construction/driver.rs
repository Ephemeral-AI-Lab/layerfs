//! One consuming captured namespace attempt; original custody never releases on Drop.
use super::namespace::{
    begin, names, seal, values, Backing, Builder, Cursor, Facts, Shared, State,
};
use super::outcome::{CapturedNamespaceAttempt, CapturedNamespaceCustody, CapturedNamespaceWork};
use crate::{
    BaseView, IndexedConstructionRecords, OverlayCapturedNamespace, OverlayOperationRecords,
    Workspace, WorkspaceResult,
};
use layerfs_content::{
    filesystem::{
        update_filesystem_streamed_backed, FilesystemObjects, FilesystemResources,
        FilesystemRootId, StreamedFilesystemInput,
    },
    AuthenticatedObjects, ConstructionCapacities, ConstructionPolicy, ContentError, ContentResult,
    FilesystemResult, FinalizedConsumer, ObjectId,
};
use layerfs_overlay::{
    CapturedReader, IndexedOperationRecordScope, OperationOwner, OverlayError, Route,
};
use layerfs_telemetry::timer::{Active, TimingScope};

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
            }),
            route: workspace.route(),
            base: workspace.base(),
            reader,
            operation,
        }
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
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> Attempt<'_, '_, P> {
    /// The reader's exact root, never the Workspace's possibly later binding.
    fn bind(&self, base: WorkspaceResult<BaseView>) -> ContentResult<BaseView> {
        if self.route != self.reader.capture().route() || self.operation.route() != self.route {
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
        let headers = names(self.provider, shared, facts)?;
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
        };
        let totals = values(&builder, consumer, timing, headers)?;
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
        let mut output = FilesystemObjects::new_with_accepted(self.objects, consumer, self.objects);
        timing.child("update").run(|_| {
            update_filesystem_streamed_backed(&mut output, &input, &mut Backing(shared), None)
        })
    }
}
