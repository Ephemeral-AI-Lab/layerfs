//! Plan/program of the actual metadata seek; physical I/O remains separate.
use crate::{
    db::integer, sql, CapturedRunCursor, Overlay, OverlayResult, StatementKind, CELL_BYTES,
};

impl Overlay {
    /// Exact query plan and VM of captured-cell metadata discovery under the
    /// retained reader. Diagnostic preparation is separate from measured jobs.
    pub fn explain_captured_run(&self, cursor: CapturedRunCursor) -> OverlayResult<Vec<String>> {
        self.check_captured_reader(cursor.reader())?;
        let ns = cursor.reader().capture().route().ns;
        let serial = integer(cursor.serial())?;
        let generation = cursor.reader().capture().generation.number();
        let offset = integer(cursor.offset() - cursor.offset() % CELL_BYTES as u64)?;
        let size = integer(cursor.logical_size())?;
        let params: [&dyn rusqlite::ToSql; 5] = [&ns, &serial, &generation, &offset, &size];
        let mut rows = self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::CAPTURED_CELL_METADATA),
            &params,
            40,
            |row| {
                Ok(format!(
                    "captured-cell-metadata: {}",
                    row.get::<_, String>(3)?
                ))
            },
        )?;
        rows.extend(self.query(
            StatementKind::Explain,
            &format!("EXPLAIN {}", sql::CAPTURED_CELL_METADATA),
            &params,
            40,
            |row| {
                Ok(format!(
                    "captured-cell-metadata-vm: {} {} {} {} {} {:?} {}",
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?
                ))
            },
        )?);
        Ok(rows)
    }
}
