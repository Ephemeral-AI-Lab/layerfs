//! The one conversion of a layout 2 home (2.0) into layout 3 (#536 §5,
//! H10), under the home lock, by the first call of this build on it.
//!
//! It is refused `InUse` while anything is live, naming each with what
//! ends it: a Sandbox whose runtime runs, or that this build cannot check
//! (one on a machine behind a worker, whose old worker it does not ask); a
//! Sandbox create in flight; an earlier create whose outcome is unknown
//! (`create.intent`); a live Store lease; a live Store move or remove
//! (`in_progress`); a live machine removal (`removing`). Otherwise, in one
//! pass:
//!
//! 1. write: each Sandbox record from `machine/<m>/sandbox/<name>/` to
//!    `sandbox/<name>/record.json` in layout 3's form, with no `state` or
//!    `pid` (no id, so `gone`, when its runtime had ended, which is
//!    reclaimed: a stopped container or local daemon deleted once; an
//!    uncertain `op` for a create whose outcome was unknown), each Store
//!    record without an ended move's or remove's `in_progress`, and
//!    `"v": 1` into every machine, Store, project and bucket record,
//!    dropping a machine record's layout 2 intent (`removing`, `state`
//!    `creating` or `unknown`);
//! 2. `log/layout-3.pending`, then `layerfs.json` at layout 3 (the flip);
//! 3. delete the layout 2 paths (`machine/*/sandbox/`,
//!    `store/*/project/*/sandboxes/`), rename `machine/native/sandbox-root/`
//!    to `machine/native/run/` (its image cache kept), remove the marker and
//!    write `log/migration-2.json`.
//!
//! A crash before the flip leaves a layout 2 home that converts again (each
//! write is idempotent); after it, the next open finds the marker and
//! finishes step 3 ([`finish`]).
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::layout::{write, HomeFile, LAYOUT_VERSION};
use layerfs_home::home::{paths, read_record, with_field, write_entity, write_record};
use layerfs_home::op::{self, Op};
use layerfs_home::outcome::{failed, refused};
use layerfs_machine::record::MachineRecord;
use layerfs_sandbox::record::SandboxRecord;
use layerfs_sandbox::recorded::{unchecked, Recorded};
use layerfs_store::lease;
use layerfs_store::record::StoreRecord;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The marker of a conversion past its flip whose old paths may remain.
const PENDING: &str = "layout-3.pending";

/// A layout 2 numbered Sandbox in this home: `<store>/<project>`, its
/// number, its folder and its record.
struct Numbered {
    project: String,
    id: String,
    dir: PathBuf,
    record: SdkValue,
}

/// What a numbered Sandbox's runtime was found to be.
enum Found {
    Running,
    Ended,
    Unknown(String),
}

