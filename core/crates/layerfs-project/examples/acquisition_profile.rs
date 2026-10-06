//! Count attribution through the ordinary public Init/acquisition ports.
//! Diagnostic instrumentation and uncontrolled cache; not a performance arm.
use layerfs_content::ObjectId;
use layerfs_history::{HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{
    Handles, PersistenceConfig, SqlWork, SqliteAcquisitionSchema, SqlitePersistenceProfile,
};
use layerfs_project::{init, InitRequest};
use layerfs_storage::{port::acquisition::*, Storage, StoragePolicy};
use layerfs_telemetry::timer::Timing;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Default)]
struct Work {
    calls: u64,
    statements: u64,
    vm_steps: u64,
    rows: u64,
    fullscan: u64,
    sorts: u64,
    reprepares: u64,
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
        self.rows += after.returned_rows - before.returned_rows;
        self.fullscan += after.fullscan_steps - before.fullscan_steps;
        self.sorts += after.sorts - before.sorts;
        self.reprepares += after.reprepares - before.reprepares;
        self.write_commits += after.write_commits - before.write_commits;
        self.statement_ns += after.statement_ns - before.statement_ns;
        self.commit_ns += after.commit_ns - before.commit_ns;
        self.unit_ns += wall;
    }
    fn print(&self, name: &str) {
        println!("{{\"unit\":\"{name}\",\"calls\":{},\"statements\":{},\"vm_steps\":{},\"returned_rows\":{},\"fullscan_steps\":{},\"sorts\":{},\"reprepares\":{},\"write_commits\":{},\"statement_ns\":{},\"commit_ns\":{},\"unit_ns\":{}}}", self.calls, self.statements, self.vm_steps, self.rows, self.fullscan, self.sorts, self.reprepares, self.write_commits, self.statement_ns, self.commit_ns, self.unit_ns);
    }
}
struct Observed<'a> {
    handles: &'a Handles,
    // Fixed method-name set: at most 18 entries, independent of input size.
    units: Mutex<BTreeMap<&'static str, Work>>,
}
impl Observed<'_> {
    fn call<T>(
        &self,
        name: &'static str,
        f: impl FnOnce() -> AcquisitionResult<T>,
    ) -> AcquisitionResult<T> {
        let before = self.handles.diagnostics().expect("before-unit counters");
        let start = Instant::now();
        let result = f();
        let wall = start.elapsed().as_nanos() as u64;
        let after = self.handles.diagnostics().expect("after-unit counters");
        self.units
            .lock()
            .unwrap()
            .entry(name)
            .or_default()
            .add(before, after, wall);
        result
    }
}
macro_rules! forward {
    ($name:ident($($arg:ident:$ty:ty),*) -> $out:ty) => {
        fn $name(&self, $($arg:$ty),*) -> AcquisitionResult<$out> {
            self.call(stringify!($name), || self.handles.acquisition.$name($($arg),*))
        }
    };
}
impl Acquisition for Observed<'_> {
    fn placement(&self) -> Option<PathBuf> {
        self.handles.acquisition.placement()
    }
    forward!(begin(facts:&Begin) -> Owner);
    forward!(put_entries(owner:Owner, entries:&[NewEntry]) -> Vec<u64>);
    forward!(unplaced_children(owner:Owner, parent:u64, after:Option<&[u8]>, limits:Limits) -> Vec<Unplaced>);
    forward!(place_children(owner:Owner, parent:u64, placed:&[Placed]) -> Vec<u64>);
    forward!(directories(owner:Owner, after:Option<u64>, limits:Limits) -> Vec<Directory>);
    forward!(directory_path(owner:Owner, position:u64) -> Option<Vec<u8>>);
    forward!(jobs(owner:Owner, after:Option<u64>, limits:Limits) -> Vec<Job>);
    forward!(job(owner:Owner, position:u64) -> Option<Job>);
    forward!(complete_files(owner:Owner, roots:&[(u64,ObjectId)]) -> ());
    forward!(file_roots(owner:Owner, after:Option<u64>, limits:Limits) -> Vec<FileRoot>);
    forward!(entries(owner:Owner, after:Option<&EntryKey>, limits:Limits) -> Vec<Entry>);
    forward!(set_directory_roots(owner:Owner, roots:&[(u64,ObjectId)]) -> ());
    forward!(advance(owner:Owner, phase:Phase) -> ());
    forward!(discard(owner:Owner, rows:usize) -> Discarded);
    forward!(release(owner:Owner) -> ());
    forward!(work(owner:Owner) -> AcquisitionWork);
    forward!(abandoned(after:Option<u64>, limit:usize) -> Vec<Abandoned>);
    forward!(discard_abandoned(abandoned:Owner, rows:usize) -> Discarded);
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("source fresh-database durable|disposable required".into());
    }
    let selected = match args[3].as_str() {
        "durable" => SqlitePersistenceProfile::Durable,
        "disposable" => SqlitePersistenceProfile::Disposable,
        _ => return Err("explicit profile required".into()),
    };
    let handles = Handles::create(
        PersistenceConfig::sqlite(&args[2])
            .with_sqlite_profile(selected)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        StoragePolicy::frozen_default(),
        &HistoryCatalogConfig {
            binding_key: b"layerfs-bench-pro".to_vec(),
            incarnation: 1,
            cursor_key: [0x28; 32],
        },
    )?;
    let observed = Observed {
        handles: &handles,
        units: Mutex::new(BTreeMap::new()),
    };
    let storage = Storage::new(handles.storage.clone())?;
    let before = handles.diagnostics()?;
    let name = HistoryName::new("acquisition-profile")?;
    let result = Timing::disabled("project.init", |timer| {
        init(
            &storage,
            &handles.history,
            InitRequest {
                source: Path::new(&args[1]),
                acquisition: &observed,
                stack: LayerStackId::from_authority([0x41; 16]),
                name,
                scope_seed: [0x42; 32],
                deadline: Instant::now() + Duration::from_secs(15),
            },
            timer,
        )
    })
    .0?;
    let after = handles.diagnostics()?;
    let mut total = Work::default();
    total.add(before, after, 0);
    total.print("complete_init");
    for (name, work) in observed.units.lock().unwrap().iter() {
        work.print(name);
    }
    let checkpoint = handles.checkpoint()?;
    if checkpoint.busy {
        return Err("checkpoint busy".into());
    }
    eprintln!("DIAGNOSTIC_ONLY cache=uncontrolled scopes=per-acquisition-unit,complete-Init; instrumentation overhead included; no speed gate or phase-residency claim");
    eprintln!(
        "profile={:?} root={:?} entries={} namespace={:?} saves={:?}",
        handles.profile(),
        result.root,
        result.entries,
        result.namespace_work,
        storage.save_work()
    );
    Ok(())
}
