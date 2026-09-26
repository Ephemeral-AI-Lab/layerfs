//! One C2 save per mutation. Root delivery follows validated EOF and finish.
use super::file_stream;
use super::metadata;
use crate::service::{
    error::{content, storage},
    handler::end_input,
    read::content::id,
};
use layerfs_bridge::contract::*;
use layerfs_content::filesystem::symlink::{emit_symlink, SymlinkTarget};
use layerfs_content::filesystem::FilesystemObjects;
use layerfs_content::{apply_edits, construct_stream, EditRequest};
use layerfs_storage::{SaveHandoff, Store, StoreProvider};
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{io::Read, time::Instant};
pub fn mutate(
    store: &Store,
    r: &Request,
    input: &mut dyn Read,
    deadline: Instant,
    scope: &TimingScope<'_, Active>,
) -> Result<Response, Failure> {
    // Validate and spool the frozen final state before acquiring save ownership.
    let mut file_input = None;
    if let Operation::SaveFile {
        base,
        base_length,
        length,
        extents,
        replacement,
    } = &r.operation
    {
        file_input = Some(file_stream::read(
            input,
            base.is_some(),
            *base_length,
            *length,
            *extents,
            *replacement,
            deadline,
        )?);
        end_input(input)?;
    }
    if matches!(
        r.operation,
        Operation::ConstructSymlink { .. }
            | Operation::UpdatePortableMetadata { .. }
            | Operation::ConstructPortableMetadata { .. }
    ) {
        end_input(input)?;
    }
    if Instant::now() >= deadline {
        return Err(Code::Deadline.into());
    }
    let mut save = store
        .begin_save(scope.child("service.begin_save"))
        .map_err(storage)?;
    let provider = StoreProvider::new(store);
    let mut handoff = SaveHandoff::new(&mut save);
    let policy = store.policy().construction();
    let capacities = policy.capacities();
    let built = match &r.operation {
        Operation::ConstructSymlink { target } => scope.child("service.symlink").run(|_| {
            let checked = SymlinkTarget::new(target.clone()).map_err(content)?;
            let root = emit_symlink(
                &mut FilesystemObjects::new(&provider, &mut handoff),
                checked,
            )
            .map_err(content)?;
            Ok((*root.as_bytes(), target.len() as u64))
        }),
        Operation::UpdatePortableMetadata { .. } | Operation::ConstructPortableMetadata { .. } => {
            scope
                .child("service.metadata")
                .run(|_| metadata::save(&provider, r, &mut handoff, deadline))
        }
        Operation::SaveFile { base, length, .. } => {
            let file_input = file_input.as_ref().ok_or(Code::InvalidInput)?;
            let built = if let Some(root) = base {
                apply_edits(
                    policy,
                    &capacities,
                    &provider,
                    EditRequest {
                        root: id(root),
                        edits: file_input,
                        source: file_input,
                    },
                    &mut handoff,
                    scope.child("service.save_file"),
                )
            } else {
                construct_stream(
                    policy,
                    &capacities,
                    &mut file_input.reader(),
                    &mut handoff,
                    scope.child("service.save_file"),
                )
            };
            built.map_err(content).and_then(|f| {
                if f.logical_len != *length {
                    return Err(Code::InvalidInput.into());
                }
                if std::env::var_os("LAYERFS_COMPLEXITY_DIAGNOSTIC").is_some() {
                    eprintln!(
                        "LFS_C1_SAVE_COUNT v=1 replacement_bytes={} spool_resident={} nodes_read={} nodes_created={} payloads_created={} payload_bytes={} peak_deferred_bytes={}",
                        file_input.replacement_bytes(),
                        file_input.resident(),
                        f.counters.nodes_read,
                        f.counters.nodes_created,
                        f.counters.payloads_created,
                        f.counters.payload_bytes,
                        f.counters.peak_deferred_bytes
                    );
                }
                Ok((*f.root.as_bytes(), f.logical_len))
            })
        }
        _ => Err(Code::Unsupported.into()),
    };
    let retained = handoff.take_failure();
    drop(handoff);
    let result = match retained {
        Some(error) => Err(storage(error)),
        None => built,
    };
    let result = result.and_then(|v| {
        if let Some(input) = file_input.as_mut() {
            input.cleanup()?;
        }
        if Instant::now() >= deadline {
            Err(Code::Deadline.into())
        } else {
            Ok(v)
        }
    });
    match result {
        Ok((root, length)) => {
            let outcome = save
                .finish(scope.child("service.finish"))
                .map_err(storage)?;
            if std::env::var_os("LAYERFS_FINISH_DIAGNOSTIC").is_some() {
                let diag = outcome.profile.diag;
                eprintln!(
                    "LFS_FINISH_COUNT v=1 request={} inserted={} reused={} full={} prefix={} packs={} pack_bytes={} statements={} commits={} batch_objects={} batch_bytes={} pool_entries={} pool_bytes={} candidates_entries={} candidates_bytes={} pool_clone_skipped={} candidates_clone_skipped={} chain_objects={} chain_encoded_bytes={} pooled_pack_fetches={} pooled_pack_bytes={} phase_pages=UNAVAILABLE phase_physical_read_bytes=UNAVAILABLE",
                    r.id,
                    outcome.inserted,
                    outcome.reused,
                    outcome.full_records,
                    outcome.prefix_records,
                    outcome.packs_created,
                    outcome.pack_bytes_written,
                    outcome.statements,
                    outcome.commits,
                    diag.finish_batch_objects,
                    diag.finish_batch_bytes,
                    diag.finish_pool_entries,
                    diag.finish_pool_bytes,
                    diag.finish_candidate_entries,
                    diag.finish_candidate_bytes,
                    diag.finish_pool_clone_skipped,
                    diag.finish_candidate_clone_skipped,
                    outcome.chain.objects,
                    outcome.chain.encoded_bytes,
                    outcome.chain.pooled.pack_fetches,
                    outcome.chain.pooled.pack_bytes,
                );
                if matches!(&r.operation, Operation::SaveFile { .. }) {
                    eprintln!(
                        "LFS_FINISH_SUBSTEP v=1 request={} scope=save-wide flush_batch_ns={} wave_ns={} offer_total_ns={} seal_total_ns={} write_pack_total_ns={} validate_ns={} collision_query_ns={} rows_ns={} members_ns={} insert_objects_ns={} sql_ns={} finish_drain_ns={} finish_batch_objects={} inserted={} reused={}",
                        r.id,
                        diag.flush_batch_ns,
                        diag.wave_ns,
                        diag.offer_total_ns,
                        diag.seal_total_ns,
                        diag.write_pack_total_ns,
                        diag.validate_ns,
                        diag.collision_query_ns,
                        diag.rows_ns,
                        diag.members_ns,
                        diag.insert_objects_ns,
                        outcome.profile.sql_ns,
                        diag.finish_drain_ns,
                        diag.finish_batch_objects,
                        outcome.inserted,
                        outcome.reused,
                    );
                }
            }
            if let Operation::UpdatePortableMetadata {
                base,
                kind,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            } = r.operation
            {
                return Ok(Response::MetadataSaved {
                    base,
                    kind,
                    mode,
                    mtime_seconds,
                    mtime_nanoseconds,
                    metadata: root,
                    inserted: outcome.inserted,
                    reused: outcome.reused,
                });
            }
            if let Operation::ConstructPortableMetadata {
                kind,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            } = r.operation
            {
                return Ok(Response::MetadataConstructed {
                    kind,
                    mode,
                    mtime_seconds,
                    mtime_nanoseconds,
                    metadata: root,
                    inserted: outcome.inserted,
                    reused: outcome.reused,
                });
            }
            Ok(Response::Saved {
                root,
                length,
                inserted: outcome.inserted,
                reused: outcome.reused,
            })
        }
        Err(mut error) => {
            if !error.unknown {
                if let Err(cleanup) = save.abort(scope.child("service.abort")) {
                    let cleanup = storage(cleanup);
                    error.cleanup = Some(cleanup.code);
                    error.unknown |= cleanup.unknown;
                }
            }
            Err(error)
        }
    }
}
