//! One captured inode row to one typed value: file root, symlink target, metadata.
use super::records::{add, Facts, Shared};
use crate::{
    BaseView, CapturedFileEdits, OverlayCapturedNamespace, OverlayOperationRecords, Workspace,
};
use layerfs_content::{
    filesystem::{
        attributes::{
            apply_patches, build_attribute_tree, emit_value, keys::PORTABLE_KEYS, AttributeEntry,
            AttributeKey, AttributePatch, PortableMetadata,
        },
        limits::PORTABLE_ATTRIBUTE_DOMAIN,
        symlink::emit_symlink,
        FilesystemObjects, SymlinkTarget,
    },
    object::inode_leaf::{InodeKind as CanonicalKind, InodeValue},
    AuthenticatedObjects, ConstructionCapacities, ConstructionPolicy, ContentError, ContentResult,
    FileView, FinalizedConsumer, ObjectId,
};
use layerfs_overlay::{IndexedOperationRecordScope, Inode, InodeKind};
use layerfs_telemetry::timer::{Active, TimingScope};

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
}
fn canonical(kind: InodeKind) -> CanonicalKind {
    match kind {
        InodeKind::File => CanonicalKind::RegularFile,
        InodeKind::Directory => CanonicalKind::Directory,
        InodeKind::Symlink => CanonicalKind::Symlink,
    }
}
/// The mode and mtime keys from Content's own export of its portable grammar,
/// confirmed by its predicates: this adapter spells no attribute name itself.
fn portable_keys() -> ContentResult<(AttributeKey, AttributeKey)> {
    let [mode, mtime] = PORTABLE_KEYS;
    let key = |name: &[u8]| AttributeKey::new(PORTABLE_ATTRIBUTE_DOMAIN.to_owned(), name.to_vec());
    let (mode, mtime) = (key(mode)?, key(mtime)?);
    if !mode.is_mode() || !mtime.is_mtime() {
        return Err(ContentError::InvalidRecord(
            "captured portable attribute keys",
        ));
    }
    Ok((mode, mtime))
}
impl<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized> Builder<'_, '_, P> {
    /// `stored` is the immutable base record of a live base inode and None for
    /// a fresh one. Base roots are kept except a file's constructed content and
    /// the patched metadata; the supplied count is derived again by Content.
    pub fn value(
        &self,
        inode: &Inode,
        stored: Option<InodeValue>,
        fresh: bool,
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
        let content_root = match (inode.kind, stored) {
            (InodeKind::File, _) => self.file(inode.serial, consumer, timing)?,
            (InodeKind::Symlink, None) => self.symlink(inode.serial, consumer)?,
            // Content builds a fresh directory's page from its header and
            // replaces this root; the zero identity names no object.
            (InodeKind::Directory, None) => ObjectId::from_bytes(&[0; 32])?,
            (_, Some(stored)) => stored.content_root,
        };
        let metadata_root = self.metadata(
            kind,
            inode,
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
    /// The existing changed-file constructor under its own file scope. It
    /// takes the new-file route itself and returns the base root when the
    /// content is unchanged. A failure keeps the file's whole custody.
    fn file(
        &self,
        serial: u64,
        consumer: &mut dyn FinalizedConsumer,
        timing: &TimingScope<'_, Active>,
    ) -> ContentResult<ObjectId> {
        self.shared.borrow().ready()?;
        let scope = IndexedOperationRecordScope {
            owner: self.facts.operation,
            file_scope: serial,
        };
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
        let attempt =
            prepared.construct(self.policy, self.capacities, consumer, timing.child("file"));
        match attempt.result {
            Ok(file) => {
                let mut owner = self.shared.borrow_mut();
                add(&mut owner.work.files_constructed, 1);
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
    /// Mode and mtime in Content's portable grammar. A base tree is patched so
    /// every key this row does not carry keeps its stored value root.
    fn metadata(
        &self,
        kind: CanonicalKind,
        inode: &Inode,
        stored: Option<ObjectId>,
        consumer: &mut dyn FinalizedConsumer,
    ) -> ContentResult<ObjectId> {
        let value = PortableMetadata {
            mode: u32::from(inode.mode),
            mtime_seconds: inode.mtime_seconds,
            mtime_nanoseconds: inode.mtime_nanoseconds,
        };
        let (mode, mtime) = (value.mode_bytes(kind)?, value.mtime_bytes()?);
        let (mode_key, mtime_key) = portable_keys()?;
        let mut objects = FilesystemObjects::new(self.objects, consumer);
        let root = match stored {
            Some(stored) => {
                let patches = [
                    AttributePatch::Set {
                        key: mode_key,
                        value: mode.to_vec(),
                    },
                    AttributePatch::Set {
                        key: mtime_key,
                        value: mtime.to_vec(),
                    },
                ];
                apply_patches(self.objects, &mut objects, stored, &patches)?.0
            }
            None => {
                let entries = [
                    AttributeEntry {
                        key: mode_key,
                        value_root: emit_value(&mut objects, &mode)?,
                    },
                    AttributeEntry {
                        key: mtime_key,
                        value_root: emit_value(&mut objects, &mtime)?,
                    },
                ];
                build_attribute_tree(&mut objects, entries.into_iter().map(Ok))?.0
            }
        };
        let mut owner = self.shared.borrow_mut();
        add(
            if stored.is_some() {
                &mut owner.work.metadata_patched
            } else {
                &mut owner.work.metadata_built
            },
            1,
        );
        Ok(root)
    }
}
