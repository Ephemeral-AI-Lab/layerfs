//! Consuming exact captured preparation; original custody never releases on Drop.
use super::{
    context, normalize,
    source::{Backing, Complete, Input},
    state::{Shared, State},
};
use crate::construction::outcome::nanos;
use crate::{
    BaseView, ConstructionBackingCustody, IndexedConstructionRecords, OverlayCapturedRuns,
    OverlayOperationRecords, Workspace, WorkspaceError,
};
use layerfs_content::{
    apply_indexed_edits_view_backed, construct_runs, construct_runs_with_predecessor,
    filesystem::{InodeIdentity, InodeScope},
    ConstructedFile, ConstructionCapacities, ConstructionPolicy, ContentError, ContentResult,
    FileView, FinalizedConsumer, IndexedEditRequest, ObjectId, PredecessorBase,
};
use layerfs_overlay::{CapturedReader, IndexedOperationRecordScope, InodeKind, OverlayError};
use layerfs_telemetry::timer::TimingScope;
use std::{fmt, time::Instant};

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
    /// Bytes of references and inherited ranges that the edit script could
    /// not retain, read from committed content and offered as new bytes.
    pub committed_bytes: u64,
    /// Successful bounded windows authenticating a shifted edit edge.
    pub edge_windows: u64,
    /// Logical bytes acquired for edge comparison, counting both sources.
    /// These are not physical IO or whole-operation read counts.
    pub edge_read_bytes: u64,
    /// Bytes whose unchanged base mapping survived authenticated edge trimming.
    pub edge_retained_bytes: u64,
    /// Nanoseconds binding the file (captured inode and base file lookups)
    /// and reading its runs (payload and committed bytes). Observations only.
    pub bind_ns: u64,
    pub source_ns: u64,
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
            normalize::prepare(
                provider,
                &owner.shared,
                facts,
                owner.base.as_ref(),
                owner.file.as_ref(),
            )
            .map_err(WorkspaceError::Content)
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
    /// A file the immutable base does not hold, constructed straight from its
    /// captured runs with no normalized-edit records. The binding is the one
    /// `prepare` makes, and it must find no base file. A base-less file's
    /// only edit is one insert of its whole final length, and the run
    /// construction never reads that edit, so no edit or context record is
    /// written, read or reclaimed. The caller's attempt guards that each
    /// serial is constructed once. Failed binding or construction returns the
    /// original custody, as `construct` does.
    ///
    /// `predecessor` is the file a rename put this one in place of (#352
    /// F-063). When the base holds it as a regular file, its chunks are
    /// offered as the new chunks' delta bases; a predecessor the base no
    /// longer holds, or holds as another kind, offers nothing.
    #[allow(clippy::too_many_arguments)]
    pub fn construct_new(
        workspace: &Workspace,
        provider: &'a P,
        reader: CapturedReader,
        serial: u64,
        records_scope: IndexedOperationRecordScope,
        predecessor: Option<u64>,
        policy: ConstructionPolicy,
        capacities: &ConstructionCapacities,
        consumer: &mut dyn FinalizedConsumer,
        scope: TimingScope<'_>,
    ) -> CapturedFileAttempt {
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
        let bound = owner.bind(workspace, records_scope).and_then(|facts| {
            if facts.file_root.is_some() {
                return Err(ContentError::InvalidRecord("captured new file has a base").into());
            }
            Ok(facts)
        });
        let result = match bound {
            Ok(facts) => {
                owner.facts = Some(facts);
                owner.run_new(predecessor, policy, capacities, consumer, scope)
            }
            Err(original) => Err(owner.shared.borrow_mut().original(original)),
        };
        if let Err(original) = &result {
            owner.shared.borrow_mut().content(original.clone());
        }
        CapturedFileAttempt {
            result,
            custody: owner.into_custody(),
        }
    }
    fn run_new(
        &self,
        predecessor: Option<u64>,
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
        let input = Input::new(
            self.provider,
            &self.shared,
            facts,
            0,
            &[],
            self.base.as_ref(),
        )?;
        let mut runs = Complete {
            input,
            position: 0,
            sealed: false,
        };
        let base = self
            .base
            .as_ref()
            .ok_or(ContentError::InvalidRecord("captured base binding"))?;
        let view = match predecessor {
            Some(serial) => predecessor_view(base, serial)
                .map_err(|original| self.shared.borrow_mut().original(original))?,
            None => None,
        };
        let client = base.client();
        let offered = view
            .as_ref()
            .map(|view| PredecessorBase::new(client.as_ref(), view.root()));
        construct_runs_with_predecessor(policy, capacities, &mut runs, offered, consumer, scope)
            .map(|result| result.file)
    }
    fn bind(
        &mut self,
        workspace: &Workspace,
        records_scope: IndexedOperationRecordScope,
    ) -> Result<Facts, WorkspaceError> {
        let started = Instant::now();
        let facts = self.bound(workspace, records_scope);
        self.shared.borrow_mut().work.bind_ns = nanos(started);
        facts
    }
    fn bound(
        &mut self,
        workspace: &Workspace,
        records_scope: IndexedOperationRecordScope,
    ) -> Result<Facts, WorkspaceError> {
        if workspace.route().engine() != self.reader.capture().engine()
            || records_scope.owner.engine() != workspace.route().engine()
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
            self.base.as_ref(),
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
                &mut Complete {
                    input,
                    position: 0,
                    sealed: true,
                },
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

/// The base's regular file `serial`, offered as a delta base. A serial the
/// base does not hold, or holds as another kind, is no candidate; any other
/// failure is the base's own and is returned.
fn predecessor_view(base: &BaseView, serial: u64) -> Result<Option<FileView>, WorkspaceError> {
    match base.captured_file(serial) {
        Ok(file) => Ok(file.map(|file| file.view)),
        Err(WorkspaceError::Content(ContentError::WrongLogicalRole)) => Ok(None),
        Err(original) => Err(original),
    }
}
