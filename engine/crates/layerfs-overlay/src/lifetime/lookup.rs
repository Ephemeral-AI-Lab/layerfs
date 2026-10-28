//! Exact lookup ownership, including non-file metadata after removal.
use crate::{
    db::integer, inode, BaseSource, Inode, InodeKind, LookupOwner, Overlay, OverlayError,
    OverlayResult, ReferenceKind,
};
impl Overlay {
    /// The authenticated root serial distinguishes its canonical zero count
    /// from removal. Base metadata is acquired outside SQL; current rows decide.
    pub fn acquire_lookup(
        &self,
        source: BaseSource,
        request: u64,
        base: &Inode,
        root: u64,
    ) -> OverlayResult<LookupOwner> {
        inode::check(base)?;
        let request = integer(request)?;
        integer(root)?;
        if request == 0 || root == 0 {
            return Err(OverlayError::Invalid("lookup identity"));
        }
        self.atomic(|| {
            let state = self.source_state(source)?;
            if state.closed {
                return Err(OverlayError::Closed);
            }
            let current = self
                .inode_at(base.serial, state.active, state.installed)?
                .unwrap_or_else(|| base.clone());
            if current.nlink == 0
                && !(current.kind == InodeKind::Directory && current.serial == root)
            {
                return Err(OverlayError::Missing);
            }
            let owner = self.mint_owner()?;
            let serial = integer(current.serial)?;
            let lookup = LookupOwner {
                engine: self.identity,
                owner,
                serial: current.serial,
            };
            self.open.borrow_mut().lookups.insert(
                owner,
                request,
                lookup,
                "duplicate lookup request",
            )?;
            self.file_ref(serial, ReferenceKind::Lookup, true)?;
            Ok(LookupOwner {
                engine: self.identity,
                owner,
                serial: current.serial,
            })
        })
    }
    /// One processing reference outlives lookup release and logical close.
    pub fn acquire_lookup_read(
        &self,
        source: BaseSource,
        lookup: LookupOwner,
        request: u64,
    ) -> OverlayResult<crate::FileRead> {
        let request = integer(request)?;
        if request == 0 {
            return Err(OverlayError::Invalid("zero read request"));
        }
        self.atomic(|| {
            let state = self.source_state(source)?;
            if state.closed {
                return Err(OverlayError::Closed);
            }
            if lookup.engine != source.engine {
                return Err(OverlayError::Stale);
            }
            self.check_lookup(lookup)?;
            self.retain_serial_read(source, lookup.serial, request)
        })
    }
    pub fn retained_lookup(&self, request: u64) -> OverlayResult<Option<LookupOwner>> {
        self.state()?;
        let request = integer(request)?;
        Ok(self.open.borrow().lookups.retained(request))
    }
    pub fn check_lookup(&self, lookup: LookupOwner) -> OverlayResult<()> {
        self.state()?;
        if self
            .open
            .borrow()
            .lookups
            .get(lookup.owner)
            .map(|(held, _)| held)
            != Some(lookup)
        {
            return Err(OverlayError::Stale);
        }
        Ok(())
    }
    /// Call only after kernel/request continuations using this reference finish.
    pub fn release_lookup(&self, lookup: LookupOwner) -> OverlayResult<()> {
        self.atomic_cleanup(|| {
            self.check_lookup(lookup)?;
            self.open
                .borrow_mut()
                .lookups
                .remove(lookup.owner, |held| *held == lookup)?;
            self.file_ref(integer(lookup.serial)?, ReferenceKind::Lookup, false)?;
            Ok(())
        })
    }
}
