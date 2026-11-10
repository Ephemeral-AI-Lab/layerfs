//! Snapshot-bound, bounded extraction of unbounded persisted history facts.
use super::{emit, transaction::View, Observation, Options, Outcome};
use crate::backend::records::{BackendError, Param};
use std::io::Read;
struct Input<'a> {
    view: &'a dyn View,
    id: Vec<u8>,
    offset: i64,
    length: i64,
    options: Options<'a>,
    error: Option<BackendError>,
}
impl Read for Input<'_> {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let result = (|| -> Result<usize, BackendError> {
            self.options.check()?;
            if self.offset >= self.length {
                return Ok(0);
            }
            let size = out
                .len()
                .min(8192)
                .min((self.length - self.offset) as usize);
            if size == 0 {
                return Ok(0);
            }
            let rows = self.view.query(
                "SELECT substr(lww,?2,?3) FROM \"commit\" WHERE commit_id=?1",
                vec![
                    Param::Bytes(self.id.clone()),
                    Param::I64(self.offset + 1),
                    Param::I64(size as i64),
                ],
            )?;
            let bytes: Vec<u8> = rows.first().ok_or(BackendError::Integrity)?.get(0)?;
            if bytes.len() != size {
                return Err(BackendError::Integrity);
            }
            out[..size].copy_from_slice(&bytes);
            self.offset += size as i64;
            Ok(size)
        })();
        result.map_err(|error| {
            self.error = Some(error);
            std::io::Error::other("snapshot fact read did not complete")
        })
    }
}
pub(super) fn run(
    view: &dyn View,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    let (mut after, mut checked, mut failed, mut incomplete) = (Vec::new(), 0u64, 0u64, false);
    loop {
        options.check()?;
        let rows=view.query("SELECT commit_id,blob_length(lww) FROM \"commit\" WHERE blob_length(commit_id)=33 AND commit_id>?1 ORDER BY commit_id LIMIT 64",vec![Param::Bytes(after.clone())])?;
        if rows.is_empty() {
            break;
        }
        emit(
            observer,
            "store.history.facts.progress",
            Outcome::Pass,
            "validating next bounded history-fact window",
        );
        for row in rows {
            options.check()?;
            after = row.get(0)?;
            let length: i64 = row.get(1)?;
            if length < 0 {
                failed += 1;
                emit(
                    observer,
                    "store.history.facts",
                    Outcome::Fail,
                    "fact payload has a non-BLOB storage class",
                );
                continue;
            }
            let mut input = Input {
                view,
                id: after.clone(),
                offset: 0,
                length,
                options,
                error: None,
            };
            let result = crate::history::validate_facts_stream(&mut input);
            checked += 1;
            options.check()?;
            if let Err(error) = result {
                let outcome = if input.error.is_some_and(|e| e != BackendError::Integrity) {
                    incomplete = true;
                    Outcome::Unknown
                } else {
                    failed += 1;
                    Outcome::Fail
                };
                emit(
                    observer,
                    "store.history.facts",
                    outcome,
                    format!("Commit {}: {error}", hex(&after)),
                );
            }
        }
    }
    emit(observer,"store.history.facts.summary",if failed>0{Outcome::Fail}else if incomplete{Outcome::Unknown}else{Outcome::Pass},format!("{checked} persisted fact blobs streamed using owning codec grammar; {failed} malformed; no total text or decision count retained"));
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    const D: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(D[(b >> 4) as usize] as char);
        out.push(D[(b & 15) as usize] as char);
    }
    out
}
