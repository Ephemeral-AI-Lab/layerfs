//! Short guarded full-key scratch jobs in the existing operation namespace.
use crate::{
    db::integer, indexed_changes_bytes, sql, ExpectedValue, IndexedApply, IndexedChange,
    IndexedKey, IndexedScope, Overlay, OverlayResult, StatementKind,
};

impl Overlay {
    /// Membership projects no value BLOB. Existing owner custody stays readable
    /// after logical close; a released or foreign owner is refused.
    pub fn indexed_scratch_contains(
        &self,
        scope: IndexedScope,
        key: IndexedKey,
    ) -> OverlayResult<bool> {
        self.check_operation(scope.owner)?;
        let file = scope.file_scope.to_be_bytes();
        Ok(!self
            .query(
                StatementKind::Scratch,
                sql::INDEXED_SCRATCH_CONTAINS,
                &[
                    &scope.owner.route.ns,
                    &integer(scope.owner.owner)?,
                    &file.as_slice(),
                    &i64::from(key.kind),
                    &key.key.as_slice(),
                ],
                64,
                |_| Ok(()),
            )?
            .is_empty())
    }

    /// Reads at most one bounded raw value without Content interpretation.
    pub fn indexed_scratch_get(
        &self,
        scope: IndexedScope,
        key: IndexedKey,
    ) -> OverlayResult<Option<Vec<u8>>> {
        self.check_operation(scope.owner)?;
        self.indexed_value(scope, key)
    }

    fn indexed_value(
        &self,
        scope: IndexedScope,
        key: IndexedKey,
    ) -> OverlayResult<Option<Vec<u8>>> {
        let file = scope.file_scope.to_be_bytes();
        self.query(
            StatementKind::Scratch,
            sql::INDEXED_SCRATCH_GET,
            &[
                &scope.owner.route.ns,
                &integer(scope.owner.owner)?,
                &file.as_slice(),
                &i64::from(key.kind),
                &key.key.as_slice(),
            ],
            64,
            |r| r.get(0),
        )
        .map(|mut rows| rows.pop())
    }

    /// One attempt: every exact precondition is checked before any row change.
    /// The capacity bound applies to this job, never to total edit state. Values
    /// larger than this raw-record protocol need an explicit sharded provider.
    pub fn indexed_scratch_apply(
        &self,
        scope: IndexedScope,
        changes: &[IndexedChange],
    ) -> OverlayResult<IndexedApply> {
        indexed_changes_bytes(changes)?;
        self.atomic(|| {
            self.live(scope.owner.route)?;
            self.check_operation(scope.owner)?;
            for (index, change) in changes.iter().enumerate() {
                let actual = self.indexed_value(scope, change.key)?;
                let matches = match (&change.expected, &actual) {
                    (ExpectedValue::Missing, None) => true,
                    (ExpectedValue::ExactBytes(expected), Some(actual)) => expected == actual,
                    _ => false,
                };
                if !matches {
                    return Ok(IndexedApply::NotApplied {
                        index,
                        key: change.key,
                        actual,
                    });
                }
            }
            let file = scope.file_scope.to_be_bytes();
            let operation = integer(scope.owner.owner)?;
            for change in changes {
                let params: [&dyn rusqlite::ToSql; 5] = [
                    &scope.owner.route.ns,
                    &operation,
                    &file.as_slice(),
                    &i64::from(change.key.kind),
                    &change.key.key.as_slice(),
                ];
                if let Some(value) = &change.value {
                    self.execute(
                        StatementKind::Scratch,
                        sql::INDEXED_SCRATCH_PUT,
                        &[params[0], params[1], params[2], params[3], params[4], value],
                        64 + value.len() as u64,
                    )?;
                } else {
                    self.execute(
                        StatementKind::Scratch,
                        sql::INDEXED_SCRATCH_DELETE,
                        &params,
                        64,
                    )?;
                }
            }
            Ok(IndexedApply::Applied)
        })
    }

    /// First eligible keys only, excluding exactly one retained root. Deleting
    /// a candidate may insert lower keys; the next window starts from the first
    /// eligible key again. No continuation cursor skips such children.
    pub fn indexed_scratch_first_keys(
        &self,
        scope: IndexedScope,
        kind: u32,
        excluded_root: [u8; 32],
    ) -> OverlayResult<Vec<[u8; 32]>> {
        self.indexed_scratch_keys(scope, kind, Some(excluded_root))
    }

    /// First keys with an optional exact exclusion. None excludes nothing;
    /// every 32-byte identity, including all-zero bytes, is representable.
    pub fn indexed_scratch_keys(
        &self,
        scope: IndexedScope,
        kind: u32,
        excluded_root: Option<[u8; 32]>,
    ) -> OverlayResult<Vec<[u8; 32]>> {
        self.check_operation(scope.owner)?;
        let file = scope.file_scope.to_be_bytes();
        let operation = integer(scope.owner.owner)?;
        let kind = i64::from(kind);
        let prefix: [&dyn rusqlite::ToSql; 4] =
            [&scope.owner.route.ns, &operation, &file.as_slice(), &kind];
        let decode = |r: &rusqlite::Row<'_>| {
            let key: Vec<u8> = r.get(0)?;
            key.try_into().map_err(|_| rusqlite::Error::InvalidQuery)
        };
        match excluded_root {
            Some(root) => self.query(
                StatementKind::Scratch,
                sql::INDEXED_SCRATCH_KEYS,
                &[prefix[0], prefix[1], prefix[2], prefix[3], &root.as_slice()],
                64,
                decode,
            ),
            None => self.query(
                StatementKind::Scratch,
                sql::INDEXED_SCRATCH_ALL_KEYS,
                &prefix,
                32,
                decode,
            ),
        }
    }

    /// Non-destructive complete-key enumeration within one retained operation
    /// scope. None includes every identity; Some is an exclusive full-key seek.
    /// Membership must be sealed by the owning caller for a complete pass.
    pub fn indexed_scratch_keys_after(
        &self,
        scope: IndexedScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> OverlayResult<Vec<[u8; 32]>> {
        let Some(after) = after else {
            return self.indexed_scratch_keys(scope, kind, None);
        };
        self.check_operation(scope.owner)?;
        let file = scope.file_scope.to_be_bytes();
        self.query(
            StatementKind::Scratch,
            sql::INDEXED_SCRATCH_KEYS_AFTER,
            &[
                &scope.owner.route.ns,
                &integer(scope.owner.owner)?,
                &file.as_slice(),
                &i64::from(kind),
                &after.as_slice(),
            ],
            64,
            |r| {
                let key: Vec<u8> = r.get(0)?;
                key.try_into().map_err(|_| rusqlite::Error::InvalidQuery)
            },
        )
    }
}
