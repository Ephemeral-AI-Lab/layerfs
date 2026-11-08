//! Complete auxiliary-block and whole-file-digest verification.
use super::{emit, reader::Reader, transaction::View, Observation, Options, Outcome};
use crate::backend::records::{BackendError, Param};
use layerfs_content::ObjectId;
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
        let rows=view.query("SELECT grp,seq,CASE WHEN length(body)<=4000 THEN body ELSE NULL END FROM binding_hint_block WHERE grp>?1 OR (grp=?1 AND seq>?2) ORDER BY grp,seq LIMIT 64",vec![Param::I64(group),Param::I64(sequence)])?;
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
    let mut signatures = Vec::new();
    match view.signatures(&mut signatures){Ok(())=>emit(observer,"store.advisory.signatures",Outcome::Pass,format!("{} bounded signature-ring rows decoded",signatures.len())),Err(_)=>emit(observer,"store.advisory.signatures",Outcome::Fail,"signature accelerator is malformed; this is distinct from authenticated content corruption")}
    let (mut after, mut length, mut digests, mut stale, mut damaged) =
        (Vec::<u8>::new(), -1i64, 0u64, 0u64, 0u64);
    loop {
        options.check()?;
        let rows=view.query("SELECT digest,length,root FROM file_digest WHERE digest>?1 OR (digest=?1 AND length>?2) ORDER BY digest,length LIMIT 64",vec![Param::Bytes(after.clone()),Param::I64(length)])?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            options.check()?;
            after = row.get(0)?;
            length = row.get(1)?;
            digests += 1;
            let root = row.get::<Vec<u8>>(2)?;
            let result = (|| -> Result<(), String> {
                if after.len() != 32 || length < 0 {
                    return Err("digest/length fields malformed".to_owned());
                }
                let id = ObjectId::from_bytes(&root).map_err(|e| e.to_string())?;
                let mut locations = Vec::new();
                view.locate(&[id], &mut locations)
                    .map_err(|e| e.to_string())?;
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
                    .0
                    .map_err(|e| e.to_string())?;
                    if actual.as_slice() != after || bytes != length as u64 {
                        return Err(
                            "whole-content digest or logical length differs from referenced file"
                                .to_owned(),
                        );
                    }
                }
                Ok(())
            })();
            options.check()?;
            if let Err(error) = result {
                damaged += 1;
                emit(
                    observer,
                    "store.advisory.file_digests",
                    Outcome::Fail,
                    format!("file digest root {}: {error}", hex(&root)),
                );
            }
        }
    }
    emit(observer,"store.advisory.file_digests.summary",if damaged==0{Outcome::Pass}else{Outcome::Fail},format!("{digests} digest records {}; {stale} legitimately stale/unsaved roots; {damaged} damaged hints",if reader.is_some(){"fully streamed and compared where root exists"}else{"metadata checked; quick mode excludes logical-byte digest recomputation"}));
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
