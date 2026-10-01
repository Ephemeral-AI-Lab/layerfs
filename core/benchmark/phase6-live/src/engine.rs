use layerfs_content::{AuthenticatedObjects, ObjectId};
use rusqlite::{params, Connection, OptionalExtension};
use std::{cell::Cell, sync::Arc};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub struct Engine {
    pub db: Connection,
    pub sources: PathBuf,
    pub next: i64,
    pub nodes: i64,
    pub end: i64,
    pub revision: i64,
    pub callbacks: i64,
    pub reader: Option<Arc<dyn AuthenticatedObjects + Send + Sync>>,
    pub reads: Cell<SourceReads>,
    pub row_queries: Cell<crate::prepared::QueryCounts>,
}
/// Logical source bytes served by the engine; not physical cache or all C1 reads.
#[derive(Clone, Copy, Default, Debug)]
pub struct SourceReads {
    pub base: u64,
    pub local: u64,
}
impl SourceReads {
    pub fn since(self, before: Self) -> Self {
        Self {
            base: self.base - before.base,
            local: self.local - before.local,
        }
    }
}
#[derive(Clone)]
pub struct Node {
    pub id: i64,
    pub kind: u8,
    pub mode: u32,
    pub size: i64,
    pub seconds: i64,
    pub nanos: u32,
    pub root: Option<Vec<u8>>,
    pub dirty: bool,
    pub is_new: bool,
    pub base_size: i64,
    pub base_visible: i64,
    pub children: i64,
    pub parent: i64,
    pub links: i64,
    pub subdirs: i64,
}
pub fn now() -> (i64, u32) {
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    (t.as_secs() as i64, t.subsec_nanos())
}
impl Engine {
    pub fn create(path: &Path, start: i64) -> Result<Self, String> {
        std::fs::create_dir_all(path.join("sources")).map_err(|e| e.to_string())?;
        let db = Connection::open(path.join("metadata.sqlite")).map_err(|e| e.to_string())?;
        db.busy_timeout(Duration::ZERO).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=MEMORY;PRAGMA synchronous=OFF;PRAGMA temp_store=MEMORY;PRAGMA cache_size=-2048;PRAGMA mmap_size=0;CREATE TABLE inodes(id INTEGER PRIMARY KEY,kind INTEGER,mode INTEGER,size INTEGER,seconds INTEGER,nanos INTEGER,root BLOB,dirty INTEGER,is_new INTEGER,base_size INTEGER,base_visible INTEGER,children INTEGER,parent INTEGER,links INTEGER,subdirs INTEGER);CREATE INDEX dirty_inodes ON inodes(id) WHERE dirty=1;CREATE TABLE names(parent INTEGER,name BLOB,ino INTEGER,PRIMARY KEY(parent,name)) WITHOUT ROWID;CREATE INDEX names_ino ON names(ino);CREATE TABLE changed_names(parent INTEGER,name BLOB,ino INTEGER,PRIMARY KEY(parent,name)) WITHOUT ROWID;CREATE TABLE prepared(id INTEGER PRIMARY KEY,kind INTEGER,content BLOB,metadata BLOB,new_pos INTEGER);CREATE TABLE edits(ordinal INTEGER PRIMARY KEY,start INTEGER,end INTEGER,len INTEGER,source INTEGER,offset INTEGER);CREATE TABLE sources(id INTEGER PRIMARY KEY AUTOINCREMENT);CREATE TABLE extents(ino INTEGER,start INTEGER,end INTEGER,source INTEGER,offset INTEGER,PRIMARY KEY(ino,start)) WITHOUT ROWID;CREATE TABLE handles(id INTEGER PRIMARY KEY AUTOINCREMENT,ino INTEGER,flags INTEGER);").map_err(|e|e.to_string())?;
        db.execute(
            "INSERT INTO inodes VALUES(1,2,493,0,0,0,NULL,0,0,0,0,0,1,1,0)",
            [],
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            db,
            sources: path.join("sources"),
            next: start,
            nodes: 1,
            end: start + 512,
            revision: 1,
            callbacks: 0,
            reader: None,
            reads: Cell::new(SourceReads::default()),
            row_queries: Cell::new(crate::prepared::QueryCounts::default()),
        })
    }
    pub fn node(&self, id: i64) -> Result<Node, String> {
        self.db
            .query_row(
                "SELECT kind,mode,size,seconds,nanos,root,dirty,is_new,base_size,base_visible,children,parent,links,subdirs FROM inodes WHERE id=?1",
                [id],
                |r| {
                    Ok(Node {
                        id,
                        kind: r.get(0)?,
                        mode: r.get(1)?,
                        size: r.get(2)?,
                        seconds: r.get(3)?,
                        nanos: r.get(4)?,
                        root: r.get(5)?,
                        dirty: r.get(6)?,
                        is_new: r.get(7)?,
                        base_size: r.get(8)?,
                        base_visible: r.get(9)?,
                        children: r.get(10)?,
                        parent: r.get(11)?,
                        links: r.get(12)?,
                        subdirs: r.get(13)?,
                    })
                },
            )
            .map_err(|e| e.to_string())
    }
    pub fn lookup(&self, parent: i64, name: &[u8]) -> Result<Option<i64>, String> {
        self.db
            .query_row(
                "SELECT ino FROM names WHERE parent=?1 AND name=?2",
                params![parent, name],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
    }
    pub fn directory_entries(&self, id: i64) -> Result<Vec<(i64, Vec<u8>, u8)>, String> {
        let mut q=self.db.prepare_cached("SELECT names.ino,names.name,inodes.kind FROM names JOIN inodes ON inodes.id=names.ino WHERE names.parent=?1 ORDER BY names.name LIMIT 512").map_err(|e|e.to_string())?;
        let rows = q
            .query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }
    pub fn create_node(
        &mut self,
        parent: i64,
        name: &[u8],
        kind: u8,
        mode: u32,
    ) -> Result<Node, String> {
        if self.node(parent)?.links == 0 {
            return Err("ENOENT".into());
        }
        if self.node(parent)?.kind != 2 {
            return Err("ENOTDIR".into());
        }
        if name.is_empty()
            || name == b"."
            || name == b".."
            || name.len() > 255
            || name.contains(&0)
            || name.contains(&b'/')
        {
            return Err("EINVAL".into());
        }
        if self.lookup(parent, name)?.is_some() {
            return Err("EEXIST".into());
        }
        if self.nodes >= 512 || self.node(parent)?.children >= 512 || self.next >= self.end {
            return Err("ENOSPC".into());
        }
        let id = self.next;
        self.next += 1;
        let (sec, nano) = now();
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO inodes VALUES(?1,?2,?3,0,?4,?5,NULL,1,1,0,0,0,?6,1,0)",
            params![id, kind, mode & 0o777, sec, nano, parent],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO names VALUES(?1,?2,?3)",
            params![parent, name, id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO changed_names VALUES(?1,?2,?3) ON CONFLICT(parent,name) DO UPDATE SET ino=excluded.ino", params![parent,name,id]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE inodes SET dirty=1,children=children+1,subdirs=subdirs+?4,seconds=?2,nanos=?3 WHERE id=?1",
            params![parent, sec, nano,i64::from(kind==2)],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.nodes += 1;
        self.revision += 1;
        self.node(id)
    }
    pub fn open(&mut self, id: i64, flags: i32) -> Result<i64, String> {
        self.node(id)?;
        let count: i64 = self
            .db
            .query_row("SELECT count(*) FROM handles", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if count >= 256 {
            return Err("EMFILE".into());
        }
        self.db
            .execute(
                "INSERT INTO handles(ino,flags) VALUES(?1,?2)",
                params![id, flags],
            )
            .map_err(|e| e.to_string())?;
        Ok(self.db.last_insert_rowid())
    }
    pub fn handle(&self, handle: i64, id: i64, write: bool) -> Result<(), String> {
        let flags: i32 = self
            .db
            .query_row(
                "SELECT flags FROM handles WHERE id=?1 AND ino=?2",
                params![handle, id],
                |r| r.get(0),
            )
            .map_err(|_| "EBADF")?;
        if write && (flags & libc::O_ACCMODE) == libc::O_RDONLY {
            return Err("EBADF".into());
        }
        Ok(())
    }
    pub fn close(&mut self, handle: i64, id: i64) -> Result<(), String> {
        let n = self
            .db
            .execute(
                "DELETE FROM handles WHERE id=?1 AND ino=?2",
                params![handle, id],
            )
            .map_err(|e| e.to_string())?;
        if n != 1 {
            return Err("EBADF".into());
        }
        Ok(())
    }
    pub fn write(&mut self, id: i64, start: i64, bytes: &[u8]) -> Result<(), String> {
        let n = self.node(id)?;
        if n.kind != 1 {
            return Err("EISDIR".into());
        }
        if bytes.len() > 128 * 1024 {
            return Err("E2BIG".into());
        }
        if start < 0 {
            return Err("EINVAL".into());
        }
        let end = start
            .checked_add(bytes.len() as i64)
            .filter(|n| *n >= 0)
            .ok_or("EFBIG")?;
        if bytes.is_empty() {
            return Ok(());
        }
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO sources DEFAULT VALUES", [])
            .map_err(|e| e.to_string())?;
        let source = tx.last_insert_rowid();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.sources.join(source.to_string()))
            .map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        drop(file);
        let left:Option<(i64,i64,i64,i64)>=tx.query_row("SELECT start,end,source,offset FROM extents WHERE ino=?1 AND start<?2 AND end>?2 ORDER BY start DESC LIMIT 1",params![id,start],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?;
        let right:Option<(i64,i64,i64,i64)>=tx.query_row("SELECT start,end,source,offset FROM extents WHERE ino=?1 AND start<?2 AND end>?2 ORDER BY start DESC LIMIT 1",params![id,end],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional().map_err(|e|e.to_string())?;
        tx.execute(
            "DELETE FROM extents WHERE ino=?1 AND start<?2 AND end>?3",
            params![id, end, start],
        )
        .map_err(|e| e.to_string())?;
        if let Some((a, _, s, o)) = left {
            tx.execute(
                "INSERT INTO extents VALUES(?1,?2,?3,?4,?5)",
                params![id, a, start, s, o],
            )
            .map_err(|e| e.to_string())?;
        }
        if let Some((a, z, s, o)) = right {
            tx.execute(
                "INSERT INTO extents VALUES(?1,?2,?3,?4,?5)",
                params![id, end, z, s, o + end - a],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute(
            "INSERT INTO extents VALUES(?1,?2,?3,?4,0)",
            params![id, start, end, source],
        )
        .map_err(|e| e.to_string())?;
        let (sec, nano) = now();
        tx.execute(
            "UPDATE inodes SET size=max(size,?2),seconds=?3,nanos=?4,dirty=1 WHERE id=?1",
            params![id, end, sec, nano],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.revision += 1;
        Ok(())
    }
    pub fn truncate(&mut self, id: i64, size: i64) -> Result<(), String> {
        if size < 0 {
            return Err("EFBIG".into());
        }
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "DELETE FROM extents WHERE ino=?1 AND start>=?2",
            params![id, size],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE extents SET end=?2 WHERE ino=?1 AND start<?2 AND end>?2",
            params![id, size],
        )
        .map_err(|e| e.to_string())?;
        let (sec, nano) = now();
        tx.execute(
            "UPDATE inodes SET size=?2,base_visible=min(base_visible,?2),seconds=?3,nanos=?4,dirty=1 WHERE id=?1",
            params![id, size, sec, nano],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.revision += 1;
        Ok(())
    }
    pub fn read(&self, id: i64, at: i64, out: &mut [u8]) -> Result<usize, String> {
        if at < 0 {
            return Err("EINVAL".into());
        }
        let node = self.node(id)?;
        let size = node.size;
        if at >= size {
            return Ok(0);
        }
        let n = (size - at).min(out.len() as i64) as usize;
        out[..n].fill(0);
        let end = at + n as i64;
        if let Some(root) = node.root.as_deref() {
            let stop = end.min(node.base_visible);
            if at < stop {
                let reader = self
                    .reader
                    .as_deref()
                    .ok_or("immutable base reader unavailable")?;
                let root = ObjectId::from_bytes(root).map_err(|e| e.to_string())?;
                let (result, _) = layerfs_telemetry::timer::Timing::disabled("base.read", |t| {
                    let mut sink = &mut out[..(stop - at) as usize];
                    layerfs_content::read_range(
                        reader,
                        root,
                        at as u64..stop as u64,
                        &mut sink,
                        t.child("range"),
                    )
                });
                result.map_err(|e| e.to_string())?;
                let mut reads = self.reads.get();
                reads.base += (stop - at) as u64;
                self.reads.set(reads);
            }
        }
        let mut statement=self.db.prepare_cached("SELECT start,end,source,offset FROM extents WHERE ino=?1 AND start<?2 AND end>?3 ORDER BY start").map_err(|e|e.to_string())?;
        let mut rows = statement
            .query(params![id, end, at])
            .map_err(|e| e.to_string())?;
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            let start: i64 = r.get(0).map_err(|e| e.to_string())?;
            let stop: i64 = r.get(1).map_err(|e| e.to_string())?;
            let source: i64 = r.get(2).map_err(|e| e.to_string())?;
            let offset: i64 = r.get(3).map_err(|e| e.to_string())?;
            let a = start.max(at);
            let z = stop.min(end);
            let mut file =
                File::open(self.sources.join(source.to_string())).map_err(|e| e.to_string())?;
            file.seek(SeekFrom::Start((offset + a - start) as u64))
                .map_err(|e| e.to_string())?;
            file.read_exact(&mut out[(a - at) as usize..(z - at) as usize])
                .map_err(|e| e.to_string())?;
            let mut reads = self.reads.get();
            reads.local += (z - a) as u64;
            self.reads.set(reads);
        }
        Ok(n)
    }
    pub fn unlink(&mut self, parent: i64, name: &[u8], directory: bool) -> Result<(), String> {
        let id = self.lookup(parent, name)?.ok_or("ENOENT")?;
        let n = self.node(id)?;
        if (n.kind == 2) != directory {
            return Err(if directory { "ENOTDIR" } else { "EISDIR" }.into());
        }
        if directory && n.children != 0 {
            return Err("ENOTEMPTY".into());
        }
        let (sec, nano) = now();
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        tx.execute("UPDATE inodes SET links=links-1 WHERE id=?1", [id])
            .map_err(|e| e.to_string())?;
        tx.execute(
            "DELETE FROM names WHERE parent=?1 AND name=?2",
            params![parent, name],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO changed_names VALUES(?1,?2,NULL) ON CONFLICT(parent,name) DO UPDATE SET ino=NULL",params![parent,name]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE inodes SET dirty=1,children=children-1,subdirs=subdirs-?4,seconds=?2,nanos=?3 WHERE id=?1",
            params![parent, sec, nano,i64::from(n.kind==2)],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.revision += 1;
        Ok(())
    }
    /// Keyset access uses the partial dirty index and indexed live-name membership.
    pub fn next_dirty(&self, after: i64) -> Result<Option<Node>, String> {
        let id: Option<i64> = self.db.query_row(
            "SELECT id FROM inodes WHERE dirty=1 AND id>?1 AND (id=1 OR EXISTS(SELECT 1 FROM names WHERE ino=inodes.id)) ORDER BY id LIMIT 1",
            [after], |r|r.get(0)).optional().map_err(|e|e.to_string())?;
        id.map(|id| self.node(id)).transpose()
    }
    /// The caller holds the mutation lock through capture, publication and install.
    pub fn install_prepared(&mut self) -> Result<(), String> {
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        tx.execute("UPDATE inodes SET root=(SELECT content FROM prepared WHERE id=inodes.id),base_size=size,base_visible=size,dirty=0,is_new=0 WHERE id IN(SELECT id FROM prepared) AND kind=1",[]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE inodes SET dirty=0,is_new=0 WHERE id IN(SELECT id FROM prepared) AND kind=2",
            [],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "DELETE FROM extents WHERE ino IN(SELECT id FROM prepared WHERE kind=1)",
            [],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "DELETE FROM changed_names WHERE parent IN(SELECT id FROM prepared WHERE kind=2)",
            [],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub struct Source<'a> {
    pub engine: &'a Engine,
    pub id: i64,
    pub at: i64,
}
impl Read for Source<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = self
            .engine
            .read(self.id, self.at, out)
            .map_err(std::io::Error::other)?;
        self.at += n as i64;
        Ok(n)
    }
}
