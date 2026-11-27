//! Exact bounded raw-record deletion after the last operation owner releases.
use crate::{db::unsigned, sql, Overlay, OverlayResult, StatementKind, OPERATION_RECORD_BYTES};

struct Record {
    file_scope: Vec<u8>,
    kind: i64,
    key: Vec<u8>,
    bytes: u64,
}

impl Overlay {
    pub(crate) fn delete_indexed_operation_record(
        &self,
        operation: i64,
    ) -> OverlayResult<(u64, u64)> {
        let decode = |r: &rusqlite::Row<'_>| {
            Ok(Record {
                file_scope: r.get(0)?,
                kind: r.get(1)?,
                key: r.get(2)?,
                bytes: unsigned(r, 3)?,
            })
        };
        let records = self.query(
            StatementKind::Reclaim,
            sql::INDEXED_OPERATION_RECORD_RECLAIM_OPERATION,
            &[&operation],
            16,
            decode,
        )?;
        // One statement deletes every record up to the last one that fits.
        let (mut count, mut bytes) = (0, 0);
        let mut last = None;
        for record in &records {
            if bytes + record.bytes > OPERATION_RECORD_BYTES as u64 {
                break;
            }
            count += 1;
            bytes += record.bytes;
            last = Some(record);
        }
        let Some(last) = last else {
            return Ok((0, 0));
        };
        let bound = 32 + (last.file_scope.len() + last.key.len()) as u64;
        self.execute(
            StatementKind::Reclaim,
            sql::INDEXED_OPERATION_RECORD_DELETE_OPERATION,
            &[&operation, &last.file_scope, &last.kind, &last.key],
            bound,
        )?;
        Ok((count, bytes))
    }
}
