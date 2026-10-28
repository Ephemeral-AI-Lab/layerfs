//! One captured inode row to one typed value: file root, symlink target, metadata.
use super::records::{add, key, read_number, Facts, Shared, HEADER};
use crate::construction::outcome::nanos;
use crate::{
    BaseView, CapturedFileEdits, FileDigests, OverlayCapturedNamespace, OverlayOperationRecords,
    Workspace,
};
use layerfs_content::{
    filesystem::{
        attributes::{
            patch::{visit_keys_counted, AttributePatchWork},
            portable_root, read_portable, AttributeReadWork, PortableMetadata,
        },
        symlink::emit_symlink,
        FilesystemObjects, SymlinkTarget,
    },
    object::inode_leaf::{InodeKind as CanonicalKind, InodeMetadata, InodeValue},
    AuthenticatedObjects, ConstructionCapacities, ConstructionPolicy, ContentError, ContentResult,
    FileView, FinalizedConsumer, ObjectId,
};
use layerfs_overlay::{IndexedOperationRecordScope, Inode, InodeKind};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::time::Instant;

/// Exact inputs of every value this attempt builds. Canonical bytes come only
/// from Content's constructors; this adapter selects their inputs.
pub(crate) struct Builder<'s, 'a, P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> {
    pub provider: &'a P,
    pub shared: &'s Shared<'a, P>,
    pub facts: Facts,
    /// The immutable base at exactly the reader's root.
    pub base: &'s BaseView,
    /// The same base under the operation's route, for each file's own binding.
    pub workspace: &'s Workspace,
    pub policy: ConstructionPolicy,
    pub capacities: &'s ConstructionCapacities,
    pub objects: &'s dyn AuthenticatedObjects,
    /// Where fresh files' digests are recorded; None records nothing.
    pub digests: Option<&'a dyn FileDigests>,
}
fn canonical(kind: InodeKind) -> CanonicalKind {
    match kind {
        InodeKind::File => CanonicalKind::RegularFile,
        InodeKind::Directory => CanonicalKind::Directory,
        InodeKind::Symlink => CanonicalKind::Symlink,
    }
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> Builder<'_, '_, P> {
    /// `stored` is the immutable base record of a live base inode and None for
    /// a fresh one; `reuse` is a saved root holding a fresh file's exact
    /// bytes; `predecessor` is the file a rename put a fresh file in place
    /// of. Base roots are kept except a file's constructed content and
    /// the patched metadata; the supplied count is derived again by Content.
    #[allow(clippy::too_many_arguments)]
    pub fn value(
        &self,
        inode: &Inode,
        stored: Option<InodeValue>,
        fresh: bool,
        reuse: Option<ObjectId>,
        predecessor: Option<u64>,
        consumer: &mut dyn FinalizedConsumer,
        timing: &TimingScope<'_, Active>,
    ) -> ContentResult<InodeValue> {
        let root = inode.serial == self.facts.root_serial;
        let kind = canonical(inode.kind);
        if root && kind != CanonicalKind::Directory {
            return Err(ContentError::InvalidRecord("captured root inode kind"));
        }
        let stored = match (fresh, stored) {
            (true, _) => None,
            (false, Some(stored)) if stored.kind == kind => Some(stored),
            (false, Some(_)) => return Err(ContentError::InvalidRecord("captured inode kind")),
            (false, None) => {
                return Err(ContentError::InvalidRecord(
                    "captured inode creation binding",
                ))
            }
        };
        let content_root = match (inode.kind, stored, reuse) {
            // A fresh file whose bytes a saved root already holds reuses
            // that object: neither read nor built again (#364).
            (InodeKind::File, None, Some(root)) => {
                add(&mut self.shared.borrow_mut().work.files_reused, 1);
                root
            }
            (InodeKind::File, _, _) => self.file(
                inode.serial,
                stored.is_none(),
                predecessor,
                consumer,
                timing,
            )?,
            (InodeKind::Symlink, None, _) => self.symlink(inode.serial, consumer)?,
            // Content builds a fresh directory's page from its header and
            // replaces this root; the zero identity names no object.
            (InodeKind::Directory, None, _) => ObjectId::from_bytes(&[0; 32])?,
            (_, Some(stored), _) => stored.content_root,
        };
        let live = PortableMetadata {
            mode: u32::from(inode.mode),
            mtime_seconds: inode.mtime_seconds,
            mtime_nanoseconds: inode.mtime_nanoseconds,
        };
        // An inode whose live facts all match its base row keeps the row
        // exactly - form and stored mtime included - so an mtime-only capture
        // is not a change (STORAGE_BATCH_SPEC section 4.8).
        if let Some(stored) = stored {
            if let Some(base) = self.unchanged(inode, root, kind, content_root, stored, live)? {
                // The live Workspace still shows its own time until unmount
                // (§4.8): the row outlives the install that keeps the base.
                if (base.mtime_seconds, base.mtime_nanoseconds)
                    != (live.mtime_seconds, live.mtime_nanoseconds)
                {
                    let kept = self
                        .provider
                        .keep_live_row(self.facts.reader, inode.serial)
                        .map_err(|original| self.shared.borrow_mut().original(original))?;
                    if kept {
                        add(&mut self.shared.borrow_mut().work.live_rows_kept, 1);
                    }
                }
                return Ok(stored);
            }
        }
        let metadata_root = self.metadata(
            kind,
            live,
            stored.map(|stored| stored.metadata_root),
            consumer,
        )?;
        Ok(InodeValue {
            kind,
            namespace_ref_count: if root { 0 } else { inode.nlink },
            content_root,
            metadata_root,
        })
    }
    /// The base row's portable metadata when every value-bearing fact of
    /// `inode` equals that row: content root (and, for a symlink, the
    /// decoded target), directory entries (no header change record), mode
    /// and namespace count. The stored mtime is deliberately not part of the
    /// check.
    fn unchanged(
        &self,
        inode: &Inode,
        root: bool,
        kind: CanonicalKind,
        content_root: ObjectId,
        stored: InodeValue,
        live: PortableMetadata,
    ) -> ContentResult<Option<PortableMetadata>> {
        if content_root != stored.content_root {
            return Ok(None);
        }
        if kind == CanonicalKind::Directory {
            let header = self.shared.borrow_mut().get(key(HEADER, inode.serial))?;
            let changed = header
                .map(|raw| read_number(&raw, "captured namespace header"))
                .transpose()?
                .unwrap_or(0);
            if changed != 0 {
                return Ok(None);
            }
        }
        if stored.namespace_ref_count != (if root { 0 } else { inode.nlink }) {
            return Ok(None);
        }
        let portable = read_portable(
            self.objects,
            stored.metadata_root,
            kind,
            &mut AttributeReadWork::default(),
        )?;
        Ok((portable.mode == live.mode).then_some(portable))
    }
    /// The existing changed-file constructor under its own file scope. It
    /// returns the base root when the content is unchanged. A fresh file has
    /// no base, so it takes the direct route with no edit records. A failure
    /// keeps the file's whole custody. A fresh file is offered its recorded
    /// predecessor as a delta base.
    fn file(
        &self,
        serial: u64,
        fresh: bool,
        predecessor: Option<u64>,
        consumer: &mut dyn FinalizedConsumer,
        timing: &TimingScope<'_, Active>,
    ) -> ContentResult<ObjectId> {
        self.shared.borrow().ready()?;
        let started = Instant::now();
        let scope = IndexedOperationRecordScope {
            owner: self.facts.operation,
            file_scope: serial,
        };
        let attempt = if fresh {
            CapturedFileEdits::construct_new(
                self.workspace,
                self.provider,
                self.facts.reader,
                serial,
                scope,
                predecessor,
                self.policy,
                self.capacities,
                consumer,
                timing.child("file"),
            )
        } else {
            let prepared = match CapturedFileEdits::prepare(
                self.workspace,
                self.provider,
                self.facts.reader,
                serial,
                scope,
            ) {
                Ok(prepared) => prepared,
                Err(custody) => return Err(self.shared.borrow_mut().file(custody)),
            };
            prepared.construct(self.policy, self.capacities, consumer, timing.child("file"))
        };
        {
            let mut owner = self.shared.borrow_mut();
            add(&mut owner.work.file_ns, nanos(started));
            add(&mut owner.work.file_bind_ns, attempt.custody.work.bind_ns);
            add(
                &mut owner.work.file_source_ns,
                attempt.custody.work.source_ns,
            );
        }
        match attempt.result {
            Ok(file) => {
                let mut owner = self.shared.borrow_mut();
                add(&mut owner.work.files_constructed, 1);
                add(
                    &mut owner.work.file_edge_windows,
                    attempt.custody.work.edge_windows,
                );
                add(
                    &mut owner.work.file_edge_read_bytes,
                    attempt.custody.work.edge_read_bytes,
                );
                add(
                    &mut owner.work.file_edge_retained_bytes,
                    attempt.custody.work.edge_retained_bytes,
                );
                if attempt.custody.file.as_ref().map(FileView::root) == Some(file.root) {
                    add(&mut owner.work.files_unchanged, 1);
                }
                Ok(file.root)
            }
            Err(original) => {
                self.shared.borrow_mut().file(attempt.custody);
                Err(original)
            }
        }
    }
    fn symlink(
        &self,
        serial: u64,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<ObjectId> {
        self.shared.borrow().ready()?;
        let target = self
            .provider
            .captured_symlink(self.facts.reader, serial)
            .map_err(|original| self.shared.borrow_mut().original(original))?;
        let mut objects = FilesystemObjects::new(self.objects, consumer);
        let root = emit_symlink(&mut objects, SymlinkTarget::new(target)?)?;
        add(&mut self.shared.borrow_mut().work.symlinks, 1);
        Ok(root)
    }
    /// Mode and mtime in Content's inline form. A stored root whose attribute
    /// tree holds any key beyond the two portable ones keeps the root form,
    /// patched so every key this row does not carry keeps its stored root.
    fn metadata(
        &self,
        kind: CanonicalKind,
        value: PortableMetadata,
        stored: Option<InodeMetadata>,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<InodeMetadata> {
        let Some(InodeMetadata::Root(root)) = stored else {
            let mut owner = self.shared.borrow_mut();
            add(&mut owner.work.metadata_built, 1);
            return Ok(InodeMetadata::Inline(value));
        };
        let (mut count, mut other) = (0usize, false);
        visit_keys_counted(
            self.objects,
            root,
            &mut AttributePatchWork::default(),
            |key, _| {
                count += 1;
                other |= key.domain() != "portable" || !matches!(key.key(), b"mode" | b"mtime");
                Ok(())
            },
        )?;
        if !other && count <= 2 {
            let mut owner = self.shared.borrow_mut();
            add(&mut owner.work.metadata_patched, 1);
            return Ok(InodeMetadata::Inline(value));
        }
        let mut objects = FilesystemObjects::new(self.objects, consumer);
        let root = portable_root(self.objects, &mut objects, kind, value, Some(root))?;
        let mut owner = self.shared.borrow_mut();
        add(&mut owner.work.metadata_patched, 1);
        Ok(InodeMetadata::Root(root))
    }
}
