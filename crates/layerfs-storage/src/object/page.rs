use crate::integrity::IntegrityMode;
use crate::{
    checked_add, elapsed_ns, map_sqlite_error, payload_batch_sql, BatchTimings, Engine,
    EngineError, EngineResult,
};
use layerfs_core::{authenticate_identity, decode_bytes_object, CoreError, ObjectId, ObjectKind};
use rusqlite::params;
use rusqlite::types::ValueRef;
use std::ops::Range;
use std::time::Instant;

impl Engine {
    pub fn object_ids_page(
        &self,
        after: Option<ObjectId>,
        limit: usize,
    ) -> EngineResult<Vec<ObjectId>> {
        if limit == 0 || limit > 1024 {
            return Err(EngineError::InvalidRecord("object page limit"));
        }
        let connection = self.lock_connection()?;
        let mut statement = connection
            .prepare(
                "SELECT object_id FROM layerfs_objects
                 WHERE ?1 IS NULL OR object_id > ?1
                 ORDER BY object_id LIMIT ?2",
            )
            .map_err(map_sqlite_error)?;
        let page = statement
            .query_map(
                params![
                    after.map(|id| id.as_bytes().as_slice().to_vec()),
                    i64::try_from(limit).map_err(|_| EngineError::CounterOverflow)?
                ],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .map_err(map_sqlite_error)?
            .map(|row| {
                ObjectId::from_bytes(&row.map_err(map_sqlite_error)?).map_err(EngineError::Core)
            })
            .collect();
        page
    }

    pub fn for_each_authenticated_payload_range_batch<F>(
        &self,
        requests: &[(ObjectId, Range<u64>)],
        maximum_payload_len: u64,
        mut callback: F,
    ) -> EngineResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> EngineResult<()>,
    {
        let ids = requests.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        let mut ordinal = 0;
        self.for_each_authenticated_payload_batch(&ids, |id, payload| {
            let (expected_id, range) = requests
                .get(ordinal)
                .ok_or(EngineError::InvalidRecord("payload batch cardinality"))?;
            if id != *expected_id {
                return Err(EngineError::InvalidRecord("payload batch order"));
            }
            ordinal += 1;
            validate_payload_range(payload.len() as u64, maximum_payload_len, range)?;
            let start = usize::try_from(range.start).map_err(|_| EngineError::CounterOverflow)?;
            let end = usize::try_from(range.end).map_err(|_| EngineError::CounterOverflow)?;
            callback(id, &payload[start..end])
        })?;
        if ordinal != requests.len() {
            return Err(EngineError::InvalidRecord("payload batch cardinality"));
        }
        Ok(())
    }
}

fn validate_payload_range(
    length: u64,
    maximum_payload_len: u64,
    range: &Range<u64>,
) -> EngineResult<()> {
    if length > maximum_payload_len {
        return Err(EngineError::Core(CoreError::ChunkLengthMismatch));
    }
    if range.start > range.end || range.end > length {
        return Err(EngineError::InvalidRange {
            start: range.start,
            end: range.end,
            length,
        });
    }
    Ok(())
}

impl Engine {
    pub fn for_each_authenticated_payload_batch<F>(
        &self,
        ids: &[ObjectId],
        mut callback: F,
    ) -> EngineResult<()>
    where
        F: FnMut(ObjectId, &[u8]) -> EngineResult<()>,
    {
        if ids.len() > 64 {
            return Err(EngineError::InvalidRecord("payload batch exceeds 64"));
        }
        if ids.is_empty() {
            return Ok(());
        }
        let authenticate_reads = self.mode == IntegrityMode::Verified;
        let mut timings = BatchTimings::default();
        let mut objects_validated = 0_u64;
        let mut object_bytes_read = 0_u64;
        let mut fetched_rows = 0_u64;
        let mut authentication_passes = 0_u64;
        let mut role_decode_passes = 0_u64;
        let result = (|| {
            let counter_started = Instant::now();
            self.bump(|counters| {
                checked_add(&mut counters.payload_batch_queries, 1)?;
                checked_add(
                    &mut counters.payload_batch_references,
                    u64::try_from(ids.len()).map_err(|_| EngineError::CounterOverflow)?,
                )?;
                counters.payload_batch_maximum = counters
                    .payload_batch_maximum
                    .max(u64::try_from(ids.len()).map_err(|_| EngineError::CounterOverflow)?);
                Ok(())
            })?;
            timings.counter_merge_ns = timings
                .counter_merge_ns
                .saturating_add(elapsed_ns(counter_started));
            let connection = self.lock_connection()?;
            let sql = payload_batch_sql(ids.len())?;
            let query_started = Instant::now();
            self.mark_statement()?;
            let mut statement = connection.prepare_cached(&sql).map_err(map_sqlite_error)?;
            let mut rows = statement
                .query(rusqlite::params_from_iter(
                    ids.iter().map(|id| id.as_bytes().as_slice()),
                ))
                .map_err(map_sqlite_error)?;
            timings.payload_query_ns = timings
                .payload_query_ns
                .saturating_add(elapsed_ns(query_started));
            let mut ordinal = 0;
            loop {
                let step_started = Instant::now();
                let row = rows.next().map_err(map_sqlite_error)?;
                timings.payload_query_ns = timings
                    .payload_query_ns
                    .saturating_add(elapsed_ns(step_started));
                let Some(row) = row else { break };
                let observed_ordinal = row.get::<_, i64>(0).map_err(map_sqlite_error)?;
                if observed_ordinal != ordinal as i64 {
                    return if observed_ordinal > ordinal as i64 {
                        Err(EngineError::MissingObject(ids[ordinal]))
                    } else {
                        Err(EngineError::InvalidRecord("payload batch order"))
                    };
                }
                let id = ids[ordinal];
                let kind = row
                    .get::<_, Option<i64>>(1)
                    .map_err(map_sqlite_error)?
                    .ok_or(EngineError::MissingObject(id))?;
                let length = row
                    .get::<_, Option<i64>>(2)
                    .map_err(map_sqlite_error)?
                    .ok_or(EngineError::MissingObject(id))?;
                let bytes = match row.get_ref(3).map_err(map_sqlite_error)? {
                    ValueRef::Blob(bytes) => bytes,
                    _ => return Err(EngineError::InvalidRecord("object bytes")),
                };
                checked_add(&mut fetched_rows, 1)?;
                let summary = if authenticate_reads {
                    let authentication_started = Instant::now();
                    let result = authenticate_identity(bytes, id)
                        .map_err(|cause| EngineError::MalformedObject { id, cause });
                    timings.identity_authentication_ns = timings
                        .identity_authentication_ns
                        .saturating_add(elapsed_ns(authentication_started));
                    let summary = result?;
                    checked_add(&mut authentication_passes, 1)?;
                    Some(summary)
                } else {
                    None
                };
                let role_started = Instant::now();
                let role = (|| {
                    let expected_kind = ObjectKind::try_from(
                        u8::try_from(kind)
                            .map_err(|_| EngineError::InvalidRecord("object kind"))?,
                    )?;
                    let expected_length = u64::try_from(length)
                        .map_err(|_| EngineError::InvalidRecord("object length"))?;
                    let actual_length =
                        u64::try_from(bytes.len()).map_err(|_| EngineError::CounterOverflow)?;
                    if summary.is_some_and(|summary| summary.kind != expected_kind)
                        || expected_kind != ObjectKind::Bytes
                        || actual_length != expected_length
                    {
                        return Err(EngineError::MalformedObject {
                            id,
                            cause: CoreError::LengthMismatch {
                                expected: expected_length,
                                actual: actual_length,
                            },
                        });
                    }
                    let payload = decode_bytes_object(bytes).map_err(EngineError::Core)?;
                    Ok((payload, actual_length))
                })();
                timings.role_decode_ns = timings
                    .role_decode_ns
                    .saturating_add(elapsed_ns(role_started));
                let (payload, actual_length) = role?;
                checked_add(&mut objects_validated, 1)?;
                checked_add(&mut object_bytes_read, actual_length)?;
                checked_add(&mut role_decode_passes, 1)?;
                let callback_started = Instant::now();
                let callback_result = callback(id, payload);
                timings.payload_callback_inclusive_ns = timings
                    .payload_callback_inclusive_ns
                    .saturating_add(elapsed_ns(callback_started));
                callback_result?;
                ordinal += 1;
            }
            if ordinal != ids.len() {
                return Err(EngineError::MissingObject(ids[ordinal]));
            }
            Ok(())
        })();
        let counter_started = Instant::now();
        let merged = self.bump(|counters| {
            checked_add(&mut counters.objects_validated, objects_validated)?;
            checked_add(&mut counters.object_bytes_read, object_bytes_read)?;
            checked_add(&mut counters.fetched_rows, fetched_rows)?;
            checked_add(
                &mut counters.fetched_row_authentication_passes,
                authentication_passes,
            )?;
            checked_add(
                &mut counters.fetched_row_role_decode_passes,
                role_decode_passes,
            )
        });
        timings.counter_merge_ns = timings
            .counter_merge_ns
            .saturating_add(elapsed_ns(counter_started));
        timings.record(&self.timings);
        merged?;
        result
    }
}
