//! Count diagnostic using the actual pinned public Project Init; no product changes.
#[path = "attribution_vfs.rs"]
mod vfs;
use layerfs_content::ObjectId;
use layerfs_history::{HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{Handles, PersistenceConfig, SqlWork, SqlitePersistenceProfile};
use layerfs_project::{init, InitRequest};
use layerfs_storage::{Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::{collections::BTreeMap,path::Path,sync::{Arc,Mutex},time::{Duration,Instant}};
#[derive(Clone, Copy, Default)]
struct Work {
    calls: u64,
    statements: u64,
    vm_steps: u64,
    write_commits: u64,
    statement_ns: u64,
    commit_ns: u64,
    unit_ns: u64,
}
impl Work {
    fn add(&mut self, before: SqlWork, after: SqlWork, wall: u64) {
        self.calls += 1;
        self.statements += after.statements - before.statements;
        self.vm_steps += after.vm_steps - before.vm_steps;
        self.write_commits += after.write_commits - before.write_commits;
        self.statement_ns += after.statement_ns - before.statement_ns;
        self.commit_ns += after.commit_ns - before.commit_ns;
        self.unit_ns += wall;
    }
    fn print(&self, name: &str) {
        println!("{{\"unit\":\"{name}\",\"calls\":{},\"statements\":{},\"vm_steps\":{},\"returned_rows\":null,\"fullscan_steps\":null,\"sorts\":null,\"reprepares\":null,\"write_commits\":{},\"statement_ns\":{},\"commit_ns\":{},\"unit_ns\":{}}}",self.calls,self.statements,self.vm_steps,self.write_commits,self.statement_ns,self.commit_ns,self.unit_ns);
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 { return Err("source fresh-db scratch name required".into()); }
    vfs::initialize(std::env::var("LAYERFS_ATTRIBUTION_VFS").as_deref()==Ok("1"))?;
    let started=Instant::now();
    let io=vfs::span("bootstrap");
    let handles=Arc::new(Handles::create(
        PersistenceConfig::sqlite(&args[2]).with_sqlite_profile(SqlitePersistenceProfile::Durable),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {binding_key:b"layerfs-bench-pro".to_vec(),incarnation:1,cursor_key:[0x28;32]},
    )?);
    let pack=Arc::new(PackObserved {handles:handles.clone(),units:Mutex::new(BTreeMap::new())});
    let storage=Storage::new(pack.clone())?;
    let bootstrap_ns=started.elapsed().as_nanos();io.finish();
    let before=handles.diagnostics()?;
    let (result,timing)=Timing::record("project.init",|timer|init(&storage,&handles.history,
        InitRequest {source:Path::new(&args[1]),scratch_parent:Path::new(&args[3]),stack:LayerStackId::from_authority([0x41;16]),name:HistoryName::new(&args[4]).unwrap(),scope_seed:[0x42;32],deadline:started+Duration::from_secs(15)},timer));
    let result=result?;
    let mut total=Work::default();total.add(before,handles.diagnostics()?,0);total.print("complete_init");
    for (name,work) in pack.units.lock().unwrap().iter(){work.print(name);}
    eprintln!("TIMING_DIAGNOSTIC {timing:?}");
    eprintln!("BASE_DIAGNOSTIC bootstrap_ns={bootstrap_ns} profile={:?} root={:?} entries={} namespace={:?} saves={:?}",handles.profile(),result.root,result.entries,result.namespace_work,storage.save_work());
    let io=vfs::span("checkpoint");let checkpoint=handles.checkpoint()?;io.finish();
    if checkpoint.busy{return Err("checkpoint busy".into());}
    eprintln!("CHECKPOINT_DIAGNOSTIC {checkpoint:?}");
    let close=Instant::now();drop(storage);drop(pack);drop(handles);
    eprintln!("CLOSE_DIAGNOSTIC wall_ns={}",close.elapsed().as_nanos());
    Ok(())
}
/// Fixed public physical-operation classes, never an unbounded SQL trace.
struct PackObserved {
    handles: Arc<Handles>,
    units: Mutex<BTreeMap<&'static str, Work>>,
}
impl PackObserved {
    fn call<T>(
        &self,
        name: &'static str,
        body: impl FnOnce() -> Result<T, layerfs_storage::port::PersistenceError>,
    ) -> Result<T, layerfs_storage::port::PersistenceError> {
        let before = self.handles.diagnostics()?;
        let io = vfs::span(name);
        let start = Instant::now();
        let result = body();
        let wall = start.elapsed().as_nanos() as u64;
        let after = self.handles.diagnostics()?;
        io.finish();
        self.units
            .lock()
            .unwrap()
            .entry(name)
            .or_default()
            .add(before, after, wall);
        result
    }
}
impl layerfs_storage::port::PackPersistence for PackObserved {
    fn policy(&self) -> Result<StoragePolicy, layerfs_storage::port::PersistenceError> {
        self.call("storage.policy", || self.handles.storage.policy())
    }
    fn locate(
        &self,
        ids: &[ObjectId],
        out: &mut Vec<layerfs_storage::location::LocatedObject>,
    ) -> Result<(), layerfs_storage::port::PersistenceError> {
        self.call("storage.locate", || self.handles.storage.locate(ids, out))
    }
    fn read_packs(
        &self,
        ids: &[i64],
        out: &mut Vec<layerfs_storage::port::PersistedPack>,
    ) -> Result<(), layerfs_storage::port::PersistenceError> {
        self.call("storage.read_packs", || {
            self.handles.storage.read_packs(ids, out)
        })
    }
    fn read_pack_selection(
        &self,
        id: i64,
        plan: &mut dyn layerfs_storage::port::PackReadPlan,
    ) -> Result<layerfs_storage::port::PersistedPackRead, layerfs_storage::port::PersistenceError>
    {
        self.call("storage.read_pack_selection", || {
            self.handles.storage.read_pack_selection(id, plan)
        })
    }
    fn read_scoped_pack(
        &self,
        id: i64,
        plan: &mut dyn layerfs_storage::port::PackReadPlan,
    ) -> Result<layerfs_storage::port::AcquiredPackRead, layerfs_storage::port::PersistenceError>
    {
        self.call("storage.read_scoped_pack", || {
            self.handles.storage.read_scoped_pack(id, plan)
        })
    }
    fn value_groups(
        &self,
        query: layerfs_storage::port::ValueGroupQuery<'_>,
    ) -> Result<layerfs_storage::port::ValueGroups, layerfs_storage::port::PersistenceError> {
        self.call("storage.value_groups", || {
            self.handles.storage.value_groups(query)
        })
    }
    fn signatures(
        &self,
        out: &mut Vec<layerfs_storage::location::SignatureRow>,
    ) -> Result<(), layerfs_storage::port::PersistenceError> {
        self.call("storage.signatures", || {
            self.handles.storage.signatures(out)
        })
    }
    fn reserve(
        &self,
        request: layerfs_storage::port::Reserve,
    ) -> Result<layerfs_storage::port::Reserved, layerfs_storage::port::PersistenceError> {
        eprintln!("RESERVE_DIAGNOSTIC {request:?}");
        self.call("storage.reserve", || self.handles.storage.reserve(request))
    }
    fn publication_pack_cost(
        &self,
        pack: &layerfs_storage::port::PublishedPack,
    ) -> Result<(usize, u64), layerfs_storage::port::PersistenceError> {
        self.handles.storage.publication_pack_cost(pack)
    }
    fn publish(
        &self,
        batch: &layerfs_storage::port::Publication,
    ) -> Result<layerfs_storage::port::Published, layerfs_storage::port::PersistenceError> {
        eprintln!("PUBLICATION_DIAGNOSTIC packs={} body_bytes={} objects={} value_groups={} signatures={} release={:?}",batch.packs.len(),batch.packs.iter().map(|p|p.body.len()).sum::<usize>(),batch.objects.len(),batch.value_groups.len(),batch.signatures.len(),batch.release_ordinals);
        self.call("storage.publish", || self.handles.storage.publish(batch))
    }
}
