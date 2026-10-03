//! Legacy SQLite implementation of the engine-independent encoding seam.

use super::{lookup, pool};
use crate::error::{StorageError, StorageResult};
use crate::location::{ObjectLocation, SignatureRow, ValueGroupRow};
use crate::source::Source;
use layerfs_content::ObjectId;
use rusqlite::Connection;

impl Source for Connection {
    fn location(&self, id: ObjectId, ceiling: i64) -> StorageResult<Option<ObjectLocation>> {
        lookup::location(self, id, ceiling)
    }
    fn pack_bytes(&self, pack_id: i64) -> StorageResult<Vec<u8>> {
        lookup::pack_bytes(self, pack_id)
    }
    fn value_group(&self, ordinal: u32) -> StorageResult<Option<ValueGroupRow>> {
        pool::group_for(self, ordinal)
    }
    fn value_groups(
        &self,
        from: Option<u32>,
        visit: &mut dyn FnMut(ValueGroupRow) -> StorageResult<()>,
    ) -> StorageResult<()> {
        pool::for_each_group(self, from, visit)
    }
    fn window_start(&self) -> StorageResult<u32> {
        pool::window_start(self)
    }
    fn signatures(&self) -> StorageResult<Vec<SignatureRow>> {
        let mut statement = self.prepare("SELECT c.stamp, c.object_id, c.signature FROM content_signatures c JOIN saves s USING(save_id), temp.layerfs_read_scope r WHERE c.save_id = r.save_id OR s.publication <= r.publication ORDER BY c.stamp")?;
        let mut cursor = statement.query([])?;
        let mut rows = Vec::new();
        while let Some(row) = cursor.next()? {
            if rows.len() >= 8192 {
                return Err(StorageError::Integrity("content index over slot bound"));
            }
            let stamp = u64::try_from(row.get::<_, i64>(0)?)
                .map_err(|_| StorageError::Integrity("content index stamp range"))?;
            let raw: Vec<u8> = row.get(2)?;
            rows.push(SignatureRow {
                slot: (stamp.saturating_sub(1) % 8192) as usize,
                stamp,
                object_id: ObjectId::from_bytes(&row.get::<_, Vec<u8>>(1)?)?,
                signature: raw
                    .try_into()
                    .map_err(|_| StorageError::Integrity("content index signature width"))?,
            });
        }
        Ok(rows)
    }
    fn write_signatures(&self, rows: &[SignatureRow]) -> StorageResult<usize> {
        let mut statement = self.prepare_cached(
            "INSERT OR REPLACE INTO content_signatures (slot, stamp, object_id, signature, save_id) \
             VALUES (?1, ?2, ?3, ?4, (SELECT save_id FROM temp.layerfs_read_scope))",
        )?;
        let mut written = 0;
        for row in rows {
            written += statement.execute(rusqlite::params![
                row.slot as i64,
                row.stamp as i64,
                row.object_id.as_bytes().as_slice(),
                row.signature.as_slice()
            ])?;
        }
        Ok(written)
    }
}
