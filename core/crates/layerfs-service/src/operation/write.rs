//! One C2 save per mutation. Root delivery follows validated EOF and finish.
use super::{
    dispatch::end_input,
    failure::{content, storage},
    filesystem,
    read::id,
};
use crate::input::Exact;
use layerfs_bridge::contract::*;
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
    if let Operation::UpdatePreparedFilesystem { .. } = &r.operation {
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
        } => filesystem::update(
            &provider,
            &filesystem::PreparedUpdate {
                base: *base,
                scope: *allocation,
                root_serial: *root_serial,
                directories,
                inodes,
            },
            &mut handoff,
            scope,
        ),
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
