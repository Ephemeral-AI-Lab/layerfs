use super::replay::load_record;
use super::spool::{load_spooled_metadata, MAX_SPOOLED_METADATA_BYTES};
use super::write_spooled_metadata;
use crate::legacy_full::{OperationCounters, VfsError};
use layerfs_core::content::rope::ObjectStore;
use layerfs_core::inode::{inode_table_from_root, InodeId, InodeKind, InodeRecordV1};
use layerfs_core::namespace_codec::encode_inode_record;
use layerfs_core::{CoreError, CoreResult, ObjectId};
use layerfs_materialization::driver::{NativeMetadata, OwnedTempHandle, MAX_NATIVE_XATTR_BYTES};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

struct MemoryTemp {
    inner: Cursor<Vec<u8>>,
    largest_write: usize,
}

impl Read for MemoryTemp {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buffer)
    }
}

impl Write for MemoryTemp {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.largest_write = self.largest_write.max(buffer.len());
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Seek for MemoryTemp {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}

impl OwnedTempHandle for MemoryTemp {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn set_len(&mut self, len: u64) -> layerfs_materialization::driver::Result<()> {
        self.inner.get_mut().resize(len as usize, 0);
        Ok(())
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

#[derive(Default)]
struct TrackingStore {
    objects: BTreeMap<ObjectId, Vec<u8>>,
    gets: Cell<u64>,
    authenticated: Cell<u64>,
}

impl ObjectStore for TrackingStore {
    fn get(&self, id: ObjectId) -> CoreResult<Vec<u8>> {
        self.gets.set(self.gets.get() + 1);
        self.objects
            .get(&id)
            .cloned()
            .ok_or(CoreError::MissingObject)
    }

    fn put(&mut self, bytes: &[u8]) -> CoreResult<ObjectId> {
        let id = ObjectId::for_bytes(bytes);
        self.objects.entry(id).or_insert_with(|| bytes.to_vec());
        Ok(id)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> CoreResult<T>
    where
        F: FnOnce(&[u8]) -> CoreResult<T>,
    {
        self.authenticated.set(self.authenticated.get() + 1);
        let bytes = self.objects.get(&id).ok_or(CoreError::MissingObject)?;
        if ObjectId::for_bytes(bytes) != id {
            return Err(CoreError::IdentityMismatch);
        }
        callback(bytes)
    }
}

#[test]
fn managed_inode_record_uses_the_borrowed_authenticated_route() {
    let mut store = TrackingStore::default();
    let inode = InodeId::allocate([7; 32], 1);
    let record = InodeRecordV1 {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 1,
        content_root: ObjectId::for_bytes(b"content"),
        metadata_root: ObjectId::for_bytes(b"metadata"),
    };
    let record_id = store.put(&encode_inode_record(record).unwrap()).unwrap();
    let table = inode_table_from_root(&mut store, inode, record_id).unwrap();
    store.gets.set(0);
    store.authenticated.set(0);

    assert_eq!(
        load_record(&store, table, inode, &mut OperationCounters::default()).unwrap(),
        record
    );
    assert_eq!(store.gets.get(), 0);
    assert_eq!(store.authenticated.get(), 2);
}

#[test]
fn managed_metadata_spool_streams_the_full_xattr_envelope_and_rejects_one_over() {
    let metadata = NativeMetadata {
        mode: 0o644,
        mtime_seconds: 7,
        mtime_nanoseconds: 8,
        xattrs: layerfs_materialization::driver::NativeXattrs::from_entries(
            (0..1024).map(|index| (format!("x{index:015}").into_bytes(), vec![9; 1008])),
        )
        .unwrap(),
        acl: None,
        bsd_flags: 0,
    };
    let mut spool = MemoryTemp {
        inner: Cursor::new(Vec::new()),
        largest_write: 0,
    };
    let len = write_spooled_metadata(&metadata, &mut spool).unwrap();
    assert_eq!(
        len,
        36 + MAX_NATIVE_XATTR_BYTES as u64 + 6 * metadata.xattrs.len() as u64
    );
    assert!(len > MAX_NATIVE_XATTR_BYTES as u64);
    assert!(spool.largest_write < MAX_NATIVE_XATTR_BYTES);
    assert_eq!(load_spooled_metadata(&mut spool, 0, len).unwrap(), metadata);

    let oversized = layerfs_materialization::driver::NativeXattrs::from_entries(std::iter::once((
        b"x".to_vec(),
        vec![0; MAX_NATIVE_XATTR_BYTES],
    )))
    .and_then(|mut xattrs| xattrs.push(b"y", b"").map(|_| xattrs));
    assert!(matches!(
        oversized,
        Err(layerfs_materialization::driver::DriverError::Unsupported)
    ));

    let mut corrupt = Vec::from(b"LFSMETA1".as_slice());
    corrupt.extend_from_slice(&0o644_u32.to_be_bytes());
    corrupt.extend_from_slice(&0_i64.to_be_bytes());
    corrupt.extend_from_slice(&0_u32.to_be_bytes());
    corrupt.extend_from_slice(&0_u32.to_be_bytes());
    corrupt.extend_from_slice(&u32::MAX.to_be_bytes());
    corrupt.extend_from_slice(&((MAX_NATIVE_XATTR_BYTES + 1) as u32).to_be_bytes());
    let corrupt_len = corrupt.len() as u64;
    let mut corrupt = MemoryTemp {
        inner: Cursor::new(corrupt),
        largest_write: 0,
    };
    assert!(matches!(
        load_spooled_metadata(&mut corrupt, 0, corrupt_len),
        Err(VfsError::InvalidState)
    ));
    assert!(matches!(
        load_spooled_metadata(&mut corrupt, 0, MAX_SPOOLED_METADATA_BYTES + 1),
        Err(VfsError::InvalidState)
    ));
}
