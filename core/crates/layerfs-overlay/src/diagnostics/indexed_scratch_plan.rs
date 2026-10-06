//! Exact strategy/program diagnostics; execution and residency remain separate.
use crate::{db::integer, sql, IndexedKey, IndexedScope, Overlay, OverlayResult, StatementKind};

impl Overlay {
    /// Plans for complete-key points, the one-root exclusion and both cleanup
    /// prefixes, plus the UPSERT VM including accounting trigger programs.
    /// Human-readable output is an operator diagnostic, not a parsed contract.
    pub fn explain_indexed_scratch(
        &self,
        scope: IndexedScope,
        key: IndexedKey,
        excluded_root: [u8; 32],
    ) -> OverlayResult<Vec<String>> {
        self.check_operation(scope.owner)?;
        let ns = scope.owner.route.ns;
        let operation = integer(scope.owner.owner)?;
        let file = scope.file_scope.to_be_bytes();
        let kind = i64::from(key.kind);
        let point: [&dyn rusqlite::ToSql; 5] = [
            &ns,
            &operation,
            &file.as_slice(),
            &kind,
            &key.key.as_slice(),
        ];
        let keys: [&dyn rusqlite::ToSql; 5] = [
            &ns,
            &operation,
            &file.as_slice(),
            &kind,
            &excluded_root.as_slice(),
        ];
        let operation_params: [&dyn rusqlite::ToSql; 2] = [&ns, &operation];
        let namespace_params: [&dyn rusqlite::ToSql; 1] = [&ns];
        let mut plans = Vec::new();
        for (label, statement, params, bytes) in [
            (
                "contains",
                sql::INDEXED_SCRATCH_CONTAINS,
                point.as_slice(),
                64,
            ),
            ("get", sql::INDEXED_SCRATCH_GET, point.as_slice(), 64),
            ("delete", sql::INDEXED_SCRATCH_DELETE, point.as_slice(), 64),
            ("keys", sql::INDEXED_SCRATCH_KEYS, keys.as_slice(), 64),
            ("all-keys", sql::INDEXED_SCRATCH_ALL_KEYS, &keys[..4], 32),
            (
                "operation-cleanup",
                sql::INDEXED_SCRATCH_RECLAIM_OPERATION,
                operation_params.as_slice(),
                16,
            ),
            (
                "namespace-cleanup",
                sql::INDEXED_SCRATCH_RECLAIM_NAMESPACE,
                namespace_params.as_slice(),
                8,
            ),
        ] {
            plans.extend(self.query(
                StatementKind::Explain,
                &format!("EXPLAIN QUERY PLAN {statement}"),
                params,
                bytes,
                |row| Ok(format!("{label}: {}", row.get::<_, String>(3)?)),
            )?);
        }
        let empty: &[u8] = &[];
        let put: [&dyn rusqlite::ToSql; 6] =
            [point[0], point[1], point[2], point[3], point[4], &empty];
        for (label, statement, params, bytes) in [
            ("put-vm", sql::INDEXED_SCRATCH_PUT, put.as_slice(), 64),
            ("keys-vm", sql::INDEXED_SCRATCH_KEYS, keys.as_slice(), 64),
            ("all-keys-vm", sql::INDEXED_SCRATCH_ALL_KEYS, &keys[..4], 32),
            (
                "delete-vm",
                sql::INDEXED_SCRATCH_DELETE,
                point.as_slice(),
                64,
            ),
        ] {
            plans.extend(self.query(
                StatementKind::Explain,
                &format!("EXPLAIN {statement}"),
                params,
                bytes,
                |row| {
                    Ok(format!(
                        "{label}: {} {} {} {} {} {:?}",
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )?);
        }
        Ok(plans)
    }
}
