//! Exact bounded raw-record deletion after the last operation owner releases.
use crate::{db::unsigned, sql, Overlay, OverlayResult, StatementKind, OPERATION_RECORD_BYTES};

struct Record {
    operation: i64,
    file_scope: Vec<u8>,
    kind: i64,
    key: Vec<u8>,
    bytes: u64,
}

impl Overlay {
    pub(crate) fn delete_indexed_operation_record(
        &self,
        ns: i64,
        operation: Option<i64>,
    ) -> OverlayResult<(u64, u64)> {
        let decode = |r: &rusqlite::Row<'_>| {
            let shift = usize::from(operation.is_none());
            Ok(Record {
                operation: if let Some(operation) = operation {
                    operation
                } else {
                    r.get(0)?
                },
                file_scope: r.get(shift)?,
                kind: r.get(shift + 1)?,
                key: r.get(shift + 2)?,
                bytes: unsigned(r, shift + 3)?,
            })
        };
        let records = if let Some(operation) = operation {
            self.query(
                StatementKind::Reclaim,
                sql::INDEXED_OPERATION_RECORD_RECLAIM_OPERATION,
                &[&ns, &operation],
                16,
                decode,
            )?
        } else {
            self.query(
                StatementKind::Reclaim,
                sql::INDEXED_OPERATION_RECORD_RECLAIM_NAMESPACE,
                &[&ns],
                8,
                decode,
            )?
        };
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
        // Both statements bind the same key; one stays inside its operation.
        let through = if operation.is_some() {
            sql::INDEXED_OPERATION_RECORD_DELETE_OPERATION
        } else {
            sql::INDEXED_OPERATION_RECORD_DELETE_NAMESPACE
        };
        self.execute(
            StatementKind::Reclaim,
            through,
            &[
                &ns,
                &last.operation,
                &last.file_scope,
                &last.kind,
                &last.key,
            ],
            bound,
        )?;
        Ok((count, bytes))
    }
}
