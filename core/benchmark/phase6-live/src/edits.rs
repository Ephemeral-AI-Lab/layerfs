//! Replayable immediate-base edits over final local spans, never WRITE history.
use crate::engine::Engine;
use layerfs_content::{ContentError, ContentResult, Edit, EditSequence, EditSource};
use rusqlite::{params, OptionalExtension};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};

pub struct Edits<'a> {
    engine: &'a Engine,
    id: i64,
    base: u64,
    final_size: u64,
    count: usize,
}
struct Row {
    start: i64,
    end: i64,
    len: i64,
    source: Option<i64>,
    offset: i64,
}
impl<'a> Edits<'a> {
    pub fn prepare(engine: &'a Engine, id: i64) -> Result<Self, String> {
        let node = engine.node(id)?;
        if node.root.is_none()
            || node.base_visible > node.base_size
            || node.base_visible > node.size
        {
            return Err("invalid immediate file base".into());
        }
        engine.source_ready()?;
        crate::sql_windows::clear(&engine.db, crate::sql_windows::Table::Edits)?;
        let mut q = engine.db.prepare_cached("SELECT start,min(end,?2),source,offset FROM extents WHERE ino=?1 AND start<?2 ORDER BY start").map_err(|e|e.to_string())?;
        let mut rows = q
            .query(params![id, node.base_visible])
            .map_err(|e| e.to_string())?;
        let mut count = 0;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let start: i64 = row.get(0).map_err(|e| e.to_string())?;
            let end: i64 = row.get(1).map_err(|e| e.to_string())?;
            let source: i64 = row.get(2).map_err(|e| e.to_string())?;
            let offset: i64 = row.get(3).map_err(|e| e.to_string())?;
            if count >= 512 {
                return Err("edit spool admission 512".into());
            }
            engine
                .db
                .execute(
                    "INSERT INTO edits VALUES(?1,?2,?3,?4,?5,?6)",
                    params![count as i64, start, end, end - start, source, offset],
                )
                .map_err(|e| e.to_string())?;
            count += 1;
        }
        if node.base_visible != node.base_size || node.size != node.base_size {
            if count >= 512 {
                return Err("edit spool admission 512".into());
            }
            engine
                .db
                .execute(
                    "INSERT INTO edits VALUES(?1,?2,?3,?4,NULL,?2)",
                    params![
                        count as i64,
                        node.base_visible,
                        node.base_size,
                        node.size - node.base_visible
                    ],
                )
                .map_err(|e| e.to_string())?;
            count += 1;
        }
        Ok(Self {
            engine,
            id,
            base: node.base_size as u64,
            final_size: node.size as u64,
            count,
        })
    }
    fn row(&self, index: usize) -> ContentResult<Row> {
        let ordinal = i64::try_from(index).map_err(|_| ContentError::Io)?;
        self.engine
            .db
            .query_row(
                "SELECT start,end,len,source,offset FROM edits WHERE ordinal=?1",
                [ordinal],
                |r| {
                    Ok(Row {
                        start: r.get(0)?,
                        end: r.get(1)?,
                        len: r.get(2)?,
                        source: r.get(3)?,
                        offset: r.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|_| ContentError::Io)?
            .ok_or(ContentError::InvalidRecord("edit ordinal"))
    }
}
impl EditSequence for Edits<'_> {
    fn base_len(&self) -> u64 {
        self.base
    }
    fn final_len(&self) -> u64 {
        self.final_size
    }
    fn len(&self) -> usize {
        self.count
    }
    fn edit_at(&self, index: usize) -> ContentResult<Edit> {
        let r = self.row(index)?;
        Ok(Edit::new(r.start as u64, r.end as u64, r.len as u64))
    }
}
impl EditSource for Edits<'_> {
    fn replacement_len(&self, index: usize) -> u64 {
        self.row(index).map_or(0, |r| r.len as u64)
    }
    fn read_at(&self, index: usize, offset: u64, out: &mut [u8]) -> ContentResult<usize> {
        let row = self.row(index)?;
        if offset >= row.len as u64 {
            return Ok(0);
        }
        let n = (row.len as u64 - offset).min(out.len() as u64) as usize;
        let at = (row.offset as u64)
            .checked_add(offset)
            .ok_or(ContentError::LengthOverflow)?;
        match row.source {
            Some(source) => {
                let mut f = File::open(self.engine.sources.join(source.to_string()))
                    .map_err(|_| ContentError::Io)?;
                f.seek(SeekFrom::Start(at)).map_err(|_| ContentError::Io)?;
                f.read_exact(&mut out[..n]).map_err(|_| ContentError::Io)?;
                let mut reads = self.engine.reads.get();
                reads.local += n as u64;
                self.engine.reads.set(reads);
                Ok(n)
            }
            None => self
                .engine
                .read(
                    self.id,
                    i64::try_from(at).map_err(|_| ContentError::Io)?,
                    &mut out[..n],
                )
                .map_err(|_| ContentError::Io),
        }
    }
}