/// What the conversion found: what refuses it, and what it writes.
#[derive(Default)]
struct Plan {
    live: Vec<(&'static str, String, String)>,
    sandboxes: Vec<(String, SdkValue)>,
    /// Store records without layout 2's ended `in_progress`.
    stores: Vec<(String, SdkValue)>,
    ended: Vec<Numbered>,
    notes: Vec<String>,
}

impl Plan {
    fn live(&mut self, kind: &'static str, name: &str, why: String) {
        self.live.push((kind, name.to_owned(), why));
    }
}

fn names(dir: &Path) -> Result<Vec<String>> {
    paths::names(dir).map_err(failed)
}

/// Converts layout 2 home `home` (`file`) to layout 3, or refuses `InUse`
/// naming everything live. The caller holds the home lock.
pub(super) fn convert(home: &Path, file: &HomeFile) -> Result<HomeFile> {
    let plan = survey(home, lease::holders)?;
    if !plan.live.is_empty() {
        return Err(in_use(home, &plan.live));
    }
    let mut report = vec![("from", SdkValue::Int(2)), ("at", SdkValue::Int(now()))];
    let mut converted = Vec::new();
    for (name, record) in &plan.sandboxes {
        write_entity(&paths::sandbox_record(home, name), record)?;
        converted.push(SdkValue::text(name));
    }
    let mut reclaimed = Vec::new();
    for ended in &plan.ended {
        reclaim(ended)?;
        reclaimed.push(SdkValue::text(format!("{}#{}", ended.project, ended.id)));
    }
    for (name, record) in &plan.stores {
        write_entity(&paths::store_record(home, name), record)?;
    }
    drop_machine_intents(home)?;
    let versioned = version_records(home)?;
    let marker = paths::log_dir(home).join(PENDING);
    write_record(&marker, &SdkValue::map([("at", SdkValue::Int(now()))]))?;
    let file = HomeFile {
        layout: LAYOUT_VERSION,
        id: file.id.clone(),
    };
    write(home, &file)?;
    let (removed, mut notes) = clean(home)?;
    remove(&marker)?;
    notes.extend(plan.notes);
    let list = |items: Vec<String>| SdkValue::List(items.into_iter().map(SdkValue::Text).collect());
    report.extend([
        ("sandboxes", SdkValue::List(converted)),
        ("reclaimed", SdkValue::List(reclaimed)),
        ("versioned", SdkValue::Int(versioned)),
        ("removed", list(removed)),
        ("notes", list(notes)),
    ]);
    let report = SdkValue::Map(report.into_iter().map(|(n, v)| (n.to_owned(), v)).collect());
    write_record(&paths::log_dir(home).join("migration-2.json"), &report)?;
    Ok(file)
}

/// Finishes a conversion that stopped after its flip: the layout 2 paths it
/// had not deleted yet. Cheap when there is none (one look at the marker).
pub(super) fn finish(home: &Path) -> Result<()> {
    let marker = paths::log_dir(home).join(PENDING);
    if !marker.exists() {
        return Ok(());
    }
    let _lock = layerfs_home::home::lock::take(home)?;
    if marker.exists() {
        clean(home)?;
        remove(&marker)?;
    }
    Ok(())
}

fn now() -> i64 {
    layerfs_home::home::now_ms()
}

fn remove(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(failed(error)),
        _ => Ok(()),
    }
}

/// What refuses the conversion of layout 2 home `home` now, read only
/// (`layerfs doctor`): `(kind, name, what ends it)` for each live thing.
/// Stale leases stay (the conversion removes them); no lock is taken.
pub(crate) fn blockers(home: &Path) -> Result<Vec<(&'static str, String, String)>> {
    let live = |home: &Path, store: &str| lease::read_holders(home, store).map(|(live, _)| live);
    Ok(survey(home, live)?.live)
}

