//! One C2 save per mutation. Root delivery follows validated EOF and finish.
use super::{filesystem, metadata, validation::validate_inode_role};
use crate::service::input::Exact;
use crate::service::{
    error::{content, storage},
    handler::end_input,
    read::content::id,
};
use layerfs_bridge::contract::*;
use layerfs_content::filesystem::symlink::{emit_symlink, SymlinkTarget};
use layerfs_content::filesystem::FilesystemObjects;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{apply_edits, construct_stream, EditRequest, EditStream, Replacements};
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
    // Replay acquisition is part of this operation and precedes mutation ownership.
    let mut replacements = Replacements::new();
    if let Operation::EditFile { edits, .. } = &r.operation {
        for e in edits {
            let n = usize::try_from(e.replacement).map_err(|_| Code::Capacity)?;
            let mut part = vec![0; n];
            input.read_exact(&mut part)?;
            replacements.push(part);
        }
        end_input(input)?;
    }
    if matches!(
        r.operation,
        Operation::UpdatePreparedFilesystem { .. }
            | Operation::ConstructSymlink { .. }
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
        Operation::ConstructFile { length } => {
            let mut source = Exact::new(input, *length, deadline);
            construct_stream(
                policy,
                &capacities,
                &mut source,
                &mut handoff,
                scope.child("service.construct"),
            )
            .map_err(content)
            .and_then(|file| {
                end_input(&mut source)?;
                if file.logical_len != *length {
                    return Err(Code::InvalidInput.into());
                }
                Ok((*file.root.as_bytes(), file.logical_len))
            })
        }
        Operation::EditFile {
            root,
            base_length,
            edits,
        } => {
            let edits = EditStream::new(
                *base_length,
                edits
                    .iter()
                    .map(|e| layerfs_content::Edit::new(e.start, e.end, e.replacement))
                    .collect(),
            )
            .map_err(content);
            edits.and_then(|edits| {
                apply_edits(
                    policy,
                    &capacities,
                    &provider,
                    EditRequest {
                        root: id(root),
                        edits: &edits,
                        source: &replacements,
                    },
                    &mut handoff,
                    scope.child("service.edit"),
                )
                .map(|f| (*f.root.as_bytes(), f.logical_len))
                .map_err(content)
            })
        }
        Operation::UpdatePreparedFilesystem {
            base,
            scope: allocation,
            root_serial,
            directories,
            inodes,
            new_directories,
            directory_metadata,
            new_file_serials,
            new_symlink_serials,
        } => (|| {
            for (serials, kind) in [
                (new_file_serials, InodeKind::RegularFile),
                (new_symlink_serials, InodeKind::Symlink),
            ] {
                for serial in serials {
                    if Instant::now() >= deadline {
                        return Err(Code::Deadline.into());
                    }
                    let index = inodes
                        .binary_search_by_key(serial, |inode| inode.serial)
                        .map_err(|_| Code::InvalidInput)?;
                    let inode = &inodes[index];
                    validate_inode_role(
                        &provider,
                        kind,
                        id(&inode.content),
                        id(&inode.metadata),
                        scope,
                    )?;
                }
            }
            filesystem::update(
                &provider,
                &filesystem::PreparedUpdate {
                    base: *base,
                    scope: *allocation,
                    root_serial: *root_serial,
                    directories,
                    inodes,
                    new_directories,
                    directory_metadata,
                    new_file_serials,
                    new_symlink_serials,
                },
                &mut handoff,
                deadline,
                scope,
            )
        })(),
        _ => Err(Code::Unsupported.into()),
    };
    let retained = handoff.take_failure();
    drop(handoff);
    let result = match retained {
        Some(error) => Err(storage(error)),
        None => built,
    };
    let result = result.and_then(|v| {
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
            if matches!(r.operation, Operation::UpdatePreparedFilesystem { .. }) {
                return Ok(Response::FilesystemSaved {
                    root,
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
