//! Consuming exact captured preparation; original custody never releases on Drop.
use super::{
    context, normalize,
    source::{Backing, Complete, Input},
    state::{Shared, State},
};
use crate::{
    BaseView, ConstructionBackingCustody, IndexedConstructionRecords, OverlayCapturedRuns,
    OverlayOperationRecords, Workspace, WorkspaceError,
};
use layerfs_content::{
    apply_indexed_edits_view_backed, construct_runs,
    filesystem::{InodeIdentity, InodeScope},
    ConstructedFile, ConstructionCapacities, ConstructionPolicy, ContentError, ContentResult,
    FileView, FinalizedConsumer, IndexedEditRequest, ObjectId,
};
use layerfs_overlay::{CapturedReader, IndexedOperationRecordScope, InodeKind, OverlayError};
use layerfs_telemetry::timer::TimingScope;
use std::fmt;

/// Actual adapter visits only; not whole-operation I/O, copies or residency.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CapturedFileWork {
    /// Actual captured-run calls made while preparing final changes.
    pub normalization_steps: u64,
    /// Actual captured-run calls made by the construction/comparison source.
    pub source_steps: u64,
    /// Rows from successful returned units; failed original SQL work remains
    /// in its retained provider Completion and is not declared zero here.
    pub metadata_rows: u64,
    /// Stale rows from successful returned units, with the same scope.
    pub stale_rows: u64,
    /// Exact number of sealed normalized records.
    pub normalized_edits: u64,
    /// Planned compare-to-construction backwards restarts, never failed replay.
    pub source_resets: u64,
    /// Successfully retained immutable file classifications; failed opens are
    /// represented by their original cause rather than counted as success.
    pub classifications: u64,
}
/// Exact retained reader, record scope and first-original errors for caller fences.
/// This value and its Drop perform no reader/operation release or retry.
pub struct CapturedFileCustody {
    pub reader: CapturedReader,
    pub serial: u64,
    /// Retains the operation's authorized client, including original port failures.
    pub base: Option<BaseView>,
    pub file: Option<FileView>,
    pub records: ConstructionBackingCustody,
    /// First adapter/content cause; record failures remain in records.failure.
    pub failure: Option<WorkspaceError>,
    pub work: CapturedFileWork,
}
/// One construction result and all original local custody, including on failure.
pub struct CapturedFileAttempt {
    pub result: ContentResult<ConstructedFile>,
    pub custody: CapturedFileCustody,
}
impl fmt::Debug for CapturedFileCustody {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("CapturedFileCustody")
            .field("reader", &self.reader)
            .field("serial", &self.serial)
            .field("base", &self.base.as_ref().map(BaseView::identity))
            .field("file", &self.file.as_ref().map(FileView::root))
            .field("records", &self.records)
            .field("failure", &self.failure)
            .field("work", &self.work)
            .finish()
    }
}
#[derive(Clone, Copy)]
pub(super) struct Facts {
    pub reader: CapturedReader,
    pub records_scope: IndexedOperationRecordScope,
    pub serial: u64,
    pub scope: InodeScope,
    pub profile: ObjectId,
    pub file_root: Option<ObjectId>,
    pub base_size: u64,
    pub final_size: u64,
}
/// One sealed final-state plan over exact captured input and indexed records.
/// Consuming construct prevents recovery of a failed attempt as another plan.
pub struct CapturedFileEdits<'a, P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized> {
    provider: &'a P,
    shared: Shared<'a, P>,
    base: Option<BaseView>,
    file: Option<FileView>,
    reader: CapturedReader,
    serial: u64,
    facts: Option<Facts>,
    count: usize,
    context: Vec<u8>,
}
impl<'a, P: OverlayCapturedRuns + OverlayOperationRecords + ?Sized> CapturedFileEdits<'a, P> {
    /// Derives every root/kind/length from this Workspace's operation provider,
    /// exact retained reader and captured point; callers select only the serial
    /// and actual record owner/scope. Failed preparation returns original custody.
    // Keep already-owned failure/input custody inline: reporting admission
    // failure must not first allocate a second error envelope.
    #[allow(clippy::result_large_err)]
    pub fn prepare(
        workspace: &Workspace,
        provider: &'a P,
        reader: CapturedReader,
        serial: u64,
        records_scope: IndexedOperationRecordScope,
    ) -> Result<Self, CapturedFileCustody> {
        let mut owner = Self {
            provider,
            shared: Shared::new(State {
                records: IndexedConstructionRecords::new(provider, records_scope),
                failure: None,
                work: CapturedFileWork::default(),
                cache: None,
            }),
            base: None,
            file: None,
            reader,
            serial,
            facts: None,
            count: 0,
            context: Vec::new(),
        };
        let result = owner.bind(workspace, records_scope).and_then(|facts| {
            owner.facts = Some(facts);
            normalize::prepare(provider, &owner.shared, facts).map_err(WorkspaceError::Content)
        });
        match result {
            Ok((count, context)) => {
                owner.count = count;
                owner.context = context;
                Ok(owner)
            }
            Err(original) => {
                owner.shared.borrow_mut().original(original);
                Err(owner.into_custody())
            }
        }
    }
    fn bind(
        &mut self,
        workspace: &Workspace,
        records_scope: IndexedOperationRecordScope,
    ) -> Result<Facts, WorkspaceError> {
        if workspace.route() != self.reader.capture().route()
            || records_scope.owner.route() != workspace.route()
        {
            return Err(OverlayError::Stale.into());
        }
        let mut base = workspace.base()?;
        self.base = Some(base.clone());
        if base.identity().0.to_bytes() != self.reader.root() {
            base = base.rebind(layerfs_content::filesystem::FilesystemRootId(
                ObjectId::from_bytes(&self.reader.root())?,
            ))?;
        }
        self.base = Some(base.clone());
        InodeIdentity::new(base.root().scope(), self.serial)?;
        let local = self.provider.captured_inode(self.reader, self.serial)?;
        if local.as_ref().is_some_and(|inode| {
            inode.serial != self.serial || inode.kind != InodeKind::File || inode.nlink == 0
        }) {
            return Err(OverlayError::Missing.into());
        }
        let file = base.captured_file(self.serial)?;
        if file.is_none() && local.is_none() {
            return Err(ContentError::PathNotFound.into());
        }
        if let Some(inode) = local.as_ref() {
            if self.reader.created_above(inode.born) == file.is_some() {
                return Err(ContentError::InvalidRecord("captured file creation binding").into());
            }
        }
        let (file_root, base_size) = match file {
            Some(file) => {
                let root = file.view.root();
                let size = file.view.logical_len();
                self.file = Some(file.view);
                self.shared.borrow_mut().work.classifications = 1;
                (Some(root), size)
            }
            None => (None, 0),
        };
        Ok(Facts {
            reader: self.reader,
            records_scope,
            serial: self.serial,
            scope: base.root().scope(),
            profile: base.root().profile(),
            file_root,
            base_size,
            final_size: local.as_ref().map_or(base_size, |inode| inode.size),
        })
    }
    /// Consumes one prepared plan with the caller's actual Store-derived policy
    /// and consumer. It neither saves/publishes history nor releases local owners.
    pub fn construct(
        self,
        policy: ConstructionPolicy,
        capacities: &ConstructionCapacities,
        consumer: &mut dyn FinalizedConsumer,
        scope: TimingScope<'_>,
    ) -> CapturedFileAttempt {
        let result = self.run(policy, capacities, consumer, scope);
        if let Err(original) = &result {
            self.shared.borrow_mut().content(original.clone());
        }
        CapturedFileAttempt {
            result,
            custody: self.into_custody(),
        }
    }
    fn run(
        &self,
        policy: ConstructionPolicy,
        capacities: &ConstructionCapacities,
        consumer: &mut dyn FinalizedConsumer,
        scope: TimingScope<'_>,
    ) -> ContentResult<ConstructedFile> {
        self.shared.borrow().ready()?;
        policy.validated()?;
        if *capacities != policy.capacities() {
            return Err(ContentError::InvalidRecord(
                "captured file policy capacities",
            ));
        }
        let facts = self
            .facts
            .ok_or(ContentError::InvalidRecord("unbound captured plan"))?;
        context::verify(&self.shared, &self.context)?;
        let input = Input::new(
            self.provider,
            &self.shared,
            facts,
            self.count,
            &self.context,
        )?;
        if let Some(view) = &self.file {
            let client = self
                .base
                .as_ref()
                .ok_or(ContentError::InvalidRecord("captured base binding"))?
                .client();
            apply_indexed_edits_view_backed(
                policy,
                capacities,
                client.as_ref(),
                IndexedEditRequest {
                    view,
                    edits: &input,
                    source: &input,
                },
                consumer,
                &mut Backing(&self.shared),
                scope,
            )
        } else {
            construct_runs(
                policy,
                capacities,
                &mut Complete { input, position: 0 },
                consumer,
                scope,
            )
            .map(|result| result.file)
        }
    }
    fn into_custody(self) -> CapturedFileCustody {
        let state = self.shared.into_inner();
        CapturedFileCustody {
            reader: self.reader,
            serial: self.serial,
            base: self.base,
            file: self.file,
            records: state.records.into_custody(),
            failure: state.failure,
            work: state.work,
        }
    }
}
