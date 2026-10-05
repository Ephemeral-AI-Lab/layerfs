//! Actual mutation plans for short base-source ownership/count transitions.
use crate::{db::integer, sql, Overlay, OverlayResult, Route, StatementKind};
impl Overlay {
    /// Read-only strategy diagnostics for exact acquire/release SQL. INSERT
    /// needs its VM program (VALUES has no query-plan row); updates/deletion
    /// expose their actual point access plans. Runtime counters remain separate.
    pub fn explain_base_source_changes(
        &self,
        route: Route,
        owner: u64,
    ) -> OverlayResult<Vec<String>> {
        let state = self.state(route)?;
        let mut plans = self.query(
            StatementKind::Explain,
            &format!("EXPLAIN {}", sql::BASE_SOURCE_INSERT),
            &[&route.ns, &integer(owner)?, &state.base_root.as_slice()],
            48,
            |row| {
                Ok(format!(
                    "insert: {} {} {} {} {}",
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?
                ))
            },
        )?;
        for (label, sql) in [
            ("increment", sql::BASE_SOURCE_INCREMENT),
            ("decrement", sql::BASE_SOURCE_DECREMENT),
        ] {
            plans.extend(self.query(
                StatementKind::Explain,
                &format!("EXPLAIN QUERY PLAN {sql}"),
                &[&route.ns],
                8,
                |row| Ok(format!("{label}: {}", row.get::<_, String>(3)?)),
            )?);
        }
        plans.extend(self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::BASE_SOURCE_DELETE),
            &[&route.ns, &integer(owner)?, &state.base_root.as_slice()],
            48,
            |row| Ok(format!("delete: {}", row.get::<_, String>(3)?)),
        )?);
        Ok(plans)
    }
}
