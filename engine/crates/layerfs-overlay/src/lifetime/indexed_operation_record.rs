//! Short guarded full-key operation_record jobs in the existing operation namespace.
use crate::{
    db::integer, indexed_operation_record_changes_bytes, sql, IndexedOperationRecordApply,
    IndexedOperationRecordChange, IndexedOperationRecordKey, IndexedOperationRecordScope,
    OperationRecordExpectedValue, Overlay, OverlayError, OverlayResult, StatementKind,
    OPERATION_RECORD_BYTES, PAGE_ROWS,
};

/// Key-ordered (key, value) rows and whether the kind ends after them.
type Window = (Vec<([u8; 32], Vec<u8>)>, bool);

impl Overlay {
    /// Membership projects no value BLOB. Existing owner custody stays readable
    /// after logical close; a released or foreign owner is refused.
    pub fn indexed_operation_record_contains(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> OverlayResult<bool> {
        self.fence_operation(scope.owner, false)?;
        let file = scope.file_scope.to_be_bytes();
        Ok(!self
            .query(
                StatementKind::OperationRecord,
                sql::INDEXED_OPERATION_RECORD_CONTAINS,
                &[
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
    pub fn indexed_operation_record_get(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> OverlayResult<Option<Vec<u8>>> {
        self.fence_operation(scope.owner, false)?;
        self.indexed_value(scope, key)
    }

    /// Reads the bounded raw values of at most `PAGE_ROWS` keys under one
    /// fence, in the order asked. Together they stay inside one record envelope.
    pub fn indexed_operation_record_get_many(
        &self,
        scope: IndexedOperationRecordScope,
        keys: &[IndexedOperationRecordKey],
    ) -> OverlayResult<Vec<Option<Vec<u8>>>> {
        if keys.len() > PAGE_ROWS {
            return Err(OverlayError::Invalid("indexed operation_record key list"));
        }
        self.fence_operation(scope.owner, false)?;
        let mut values = Vec::with_capacity(keys.len());
        let mut bytes = 0_usize;
        for key in keys {
            let value = self.indexed_value(scope, *key)?;
            bytes = bytes.saturating_add(value.as_ref().map_or(0, Vec::len));
            if bytes > OPERATION_RECORD_BYTES {
                return Err(OverlayError::Invalid("indexed operation_record values"));
            }
            values.push(value);
        }
        Ok(values)
    }

    fn indexed_value(
        &self,
        scope: IndexedOperationRecordScope,
        key: IndexedOperationRecordKey,
    ) -> OverlayResult<Option<Vec<u8>>> {
        let file = scope.file_scope.to_be_bytes();
        self.query(
            StatementKind::OperationRecord,
            sql::INDEXED_OPERATION_RECORD_GET,
            &[
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
    pub fn indexed_operation_record_apply(
        &self,
        scope: IndexedOperationRecordScope,
        changes: &[IndexedOperationRecordChange],
    ) -> OverlayResult<IndexedOperationRecordApply> {
        indexed_operation_record_changes_bytes(changes)?;
        self.atomic(|| {
            self.fence_operation(scope.owner, true)?;
            for (index, change) in changes.iter().enumerate() {
                let actual = self.indexed_value(scope, change.key)?;
                let matches = match (&change.expected, &actual) {
                    (OperationRecordExpectedValue::Missing, None) => true,
                    (OperationRecordExpectedValue::ExactBytes(expected), Some(actual)) => {
                        expected == actual
                    }
                    _ => false,
                };
                if !matches {
                    return Ok(IndexedOperationRecordApply::NotApplied {
                        index,
                        key: change.key,
                        actual,
                    });
                }
            }
            let file = scope.file_scope.to_be_bytes();
            let operation = integer(scope.owner.owner)?;
            for change in changes {
                let params: [&dyn rusqlite::ToSql; 4] = [
                    &operation,
                    &file.as_slice(),
                    &i64::from(change.key.kind),
                    &change.key.key.as_slice(),
                ];
                if let Some(value) = &change.value {
                    self.execute(
                        StatementKind::OperationRecord,
                        sql::INDEXED_OPERATION_RECORD_PUT,
                        &[params[0], params[1], params[2], params[3], value],
                        64 + value.len() as u64,
                    )?;
                } else {
                    self.execute(
                        StatementKind::OperationRecord,
                        sql::INDEXED_OPERATION_RECORD_DELETE,
                        &params,
                        64,
                    )?;
                }
            }
            Ok(IndexedOperationRecordApply::Applied)
        })
    }

    /// First eligible keys only, excluding exactly one retained root. Deleting
    /// a candidate may insert lower keys; the next window starts from the first
    /// eligible key again. No continuation cursor skips such children.
    pub fn indexed_operation_record_first_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded_root: [u8; 32],
    ) -> OverlayResult<Vec<[u8; 32]>> {
        self.indexed_operation_record_keys(scope, kind, Some(excluded_root))
    }

    /// First keys with an optional exact exclusion. None excludes nothing;
    /// every 32-byte identity, including all-zero bytes, is representable.
    pub fn indexed_operation_record_keys(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        excluded_root: Option<[u8; 32]>,
    ) -> OverlayResult<Vec<[u8; 32]>> {
        self.fence_operation(scope.owner, false)?;
        let file = scope.file_scope.to_be_bytes();
        let operation = integer(scope.owner.owner)?;
        let kind = i64::from(kind);
        let prefix: [&dyn rusqlite::ToSql; 3] = [&operation, &file.as_slice(), &kind];
        let decode = |r: &rusqlite::Row<'_>| {
            let key: Vec<u8> = r.get(0)?;
            key.try_into().map_err(|_| rusqlite::Error::InvalidQuery)
        };
        match excluded_root {
            Some(root) => self.query(
                StatementKind::OperationRecord,
                sql::INDEXED_OPERATION_RECORD_KEYS,
                &[prefix[0], prefix[1], prefix[2], &root.as_slice()],
                64,
                decode,
            ),
            None => self.query(
                StatementKind::OperationRecord,
                sql::INDEXED_OPERATION_RECORD_ALL_KEYS,
                &prefix,
                32,
                decode,
            ),
        }
    }

    /// Non-destructive complete-key enumeration within one retained operation
    /// scope. None includes every identity; Some is an exclusive full-key seek.
    /// Membership must be sealed by the owning caller for a complete pass.
    pub fn indexed_operation_record_keys_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> OverlayResult<Vec<[u8; 32]>> {
        let Some(after) = after else {
            return self.indexed_operation_record_keys(scope, kind, None);
        };
        self.fence_operation(scope.owner, false)?;
        let file = scope.file_scope.to_be_bytes();
        self.query(
            StatementKind::OperationRecord,
            sql::INDEXED_OPERATION_RECORD_KEYS_AFTER,
            &[
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

    /// Keys after `after` (None: from the first) with their values, in key
    /// order: at most `PAGE_ROWS` rows, stopped before the row that would take
    /// the window past one record envelope. The first row is always returned,
    /// so a window pages and never refuses; the caller continues after its
    /// last key. The flag says no row of the kind follows the window.
    pub fn indexed_operation_record_entries_after(
        &self,
        scope: IndexedOperationRecordScope,
        kind: u32,
        after: Option<[u8; 32]>,
    ) -> OverlayResult<Window> {
        self.fence_operation(scope.owner, false)?;
        let file = scope.file_scope.to_be_bytes();
        let operation = integer(scope.owner.owner)?;
        let kind = i64::from(kind);
        let mut bytes = 0_usize;
        let mut stopped = false;
        // A row past the envelope is stepped but its value is never copied.
        let decode = |r: &rusqlite::Row<'_>| {
            if stopped {
                return Ok(None);
            }
            let key: Vec<u8> = r.get(0)?;
            let key: [u8; 32] = key.try_into().map_err(|_| rusqlite::Error::InvalidQuery)?;
            let length = match r.get_ref(1)? {
                rusqlite::types::ValueRef::Blob(value) => value.len(),
                _ => return Err(rusqlite::Error::InvalidQuery),
            };
            if bytes != 0 && bytes.saturating_add(length) > OPERATION_RECORD_BYTES {
                stopped = true;
                return Ok(None);
            }
            bytes = bytes.saturating_add(length);
            Ok(Some((key, r.get::<_, Vec<u8>>(1)?)))
        };
        let rows = match after {
            Some(after) => self.query(
                StatementKind::OperationRecord,
                sql::INDEXED_OPERATION_RECORD_ENTRIES_AFTER,
                &[&operation, &file.as_slice(), &kind, &after.as_slice()],
                64,
                decode,
            )?,
            None => self.query(
                StatementKind::OperationRecord,
                sql::INDEXED_OPERATION_RECORD_ENTRIES,
                &[&operation, &file.as_slice(), &kind],
                32,
                decode,
            )?,
        };
        let ended = rows.len() < PAGE_ROWS && !stopped;
        let entries = rows.into_iter().map_while(|row| row).collect();
        Ok((entries, ended))
    }
}
