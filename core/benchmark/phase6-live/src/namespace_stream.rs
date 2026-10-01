//! Keyset streams over one captured SQL operation, one row at a time.
use crate::engine::Engine;
use layerfs_content::{
    inode_leaf::{InodeKind, InodeValue},
    ContentError, ContentResult, ObjectId, PathName,
};
use rusqlite::{params, OptionalExtension};
pub struct Names<'a> {
    pub engine: &'a Engine,
    pub parent: i64,
    pub after: Vec<u8>,
    pub done: bool,
}
impl Iterator for Names<'_> {
    type Item = ContentResult<(PathName, Option<u64>)>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let result = (|| {
            let mut count = self.engine.row_queries.get();
            count.directory_lookups += 1;
            self.engine.row_queries.set(count);
            let mut q=self.engine.db.prepare_cached("SELECT name,ino FROM changed_names WHERE parent=?1 AND name>?2 ORDER BY name LIMIT 1").map_err(|_|ContentError::Io)?;
            let row: Option<(Vec<u8>, Option<i64>)> = q
                .query_row(params![self.parent, &self.after], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()
                .map_err(|_| ContentError::Io)?;
            row.map(|(name, id)| {
                self.after = name.clone();
                let mut count = self.engine.row_queries.get();
                count.name_rows += 1;
                self.engine.row_queries.set(count);
                let name =
                    PathName::new(std::str::from_utf8(&name).map_err(|_| {
                        ContentError::InvalidRecord("non UTF8 C1 name unsupported")
                    })?)?;
                Ok((name, id.map(|id| id as u64)))
            })
            .transpose()
        })();
        match result {
            Ok(Some(row)) => Some(Ok(row)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}
struct StoredValue {
    id: i64,
    kind: u8,
    content: Option<Vec<u8>>,
    meta: Option<Vec<u8>>,
    links: i64,
}
pub struct Values<'a> {
    pub engine: &'a Engine,
    pub after: i64,
    pub done: bool,
}
impl Iterator for Values<'_> {
    type Item = ContentResult<(u64, Option<InodeValue>)>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let result = (|| {
            let mut count = self.engine.row_queries.get();
            count.inode_lookups += 1;
            self.engine.row_queries.set(count);
            let mut q=self.engine.db.prepare_cached("SELECT p.id,p.kind,p.content,p.metadata,i.links FROM prepared p JOIN inodes i ON i.id=p.id WHERE p.id>?1 ORDER BY p.id LIMIT 1").map_err(|_|ContentError::Io)?;
            let row: Option<StoredValue> = q
                .query_row([self.after], |r| {
                    Ok(StoredValue {
                        id: r.get(0)?,
                        kind: r.get(1)?,
                        content: r.get(2)?,
                        meta: r.get(3)?,
                        links: r.get(4)?,
                    })
                })
                .optional()
                .map_err(|_| ContentError::Io)?;
            row.map(
                |StoredValue {
                     id,
                     kind,
                     content,
                     meta,
                     links,
                 }| {
                    self.after = id;
                    let value = if kind == 0 {
                        None
                    } else {
                        Some(InodeValue {
                            kind: InodeKind::from_code(kind)?,
                            namespace_ref_count: if id == 1 { 0 } else { links as u64 },
                            content_root: ObjectId::from_bytes(&content.ok_or(ContentError::Io)?)?,
                            metadata_root: ObjectId::from_bytes(&meta.ok_or(ContentError::Io)?)?,
                        })
                    };
                    Ok((id as u64, value))
                },
            )
            .transpose()
        })();
        match result {
            Ok(Some(row)) => Some(Ok(row)),
            Ok(None) => {
                self.done = true;
                None
            }
            Err(e) => {
                self.done = true;
                Some(Err(e))
            }
        }
    }
}
