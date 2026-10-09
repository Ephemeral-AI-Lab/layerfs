use crate::{args::Args, events::Events, runtime, Result};
use layerfs_bridge::{
    control::{Reply, Request},
    provision::StoreManifest,
};
use layerfs_history::{BranchId, CommitStagedOutcome, WorkspaceId};
use layerfs_sdk::{MountedWorkspace, WorkspaceApi};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, BufRead},
    path::Path,
    time::Instant,
};

fn workspace(key: &str) -> Result<WorkspaceId> {
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("workspace key must be 64 hex digits".into());
    }
    let mut id = [0; 32];
    for (index, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&key[index * 2..index * 2 + 2], 16)?;
    }
    Ok(WorkspaceId::from_authority(id)?)
}
fn selected<'a>(
    mounts: &'a BTreeMap<String, MountedWorkspace>,
    key: &str,
) -> Result<&'a MountedWorkspace> {
    mounts
        .get(key)
        .ok_or_else(|| format!("unknown mounted key {key}").into())
}

/// Keeps daemon caches, Store sessions and one overlay alive across explicit calls.
pub fn serve(args: &Args, events: &mut Events) -> Result<()> {
    let manifest = StoreManifest::decode(&fs::read(&args.manifest)?)?;
    let branch = BranchId::from_bytes(manifest.branch)?;
    let mut owner = runtime::start(args, Some(manifest), events)?;
    let mut mounts = BTreeMap::new();
    let mut closed = BTreeMap::new();
    let mut used_keys = BTreeSet::new();
    let origin = owner.control.channel_work().0.records;
    if origin != 1 {
        return Err(
            "serve fresh Control must have exactly one Hello and no installation sends".into(),
        );
    }
    events.numeric_fields(
        "control_origin",
        None,
        "fresh serve Control; records equal successful call IDs on this channel",
        &[],
        &[("control_send_records", origin)],
    )?;
    events.value("protocol_ready", "mount KEY | command KEY BODY_FILE | commit KEY | snapshot KEY | unmount KEY | cleanup KEY | observe | stop; fields separated by tabs")?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let fields: Vec<&str> = line.splitn(3, '\t').collect();
        let operation = fields[0];
        let key = fields.get(1).copied().unwrap_or("");
        match operation {
            "mount" => {
                if used_keys.contains(&key.to_ascii_lowercase()) {
                    return Err("reused Workspace authority key; fresh incarnation required".into());
                }
                let id = workspace(key)?;
                let before = owner.control.channel_work().0.records;
                let begun = Instant::now();
                let mounted =
                    WorkspaceApi::with_observations(&mut owner.control, &args.observation_scope)
                        .mount(id, branch)
                        .map_err(|failure| format!("mount original failure {failure:?}"))?;
                let elapsed = events.span(begun);
                let after = owner.control.channel_work().0.records;
                events.numeric_fields(
                    "mount",
                    Some(elapsed),
                    &format!("bound={:?} ready={:?}", mounted.bound, mounted.ready),
                    &[
                        ("key", key.into()),
                        ("directory", mounted.ready.directory.clone()),
                    ],
                    &[
                        ("control_send_records_before", before),
                        ("control_send_records_after", after),
                    ],
                )?;
                used_keys.insert(key.to_ascii_lowercase());
                mounts.insert(key.into(), mounted);
            }
            "command" | "verify" => {
                let directory = if let Some(native) = key.strip_prefix("native:") {
                    if !native.starts_with('/') {
                        return Err("native directory must be absolute".into());
                    }
                    native.to_owned()
                } else {
                    selected(&mounts, key)?.ready.directory.clone()
                };
                let path = Path::new(fields.get(2).ok_or("command body file required")?);
                let body = runtime::body(path)?;
                runtime::command(
                    &owner,
                    &body,
                    &directory,
                    operation,
                    false,
                    &args.environment,
                    events,
                )?;
            }
            "commit" => {
                let token = selected(&mounts, key)?.bound.token;
                let before = owner.control.channel_work().0.records;
                let begun = Instant::now();
                let outcome =
                    WorkspaceApi::with_observations(&mut owner.control, &args.observation_scope)
                        .commit(token)?;
                let elapsed = events.span(begun);
                let after = owner.control.channel_work().0.records;
                let commit_kind = match &outcome {
                    CommitStagedOutcome::Committed(_) => "Committed",
                    CommitStagedOutcome::UpToDate { .. } => "UpToDate",
                };
                events.numeric_fields(
                    "commit",
                    Some(elapsed),
                    &format!("key={key} original_outcome={outcome:?}"),
                    &[("key", key.into()), ("commit_kind", commit_kind.into())],
                    &[
                        ("control_send_records_before", before),
                        ("control_send_records_after", after),
                    ],
                )?;
            }
            "snapshot" => {
                let token = selected(&mounts, key)?.bound.token;
                let before = owner.control.channel_work().0.records;
                let observed =
                    WorkspaceApi::with_observations(&mut owner.control, &args.observation_scope)
                        .status(token)?;
                let after = owner.control.channel_work().0.records;
                let mut numbers = vec![
                    ("control_send_records_before", before),
                    ("control_send_records_after", after),
                    (
                        "status_state_attribution_available",
                        u64::from(observed.local.is_some()),
                    ),
                ];
                if observed.local.is_some() {
                    numbers.push(("status_state_lifecycle_jobs", 1));
                }
                events.numeric_fields(
                    "workspace_status",
                    None,
                    &format!("key={key} observed={observed:?}"),
                    &[("key", key.into())],
                    &numbers,
                )?;
                runtime::snapshot(&owner, &args.socket, events)?;
            }
            "observe" => runtime::snapshot(&owner, &args.socket, events)?,
            "unmount" => {
                let token = selected(&mounts, key)?.bound.token;
                let before = owner.control.channel_work().0.records;
                let begun = Instant::now();
                WorkspaceApi::with_observations(&mut owner.control, &args.observation_scope)
                    .unmount(token)?;
                let elapsed = events.span(begun);
                let after = owner.control.channel_work().0.records;
                events.numeric_fields("unmount", Some(elapsed), &format!("key={key} token={token:?}; logical_terminal=true; physical_cleanup requires its own original observation"), &[("key",key.into())], &[("control_send_records_before",before),("control_send_records_after",after)])?;
                mounts.remove(key);
                closed.insert(key.to_owned(), token);
            }
            "cleanup" => {
                let token = *closed
                    .get(key)
                    .ok_or("unknown original closed Workspace key")?;
                let before = owner.control.channel_work().0.records;
                let begun = Instant::now();
                let state =
                    WorkspaceApi::with_observations(&mut owner.control, &args.observation_scope)
                        .cleanup(token)?;
                let span = events.span(begun);
                let after = owner.control.channel_work().0.records;
                events.numeric_fields(
                    "cleanup",
                    Some(span),
                    "original read-only cleanup observation; no replay or maintenance invocation",
                    &[("key", key.into()), ("state", format!("{state:?}"))],
                    &[
                        ("control_send_records_before", before),
                        ("control_send_records_after", after),
                    ],
                )?;
                if matches!(state, layerfs_bridge::control::CleanupObservation::Gone) {
                    closed.remove(key);
                }
            }
            "stop" => {
                if !mounts.is_empty() {
                    return Err("explicit unmount required before stop".into());
                }
                if owner.control.call(Request::EndSession)? != Reply::SessionEnded {
                    return Err("unexpected original EndSession acknowledgement".into());
                }
                owner
                    .runtime
                    .stop(1)
                    .map_err(|e| format!("explicit container stop {e:?}"))?;
                events.value("container_stopped", &owner.runtime.identity().to_string())?;
                return Ok(());
            }
            _ => return Err(format!("unknown protocol command {operation}").into()),
        }
    }
    Err("stdin ended without explicit stop; container and original custody retained".into())
}
