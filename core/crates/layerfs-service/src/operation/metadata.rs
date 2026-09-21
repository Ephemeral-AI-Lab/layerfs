//! Two portable patches over an existing metadata root, under one save owner.
use super::{failure::content, read::id};
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

pub(crate) fn update(
    provider: &StoreProvider<'_>,
    request: &Request,
    consumer: &mut dyn FinalizedConsumer,
    deadline: Instant,
) -> Result<(Root, u64), Failure> {
    let Operation::UpdatePortableMetadata {
        base,
        kind,
        mode,
        mtime_seconds,
        mtime_nanoseconds,
    } = &request.operation
    else {
        return Err(Code::Unsupported.into());
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
        deadline.check()?;
        let kind = InodeKind::from_code(*kind)?;
        let metadata = PortableMetadata {
            mode: *mode,
            mtime_seconds: *mtime_seconds,
            mtime_nanoseconds: *mtime_nanoseconds,
        };
        metadata.validate(kind)?;
        // A base must already be an attribute tree with valid typed fields.
        // The requested inode kind governs mode validation; no inode is attached here.
        read_portable(&reader, id(base), kind, &mut AttributeReadWork::default())?;
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
        let mut output = Output {
            inner: consumer,
            deadline: &deadline,
        };
        let mut objects = FilesystemObjects::new(&reader, &mut output);
        apply_patches(&reader, &mut objects, id(base), &patches)
            .map(|(root, _)| (*root.as_bytes(), 0))
    })();
    // C1 has no deadline variant. The operation-owned flag distinguishes this
    // expiry from provider I/O without inspecting diagnostics. The save owner
    // separately gives a retained C2 failure precedence over this result.
    if deadline.expired.get() {
        Err(Code::Deadline.into())
    } else {
        result.map_err(content)
    }
}