/// Reads every layout 2 Sandbox record and probes the runtimes this home
/// runs; reads the leases (`holders`) and intents.
fn survey(home: &Path, holders: impl Fn(&Path, &str) -> Result<Vec<u32>>) -> Result<Plan> {
    let mut plan = Plan::default();
    let mut numbered = numbered(home, &mut plan)?;
    for machine in names(&home.join("machine"))? {
        let folder = paths::machine_dir(home, &machine).join("sandbox");
        for name in names(&folder)? {
            let Some(value) = read_record(&folder.join(&name).join("record.json"))? else {
                continue;
            };
            let record = SandboxRecord::from_record(&name, value.clone());
            let Some(id) = record.id() else {
                match creator(&value) {
                    Some(pid) => plan.live(
                        "sandbox",
                        &name,
                        format!("is being created by process {pid}; let it finish"),
                    ),
                    None => plan.sandboxes.push((name, layout_3(value)?)),
                }
                continue;
            };
            let project = record.project().unwrap_or("");
            let at = numbered
                .iter()
                .position(|n| n.project == project && n.id == id);
            let Some(at) = at else {
                plan.live("sandbox", &name, format!("runs on machine {machine}, whose worker this build does not ask; close it with the build that started it (`layerfs sandbox close {name}`)"));
                continue;
            };
            let entry = numbered.remove(at);
            match probe(home, &entry) {
                Found::Running => plan.live("sandbox", &name, format!("runs on machine {machine}; close it with the build that started it (`layerfs sandbox close {name}`)")),
                Found::Unknown(why) => plan.live("sandbox", &name, format!("cannot be checked ({why}); close it with the build that started it (`layerfs sandbox close {name}`)")),
                // no id: layout 3 derives `gone`
                Found::Ended => {
                    let gone = SandboxRecord::from_record(&name, value).with("id", SdkValue::Null);
                    plan.sandboxes.push((name, layout_3(gone.to_record().clone())?));
                    plan.ended.push(entry);
                }
            }
        }
    }
    // numbered Sandboxes no Sandbox record names (an earlier bundle's)
    for entry in numbered {
        let name = format!("{}#{}", entry.project, entry.id);
        match probe(home, &entry) {
            Found::Running => plan.live("sandbox", &name, "runs with no Sandbox record; stop it with the build that started it".to_owned()),
            Found::Unknown(why) => plan.live("sandbox", &name, format!("has no Sandbox record and cannot be checked ({why}); remove it by hand, then its folder {}", entry.dir.display())),
            Found::Ended => plan.ended.push(entry),
        }
    }
    for store in names(&home.join("store"))? {
        for pid in holders(home, &store)? {
            plan.live(
                "lease",
                &store,
                format!("is held open by process {pid}; end that process"),
            );
        }
        // layout 2's `in_progress`: a live one refuses; a gone one did not
        // hold the Store in 2.0 and goes (layout 3 holds `op` instead)
        let Some(record) = StoreRecord::read(home, &store)? else {
            continue;
        };
        match record.field("in_progress") {
            None => {}
            Some(progress @ SdkValue::Map(_))
                if layerfs_home::process::recorded_alive(progress) =>
            {
                let pid = progress.get_int("pid").unwrap_or(0);
                plan.live(
                    "store",
                    &store,
                    format!("is being moved or removed by process {pid}; let it finish"),
                );
            }
            Some(progress) => {
                if let SdkValue::Map(_) = progress {
                    let pid = progress.get_int("pid").unwrap_or(0);
                    plan.notes.push(format!("Store {store}: dropped the record of a move or remove by process {pid}, which ended; an uncertain outcome is in log/uncertain.jsonl"));
                }
                let record = record.without("in_progress");
                plan.stores.push((store, record.to_record().clone()));
            }
        }
    }
    // a 2.0 record names its remover by process id alone: that is all
    // there is to check
    for machine in names(&home.join("machine"))? {
        let record = MachineRecord::read(home, &machine)?;
        let removing = record.and_then(|r| r.field("removing").cloned());
        if let Some(removing) = removing.filter(layerfs_home::process::recorded_alive) {
            let pid = removing.get_int("pid").unwrap_or(0);
            plan.live(
                "machine",
                &machine,
                format!("is being removed by process {pid}; let it finish"),
            );
        }
    }
    Ok(plan)
}

/// The live process a layout 2 Sandbox record names as creating it: a 2.0
/// record says `state: "creating"` and its creator's `pid`, and nothing
/// more, so this is the one check that reads a process by id alone.
fn creator(value: &SdkValue) -> Option<i64> {
    let pid = value.get_int("pid").filter(|pid| *pid > 0)?;
    let named = SdkValue::map([("pid", SdkValue::Int(pid))]);
    (value.get_text("state") == Some("creating") && layerfs_home::process::recorded_alive(&named))
        .then_some(pid)
}

/// Layout 2 Sandbox record `value` in layout 3's form: no stored `state`
/// and no creator `pid`. One whose create's outcome was unknown (2.0's
/// `state: "unknown"`) holds an uncertain `sandbox.create` intent owned by
/// no process (pid 0), which `sandbox.close` clears; one whose creator is
/// gone holds none, and layout 3 derives `gone` from its missing id.
fn layout_3(value: SdkValue) -> Result<SdkValue> {
    let unknown = value.get_text("state") == Some("unknown");
    let SdkValue::Map(mut fields) = value else {
        return Ok(value);
    };
    fields.retain(|(name, _)| name != "state" && name != "pid");
    let record = SdkValue::Map(fields);
    if !unknown {
        return Ok(record);
    }
    let op = Op::begin("sandbox.create", "start", SdkValue::Null)?.as_uncertain();
    let nobody = SdkValue::map([("pid", SdkValue::Int(0))]);
    let op = with_field(op.to_record().clone(), "owner", nobody);
    Ok(with_field(record, op::FIELD, op))
}

