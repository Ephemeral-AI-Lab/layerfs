//! Operation-owned SQL rows with indexed lookup and replayable keyset cursors.
use crate::engine::Engine;
use layerfs_content::{
    filesystem::{
        rows::RowSource, DirectoryRowSource, FilesystemRootId, InodeRowSource, PreparedRows,
        SerialRowSource,
    },
    inode_leaf::{InodeKind, InodeValue},
    *,
};
use rusqlite::{params, OptionalExtension};

#[derive(Clone, Copy, Default, Debug)]
pub struct QueryCounts {
    pub directory_lookups: u64,
    pub inode_lookups: u64,
    pub fresh_lookups: u64,
    pub name_rows: u64,
}
impl QueryCounts {
    pub fn since(self, before: Self) -> Self {
        Self {
            directory_lookups: self.directory_lookups - before.directory_lookups,
            inode_lookups: self.inode_lookups - before.inode_lookups,
            fresh_lookups: self.fresh_lookups - before.fresh_lookups,
            name_rows: self.name_rows - before.name_rows,
        }
    }
}
pub struct Prepared<'a> {
    pub engine: &'a Engine,
    pub root: FilesystemRootId,
    pub inode_scope: InodeScope,
    pub counts: (usize, usize, usize),
}
fn io<T>(r: rusqlite::Result<T>) -> ContentResult<T> {
    r.map_err(|_| ContentError::Io)
}
impl Prepared<'_> {
    fn next(&self, after: u64, predicate: &str) -> ContentResult<Option<u64>> {
        let mut s = io(self.engine.db.prepare_cached(&format!(
            "SELECT id FROM prepared WHERE id>?1 AND {predicate} ORDER BY id LIMIT 1"
        )))?;
        io(s.query_row([after as i64], |r| r.get::<_, i64>(0))
            .optional())
        .map(|id| id.map(|id| id as u64))
    }
}
struct Cursor<'a, 'b> {
    rows: &'a Prepared<'b>,
    after: u64,
}
impl DirectoryRowSource for Cursor<'_, '_> {
    fn next_row(&mut self) -> ContentResult<Option<DirectoryUpdate>> {
        match self.rows.next(self.after, "kind=2")? {
            Some(id) => {
                self.after = id;
                self.rows.directory_for(id)
            }
            None => Ok(None),
        }
    }
}
impl InodeRowSource for Cursor<'_, '_> {
    fn next_row(&mut self) -> ContentResult<Option<InodeUpdate>> {
        match self.rows.next(self.after, "1")? {
            Some(id) => {
                self.after = id;
                Ok(Some(InodeUpdate {
                    serial: id,
                    value: self
                        .rows
                        .value_for(id)?
                        .ok_or(ContentError::InvalidRecord("prepared value"))?,
                }))
            }
            None => Ok(None),
        }
    }
}
impl SerialRowSource for Cursor<'_, '_> {
    fn next_row(&mut self) -> ContentResult<Option<u64>> {
        let id = self.rows.next(self.after, "new_pos IS NOT NULL")?;
        if let Some(id) = id {
            self.after = id;
        }
        Ok(id)
    }
}
impl RowSource for Prepared<'_> {
    fn directory_rows(&self) -> usize {
        self.counts.0
    }
    fn inode_rows(&self) -> usize {
        self.counts.1
    }
    fn new_rows(&self) -> usize {
        self.counts.2
    }
    fn directories(&self) -> ContentResult<Box<dyn DirectoryRowSource + '_>> {
        Ok(Box::new(Cursor {
            rows: self,
            after: 0,
        }))
    }
    fn inodes(&self) -> ContentResult<Box<dyn InodeRowSource + '_>> {
        Ok(Box::new(Cursor {
            rows: self,
            after: 0,
        }))
    }
    fn new_inodes(&self) -> ContentResult<Box<dyn SerialRowSource + '_>> {
        Ok(Box::new(Cursor {
            rows: self,
            after: 0,
        }))
    }
    fn directory_for(&self, parent: u64) -> ContentResult<Option<DirectoryUpdate>> {
        let mut count = self.engine.row_queries.get();
        count.directory_lookups += 1;
        self.engine.row_queries.set(count);
        let exists: Option<i64> = io(self
            .engine
            .db
            .query_row(
                "SELECT id FROM prepared WHERE id=?1 AND kind=2",
                [parent as i64],
                |r| r.get(0),
            )
            .optional())?;
        if exists.is_none() {
            return Ok(None);
        }
        let mut q = io(self
            .engine
            .db
            .prepare_cached("SELECT name,ino FROM changed_names WHERE parent=?1 ORDER BY name"))?;
        let mut rows = io(q.query([parent as i64]))?;
        let mut changes = Vec::new();
        while let Some(row) = io(rows.next())? {
            if changes.len() >= 512 {
                return Err(ContentError::ObjectLimitExceeded {
                    limit: 512,
                    actual: changes.len() + 1,
                });
            }
            let name: Vec<u8> = io(row.get(0))?;
            let ino: Option<i64> = io(row.get(1))?;
            let name = PathName::new(
                std::str::from_utf8(&name)
                    .map_err(|_| ContentError::InvalidRecord("non UTF8 C1 name unsupported"))?,
            )?;
            changes.push((name, ino.map(|id| id as u64)));
            let mut count = self.engine.row_queries.get();
            count.name_rows += 1;
            self.engine.row_queries.set(count);
        }
        Ok(Some(DirectoryUpdate { parent, changes }))
    }
    fn value_for(&self, serial: u64) -> ContentResult<Option<InodeValue>> {
        let mut count = self.engine.row_queries.get();
        count.inode_lookups += 1;
        self.engine.row_queries.set(count);
        let row: Option<(u8, Vec<u8>, Vec<u8>)> = io(self
            .engine
            .db
            .query_row(
                "SELECT kind,content,metadata FROM prepared WHERE id=?1",
                [serial as i64],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional())?;
        row.map(|(kind, content, meta)| {
            Ok(InodeValue {
                kind: InodeKind::from_code(kind)?,
                namespace_ref_count: u64::from(serial != 1),
                content_root: ObjectId::from_bytes(&content)?,
                metadata_root: ObjectId::from_bytes(&meta)?,
            })
        })
        .transpose()
    }
    fn new_position(&self, serial: u64) -> ContentResult<Option<usize>> {
        let mut count = self.engine.row_queries.get();
        count.fresh_lookups += 1;
        self.engine.row_queries.set(count);
        let pos: Option<i64> = io(self
            .engine
            .db
            .query_row(
                "SELECT new_pos FROM prepared WHERE id=?1 AND new_pos IS NOT NULL",
                params![serial as i64],
                |r| r.get(0),
            )
            .optional())?;
        Ok(pos.map(|p| p as usize))
    }
}
impl PreparedRows for Prepared<'_> {
    fn base(&self) -> Option<FilesystemRootId> {
        Some(self.root)
    }
    fn scope(&self) -> InodeScope {
        self.inode_scope
    }
    fn root_serial(&self) -> u64 {
        1
    }
    fn resources(&self) -> FilesystemResources {
        FilesystemResources::default()
    }
}
