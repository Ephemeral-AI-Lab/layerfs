//! Complete auxiliary-block and whole-file-digest verification.
use super::{emit, reader::Reader, transaction::View, Observation, Options, Outcome};
use crate::backend::records::{BackendError, Param};
use layerfs_content::ObjectId;
use layerfs_storage::StorageError;
use layerfs_telemetry::timer::Timing;

pub(super) fn run(
    view: &dyn View,
    reader: Option<&Reader<'_>>,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    let (mut group, mut sequence, mut checked, mut failed) = (-1i64, -1i64, 0u64, 0u64);
    loop {
        options.check()?;
        let rows=view.query("SELECT grp,seq,CASE WHEN blob_length(body) BETWEEN 0 AND 4000 THEN body ELSE NULL END FROM binding_hint_block WHERE grp>?1 OR (grp=?1 AND seq>?2) ORDER BY grp,seq LIMIT 64",vec![Param::I64(group),Param::I64(sequence)])?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            group = row.get(0)?;
            sequence = row.get(1)?;
            checked += 1;
            let result = row
                .get::<Vec<u8>>(2)
                .and_then(|body| crate::blocks::hints::decode(group, &body));
            if result.is_err() {
                failed += 1;
                emit(observer,"store.advisory.binding_hints",Outcome::Fail,format!("binding hint block {group}/{sequence}: invalid framing/checksum; advisory hint damage does not establish namespace loss"));
            }
        }
    }
    emit(observer,"store.advisory.binding_hints.summary",if failed==0{Outcome::Pass}else{Outcome::Fail},format!("{checked} standalone binding hint blocks checked, including unused hints; {failed} failed"));
    signatures(view, options, observer)?;
    let (mut after, mut length, mut digests, mut stale, mut damaged) =
        (Vec::<u8>::new(), -1i64, 0u64, 0u64, 0u64);
    let mut incomplete = false;
    super::catalog::check(view,options,observer,"store.advisory.file_digests.shape","SELECT count(*) FROM file_digest WHERE blob_length(digest)!=32 OR blob_length(root)!=32 OR length<0","malformed digest-hint key/root/length rows; invalid widths are counted before extraction")?;
    loop {
        options.check()?;
        let rows=view.query("SELECT digest,length,root FROM file_digest WHERE blob_length(digest)=32 AND blob_length(root)=32 AND (digest>?1 OR (digest=?1 AND length>?2)) ORDER BY digest,length LIMIT 64",vec![Param::Bytes(after.clone()),Param::I64(length)])?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            options.check()?;
            after = row.get(0)?;
            length = row.get(1)?;
            digests += 1;
            let root = row.get::<Vec<u8>>(2)?;
            let result = (|| -> Result<(), StorageError> {
                if after.len() != 32 || length < 0 {
                    return Err(StorageError::Integrity("digest/length fields malformed"));
                }
                let id = ObjectId::from_bytes(&root)?;
                let mut locations = Vec::new();
                view.locate(&[id], &mut locations)?;
                // A digest written before an interrupted Commit can legally
                // outlive a root that was never saved; ordinary lookup ignores it.
                if locations.is_empty() {
                    stale += 1;
                    return Ok(());
                }
                if let Some(reader) = reader {
                    let (actual, bytes) = Timing::disabled("diagnostic.digest", |timing| {
                        layerfs_content::file::content_digest(reader, id, timing.child("read"))
                    })
                    .0?;
                    if actual.as_slice() != after || bytes != length as u64 {
                        return Err(StorageError::Integrity(
                            "whole-content digest or logical length differs from referenced file",
                        ));
                    }
                }
                Ok(())
            })();
            options.check()?;
            if let Err(error) = result {
                let outcome = super::observation::storage_outcome(&error);
                if outcome == Outcome::Fail {
                    damaged += 1;
                } else {
                    incomplete = true;
                }
                emit(
                    observer,
                    "store.advisory.file_digests",
                    outcome,
                    format!("file digest root {}: {error}", hex(&root)),
                );
            }
        }
    }
    emit(observer,"store.advisory.file_digests.summary",if damaged>0{Outcome::Fail}else if incomplete{Outcome::Unknown}else{Outcome::Pass},format!("{digests} digest records {}; {stale} legitimately stale/unsaved roots; {damaged} damaged hints",if reader.is_some(){"fully streamed and compared where root exists"}else{"metadata checked; quick mode excludes logical-byte digest recomputation"}));
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 15) as usize] as char);
    }
    out
}

fn signatures(
    view: &dyn View,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    options.check()?;
    let rows=match view.query("SELECT slot,stamp,CASE WHEN blob_length(object_id)=32 THEN object_id END,CASE WHEN blob_length(signature)=32 THEN signature END FROM content_signature ORDER BY slot LIMIT 8193",vec![]){
        Ok(rows)=>rows,Err(error)=>{emit(observer,"store.advisory.signatures",if error==BackendError::Integrity{Outcome::Fail}else{Outcome::Unknown},"signature accelerator could not be decoded/read; typed availability failures do not establish corruption");return Ok(());}
    };
    let mut failed = 0;
    if rows.len() > 8192 {
        failed += 1;
    }
    for row in &rows {
        options.check()?;
        let valid = (|| -> Result<(), BackendError> {
            let slot: i64 = row.get(0)?;
            let stamp: i64 = row.get(1)?;
            let id: Vec<u8> = row.get(2)?;
            let signature: Vec<u8> = row.get(3)?;
            if !(0..8192).contains(&slot)
                || stamp <= 0
                || (stamp - 1) % 8192 != slot
                || id.len() != 32
                || signature.len() != 32
            {
                return Err(BackendError::Integrity);
            }
            Ok(())
        })();
        if valid.is_err() {
            failed += 1;
        }
    }
    emit(observer,"store.advisory.signatures",if failed==0{Outcome::Pass}else{Outcome::Fail},format!("{} bounded signature-ring rows decoded; {failed} invalid; fixed-width blobs bounded inside SQL",rows.len()));
    Ok(())
}
