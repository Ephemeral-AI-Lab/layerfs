//! Fresh portable fields or two existing-root patches, under one save owner.
use crate::service::{error::content, read::content::id};
use layerfs_bridge::contract::*;
use layerfs_content::{
    filesystem::attributes::{
        patch::{apply_patches, AttributePatch},
        read::{read_portable, AttributeReadWork},
        AttributeKey, PortableMetadata,
    },
    object::inode_leaf::InodeKind,
    AuthenticatedObjects, ContentError, ContentResult, FilesystemObjects, FinalizedConsumer,
    FinalizedObject, ObjectId,
};
use layerfs_storage::StoreProvider;
use layerfs_telemetry::timer::TimingScope;
use std::{cell::Cell, time::Instant};

struct Deadline {
    end: Instant,
    expired: Cell<bool>,
}
impl Deadline {
    fn check(&self) -> ContentResult<()> {
        if Instant::now() >= self.end {
            self.expired.set(true);
            return Err(ContentError::Io);
        }
        Ok(())
    }
}
struct Reader<'a, 'store> {
    inner: &'a StoreProvider<'store>,
    deadline: &'a Deadline,
}
impl AuthenticatedObjects for Reader<'_, '_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.deadline.check()?;
        let result = self.inner.read_canonical_batch(ids);
        self.deadline.check()?;
        result
    }
    fn read_canonical_batch_scoped(
        &self,
        ids: &[ObjectId],
        scope: TimingScope<'_>,
    ) -> ContentResult<Vec<Vec<u8>>> {
        self.deadline.check()?;
        let result = self.inner.read_canonical_batch_scoped(ids, scope);
        self.deadline.check()?;
        result
    }
}
struct Output<'a> {
    inner: &'a mut dyn FinalizedConsumer,
    deadline: &'a Deadline,
}
impl FinalizedConsumer for Output<'_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.deadline.check()?;
        let result = self.inner.accept(object);
        self.deadline.check()?;
        result
    }
}

pub(crate) fn save(
    provider: &StoreProvider<'_>,
    request: &Request,
    consumer: &mut dyn FinalizedConsumer,
    deadline: Instant,
) -> Result<(Root, u64), Failure> {
    let (base, kind, mode, mtime_seconds, mtime_nanoseconds) = match &request.operation {
        Operation::UpdatePortableMetadata {
            base,
            kind,
            mode,
            mtime_seconds,
            mtime_nanoseconds,
        } => (
            Some(id(base)),
            *kind,
            *mode,
            *mtime_seconds,
            *mtime_nanoseconds,
        ),
        Operation::ConstructPortableMetadata {
            kind,
            mode,
            mtime_seconds,
            mtime_nanoseconds,
        } => (None, *kind, *mode, *mtime_seconds, *mtime_nanoseconds),
        _ => return Err(Code::Unsupported.into()),
    };
    let deadline = Deadline {
        end: deadline,
        expired: Cell::new(false),
    };
    let reader = Reader {
        inner: provider,
        deadline: &deadline,
    };
    let result = (|| {
        deadline.check().map_err(content)?;
        let kind = InodeKind::from_code(kind).map_err(content)?;
        let metadata = PortableMetadata {
            mode,
            mtime_seconds,
            mtime_nanoseconds,
        };
        let mut output = Output {
            inner: consumer,
            deadline: &deadline,
        };
        let mut objects = FilesystemObjects::new(&reader, &mut output);
        let root = match base {
            Some(base) => patch_portable(&mut objects, base, kind, metadata).map_err(content)?,
            None => build_metadata(&mut objects, kind, metadata)?,
        };
        Ok((*root.as_bytes(), 0))
    })();
    // C1 has no deadline variant. The operation-owned flag distinguishes this
    // expiry from provider I/O without inspecting diagnostics. The save owner
    // separately gives a retained C2 failure precedence over this result.
    if deadline.expired.get() {
        Err(Code::Deadline.into())
    } else {
        result
    }
}

/// Updates only portable mode/mtime, preserving all other attributes in the base.
/// Emitted objects remain under the caller's existing save owner.
pub(crate) fn patch_portable(
    objects: &mut FilesystemObjects<'_>,
    base: ObjectId,
    kind: InodeKind,
    metadata: PortableMetadata,
) -> ContentResult<ObjectId> {
    metadata.validate(kind)?;
    let reader = objects.reader();
    read_portable(reader, base, kind, &mut AttributeReadWork::default())?;
    let patches = [
        AttributePatch::Set {
            key: AttributeKey::new("portable".into(), b"mode".to_vec())?,
            value: metadata.mode_bytes(kind)?.to_vec(),
        },
        AttributePatch::Set {
            key: AttributeKey::new("portable".into(), b"mtime".to_vec())?,
            value: metadata.mtime_bytes()?.to_vec(),
        },
    ];
    apply_patches(reader, objects, base, &patches).map(|(root, _)| root)
}

use layerfs_content::filesystem::attributes::{
    build::build_attribute_tree, codec::AttributeEntry, value::emit_value,
};

/// Builds the typed portable fields through the caller's existing save owner.
/// The constructors only emit; they never read these unpublished new objects.
pub(crate) fn build_metadata(
    objects: &mut FilesystemObjects<'_>,
    kind: InodeKind,
    value: PortableMetadata,
) -> Result<ObjectId, Failure> {
    value.validate(kind).map_err(content)?;
    let mode = emit_value(objects, &value.mode_bytes(kind).map_err(content)?).map_err(content)?;
    let mtime = emit_value(objects, &value.mtime_bytes().map_err(content)?).map_err(content)?;
    build_attribute_tree(
        objects,
        vec![
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mode".to_vec()).map_err(content)?,
                value_root: mode,
            }),
            Ok(AttributeEntry {
                key: AttributeKey::new("portable".into(), b"mtime".to_vec()).map_err(content)?,
                value_root: mtime,
            }),
        ]
        .into_iter(),
    )
    .map(|(root, _)| root)
    .map_err(content)
}
