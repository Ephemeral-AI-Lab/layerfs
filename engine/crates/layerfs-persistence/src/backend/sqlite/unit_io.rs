//! One bounded acquisition owns one read-only unit cursor and checked close.
use super::{rows, transaction::Transaction, units_read::Unit};
use layerfs_storage::port::PersistenceError;
use std::time::Instant;
pub(super) fn read(
    tx: &Transaction<'_>,
    units: &[Unit],
    offset: usize,
    end: usize,
    bytes: &mut [u8],
) -> Result<(), PersistenceError> {
    let mut selected = units
        .iter()
        .filter(|unit| unit.end > offset && unit.start < end);
    let first = selected.next().ok_or(PersistenceError::Malformed)?;
    if first.start > offset || end - offset != bytes.len() {
        return Err(PersistenceError::Malformed);
    }
    tx.work.borrow_mut().blob_open_calls += 1;
    let mut blob = tx
        .connection
        .blob_open("main", "pack_unit", "body", first.id, true)
        .map_err(rows::error)?;
    let result = (|| {
        let mut unit = first;
        let mut at = offset;
        loop {
            if unit.start > at || blob.len() != unit.end - unit.start {
                return Err(PersistenceError::Malformed);
            }
            let to = unit.end.min(end);
            let output = &mut bytes[at - offset..to - offset];
            {
                let mut work = tx.work.borrow_mut();
                work.blob_read_calls += 1;
                work.blob_requested_bytes += output.len() as u64;
            }
            let start = Instant::now();
            let read = blob
                .read_at_exact(output, at - unit.start)
                .map_err(rows::error);
            {
                let mut work = tx.work.borrow_mut();
                work.blob_read_ns += start.elapsed().as_nanos() as u64;
                if read.is_ok() {
                    work.blob_read_bytes += output.len() as u64;
                }
            }
            read?;
            at = to;
            if at == end {
                break;
            }
            unit = selected.next().ok_or(PersistenceError::Malformed)?;
            tx.work.borrow_mut().blob_reopen_calls += 1;
            blob.reopen(unit.id).map_err(rows::error)?;
        }
        Ok(())
    })();
    tx.work.borrow_mut().blob_close_calls += 1;
    let close: Result<(), PersistenceError> = blob.close().map_err(rows::error).map_err(Into::into);
    match (result, close) {
        (Err(PersistenceError::Uncertain), _) | (_, Err(PersistenceError::Uncertain)) => {
            Err(PersistenceError::Uncertain)
        }
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}
