use crate::{args::Args, events::Events, runtime, Result};
use layerfs_bridge::{control::{Reply, Request}, native};
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_sdk::{InitRequest, ProjectApi};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::{fs::OpenOptions, io::Write, os::unix::fs::{MetadataExt, OpenOptionsExt}, time::{Duration, Instant}};

/// Host Init from an independently prepared copy, once; SDK installation once.
pub fn provision(args: &Args, events: &mut Events) -> Result<()> {
    let source = args.source.as_ref().ok_or("--source-copy required")?.canonicalize()?;
    if source.starts_with("/Users/yifanxu/Ephemeral-AI-Lab/deepseek-harness") {
        return Err("protected fixture checkout cannot be used: prepare an independent copy".into());
    }
    let sealed = args.sealed.as_ref().ok_or("--sealed required")?;
    let metadata = source.metadata()?;
    if (metadata.uid(), metadata.gid()) != (args.uid, args.gid) {
        return Err("command identity must equal preserved source-copy owner".into());
    }
    let stack_key = native::generate_keypair()?.public;
    let branch_key = native::generate_keypair()?.public;
    let cursor_key = native::generate_keypair()?.private;
    let scope_seed = native::generate_keypair()?.public;
    let mut stack = [0;16]; stack.copy_from_slice(&stack_key[..16]);
    let mut branch = [0;16]; branch.copy_from_slice(&branch_key[..16]);
    let request = InitRequest {
        source,
        store: PersistenceConfig::sqlite(sealed).with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        locator: "/layerfs-store/global/store.sqlite".into(),
        policy: StoragePolicy::frozen_default(),
        catalog: HistoryCatalogConfig { binding_key: b"r7-exploratory-runtime".to_vec(), cursor_key, incarnation: 1 },
        stack: LayerStackId::from_authority(stack), stack_name: HistoryName::new("project")?,
        branch: BranchId::from_authority(branch), branch_name: HistoryName::new("main")?,
        scope_seed, deadline: Instant::now() + Duration::from_secs(args.init_seconds),
    };
    let begun = Instant::now();
    let project = Timing::disabled("r7.setup.init", |timer| ProjectApi::new().init(request, timer)).0?;
    let elapsed = events.span(begun);
    let sealed_stat = project.store.path.metadata()?;
    events.phase("host_init_setup", Some(elapsed), &format!("sealed={} logical_bytes={} allocated_bytes={} entries={} sqlite={} root={:?} diagnostics={:?} namespace_work={:?}", project.store.path.display(), sealed_stat.len(), sealed_stat.blocks()*512, project.initialized.entries, project.store.sqlite_version, project.initialized.root, project.initialized.diagnostics, project.initialized.namespace_work))?;
    let mut owner = runtime::start(args, None, events)?;
    let begun = Instant::now();
    let installed = owner.control.install_project(&project)?;
    let elapsed = events.span(begun);
    events.phase("install_setup", Some(elapsed), &format!("work={:?} host_sqlite={} daemon_sqlite={:?}", installed.work, installed.manifest.host_sqlite, installed.manifest.daemon_sqlite))?;
    let mut manifest = OpenOptions::new().write(true).create_new(true).mode(0o600).open(&args.manifest)?;
    manifest.write_all(&installed.manifest.encode()?)?;
    events.value("installed_manifest", &args.manifest.display().to_string())?;
    runtime::snapshot(&owner, events)?;
    if owner.control.call(Request::EndSession)? != Reply::SessionEnded {
        return Err("unexpected original EndSession acknowledgement".into());
    }
    owner.runtime.stop(1).map_err(|e| format!("explicit setup container stop {e:?}"))?;
    events.value("setup_container_stopped", &owner.runtime.identity().to_string())?;
    // Container, volume, source copy and sealed master remain retained. The
    // lead alone decides removal; this harness never deletes any resource.
    Ok(())
}
