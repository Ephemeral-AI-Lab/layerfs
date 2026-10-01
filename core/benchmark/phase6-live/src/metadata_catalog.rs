//! Single-owner locator SQL; payload authentication happens before the short transaction.
use crate::{
    minio::Minio,
    objects::{Locator, Locators, ROLES},
    read_window,
};
use layerfs_content::ObjectId;
use rusqlite::{params, types::Value, Connection, OptionalExtension};
use std::{collections::BTreeMap, path::Path, sync::Mutex, time::Duration};
pub struct LocatorDb {
    db: Mutex<Connection>,
    pub s3: Minio,
}
impl LocatorDb {
    pub fn certify_file(&self, root: ObjectId) -> Result<crate::file_facts::Work, String> {
        let db = self.db.lock().map_err(|_| "locator owner")?;
        crate::file_facts::certify_file(&db, root)
    }
    pub fn create(path: &Path, s3: Minio) -> Result<Self, String> {
        let db = Connection::open(path).map_err(|e| e.to_string())?;
        db.execute_batch("PRAGMA journal_mode=MEMORY;PRAGMA synchronous=OFF;PRAGMA temp_store=MEMORY;PRAGMA cache_size=-2048;PRAGMA mmap_size=0;PRAGMA foreign_keys=ON;CREATE TABLE packs(id INTEGER PRIMARY KEY AUTOINCREMENT,digest BLOB NOT NULL UNIQUE);CREATE TABLE objects(id BLOB PRIMARY KEY,role INTEGER NOT NULL,length INTEGER NOT NULL,pack_id INTEGER NOT NULL REFERENCES packs(id),group_no INTEGER NOT NULL,record_no INTEGER NOT NULL) WITHOUT ROWID;").map_err(|e|e.to_string())?;
        db.execute_batch(crate::file_facts::SCHEMA)
            .map_err(|e| e.to_string())?;
        db.busy_timeout(Duration::ZERO).map_err(|e| e.to_string())?;
        Ok(Self {
            db: Mutex::new(db),
            s3,
        })
    }
}
impl Locators for LocatorDb {
    fn lookup_many(&self, ids: &[ObjectId]) -> Result<Vec<Option<Locator>>, String> {
        if ids.is_empty() || ids.len() > 128 {
            return Err("lookup page admission".into());
        }
        let db = self.db.lock().map_err(|_| "locator owner")?;
        let marks = vec!["?"; ids.len()].join(",");
        let sql=format!("SELECT o.id,o.role,o.length,p.digest,o.group_no,o.record_no,p.id FROM objects o JOIN packs p ON p.id=o.pack_id WHERE o.id IN ({marks})");
        let values: Vec<_> = ids
            .iter()
            .map(|id| Value::Blob(id.as_bytes().to_vec()))
            .collect();
        let mut query = db.prepare_cached(&sql).map_err(|e| e.to_string())?;
        let mut rows = query
            .query(rusqlite::params_from_iter(values))
            .map_err(|e| e.to_string())?;
        let mut found = BTreeMap::new();
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let id: Vec<u8> = row.get(0).map_err(|e| e.to_string())?;
            let role: i64 = row.get(1).map_err(|e| e.to_string())?;
            let length: i64 = row.get(2).map_err(|e| e.to_string())?;
            let pack: Vec<u8> = row.get(3).map_err(|e| e.to_string())?;
            let group: i64 = row.get(4).map_err(|e| e.to_string())?;
            let record: i64 = row.get(5).map_err(|e| e.to_string())?;
            let pack_id: i64 = row.get(6).map_err(|e| e.to_string())?;
            let id = ObjectId::from_bytes(&id).map_err(|e| e.to_string())?;
            if pack_id <= 0 || length <= 0 {
                return Err("stored locator invariant".into());
            }
            found.insert(
                id,
                Locator {
                    id,
                    role: *ROLES.get(role as usize).ok_or("locator role")?,
                    length: usize::try_from(length).map_err(|_| "length")?,
                    pack: pack.try_into().map_err(|_| "pack digest width")?,
                    group: u32::try_from(group).map_err(|_| "group")?,
                    record: u32::try_from(record).map_err(|_| "record")?,
                    pack_id: pack_id as u64,
                },
            );
        }
        Ok(ids.iter().map(|id| found.get(id).cloned()).collect())
    }
    fn register_many(&self, rows: &[Locator]) -> Result<Vec<Locator>, String> {
        if rows.is_empty()
            || rows.len() > 128
            || rows
                .iter()
                .any(|r| r.pack_id != 0 || r.length == 0 || r.group >= 256 || r.record >= 8191)
        {
            return Err("registration page admission".into());
        }
        let canonical = read_window::read(&self.s3, rows)?;
        let mut first = BTreeMap::new();
        for (index, row) in rows.iter().enumerate() {
            if let Some(previous) = first.insert(row.id, index) {
                if rows[previous].role != row.role
                    || rows[previous].length != row.length
                    || canonical[previous] != canonical[index]
                {
                    return Err("same-page exact collision".into());
                }
            }
        }
        let ids: Vec<_> = rows.iter().map(|r| r.id).collect();
        let old = self.lookup_many(&ids)?;
        let existing: Vec<_> = old.iter().flatten().cloned().collect();
        let old_bytes = read_window::read(&self.s3, &existing)?;
        let mut old_values = old_bytes.into_iter();
        for (index, row) in old.iter().enumerate() {
            if let Some(row) = row {
                if row.role != rows[index].role
                    || row.length != rows[index].length
                    || old_values.next().ok_or("old result cardinality")? != canonical[index]
                {
                    return Err("registered exact CAS mismatch".into());
                }
            }
        }
        let mut db = self.db.lock().map_err(|_| "locator owner")?;
        let tx = db.transaction().map_err(|e| e.to_string())?;
        let mut normalized = Vec::with_capacity(rows.len());
        let mut inserted: BTreeMap<ObjectId, Locator> = BTreeMap::new();
        for (index, (row, old)) in rows.iter().zip(old).enumerate() {
            if let Some(saved) = inserted.get(&row.id) {
                normalized.push(saved.clone());
                continue;
            }
            if let Some(old) = old {
                normalized.push(old);
                continue;
            }
            let existing_id: Option<i64> = tx
                .query_row(
                    "SELECT id FROM packs WHERE digest=?1",
                    [row.pack.as_slice()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let pack_id = match existing_id {
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
            let role = ROLES.iter().position(|r| *r == row.role).ok_or("role")? as i64;
            tx.execute(
                "INSERT INTO objects VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    row.id.as_bytes().as_slice(),
                    role,
                    row.length as i64,
                    pack_id,
                    row.group,
                    row.record
                ],
            )
            .map_err(|e| e.to_string())?;
            crate::file_facts::record(&tx, row.id, row.role, &canonical[index])?;
            let mut saved = row.clone();
            saved.pack_id = pack_id as u64;
            inserted.insert(saved.id, saved.clone());
            normalized.push(saved);
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(normalized)
    }
}
