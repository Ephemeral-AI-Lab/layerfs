use crate::{args::Args, events::Events, runtime, Result};
use layerfs_bridge::{control::{Reply, Request}, provision::StoreManifest};
use layerfs_history::{BranchId, WorkspaceId};
use layerfs_sdk::{MountedWorkspace, WorkspaceApi};
use std::{collections::{BTreeMap, BTreeSet}, fs, io::{self, BufRead}, path::Path, time::Instant};

fn workspace(key: &str) -> Result<WorkspaceId> {
    if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("workspace key must be 64 hex digits".into()); }
    let mut id = [0;32];
    for (index, byte) in id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&key[index*2..index*2+2],16)?;
    }
    Ok(WorkspaceId::from_authority(id)?)
}
fn selected<'a>(mounts: &'a BTreeMap<String, MountedWorkspace>, key: &str) -> Result<&'a MountedWorkspace> {
    mounts.get(key).ok_or_else(||format!("unknown mounted key {key}").into())
}

/// Keeps daemon caches, Store sessions and one overlay alive across explicit calls.
pub fn serve(args: &Args, events: &mut Events) -> Result<()> {
    let manifest = StoreManifest::decode(&fs::read(&args.manifest)?)?;
    let branch = BranchId::from_bytes(manifest.branch)?;
    let mut owner = runtime::start(args, Some(manifest), events)?;
    let mut mounts = BTreeMap::new();
    let mut used_keys = BTreeSet::new();
    events.value("protocol_ready", "mount KEY | command KEY BODY_FILE | commit KEY | snapshot KEY | unmount KEY | observe | stop; fields separated by tabs")?;
    for line in io::stdin().lock().lines() {
        let line = line?;
        let fields: Vec<&str> = line.splitn(3,'\t').collect();
        let operation = fields[0];
        let key = fields.get(1).copied().unwrap_or("");
        match operation {
            "mount" => {
                if used_keys.contains(&key.to_ascii_lowercase()) { return Err("reused Workspace authority key; fresh incarnation required".into()); }
                let id = workspace(key)?;
                let begun = Instant::now();
                let mounted = WorkspaceApi::new(&mut owner.control).mount(id, branch)
                    .map_err(|failure| format!("mount original failure {failure:?}"))?;
                let elapsed = events.span(begun);
                events.phase("mount", Some(elapsed), &format!("key={key} bound={:?} ready={:?}", mounted.bound, mounted.ready))?;
                used_keys.insert(key.to_ascii_lowercase());
                mounts.insert(key.into(), mounted);
            }
            "command" | "verify" => {
                let directory = if let Some(native) = key.strip_prefix("native:") {
                    if !native.starts_with('/') { return Err("native directory must be absolute".into()); }
                    native.to_owned()
                } else { selected(&mounts,key)?.ready.directory.clone() };
                let path = Path::new(fields.get(2).ok_or("command body file required")?);
                let body = runtime::body(path)?;
                runtime::command(&owner, &body, &directory, operation, false, events)?;
            }
            "commit" => {
                let token = selected(&mounts,key)?.bound.token;
                let begun = Instant::now();
                let outcome = WorkspaceApi::new(&mut owner.control).commit(token)?;
                let elapsed = events.span(begun);
                events.phase("commit", Some(elapsed), &format!("key={key} original_outcome={outcome:?}"))?;
            }
            "snapshot" => {
                let token = selected(&mounts,key)?.bound.token;
                let observed = WorkspaceApi::new(&mut owner.control).status(token)?;
                events.value("workspace_status", &format!("key={key} observed={observed:?}"))?;
                runtime::snapshot(&owner,events)?;
            }
            "observe" => runtime::snapshot(&owner,events)?,
            "unmount" => {
                let token = selected(&mounts,key)?.bound.token;
                let begun = Instant::now();
                WorkspaceApi::new(&mut owner.control).unmount(token)?;
                let elapsed = events.span(begun);
                events.phase("unmount", Some(elapsed), &format!("key={key} token={token:?}; logical_terminal=true; physical_cleanup=UNAVAILABLE via current control wire"))?;
                mounts.remove(key);
            }
            "stop" => {
                if !mounts.is_empty() { return Err("explicit unmount required before stop".into()); }
                if owner.control.call(Request::EndSession)? != Reply::SessionEnded {
                    return Err("unexpected original EndSession acknowledgement".into());
                }
                owner.runtime.stop(1).map_err(|e| format!("explicit container stop {e:?}"))?;
                events.value("container_stopped", &owner.runtime.identity().to_string())?;
                return Ok(());
            }
            _ => return Err(format!("unknown protocol command {operation}").into()),
        }
    }
    Err("stdin ended without explicit stop; container and original custody retained".into())
}
