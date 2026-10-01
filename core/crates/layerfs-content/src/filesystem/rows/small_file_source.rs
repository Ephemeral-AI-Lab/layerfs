//! Fixed genuine existing-file source with original issuer and authenticated table.
use super::{BindingAuthority, BindingSourceId, SpoolPreparation};
use crate::filesystem::{
    inode::read::{lookup_many_owned, InodeReadWork, InodeTable},
    root::FilesystemRoot,
    state::{BaseFact, GraphMemory, GraphMemoryLease, GraphSubject},
    InodeUpdate,
};
use crate::object::{
    inode_leaf::InodeKind, AuthenticatedObjects, CanonicalBudget, CanonicalReadKind,
    CanonicalReadPermit,
};
use crate::{ContentError, ContentResult, ObjectId};
use std::io::Read;
/// Fixed admitted population, independent of workload names or future growth.
pub const SMALL_FILE_ROWS: usize = 8;
pub(super) struct SmallData {
    pub(super) authority: BindingAuthority,
    pub(super) subject: GraphSubject,
    pub(super) table: InodeTable,
    pub(super) declared: usize,
    pub(super) used: usize,
    pub(super) rows: [Option<InodeUpdate>; SMALL_FILE_ROWS],
    pub(super) base: [Option<BaseFact>; SMALL_FILE_ROWS],
}
/// One exact bounded receive builder; typed immutable proof exists only after EOF.
pub struct SmallFileBuilder {
    data: Box<SmallData>,
    memory: GraphMemory,
    canonical: CanonicalBudget,
    actor: super::small_file_pending::ActorAdmission,
    _owner: GraphMemoryLease,
}
/// Concrete verified rows, original SourceId, actual table and known file values.
pub struct VerifiedSmallFileRows {
    pub(super) data: Box<SmallData>,
    pub(super) memory: GraphMemory,
    actor: std::cell::RefCell<Option<super::small_file_pending::ActorAdmission>>,
    _owner: GraphMemoryLease,
}
impl SmallFileBuilder {
    /// Verify complete class before reading base/table or allocating the source.
    pub fn new(
        preparation: SpoolPreparation,
        subject: GraphSubject,
        reader: &dyn AuthenticatedObjects,
    ) -> ContentResult<Self> {
        Self::from_pending(
            super::small_file_pending::PendingSmallFiles::compatibility(subject, preparation)?,
            reader,
        )
    }
    /// Consume the original source and all pre-Save credits without a second controller.
    pub fn from_pending(
        pending: super::small_file_pending::PendingSmallFiles,
        reader: &dyn AuthenticatedObjects,
    ) -> ContentResult<Self> {
        let super::small_file_pending::PendingSmallFiles {
            preparation,
            subject,
            memory,
            source: owner,
            actor,
        } = pending;
        let d = preparation.declaration;
        let canonical = CanonicalBudget::new(256 * 1024)?;
        let id = subject.base().unwrap().0;
        let permit = CanonicalReadPermit::new(&canonical, 1, 8192, CanonicalReadKind::Any)?;
        let (acquired, _) = layerfs_telemetry::timer::Timing::disabled("small.base", |scope| {
            reader.read_canonical_owned(&[id], permit, scope.child("small.base"))
        });
        let acquired = acquired?;
        let buffer = acquired
            .into_iter()
            .next()
            .ok_or(ContentError::MissingObject)?;
        if ObjectId::for_bytes(&buffer) != id {
            return Err(ContentError::IdentityMismatch);
        }
        let root = FilesystemRoot::decode(&buffer)?;
        if root.scope() != subject.namespace()
            || root.root_inode().serial() != subject.root_serial()
        {
            return Err(ContentError::InvalidRecord("small file base subject"));
        }
        drop(buffer);
        let table = InodeTable {
            root: root.inode_table(),
            root_serial: root.root_inode().serial(),
        };
        let root_value = lookup_many_owned(
            reader,
            table,
            &[table.root_serial],
            &mut InodeReadWork::default(),
            &canonical,
        )?
        .pop()
        .flatten()
        .ok_or(ContentError::InvalidRecord("small file base root"))?;
        if root_value.kind != InodeKind::Directory {
            return Err(ContentError::WrongLogicalRole);
        }
        Ok(Self {
            data: Box::new(SmallData {
                authority: preparation.authority,
                subject,
                table,
                declared: d.inodes,
                used: 0,
                rows: [None; 8],
                base: [None; 8],
            }),
            memory,
            canonical,
            actor,
            _owner: owner,
        })
    }
    /// Admit one explicit existing RegularFile only after actual table/kind lookup.
    pub fn push(
        &mut self,
        reader: &dyn AuthenticatedObjects,
        row: InodeUpdate,
    ) -> ContentResult<()> {
        if self.data.used >= self.data.declared
            || row.value.kind != InodeKind::RegularFile
            || row.serial == self.data.subject.root_serial()
            || row.serial == 0
            || row.serial > i64::MAX as u64
            || self.data.used != 0
                && self.data.rows[self.data.used - 1].unwrap().serial >= row.serial
        {
            return Err(ContentError::InvalidRecord("small file row class/order"));
        }
        let value = lookup_many_owned(
            reader,
            self.data.table,
            &[row.serial],
            &mut InodeReadWork::default(),
            &self.canonical,
        )?
        .pop()
        .flatten()
        .ok_or(ContentError::InvalidRecord("small file missing base"))?;
        if value.kind != InodeKind::RegularFile {
            return Err(ContentError::WrongLogicalRole);
        }
        let at = self.data.used;
        self.data.rows[at] = Some(row);
        self.data.base[at] = Some(BaseFact {
            serial: row.serial,
            value: Some(value),
        });
        self.data.used += 1;
        Ok(())
    }
    /// Exact declared count and actual remaining-input EOF preserve the original issuer.
    pub fn finish(self, input: &mut dyn Read) -> ContentResult<VerifiedSmallFileRows> {
        if self.data.used != self.data.declared {
            return Err(ContentError::IncompleteOperation);
        }
        let mut byte = [0];
        if input.read(&mut byte).map_err(|_| ContentError::Io)? != 0 {
            return Err(ContentError::TrailingBytes);
        }
        Ok(VerifiedSmallFileRows {
            data: self.data,
            memory: self.memory,
            actor: std::cell::RefCell::new(Some(self.actor)),
            _owner: self._owner,
        })
    }
}
impl VerifiedSmallFileRows {
    pub(crate) fn captured_selector(&self) -> Option<[u8; 32]> {
        self.actor
            .borrow()
            .as_ref()
            .and_then(|claim| claim.selector)
    }
    pub(crate) fn take_actor_credit(&self, selector: [u8; 32]) -> ContentResult<GraphMemoryLease> {
        if self
            .actor
            .borrow()
            .as_ref()
            .is_none_or(|claim| claim.selector.is_some_and(|captured| captured != selector))
        {
            return Err(ContentError::InvalidRecord(
                "small file admission replay/foreign selection",
            ));
        }
        let claim = self.actor.borrow_mut().take().unwrap();
        // This prospective helper window owns no allocated data. Its exact
        // controller becomes available for real helper/page leases after transfer.
        drop(claim.window);
        Ok(claim.actor)
    }
    /// The original live source issued before body effects.
    pub fn source_id(&self) -> BindingSourceId {
        self.data.authority.source_id()
    }
    /// Full captured UpdateBase/namespace/root/logicalS context.
    pub fn subject(&self) -> &GraphSubject {
        &self.data.subject
    }
    /// Actual authenticated immutable table, not an adopted caller table.
    pub fn table(&self) -> InodeTable {
        self.data.table
    }
    /// Fixed genuine source memory used by its state and returned-page owners.
    pub fn memory(&self) -> GraphMemory {
        self.memory.clone()
    }
    /// Verified declared serials/values; no raw mutable array escapes.
    pub fn row(&self, at: usize) -> Option<InodeUpdate> {
        self.data.rows.get(at).copied().flatten()
    }
    /// Independently authenticated existing file answer for one declared serial.
    pub fn base(&self, serial: u64) -> Option<BaseFact> {
        self.data
            .base
            .iter()
            .flatten()
            .find(|row| row.serial == serial)
            .copied()
    }
    /// Exact immutable population.
    pub fn len(&self) -> usize {
        self.data.used
    }
    /// This admitted class always has at least one declared identity.
    pub fn is_empty(&self) -> bool {
        self.data.used == 0
    }
    /// Logical cleanup has no native close/unlink/reset effects.
    pub fn cleanup(self) -> ContentResult<()> {
        Ok(())
    }
}
