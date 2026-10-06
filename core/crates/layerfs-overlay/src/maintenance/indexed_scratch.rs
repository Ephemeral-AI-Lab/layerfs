//! Exact bounded raw-record deletion after the last operation owner releases.
use crate::{db::unsigned, sql, Overlay, OverlayResult, StatementKind, SCRATCH_BYTES};

struct Record {
    operation: i64,
    file_scope: Vec<u8>,
    kind: i64,
    key: Vec<u8>,
    bytes: u64,
}

impl Overlay {
    pub(crate) fn delete_indexed_scratch(
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
                sql::INDEXED_SCRATCH_RECLAIM_OPERATION,
                &[&ns, &operation],
                16,
                decode,
            )?
        } else {
            self.query(
                StatementKind::Reclaim,
                sql::INDEXED_SCRATCH_RECLAIM_NAMESPACE,
                &[&ns],
                8,
                decode,
            )?
        };
        let (mut count, mut bytes) = (0, 0);
        for record in records {
            if bytes + record.bytes > SCRATCH_BYTES as u64 {
                break;
            }
            self.execute(
                StatementKind::Reclaim,
                sql::INDEXED_SCRATCH_DELETE,
                &[
                    &ns,
                    &record.operation,
                    &record.file_scope,
                    &record.kind,
                    &record.key,
                ],
                64,
            )?;
            count += 1;
            bytes += record.bytes;
        }
        Ok((count, bytes))
    }
}
