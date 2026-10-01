use rusqlite::{params, Connection, OptionalExtension};
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
    pub end: i64,
    pub revision: i64,
    pub callbacks: i64,
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
        db.execute_batch("PRAGMA journal_mode=MEMORY;PRAGMA synchronous=OFF;PRAGMA temp_store=MEMORY;PRAGMA cache_size=-2048;PRAGMA mmap_size=0;CREATE TABLE inodes(id INTEGER PRIMARY KEY,kind INTEGER,mode INTEGER,size INTEGER,seconds INTEGER,nanos INTEGER,root BLOB,dirty INTEGER);CREATE TABLE names(parent INTEGER,name BLOB,ino INTEGER,PRIMARY KEY(parent,name)) WITHOUT ROWID;CREATE TABLE sources(id INTEGER PRIMARY KEY AUTOINCREMENT);CREATE TABLE extents(ino INTEGER,start INTEGER,end INTEGER,source INTEGER,offset INTEGER,PRIMARY KEY(ino,start)) WITHOUT ROWID;CREATE TABLE handles(id INTEGER PRIMARY KEY AUTOINCREMENT,ino INTEGER,flags INTEGER);").map_err(|e|e.to_string())?;
        db.execute("INSERT INTO inodes VALUES(1,2,493,0,0,0,NULL,1)", [])
            .map_err(|e| e.to_string())?;
        Ok(Self {
            db,
            sources: path.join("sources"),
            next: start,
            end: start + 512,
            revision: 1,
            callbacks: 0,
        })
    }
    pub fn node(&self, id: i64) -> Result<Node, String> {
        self.db
            .query_row(
                "SELECT kind,mode,size,seconds,nanos,root,dirty FROM inodes WHERE id=?1",
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
    pub fn create_node(
        &mut self,
        parent: i64,
        name: &[u8],
        kind: u8,
        mode: u32,
    ) -> Result<Node, String> {
        if self.node(parent)?.kind != 2 {
            return Err("ENOTDIR".into());
        }
        if name.is_empty() || name.len() > 255 || name.contains(&0) || name.contains(&b'/') {
            return Err("EINVAL".into());
        }
        if self.lookup(parent, name)?.is_some() {
            return Err("EEXIST".into());
        }
        let count: i64 = self
            .db
            .query_row("SELECT count(*) FROM inodes", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let bindings: i64 = self
            .db
            .query_row(
                "SELECT count(*) FROM names WHERE parent=?1",
                [parent],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if count >= 512 || bindings >= 512 || self.next >= self.end {
            return Err("ENOSPC".into());
        }
        let id = self.next;
        self.next += 1;
        let (sec, nano) = now();
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO inodes VALUES(?1,?2,?3,0,?4,?5,NULL,1)",
            params![id, kind, mode & 0o777, sec, nano],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO names VALUES(?1,?2,?3)",
            params![parent, name, id],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("UPDATE inodes SET dirty=1 WHERE id=?1", [parent])
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
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
            "UPDATE inodes SET size=?2,seconds=?3,nanos=?4,dirty=1 WHERE id=?1",
            params![id, size, sec, nano],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.revision += 1;
        Ok(())
    }
    pub fn read(&self, id: i64, at: i64, out: &mut [u8]) -> Result<usize, String> {
        let size = self.node(id)?.size;
        if at >= size {
            return Ok(0);
        }
        let n = (size - at).min(out.len() as i64) as usize;
        out[..n].fill(0);
        let end = at + n as i64;
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
        }
        Ok(n)
    }
    pub fn unlink(&mut self, parent: i64, name: &[u8], directory: bool) -> Result<(), String> {
        let id = self.lookup(parent, name)?.ok_or("ENOENT")?;
        let n = self.node(id)?;
        if (n.kind == 2) != directory {
            return Err(if directory { "ENOTDIR" } else { "EISDIR" }.into());
        }
        if directory {
            let count: i64 = self
                .db
                .query_row("SELECT count(*) FROM names WHERE parent=?1", [id], |r| {
                    r.get(0)
                })
                .map_err(|e| e.to_string())?;
            if count != 0 {
                return Err("ENOTEMPTY".into());
            }
        }
        self.db
            .execute(
                "DELETE FROM names WHERE parent=?1 AND name=?2",
                params![parent, name],
            )
            .map_err(|e| e.to_string())?;
        self.db
            .execute("UPDATE inodes SET dirty=1 WHERE id=?1", [parent])
            .map_err(|e| e.to_string())?;
        self.revision += 1;
        Ok(())
    }
    pub fn live_ids(&self) -> Result<Vec<i64>, String> {
        let mut q = self
            .db
            .prepare("SELECT id FROM inodes WHERE id=1 OR id IN(SELECT ino FROM names) ORDER BY id")
            .map_err(|e| e.to_string())?;
        let rows = q.query_map([], |r| r.get(0)).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
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
