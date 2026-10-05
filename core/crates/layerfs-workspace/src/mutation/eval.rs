//! Owner-job evaluation: current local rows over supplied immutable base facts.
use crate::{BaseFacts, Need, Refusal, Time, WorkspaceError, WorkspaceResult};
use layerfs_content::filesystem::PathName;
use layerfs_content::ContentError;
use layerfs_overlay::{Inode, InodeKind, NameLayers, SourceRows};

/// One evaluation inside a single owner job. Every answer comes from rows read
/// in this job or from base facts that cannot change while the source is owned.
/// A missing fact is recorded as a need; nothing is published in that round.
pub(crate) struct Eval<'a> {
    pub rows: SourceRows<'a>,
    pub facts: &'a BaseFacts,
    pub root: u64,
    pub open_serial: Option<u64>,
    pub needs: Vec<Need>,
}
pub(crate) fn refuse<T>(refusal: Refusal) -> WorkspaceResult<T> {
    Err(WorkspaceError::Refused(refusal))
}
impl Eval<'_> {
    fn need(&mut self, need: Need) {
        if !self.needs.contains(&need) {
            self.needs.push(need);
        }
    }
    /// Outer None: undecided this round. Inner None: no such inode anywhere.
    pub fn inode(&mut self, serial: u64) -> WorkspaceResult<Option<Option<Inode>>> {
        if let Some(row) = self.rows.inode(serial)? {
            return Ok(Some(Some(row)));
        }
        match self.facts.inode(serial) {
            Some(fact) => Ok(Some(fact.clone())),
            None => {
                self.need(Need::Inode(serial));
                Ok(None)
            }
        }
    }
    /// A removed inode keeps a row with no references; the root has none by
    /// canonical grammar and cannot be removed.
    pub fn alive(&self, inode: &Inode) -> bool {
        inode.nlink != 0 || inode.serial == self.root
    }
    /// An existing, still-referenced inode or the Missing refusal.
    pub fn existing(&mut self, serial: u64) -> WorkspaceResult<Option<Inode>> {
        match self.inode(serial)? {
            None => Ok(None),
            Some(Some(inode)) if self.alive(&inode) => Ok(Some(inode)),
            Some(_) => refuse(Refusal::Missing),
        }
    }
    pub fn directory(&mut self, serial: u64) -> WorkspaceResult<Option<Inode>> {
        match self.existing(serial)? {
            Some(inode) if inode.kind != InodeKind::Directory => refuse(Refusal::NotDirectory),
            other => Ok(other),
        }
    }
    pub fn layers(&self, parent: u64, name: &PathName) -> WorkspaceResult<NameLayers> {
        Ok(self.rows.name(parent, name.as_bytes())?)
    }
    /// The base binding, consulted only where no local row decides. `parent`
    /// None means its local row is absent, so it can only be a base directory.
    fn base_name(
        &mut self,
        serial: u64,
        parent: Option<&Inode>,
        name: &PathName,
    ) -> Option<Option<u64>> {
        if parent.is_some_and(|inode| self.rows.source().created_above(inode.born)) {
            return Some(None);
        }
        let fact = self.facts.name(serial, name);
        if fact.is_none() {
            self.need(Need::Name(serial, name.clone()));
        }
        fact
    }
    /// Current view binding of one name.
    pub fn bound(
        &mut self,
        serial: u64,
        parent: Option<&Inode>,
        name: &PathName,
        layers: NameLayers,
    ) -> Option<Option<u64>> {
        match layers.active.or(layers.lower) {
            Some(row) => Some(row),
            None => self.base_name(serial, parent, name),
        }
    }
    /// The base fact a removal publishes. A lower local row decides instead of
    /// it, so no base demand is made when one exists.
    pub fn inherited(
        &mut self,
        serial: u64,
        parent: Option<&Inode>,
        name: &PathName,
        layers: NameLayers,
    ) -> Option<bool> {
        if layers.lower.is_some() {
            return Some(false);
        }
        self.base_name(serial, parent, name)
            .map(|base| base.is_some())
    }
    /// The inode a visible name binds; a binding without an inode is corrupt.
    pub fn target(&mut self, serial: u64) -> WorkspaceResult<Option<Inode>> {
        match self.inode(serial)? {
            None => Ok(None),
            Some(Some(inode)) => Ok(Some(inode)),
            Some(None) => Err(ContentError::InvalidRecord("bound inode is absent").into()),
        }
    }
}
/// The parent's final value after one entry change at `now`.
pub(crate) fn touched(
    parent: &Inode,
    now: Time,
    added: bool,
    removed: bool,
) -> WorkspaceResult<Inode> {
    let entries = parent
        .entries
        .checked_add(u64::from(added))
        .and_then(|entries| entries.checked_sub(u64::from(removed)))
        .ok_or(ContentError::InvalidRecord("directory entry count"))?;
    Ok(Inode {
        mtime_seconds: now.seconds,
        mtime_nanoseconds: now.nanoseconds,
        entries,
        ..parent.clone()
    })
}