/// Every layout 2 numbered Sandbox in this home
/// (`store/<s>/project/<p>/sandboxes/<n>/sandbox.record`); an earlier
/// create's `create.intent` is live.
fn numbered(home: &Path, plan: &mut Plan) -> Result<Vec<Numbered>> {
    let mut found = Vec::new();
    for store in names(&home.join("store"))? {
        let projects = paths::store_dir(home, &store).join("project");
        for project in names(&projects)? {
            let folder = projects.join(&project);
            let name = format!("{store}/{project}");
            let intent = folder.join("create.intent");
            if intent.exists() {
                plan.live("intent", &name, format!("has an earlier Sandbox create whose outcome is unknown: remove what it left, then delete {}", intent.display()));
            }
            for id in names(&folder.join("sandboxes"))? {
                let dir = folder.join("sandboxes").join(&id);
                let file = dir.join("sandbox.record");
                if !file.exists() {
                    continue;
                }
                let record = layerfs_store::project::discovery::load(&file)?;
                found.push(Numbered {
                    project: name.clone(),
                    id,
                    dir,
                    record,
                });
            }
        }
    }
    Ok(found)
}

/// Whether a numbered Sandbox's runtime runs: its container or daemon
/// process observed once. A local one must be in the home's old local root.
fn probe(home: &Path, entry: &Numbered) -> Found {
    let recorded = match unchecked(&entry.id, &entry.record) {
        Ok(recorded) => recorded,
        Err(refusal) => return Found::Unknown(refusal.message),
    };
    if let Err(why) = inside(home, &recorded) {
        return Found::Unknown(why);
    }
    match recorded.reopen() {
        Ok(_) => Found::Running,
        Err(failure) if failure.container_gone() || failure.container_stopped() => Found::Ended,
        Err(failure) => Found::Unknown(format!("{failure:?}")),
    }
}

/// A local runtime's directory is in the home's old local root.
fn inside(home: &Path, recorded: &Recorded) -> std::result::Result<(), String> {
    // macOS reopens only local records, Linux namespaces ones too
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[allow(irrefutable_let_patterns)]
    if let Recorded::Local { directory, .. } = recorded {
        let root = paths::machine_dir(home, "native").join("sandbox-root");
        let plain = !(directory.components()).any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::CurDir
            )
        });
        if !plain || directory.parent() != Some(root.as_path()) {
            return Err(format!(
                "its directory {} is not in {}",
                directory.display(),
                root.display()
            ));
        }
    }
    let _ = (home, recorded);
    Ok(())
}

/// Deletes an ended numbered Sandbox's stopped runtime, once, and its folder.
fn reclaim(entry: &Numbered) -> Result<()> {
    let recorded = unchecked(&entry.id, &entry.record)?;
    let stopped = match recorded.reopen() {
        Ok(_) => {
            return Err(refused(
                "InUse",
                format!("Sandbox {}#{} runs again", entry.project, entry.id),
            ))
        }
        Err(failure) if failure.container_gone() || failure.container_stopped() => {
            failure.container_stopped()
        }
        Err(failure) => return Err(failed(failure)),
    };
    if stopped {
        if let Err(failure) = unchecked(&entry.id, &entry.record)?.stopped().delete() {
            if !failure.container_gone() {
                return Err(failed(failure));
            }
        }
    }
    match std::fs::remove_dir_all(&entry.dir) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(failed(error)),
        _ => Ok(()),
    }
}

