//! Exact kernel references and the parents retained by directory custody.
use super::directories::{CookieStore, CookieUndo, DirectoryState, OpenDisposals};
use super::files::FileReferences;
use super::journal::{JournalMap, OwnerTable};
use crate::lifetime::orphan::Orphan;
use crate::{
    BaseSource, CapturedReader, FileRead, KernelMount, LookupOwner, OpenFile, OperationOwner,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub(crate) struct KernelReference {
    pub owner: u64,
    pub count: u64,
    pub implicit: bool,
}
#[derive(Default)]
pub(crate) struct OpenState {
    pub kernel: JournalMap<(u64, u64), KernelReference>,
    pub parents: JournalMap<u64, (u64, u64)>,
    /// Descriptors by owner; requests are (kernel mount owner or 0, request).
    pub files: OwnerTable<(u64, i64), OpenFile>,
    pub reads: OwnerTable<i64, FileRead>,
    pub lookups: OwnerTable<i64, LookupOwner>,
    pub custody: JournalMap<i64, FileReferences>,
    pub orphans: JournalMap<i64, Orphan>,
    /// Serials whose active row `keep_live_row` carried and nothing wrote
    /// since: their only change is a live mtime, which is not counted dirty.
    pub kept_live: JournalMap<i64, ()>,
    /// Files followed as appends for their whole-content digest (#364).
    pub digests: JournalMap<i64, crate::payload::digest::Digesting>,
    /// The last file finished with no descriptor: serial, content sequence
    /// and followed state, so its next chunked append resumes. One slot.
    pub resumable: JournalMap<(), (i64, i64, crate::payload::digest::Digesting)>,
    pub waits: JournalMap<(i64, i64), ()>,
    /// One attached/revoked mount, restored by whole-job rollback.
    pub mount: Option<(KernelMount, bool)>,
    /// Directory handles by owner; requests are (kernel mount owner, request).
    pub directories: OwnerTable<(u64, u64), DirectoryState>,
    pub operations: OwnerTable<i64, OperationOwner>,
    pub captured: OwnerTable<i64, CapturedReader>,
    pub captured_generations: JournalMap<i64, u64>,
    pub sources: JournalMap<(u64, i64), BaseSource>,
    pub cookies: BTreeMap<u64, CookieStore>,
    pub cookie_undo: Vec<CookieUndo>,
    pub disposals: OpenDisposals,
}
impl OpenState {
    pub fn commit(&mut self) {
        self.kernel.commit();
        self.parents.commit();
        self.files.commit();
        self.reads.commit();
        self.lookups.commit();
        self.custody.commit();
        self.orphans.commit();
        self.kept_live.commit();
        self.digests.commit();
        self.resumable.commit();
        self.waits.commit();
        self.directories.commit();
        self.operations.commit();
        self.captured.commit();
        self.captured_generations.commit();
        self.sources.commit();
        self.commit_cookies();
    }
    pub fn rollback(&mut self) {
        self.kernel.rollback();
        self.parents.rollback();
        self.files.rollback();
        self.reads.rollback();
        self.lookups.rollback();
        self.custody.rollback();
        self.orphans.rollback();
        self.kept_live.rollback();
        self.digests.rollback();
        self.resumable.rollback();
        self.waits.rollback();
        self.directories.rollback();
        self.operations.rollback();
        self.captured.rollback();
        self.captured_generations.rollback();
        self.sources.rollback();
        self.rollback_cookies();
    }
}
