use super::{CandidateWrite, TrustedCandidate};
use crate::error::{map_sqlite_error, EngineError, EngineResult};
use crate::object::{
    core_store_error, put_canonical_object_on_connection,
    with_authenticated_canonical_on_connection, with_read_canonical_on_connection,
};
use layerfs_core::inode::InodeId;
use layerfs_core::logical::InodeMutation;
use layerfs_core::object::access::ObjectStore;
use layerfs_core::CanonicalPath;
use layerfs_core::{CoreError, ObjectId};
use rusqlite::params;
use std::io::Read;

impl CandidateWrite<'_> {
    pub fn trusted_replace_file<R>(
        &mut self,
        root: ObjectId,
        path: &CanonicalPath,
        input: R,
        initialize: (InodeId, ObjectId),
    ) -> EngineResult<TrustedCandidate>
    where
        R: Read,
    {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::replace_file(self, root, path, input, |_| {
                Ok(initialize)
            })?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_replace_range<R: Read>(
        &mut self,
        root: ObjectId,
        path: &CanonicalPath,
        start: u64,
        delete_len: u64,
        replacement: R,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::replace_range(
                self,
                root,
                path,
                start,
                delete_len,
                replacement,
            )?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_create_directory(
        &mut self,
        root: ObjectId,
        path: &CanonicalPath,
        inode: InodeId,
        metadata_root: ObjectId,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::create_directory(
                self,
                root,
                path,
                inode,
                metadata_root,
            )?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_create_symlink(
        &mut self,
        root: ObjectId,
        path: &CanonicalPath,
        inode: InodeId,
        target: Vec<u8>,
        metadata_root: ObjectId,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::create_symlink(
                self,
                root,
                path,
                inode,
                target,
                metadata_root,
            )?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_hard_link(
        &mut self,
        root: ObjectId,
        source: &CanonicalPath,
        target: &CanonicalPath,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::hard_link(self, root, source, target)?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_rename(
        &mut self,
        root: ObjectId,
        from: &CanonicalPath,
        to: &CanonicalPath,
        source_parent_metadata_root: ObjectId,
        target_parent_metadata_root: ObjectId,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::rename(
                self,
                root,
                from,
                to,
                source_parent_metadata_root,
                target_parent_metadata_root,
            )?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_remove_path(
        &mut self,
        root: ObjectId,
        path: &CanonicalPath,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::remove_path(self, root, path)?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn trusted_apply_inode_mutations(
        &mut self,
        root: ObjectId,
        mutations: impl IntoIterator<Item = InodeMutation>,
    ) -> EngineResult<TrustedCandidate> {
        Ok(TrustedCandidate {
            candidate: layerfs_core::logical::apply_inode_mutations(self, root, mutations)?,
            store_id: self.engine.store_id,
            writer_id: self.writer_id,
        })
    }

    pub fn allocate_inode_id(&mut self) -> EngineResult<InodeId> {
        let serial = self
            .connection
            .query_row(
                "SELECT next_inode_serial FROM layerfs_authority WHERE authority_id = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        let serial =
            u64::try_from(serial).map_err(|_| EngineError::InvalidRecord("inode serial"))?;
        let next = serial.checked_add(1).ok_or(EngineError::CounterOverflow)?;
        self.connection
            .execute(
                "UPDATE layerfs_authority SET next_inode_serial = ?1 WHERE authority_id = 1",
                params![i64::try_from(next).map_err(|_| EngineError::CounterOverflow)?],
            )
            .map_err(map_sqlite_error)?;
        Ok(InodeId::allocate(self.engine.store_id, serial))
    }
}

impl ObjectStore for CandidateWrite<'_> {
    fn get(&self, id: ObjectId) -> Result<Vec<u8>, CoreError> {
        with_read_canonical_on_connection(
            self.engine,
            &self.connection,
            id,
            true,
            true,
            |_, bytes| Ok(bytes.to_vec()),
        )
        .map_err(core_store_error)
    }

    fn with_authenticated_canonical<T, F>(&self, id: ObjectId, callback: F) -> Result<T, CoreError>
    where
        F: FnOnce(&[u8]) -> Result<T, CoreError>,
    {
        with_authenticated_canonical_on_connection(
            self.engine,
            &self.connection,
            id,
            true,
            true,
            |_, bytes| callback(bytes).map_err(EngineError::Core),
        )
        .map_err(core_store_error)
    }

    fn put(&mut self, canonical: &[u8]) -> Result<ObjectId, CoreError> {
        put_canonical_object_on_connection(self.engine, &self.connection, canonical)
            .map(|(id, _)| id)
            .map_err(core_store_error)
    }
}