/// Drops the layout 2 intent fields from each machine record: a gone
/// remover's `removing`, and a `state` of `creating` or `unknown` (a 2.0
/// create or remove that did not finish, or whose outcome was unknown; its
/// `error` stays). Layout 3 keeps such intents in `op`; without one the
/// machine's live probe says what is there. Idempotent.
fn drop_machine_intents(home: &Path) -> Result<()> {
    for machine in names(&home.join("machine"))? {
        let file = paths::machine_record(home, &machine);
        let Some(SdkValue::Map(fields)) = read_record(&file)? else {
            continue;
        };
        let old = |(name, value): &(String, SdkValue)| {
            name == "removing"
                || (name == "state"
                    && matches!(value, SdkValue::Text(t) if t == "creating" || t == "unknown"))
        };
        if fields.iter().any(old) {
            let kept = fields.into_iter().filter(|field| !old(field));
            write_record(&file, &SdkValue::Map(kept.collect()))?;
        }
    }
    Ok(())
}

/// Writes `"v": 1` into each machine, Store, project and bucket record that
/// has none; answers how many it wrote.
fn version_records(home: &Path) -> Result<i64> {
    let mut files = Vec::new();
    for machine in names(&home.join("machine"))? {
        files.push(paths::machine_record(home, &machine));
    }
    for store in names(&home.join("store"))? {
        files.push(paths::store_record(home, &store));
        for project in names(&paths::store_dir(home, &store).join("project"))? {
            files.push(paths::project_record(home, &store, &project));
        }
    }
    for bucket in names(&home.join("backup"))? {
        files.push(paths::bucket_record(home, &bucket));
    }
    let mut written = 0;
    for file in files {
        if let Some(record) = read_record(&file)? {
            if record.get("v").is_none() {
                write_entity(&file, &record)?;
                written += 1;
            }
        }
    }
    Ok(written)
}

/// Deletes the layout 2 Sandbox paths and renames the old local root;
/// answers what it removed and notes what it left. Idempotent.
fn clean(home: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let (mut removed, mut notes) = (Vec::new(), Vec::new());
    let mut folders = Vec::new();
    for machine in names(&home.join("machine"))? {
        folders.push(paths::machine_dir(home, &machine).join("sandbox"));
    }
    for store in names(&home.join("store"))? {
        let projects = paths::store_dir(home, &store).join("project");
        for project in names(&projects)? {
            folders.push(projects.join(project).join("sandboxes"));
        }
    }
    for folder in folders {
        if folder.exists() {
            std::fs::remove_dir_all(&folder).map_err(failed)?;
            removed.push(folder.display().to_string());
        }
    }
    let old = paths::machine_dir(home, "native").join("sandbox-root");
    let run = paths::run_root(home);
    if old.exists() {
        if run.exists() {
            notes.push(format!(
                "{} is left: {} exists",
                old.display(),
                run.display()
            ));
        } else {
            std::fs::rename(&old, &run).map_err(failed)?;
            removed.push(format!("{} -> {}", old.display(), run.display()));
            for left in names(&run)?.into_iter().filter(|n| n != "images") {
                notes.push(format!(
                    "{} is left in {}: it belongs to no recorded Sandbox",
                    left,
                    run.display()
                ));
            }
        }
    }
    Ok((removed, notes))
}

/// The refusal while anything is live: each named, with what ends it.
fn in_use(home: &Path, live: &[(&'static str, String, String)]) -> SdkRefusal {
    let named: Vec<String> = live
        .iter()
        .map(|(kind, name, why)| format!("{kind} {name} {why}"))
        .collect();
    let mut refusal = refused(
        "InUse",
        format!(
            "{} has layout 2; this build converts it to layout 3 once nothing in it is live: {}",
            home.display(),
            named.join("; ")
        ),
    );
    let related = live.iter().map(|(kind, name, why)| {
        SdkValue::map([
            ("kind", SdkValue::text(*kind)),
            ("name", SdkValue::text(name)),
            ("why", SdkValue::text(why)),
        ])
    });
    refusal.detail = SdkValue::map([("related", SdkValue::List(related.collect()))]);
    refusal
}
