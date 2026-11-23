//! Bounded keyset scans, including retained rows outside current branch heads.
use super::{emit, reader::Reader, transaction::View, Observation, Options, Outcome};
use crate::backend::records::{BackendError, Param};
use layerfs_content::{
    filesystem::{FilesystemRoot, InodeScope},
    FinalizedObject, ObjectId, ObjectRole,
};
use layerfs_storage::{pack::layout, StorageError, StorageResult};
use layerfs_telemetry::timer::Timing;

pub(super) fn run(
    view: &dyn View,
    ceiling: i64,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    super::catalog::run(view, options, observer)?;
    super::acquisition::run(view, options, observer)?;
    super::scan::blocks(view, options, observer)?;
    if options.quick {
        super::history::run(view, None, options, observer)?;
        super::auxiliary::run(view, None, options, observer)?;
        emit(
            observer,
            "store.packs",
            Outcome::Skipped,
            "quick mode excludes complete pack-body authentication",
        );
        emit(
            observer,
            "store.objects",
            Outcome::Skipped,
            "quick mode excludes canonical object, dependency and complete content checks",
        );
        emit(
            observer,
            "store.namespace",
            Outcome::Skipped,
            "quick mode excludes whole-root topology qualification",
        );
        return Ok(());
    }
    packs(view, options, observer)?;
    chains(view, options, observer)?;
    let reader = Reader::new(view, ceiling, options).map_err(|_| BackendError::Integrity)?;
    objects(view, &reader, options, observer)?;
    super::auxiliary::run(view, Some(&reader), options, observer)?;
    super::history::run(view, Some(&reader), options, observer)?;
    Ok(())
}

fn packs(
    view: &dyn View,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    let (mut after, mut checked, mut failed) = (0i64, 0u64, 0u64);
    let mut incomplete = false;
    loop {
        options.check()?;
        let rows = view.query(
            "SELECT pack_id FROM pack WHERE pack_id>?1 ORDER BY pack_id LIMIT 64",
            vec![Param::I64(after)],
        )?;
        if rows.is_empty() {
            break;
        }
        emit(
            observer,
            "store.packs.progress",
            Outcome::Pass,
            "processing next bounded inventory window",
        );
        options.check()?;
        for row in rows {
            options.check()?;
            after = row.get(0)?;
            let result = (|| -> StorageResult<()> {
                let mut packs = Vec::new();
                view.read_packs(&[after], &mut packs)?;
                let pack = packs
                    .pop()
                    .ok_or(StorageError::Integrity("missing audited pack"))?;
                let info = pack.info();
                let header = layout::parse_header(pack.body())?;
                if info.domain != layerfs_storage::location::PackDomain::for_lane(header.lane) {
                    return Err(StorageError::Integrity("pack domain"));
                }
                for number in 0..header.group_count {
                    layout::group_view(pack.body(), header, number)?;
                }
                layerfs_storage::pack::audit::validate_records(pack.body(), &mut || {
                    options.check().is_ok()
                })?;
                Ok(())
            })();
            checked += 1;
            options.check()?;
            if let Err(error) = result {
                let outcome = classify(&error);
                if outcome == Outcome::Fail {
                    failed += 1;
                } else {
                    incomplete = true;
                }
                emit(
                    observer,
                    "store.packs",
                    outcome,
                    format!("pack {after}: {error}"),
                );
            }
        }
    }
    emit(
        observer,
        "store.packs.summary",
        if failed > 0 {
            Outcome::Fail
        } else if incomplete {
            Outcome::Unknown
        } else {
            Outcome::Pass
        },
        format!("{checked} packs acquired and length/digest/directory checked; {failed} failed"),
    );
    Ok(())
}

/// PostgreSQL ancestor lists (#758), full mode only: every entry must name a
/// stored pack and a group below its group count, and each list must be the
/// first entry's row's list behind that entry, exactly what a strict reader
/// verifies (`chain_hint_stale`); a stale or unresolvable list costs a reader
/// speed, never correctness, but no 2.0 publication or compaction makes one.
/// Linear in the entries checked.
fn chains(
    view: &dyn View,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    if !view.has_table("store_lane")? {
        return Ok(());
    }
    super::catalog::check(view,options,observer,"store.catalog.chain_entries","SELECT count(*) FROM object_location o CROSS JOIN LATERAL ancestor_entries(o.pack_id,o.ancestors) e WHERE e.pack_id IS NULL OR NOT EXISTS (SELECT 1 FROM pack p WHERE p.pack_id=e.pack_id AND e.group_number<(get_byte(p.directory,12)|(get_byte(p.directory,13)<<8)|(get_byte(p.directory,14)<<16)|(get_byte(p.directory,15)<<24)))","malformed ancestor lists or entries naming no stored pack or group")?;
    super::catalog::check(view,options,observer,"store.catalog.chain_hints","SELECT count(*) FROM object_location o CROSS JOIN LATERAL ancestor_entries(o.pack_id,o.ancestors) e WHERE e.n=1 AND e.pack_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM object_location b WHERE b.pack_id=e.pack_id AND b.group_number=e.group_number AND b.record_number=e.record_number AND b.canonical_length=e.canonical_length AND b.ancestors=substring(o.ancestors FROM e.next))","ancestor lists whose first entry names no stored row or whose rest is not that row's list (stale chain hints a strict read refuses)")
}

