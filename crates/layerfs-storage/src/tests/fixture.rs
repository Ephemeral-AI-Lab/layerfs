use super::*;
use layerfs_core::{encode_object, Object};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn test_path() -> PathBuf {
    let id = NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "layerfs-storage-{}-{id}.sqlite",
        std::process::id()
    ))
}

pub(super) struct RollbackFailure;

impl CommitDispatch for RollbackFailure {
    fn commit(&self, connection: &Connection) -> rusqlite::Result<()> {
        connection.execute_batch("COMMIT")
    }

    fn rollback(&self, _connection: &Connection) -> rusqlite::Result<()> {
        Err(rusqlite::Error::InvalidQuery)
    }
}

pub(super) fn bytes_object(value: &[u8]) -> (ObjectId, Vec<u8>) {
    let bytes =
        encode_object(&Object::bytes(value.to_vec()).expect("test object")).expect("test encoding");
    (ObjectId::for_bytes(&bytes), bytes)
}

pub(super) fn empty_directory() -> (ObjectId, Vec<u8>) {
    let bytes = encode_object(&Object::directory(Vec::new()).expect("test directory"))
        .expect("test encoding");
    (ObjectId::for_bytes(&bytes), bytes)
}

pub(super) fn root(number: u8, directory_object: ObjectId, parent: Option<ObjectId>) -> RootRecord {
    RootRecord {
        id: ObjectId::for_bytes(&[number]),
        directory_object,
        parent,
    }
}
