//! Trusted publisher locators. This owner has no object-storage I/O capability.
use crate::objects::{Locator, Locators, ROLES};
use layerfs_content::ObjectId;
use rusqlite::{params, types::Value, Connection, OptionalExtension};
use std::{collections::BTreeMap, path::Path, sync::Mutex, time::Duration};
pub const APPLICATION_ID: u32 = u32::from_be_bytes(*b"P6L2");
pub const USER_VERSION: u32 = 2;
pub struct LocatorDb {
    db: Mutex<Connection>,
    writable: bool,
}
impl LocatorDb {
    pub fn open_read_only(path: &Path) -> Result<Self, String> {
        let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA query_only=ON;PRAGMA cache_size=-2048;PRAGMA mmap_size=0")
            .map_err(|e| e.to_string())?;
        db.busy_timeout(Duration::ZERO).map_err(|e| e.to_string())?;
        let app: u32 = db
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let version: u32 = db
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let names=db.prepare("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").map_err(|e|e.to_string())?.query_map([],|r|r.get::<_,String>(0)).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        if app != APPLICATION_ID || version != USER_VERSION || names != ["objects", "packs"] {
            return Err("readonly locator schema/profile".into());
        }
        Ok(Self {
            db: Mutex::new(db),
            writable: false,
        })
    }
    pub fn create(path: &Path) -> Result<Self, String> {
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.busy_timeout(Duration::ZERO).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=MEMORY;PRAGMA synchronous=OFF;PRAGMA temp_store=MEMORY;PRAGMA cache_size=-2048;PRAGMA mmap_size=0;PRAGMA foreign_keys=ON;CREATE TABLE packs(id INTEGER PRIMARY KEY AUTOINCREMENT,digest BLOB NOT NULL UNIQUE CHECK(length(digest)=32));CREATE TABLE objects(id BLOB PRIMARY KEY CHECK(length(id)=32),role INTEGER NOT NULL,length INTEGER NOT NULL CHECK(length>0),pack_id INTEGER NOT NULL REFERENCES packs(id),group_no INTEGER NOT NULL,record_no INTEGER NOT NULL,UNIQUE(pack_id,group_no,record_no)) WITHOUT ROWID;").map_err(|e|e.to_string())?;
        db.pragma_update(None, "application_id", APPLICATION_ID)
            .map_err(|e| e.to_string())?;
        db.pragma_update(None, "user_version", USER_VERSION)
            .map_err(|e| e.to_string())?;
        Ok(Self {
            db: Mutex::new(db),
            writable: true,
        })
    }
}
fn lookup(db: &Connection, ids: &[ObjectId]) -> Result<Vec<Option<Locator>>, String> {
    let marks = vec!["?"; ids.len()].join(",");
    let sql=format!("SELECT o.id,o.role,o.length,p.digest,o.group_no,o.record_no,p.id FROM objects o JOIN packs p ON p.id=o.pack_id WHERE o.id IN ({marks})");
    let values: Vec<_> = ids
        .iter()
        .map(|id| Value::Blob(id.as_bytes().to_vec()))
        .collect();
    let mut q = db.prepare_cached(&sql).map_err(|e| e.to_string())?;
    let mut rows = q
        .query(rusqlite::params_from_iter(values))
        .map_err(|e| e.to_string())?;
    let mut found = BTreeMap::new();
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let id: Vec<u8> = row.get(0).map_err(|e| e.to_string())?;
        let role: i64 = row.get(1).map_err(|e| e.to_string())?;
        let role = usize::try_from(role).map_err(|_| "stored locator role")?;
        let length: i64 = row.get(2).map_err(|e| e.to_string())?;
        let length = usize::try_from(length).map_err(|_| "stored locator length")?;
        let pack: Vec<u8> = row.get(3).map_err(|e| e.to_string())?;
        let group: u32 = row.get(4).map_err(|e| e.to_string())?;
        let record: u32 = row.get(5).map_err(|e| e.to_string())?;
        let pack_id: i64 = row.get(6).map_err(|e| e.to_string())?;
        let pack_id = u64::try_from(pack_id).map_err(|_| "stored pack ID")?;
        if pack_id == 0 || length == 0 {
            return Err("stored locator invariant".into());
        }
        let id = ObjectId::from_bytes(&id).map_err(|e| e.to_string())?;
        found.insert(
            id,
            Locator {
                id,
                role: *ROLES.get(role).ok_or("locator role")?,
                length,
                pack: pack.try_into().map_err(|_| "pack digest width")?,
                group,
                record,
                pack_id,
            },
        );
    }
    Ok(ids.iter().map(|id| found.get(id).cloned()).collect())
}
impl Locators for LocatorDb {
    fn lookup_many(&self, ids: &[ObjectId]) -> Result<Vec<Option<Locator>>, String> {
        if ids.is_empty() || ids.len() > 128 {
            return Err("lookup page admission".into());
        }
        let db = self.db.lock().map_err(|_| "locator owner")?;
        lookup(&db, ids)
    }
    fn register_many(&self, rows: &[Locator]) -> Result<Vec<Locator>, String> {
        if !self.writable {
            return Err("readonly locator mutation refused".into());
        }
        if rows.is_empty()
            || rows.len() > 128
            || rows.iter().any(|r| {
                r.pack_id != 0
                    || r.length == 0
                    || r.length > crate::read_window::BYTES
                    || r.group >= 256
                    || r.record >= 8191
                    || !ROLES.contains(&r.role)
            })
        {
            return Err("registration page admission".into());
        }
        let total = rows.iter().try_fold(0usize, |n, r| {
            n.checked_add(r.length).ok_or("registration byte overflow")
        })?;
        if total > crate::read_window::BYTES {
            return Err("registration byte admission".into());
        }
        let mut first: BTreeMap<ObjectId, &Locator> = BTreeMap::new();
        for row in rows {
            if let Some(old) = first.insert(row.id, row) {
                if old.role != row.role || old.length != row.length {
                    return Err("same-page immutable identity mismatch".into());
                }
            }
        }
        let mut db = self.db.lock().map_err(|_| "locator owner")?;
        let ids: Vec<_> = rows.iter().map(|r| r.id).collect();
        let old = lookup(&db, &ids)?;
        for (input, saved) in rows.iter().zip(&old) {
            if saved
                .as_ref()
                .is_some_and(|r| r.role != input.role || r.length != input.length)
            {
                return Err("registered immutable identity mismatch".into());
            }
        }
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let mut normalized = Vec::with_capacity(rows.len());
        let mut inserted: BTreeMap<ObjectId, Locator> = BTreeMap::new();
        for (row, old) in rows.iter().zip(old) {
            if let Some(saved) = inserted.get(&row.id) {
                normalized.push(saved.clone());
                continue;
            }
            if let Some(saved) = old {
                normalized.push(saved);
                continue;
            }
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT id FROM packs WHERE digest=?1",
                    [row.pack.as_slice()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let pack_id = match existing {
                Some(id) => id,
                None => {
                    tx.execute(
                        "INSERT INTO packs(digest) VALUES(?1)",
                        [row.pack.as_slice()],
                    )
                    .map_err(|e| e.to_string())?;
                    tx.last_insert_rowid()
                }
            };
            let role = ROLES
                .iter()
                .position(|r| *r == row.role)
                .ok_or("locator role")?;
            tx.execute(
                "INSERT INTO objects VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    row.id.as_bytes().as_slice(),
                    role as i64,
                    row.length as i64,
                    pack_id,
                    row.group,
                    row.record
                ],
            )
            .map_err(|e| e.to_string())?;
            let mut saved = row.clone();
            saved.pack_id = pack_id as u64;
            inserted.insert(saved.id, saved.clone());
            normalized.push(saved);
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(normalized)
    }
}
