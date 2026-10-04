//! Actual retained-history producer on shared C2/C5 SQLite authority.
//! Benchmark vehicle; qualification and matched baseline are separate.
#![allow(dead_code)]
#[path = "history_support/cold.rs"]
mod cold;
#[path = "history_support/observer.rs"]
mod observer;
#[path = "history_support/producer.rs"]
mod producer;
#[path = "history_support/retained.rs"]
mod retained;
#[path = "history_support/support.rs"]
mod support;
#[path = "history_support/workload.rs"]
mod workload;
use layerfs_content::filesystem::{
    build_filesystem, references::backing::FileBacking, update_filesystem, FilesystemInput,
    FilesystemObjects, FilesystemResources,
};
use layerfs_content::inode_leaf::InodeKind;
use layerfs_content::{
    construct_bytes, construct_bytes_with_predecessor, AdvisoryPredecessors, ConstructionPolicy,
    ObjectId, PredecessorBase, PredecessorProvenance,
};
use layerfs_persistence::{Handles, PersistenceConfig, SqlitePackLayout, SqlitePersistenceProfile};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::{collections::BTreeMap, path::PathBuf, time::Instant};
use workload::{
    history::{Change, Corpus, Row},
    providers::TreeStore,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if !matches!(args.len(), 6..=8) {
        return Err(
            "usage: benchmark_history CORPUS DB SCRATCH history-stride{10,3,1} complete|probe|probe-transition [durable|disposable]"
                .into(),
        );
    }
    let row = Row::from_id(&args[4]).ok_or("invalid history selection")?;
    let probe_states = match args[5].as_str() {
        "complete" => None,
        "probe" => Some(1),
        "probe-transition" => Some(2),
        _ => return Err("invalid operation mode".into()),
    };
    let selected_profile = match args.get(6).map(String::as_str).unwrap_or("durable") {
        "durable" => SqlitePersistenceProfile::Durable,
        "disposable" => SqlitePersistenceProfile::Disposable,
        _ => return Err("explicit durable/disposable profile required".into()),
    };
    let selected_layout = match args.get(7).map(String::as_str).unwrap_or("monolithic") {
        "monolithic" => SqlitePackLayout::Monolithic,
        "group-rows" => SqlitePackLayout::GroupRows,
        "group-rows-indexed" => SqlitePackLayout::GroupRowsIndexed,
        _ => {
            return Err("explicit monolithic/group-rows/group-rows-indexed layout required".into())
        }
    };
    for (key, value) in [
        ("LAYERFS_CONSTRUCTION_WORKERS", "1"),
        ("LAYERFS_HISTORY_ADVISORY", "1"),
        ("LAYERFS_HISTORY_CHUNK_PREDECESSORS", "1"),
        ("LAYERFS_HISTORY_FULL_PRODUCER", "0"),
        ("LAYERFS_HISTORY_ORDERED_PREDECESSORS", "0"),
        ("LAYERFS_HISTORY_SIMILARITY_CANDIDATES", "0"),
        ("LAYERFS_HISTORY_DEPTH_LIMIT", "255"),
    ] {
        if std::env::var(key).ok().as_deref() != Some(value) {
            return Err(format!("required {key}={value}").into());
        }
    }
    std::env::set_var("LAYERFS_SQLITE_SCOPE", "bootstrap");
    let begin = Instant::now();
    let mut corpus = Corpus::open(&PathBuf::from(&args[1]), row)?;
    let scope = producer::scope_of(row);
    let policy = ConstructionPolicy::frozen_default();
    let caps = policy.capacities();
    let handles = Handles::create(
        PersistenceConfig::sqlite(&args[2])
            .with_sqlite_profile(selected_profile)
            .with_sqlite_pack_layout(selected_layout),
        StoragePolicy::frozen_default(),
        &retained::config(),
    )?;
    let storage = Storage::new(handles.storage.clone())?;
    let profile_identity = handles.profile().identity;
    let bootstrap_ns = begin.elapsed().as_nanos();
    let mut cold = cold::Boundary::new(vec![PathBuf::from(&args[2])], probe_states.is_none())?;
    let mut held = None;
    let mut previous_root = None;
    let mut previous_content = BTreeMap::<Vec<u8>, ObjectId>::new();
    let mut same_path_root = BTreeMap::<Vec<u8>, ObjectId>::new();
    let mut depth = BTreeMap::<Vec<u8>, u8>::new();
    let mut index = producer::SimilarityIndex::new();
    let mut chain = producer::Chain::new();
    let mut roots = Vec::new();
    let scratch = PathBuf::from(&args[3]);
    std::fs::create_dir(&scratch)?;
    let mut stages = [0u64; 4];
    let count = probe_states.unwrap_or_else(|| row.states());
    let mut inventory = BTreeMap::<ObjectId, (u8, usize)>::new();
    std::env::set_var("LAYERFS_SQLITE_SCOPE", "operation");
    for position in 0..count {
        if position != 0 {
            cold.check(position + 1)?;
        }
        let state_stages_before = stages;
        let mut engine = observer::snapshot();
        let acquire = Instant::now();
        let transition = corpus.transition(position)?;
        stages[0] += acquire.elapsed().as_nanos() as u64;
        observer::report(position + 1, "acquisition", engine);
        engine = observer::snapshot();
        let start = Instant::now();
        let mut consumer = TreeStore::new();
        let reader = storage.reader()?;
        let mut constructed = BTreeMap::new();
        let mut signatures = BTreeMap::new();
        for change in &transition.changed {
            if matches!(change.kind, Change::Removed | Change::MetadataOnly) {
                continue;
            }
            let bytes = transition
                .blobs
                .get(&change.oid)
                .ok_or("missing changed blob")?;
            let base = previous_content
                .get(&change.path)
                .copied()
                .map(|id| PredecessorBase::new(&reader, id));
            let root = if producer::kind_of(change.mode) == InodeKind::Symlink {
                producer::emit_symlink_target(bytes, &mut consumer)?
            } else {
                Timing::disabled("history.content", |scope| match base {
                    Some(base) => construct_bytes_with_predecessor(
                        policy,
                        &caps,
                        bytes,
                        Some(base),
                        &mut consumer,
                        scope.child("file"),
                    ),
                    None => {
                        construct_bytes(policy, &caps, bytes, &mut consumer, scope.child("file"))
                    }
                })
                .0?
                .root
            };
            signatures.insert(
                root,
                layerfs_storage::encoding::delta::candidates::signature(bytes),
            );
            constructed.insert(change.path.clone(), root);
        }
        let mut bases = BTreeMap::new();
        for change in &transition.changed {
            if matches!(change.kind, Change::Removed | Change::MetadataOnly) {
                continue;
            }
            let Some(new_root) = constructed.get(&change.path).copied() else {
                continue;
            };
            if !signatures.contains_key(&new_root) {
                continue;
            }
            let previous = same_path_root.get(&change.path).copied();
            if previous == Some(new_root) {
                continue;
            }
            if let Some(candidate) = previous {
                let old_depth = index
                    .path_of
                    .get(&candidate)
                    .and_then(|path| depth.get(path))
                    .copied()
                    .unwrap_or(0);
                if old_depth < 255 {
                    bases.insert(new_root, vec![candidate]);
                    depth.insert(change.path.clone(), old_depth.saturating_add(1));
                }
            } else {
                depth.insert(change.path.clone(), 0);
            }
        }
        stages[1] += start.elapsed().as_nanos() as u64;
        observer::report(position + 1, "construction", engine);
        eprintln!(
            "HISTORY_PROVIDER_WORK state={} stage=construction cumulative={:?}",
            position + 1,
            storage.diagnostics()
        );
        eprintln!(
            "HISTORY_POOLED_WORK state={} stage=construction scope=operation-reader-only cumulative={:?}",
            position + 1,
            reader.pooled_read_counters()
        );
        engine = observer::snapshot();
        let start = Instant::now();
        let mut directories = Vec::new();
        let mut inodes = Vec::new();
        let mut new_inodes = Vec::new();
        producer::filesystem_input(
            &mut chain,
            &transition,
            &constructed,
            previous_root.is_none(),
            &mut directories,
            &mut inodes,
            &mut new_inodes,
        )?;
        let input = FilesystemInput {
            base: previous_root,
            scope,
            root_serial: 1,
            directories: &directories,
            inodes: &inodes,
            new_inodes: &new_inodes,
            resources: FilesystemResources::default(),
        };
        let mut backing = FileBacking::new(&scratch);
        let mut objects = FilesystemObjects::new(&reader, &mut consumer);
        let built = if previous_root.is_none() {
            build_filesystem(&mut objects, &input, Some(&mut backing))?
        } else {
            update_filesystem(&mut objects, &input, Some(&mut backing))?
        };
        stages[2] += start.elapsed().as_nanos() as u64;
        observer::report(position + 1, "filesystem", engine);
        eprintln!(
            "HISTORY_PROVIDER_WORK state={} stage=filesystem cumulative={:?}",
            position + 1,
            storage.diagnostics()
        );
        eprintln!(
            "HISTORY_POOLED_WORK state={} stage=filesystem scope=operation-reader-only cumulative={:?}",
            position + 1,
            reader.pooled_read_counters()
        );
        engine = observer::snapshot();
        let start = Instant::now();
        let save = storage.begin_save()?;
        for id in consumer.insertion_order() {
            let mut object = consumer
                .cloned_object(*id)
                .ok_or("consumer object vanished")?;
            if let Some(ordered) = bases.get(id) {
                let mut list = AdvisoryPredecessors::new();
                for id in ordered {
                    list.push(*id, PredecessorProvenance::OriginalBase)?;
                }
                object = object.with_predecessors(list);
            }
            let descriptor = (object.role().code(), object.canonical_len());
            if inventory
                .insert(*id, descriptor)
                .is_some_and(|previous| previous != descriptor)
            {
                return Err("canonical inventory descriptor mismatch".into());
            }
            save.accept(object)?;
        }
        for (path, root) in &constructed {
            if let Some(sig) = signatures.get(root).copied() {
                index.insert(*root, path, sig);
            }
            previous_content.insert(path.clone(), *root);
            same_path_root.insert(path.clone(), *root);
        }
        save.finish()?;
        if let Some(catalog) = held.as_mut() {
            retained::RetainedHistory::publish(catalog, transition.state.ordinal, built.root.0)?;
        } else {
            held = Some(retained::RetainedHistory::create(
                &handles.history,
                scope,
                built.root.0,
            )?);
        }
        stages[3] += start.elapsed().as_nanos() as u64;
        observer::report(position + 1, "save_custody", engine);
        eprintln!(
            "HISTORY_PROVIDER_WORK state={} stage=save_custody cumulative={:?}",
            position + 1,
            storage.diagnostics()
        );
        eprintln!(
            "HISTORY_POOLED_WORK state={} stage=save_custody scope=operation-reader-only cumulative={:?}",
            position + 1,
            reader.pooled_read_counters()
        );
        previous_root = Some(built.root);
        roots.push(built.root.0);
        let state_stages = std::array::from_fn::<_, 4, _>(|i| stages[i] - state_stages_before[i]);
        eprintln!("HISTORY_STATE_WORK {{\"state\":{},\"full157\":{},\"changed_paths\":{},\"offered_objects\":{},\"acquire_ns\":{},\"construct_ns\":{},\"filesystem_ns\":{},\"save_custody_ns\":{}}}", transition.state.ordinal, transition.state.full157_index, transition.changed.len(), consumer.insertion_order().len(), state_stages[0], state_stages[1], state_stages[2], state_stages[3]);
        eprintln!(
            "DIAGNOSTIC state={} full157={} root={:?}",
            transition.state.ordinal, transition.state.full157_index, built.root.0
        );
    }
    std::env::set_var("LAYERFS_SQLITE_SCOPE", "cleanup");
    cold.check(count + 1)?;
    let finalization = Instant::now();
    let custody_start = Instant::now();
    let custody = retained::verify(&handles.history, scope, &roots)?;
    let custody_ns = custody_start.elapsed().as_nanos();
    let checkpoint = handles.checkpoint()?;
    let profile = handles.profile();
    eprintln!(
        "EFFECTIVE_PACK_LAYOUT {{\"layout\":\"{:?}\"}}",
        profile.pack_layout
    );
    eprintln!("EFFECTIVE_PROFILE {{\"identity\":\"{}\",\"journal_mode\":\"{}\",\"synchronous\":{},\"foreign_keys\":{},\"fullfsync\":{},\"checkpoint_fullfsync\":{},\"page_size\":{},\"cache_size\":{},\"mmap_size\":{},\"temp_store\":{},\"wal_checkpoint_performed\":{}}}",profile.identity,profile.journal_mode,profile.synchronous,profile.foreign_keys,profile.fullfsync,profile.checkpoint_fullfsync,profile.page_size,profile.cache_size,profile.mmap_size,profile.temp_store,checkpoint.wal_checkpoint_performed);
    if checkpoint.busy {
        return Err("final checkpoint obstructed".into());
    }
    eprintln!(
        "DIAGNOSTIC sqlite={:?} storage={:?} checkpoint={checkpoint:?}",
        handles.diagnostics()?,
        storage.diagnostics()
    );
    let checkpoint_ns = checkpoint.wall_ns;
    let finalization_ns = finalization.elapsed().as_nanos();
    let close = Instant::now();
    drop(storage);
    drop(handles);
    let close_ns = close.elapsed().as_nanos();
    eprintln!(
        "HISTORY_COLD_TOTAL {{\"checks\":{},\"wall_ns\":{}}}",
        cold.checks, cold.wall_ns
    );
    if std::fs::read_dir(&scratch)?.next().is_some() {
        return Err("ordering scratch not empty".into());
    }
    let list = roots
        .iter()
        .map(|id| format!("\"{}\"", workload::digest::hex(id.as_bytes())))
        .collect::<Vec<_>>()
        .join(",");
    let canonical_bytes = inventory
        .values()
        .map(|(_, bytes)| *bytes as u64)
        .sum::<u64>();
    let mut census = workload::digest::Sha256::new();
    for (id, (role, bytes)) in &inventory {
        census.update(id.as_bytes());
        census.update(&[*role]);
        census.update(&(*bytes as u64).to_le_bytes());
    }
    println!("{{\"status\":\"{}\",\"selected_states\":{},\"states\":{},\"custody_states\":{},\"operation_ns\":{},\"profile_identity\":\"{}\",\"bootstrap_ns\":{},\"custody_ns\":{},\"checkpoint_ns\":{},\"finalization_ns\":{},\"close_ns\":{},\"stages_ns\":{:?},\"roots\":[{}],\"canonical_objects\":{},\"canonical_bytes\":{},\"canonical_inventory_sha256\":\"{}\"}}",if probe_states.is_some(){"DIAGNOSTIC"}else{"COMPLETE"},row.states(),count,custody,begin.elapsed().as_nanos(),profile_identity,bootstrap_ns,custody_ns,checkpoint_ns,finalization_ns,close_ns,stages,list,inventory.len(),canonical_bytes,workload::digest::hex(&census.finish()));
    Ok(())
}
