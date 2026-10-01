//! Bounded live enumeration by stable per-name cookies, independent of C1 order.
use crate::engine::Engine;
use rusqlite::params;
pub const PAGE_ROWS: usize = 64;
pub const PAGE_BYTES: usize = 16 * 1024;
#[derive(Debug)]
pub struct Entry {
    pub cookie: i64,
    pub inode: i64,
    pub name: Vec<u8>,
    pub kind: u8,
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Work {
    pub pages: u64,
    pub rows: u64,
    pub peak_rows: usize,
    pub peak_payload_bytes: usize,
    pub peak_owned_bytes: usize,
}
pub const QUERY: &str = "SELECT names.cookie,names.ino,names.name,inodes.kind FROM names JOIN inodes ON inodes.id=names.ino WHERE names.parent=?1 AND names.cookie>?2 ORDER BY names.cookie LIMIT 64";
impl Engine {
    pub fn directory_page(&self, handle: i64, id: i64, after: i64) -> Result<Vec<Entry>, String> {
        self.handle(handle, id, false)?;
        if self.node(id)?.kind != 2 {
            return Err("ENOTDIR".into());
        }
        let high: i64 = self
            .db
            .query_row(
                "SELECT value FROM counters WHERE name='directory'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if after < 0 || after > high {
            return Err("EINVAL".into());
        }
        let mut page = Vec::new();
        page.try_reserve_exact(PAGE_ROWS)
            .map_err(|_| "directory page reservation")?;
        let mut q = self.db.prepare_cached(QUERY).map_err(|e| e.to_string())?;
        let mut rows = q
            .query(params![id, after.max(2)])
            .map_err(|e| e.to_string())?;
        let mut bytes = 0;
        let mut work = self.directory_work.get();
        work.pages += 1;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let entry = Entry {
                cookie: row.get(0).map_err(|e| e.to_string())?,
                inode: row.get(1).map_err(|e| e.to_string())?,
                name: row.get(2).map_err(|e| e.to_string())?,
                kind: row.get(3).map_err(|e| e.to_string())?,
            };
            work.rows += 1;
            if entry.name.is_empty() || entry.name.len() > 255 || entry.cookie <= after.max(2) {
                return Err("directory stored row invariant".into());
            }
            let payload = entry.name.len() + 17;
            if bytes + payload > PAGE_BYTES {
                break;
            }
            bytes += payload;
            page.push(entry);
        }
        let owned = page.capacity() * std::mem::size_of::<Entry>()
            + page.iter().map(|r| r.name.capacity()).sum::<usize>();
        if owned > PAGE_BYTES + PAGE_ROWS * std::mem::size_of::<Entry>() {
            return Err("directory owned byte admission".into());
        }
        work.peak_rows = work.peak_rows.max(page.len());
        work.peak_payload_bytes = work.peak_payload_bytes.max(bytes);
        work.peak_owned_bytes = work.peak_owned_bytes.max(owned);
        self.directory_work.set(work);
        Ok(page)
    }
}