fn objects(
    view: &dyn View,
    reader: &Reader<'_>,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    let (mut after, mut checked, mut failed) = (Vec::<u8>::new(), 0u64, 0u64);
    let mut incomplete = false;
    let policy = view
        .policy()
        .map_err(|_| BackendError::Integrity)?
        .construction();
    loop {
        options.check()?;
        let rows=view.query("SELECT object_id,role,canonical_length FROM object_location WHERE blob_length(object_id)=32 AND object_id>?1 ORDER BY object_id LIMIT 64",vec![Param::Bytes(after.clone())])?;
        if rows.is_empty() {
            break;
        }
        emit(
            observer,
            "store.objects.progress",
            Outcome::Pass,
            "processing next bounded inventory window",
        );
        options.check()?;
        for row in rows {
            options.check()?;
            after = row.get(0)?;
            checked += 1;
            let result = (|| -> StorageResult<()> {
                let id = ObjectId::from_bytes(&after)?;
                let role = ObjectRole::from_code(
                    u8::try_from(
                        row.get::<i64>(1)
                            .map_err(|_| StorageError::Integrity("object role"))?,
                    )
                    .map_err(|_| StorageError::Integrity("object role"))?,
                )?;
                let canonical = reader.read(id)?;
                if usize::try_from(
                    row.get::<i64>(2)
                        .map_err(|_| StorageError::Integrity("object length"))?,
                )
                .ok()
                    != Some(canonical.len())
                {
                    return Err(StorageError::Integrity("canonical length"));
                }
                let scope = if role == ObjectRole::FilesystemRoot {
                    FilesystemRoot::decode(&canonical)?.scope()
                } else {
                    InodeScope::from_object(ObjectId::for_bytes(b"diagnostic-local-grammar"))
                };
                Timing::disabled("diagnostic.admit", |timing| -> Result<(), StorageError> {
                    let object = FinalizedObject::admit(
                        id,
                        role,
                        canonical,
                        policy,
                        scope,
                        timing.child("admit"),
                    )?;
                    for child in object.references() {
                        let mut found = Vec::new();
                        view.locate(&[*child], &mut found)?;
                        if found.len() != 1 || found[0].location.object_id != *child {
                            return Err(StorageError::MissingDependency {
                                object: id,
                                reference: *child,
                            });
                        }
                    }
                    object.validate_stored_children(reader, policy, timing.child("context"))?;
                    Ok(())
                })
                .0
            })();
            options.check()?;
            if let Err(error) = result {
                let outcome = classify(&error);
                if outcome == Outcome::Fail {
                    failed += 1;
                } else {
                    incomplete = true;
                }
                emit(
                    observer,
                    "store.objects",
                    outcome,
                    format!("object {}: {error}", hex(&after)),
                );
            }
        }
    }
    emit(observer,"store.objects.summary",if failed>0{Outcome::Fail}else if incomplete{Outcome::Unknown}else{Outcome::Pass},format!("{checked} winning stored objects authenticated, locally decoded and reference-checked, including retained unreachable rows; {failed} failed"));
    Ok(())
}

fn blocks(
    view: &dyn View,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    let (mut after, mut checked, mut failed) = (0i64, 0u64, 0u64);
    loop {
        options.check()?;
        let rows=view.query("SELECT first_ordinal,last_ordinal,CASE WHEN blob_length(body) BETWEEN 0 AND 4000 THEN body ELSE NULL END FROM inode_value_block WHERE first_ordinal>?1 ORDER BY first_ordinal LIMIT 64",vec![Param::I64(after)])?;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            after = row.get(0)?;
            checked += 1;
            let result = (|| -> Result<(), BackendError> {
                let last = row.get::<i64>(1)?;
                let body = row.get::<Vec<u8>>(2)?;
                crate::blocks::values::decode(
                    u32::try_from(after).map_err(|_| BackendError::Integrity)?,
                    u32::try_from(last).map_err(|_| BackendError::Integrity)?,
                    &body,
                )?;
                Ok(())
            })();
            if result.is_err() {
                failed += 1;
                emit(observer,"store.ordinal_blocks",Outcome::Fail,format!("inode value block beginning at ordinal {after}: invalid framing/checksum/range"));
            }
        }
    }
    emit(
        observer,
        "store.ordinal_blocks.summary",
        if failed == 0 {
            Outcome::Pass
        } else {
            Outcome::Fail
        },
        format!("{checked} inode value blocks checked, including unused ordinals; {failed} failed"),
    );
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

fn classify(error: &StorageError) -> Outcome {
    super::observation::storage_outcome(error)
}
