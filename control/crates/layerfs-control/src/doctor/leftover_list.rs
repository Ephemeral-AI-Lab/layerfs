//! `home leftovers` (call surface 2.0, layerfs-project#180): what
//! interrupted operations left on `native` and on each running machine, in
//! one list. The home's own walk on `native`, and each machine's asked of
//! its worker's `doctor.sandbox` operation through `place::on_machine`, as
//! the Store checks ask for a partial copy. Nothing is started: a machine
//! that is stopped, held by an operation or does not answer is named in
//! `not_listed` instead, and so is a machine whose walk stopped at its
//! bound (what that walk found is listed).
//!
//! Each item is `{ leftover, modified_at, size, report_only, removable_at,
//! kind, machine, path }`: `leftover` is the id `home prune` takes.
//!
//! The host's Docker is asked for the image export containers killed copies
//! left (`export_container`, with `native`'s entries) once an image copy has
//! run from this home (its `run/image-lock/` folder is there): one
//! `report_only` while a copy of its image holds the image's lock. A Docker
//! that does not answer names `native` in `not_listed`, blocked by
//! `docker`'s `engine_down`; a host with no Docker lists none.
//!
//! A folder copy is removable only when a line of the home's
//! `log/uncertain.jsonl` names it with its Init's deadline
//! ([`named_copies`]), from an hour after that deadline (`removable_at`);
//! any other copy may belong to an Init that is still running, and is
//! `report_only`.
use super::leftovers::{self, Kind, BOUNDS};
use crate::inventory;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{detect, now_ms, paths, Layout};
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_home::state::State;
use layerfs_home::time::iso;
use layerfs_machine::record::MachineRecord;
use layerfs_store::place::{self, Executor};
use layerfs_store::record::StoreRecord;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// How long after its Init's recorded deadline a folder copy is removable.
pub const INIT_GRACE_MS: i64 = 60 * 60 * 1000;

/// The most of `log/uncertain.jsonl` that is read, from its end: the log
/// is never rotated, and a copy named before that is reported only.
const UNCERTAIN_TAIL: u64 = 4 << 20;

/// A folder copy a line of `log/uncertain.jsonl` names.
#[derive(Clone, Debug)]
pub struct NamedCopy {
    /// The machine whose worker root holds it.
    pub machine: String,
    /// Its id: `incoming/init-<copy>`.
    pub copy: String,
    /// The Store and project of its Init.
    pub store: String,
    /// The project's name.
    pub project: String,
    /// The Init's deadline, in ms since the epoch; `None` when the line
    /// records none.
    pub deadline: Option<i64>,
}

/// The folder copies `home`'s `log/uncertain.jsonl` names (a lost
/// `project.init` or `project.stage` answer, `layerfs-store`
/// `project/send.rs` `log_left`), the newest line of each copy last.
pub fn named_copies(home: &Path) -> Vec<NamedCopy> {
    let mut text = Vec::new();
    let read =
        std::fs::File::open(paths::log_dir(home).join("uncertain.jsonl")).and_then(|mut file| {
            let length = file.metadata()?.len();
            file.seek(SeekFrom::Start(length.saturating_sub(UNCERTAIN_TAIL)))?;
            file.take(UNCERTAIN_TAIL).read_to_end(&mut text)
        });
    if read.is_err() {
        return Vec::new();
    }
    let field = |line: &SdkValue, name: &str| line.get_text(name).map(str::to_owned);
    (String::from_utf8_lossy(&text).lines())
        .filter_map(|line| layerfs_home::json::parse(line).ok())
        .filter(|line| line.get_text("call") == Some("project.init"))
        .filter_map(|line| {
            Some(NamedCopy {
                machine: field(&line, "machine")?,
                copy: field(&line, "copy")?,
                store: field(&line, "store")?,
                project: field(&line, "project")?,
                deadline: line.get_int("deadline"),
            })
        })
        .collect()
}

/// The line naming copy `copy` on `machine`, the newest one.
pub fn named<'a>(copies: &'a [NamedCopy], machine: &str, copy: &str) -> Option<&'a NamedCopy> {
    (copies.iter()).rfind(|named| named.machine == machine && named.copy == copy)
}

/// The list in progress.
struct List {
    copies: Vec<NamedCopy>,
    items: Vec<SdkValue>,
    not_listed: Vec<SdkValue>,
}

impl List {
    /// Machine `machine` was not walked, or not to the end: `why` is
    /// `blocked_by` (the state of the thing at the root) or `reason` (one
    /// of `timeout`, `busy`, `lost`, `refused`).
    fn not_listed(&mut self, machine: &str, why: (&str, SdkValue), message: String) {
        self.not_listed.push(SdkValue::map([
            ("machine", SdkValue::text(machine)),
            why,
            ("message", SdkValue::text(message)),
        ]));
    }

    /// One side's walk on `machine`: its entries as items, and the machine
    /// in `not_listed` when the walk stopped at its bound.
    fn walked(&mut self, home: &Path, machine: &str, walked: &SdkValue) {
        if !walked.get_bool("complete") {
            self.not_listed(
                machine,
                ("reason", SdkValue::text("timeout")),
                format!(
                    "the walk on machine {machine} stopped at its bound ({} entries or {} s); there may be more than it lists",
                    BOUNDS.entries,
                    BOUNDS.within.as_secs()
                ),
            );
        }
        let Some(SdkValue::List(rows)) = walked.get("leftovers") else {
            return;
        };
        for row in rows {
            if let Some(item) = self.item(home, machine, row) {
                self.items.push(item);
            }
        }
    }

    /// One entry of a walk as an item; a worker's answer is read as data,
    /// so a kind or name the walk does not give is dropped.
    fn item(&self, home: &Path, machine: &str, row: &SdkValue) -> Option<SdkValue> {
        let kind = row.get_text("kind").and_then(Kind::named)?;
        let (name, path) = (row.get_text("name")?, row.get_text("path")?);
        if kind == Kind::ExportContainer || kind.path(Path::new("/"), name).is_none() {
            return None;
        }
        // a running `backup.create` of its Store is still using it
        if kind == Kind::BackupStaging && backup_runs(home, name) {
            return None;
        }
        let removable_at = match kind {
            Kind::InitCopy => named(&self.copies, machine, name)
                .and_then(|named| named.deadline)
                .map(|deadline| deadline.saturating_add(INIT_GRACE_MS)),
            _ => None,
        };
        // what `home prune` would refuse is listed report-only: an init copy
        // with no deadline, a run folder without an `owner` marker quiet for
        // a minute (the walk names when it was written; a live daemon is
        // only known when the prune looks)
        let quiet = super::leftover_prune::OWNER_QUIET.as_secs() as i64;
        let report_only = match kind {
            Kind::InitCopy => removable_at.is_none(),
            Kind::RunFolder => (row.get_int("owner_written"))
                .is_none_or(|written| now_ms() / 1000 - written < quiet),
            _ => false,
        };
        let modified = row.get_int("modified").unwrap_or(0);
        Some(SdkValue::map([
            (
                "leftover",
                SdkValue::text(leftovers::id(machine, kind, name)),
            ),
            (
                "modified_at",
                SdkValue::Text(iso(modified.saturating_mul(1000))),
            ),
            ("size", row.get("bytes").cloned().unwrap_or_default()),
            ("report_only", SdkValue::Bool(report_only)),
            (
                "removable_at",
                removable_at.map_or(SdkValue::Null, |at| SdkValue::Text(iso(at))),
            ),
            ("kind", SdkValue::text(kind.as_str())),
            ("machine", SdkValue::text(machine)),
            ("path", SdkValue::text(path)),
        ]))
    }

    /// The export containers killed copies left in the host's Docker, as
    /// `native`'s entries; `native` in `not_listed` when Docker does not
    /// answer.
    fn exports(&mut self, home: &Path) {
        use layerfs_sandbox::image_exports::{left_exports, not_installed};
        // a home no image copy ran from has none: Docker is not asked
        if !home.join("run").join("image-lock").is_dir() {
            return;
        }
        let listed = layerfs_sandbox::docker::Docker::of(home)
            .map_err(|refusal| layerfs_sandbox::docker::Failure::Error(refusal.message))
            .and_then(|docker| left_exports(&docker, home));
        let left = match listed {
            Ok(left) => left,
            Err(failure) if not_installed(&failure) => return,
            Err(failure) => {
                let by = SdkValue::map([
                    ("machine", SdkValue::text("docker")),
                    ("reason", SdkValue::text("engine_down")),
                ]);
                let message = format!(
                    "Docker does not answer ({}); its export containers were not listed",
                    failure.message()
                );
                return self.not_listed("native", ("blocked_by", by), message);
            }
        };
        for left in left {
            let at = left.created_ms.unwrap_or(0);
            self.items.push(SdkValue::map([
                (
                    "leftover",
                    SdkValue::text(leftovers::id("native", Kind::ExportContainer, &left.name)),
                ),
                ("modified_at", SdkValue::Text(iso(at))),
                ("size", SdkValue::Null),
                ("report_only", SdkValue::Bool(left.held)),
                ("removable_at", SdkValue::Null),
                ("kind", SdkValue::text(Kind::ExportContainer.as_str())),
                ("machine", SdkValue::text("native")),
                ("path", SdkValue::Null),
            ]));
        }
    }

    /// Machine `machine` (not `native`), asked through its worker when it
    /// runs; named in `not_listed` otherwise.
    fn machine(&mut self, home: &Path, machine: &str) {
        let record = match MachineRecord::read(home, machine) {
            Ok(record) => record,
            Err(refusal) => return self.refused(machine, refusal),
        };
        let state = match inventory::machine_state(home, machine, record.as_ref()) {
            Ok(state) => state,
            Err(refusal) => return self.refused(machine, refusal),
        };
        // a recorded worker is a running machine's, as its calls' routes
        // say; with none it is not running, and nothing starts it
        let state = match (paths::worker_record(home, machine).exists(), state) {
            (true, State::Stopped) => State::Ready,
            (false, State::Ready) => State::Stopped,
            (_, state) => state,
        };
        if let Some(State::Blocked(by)) = state.blocks("machine", machine) {
            let message = format!("machine {} is {}", by.handle, by.reason);
            return self.not_listed(machine, ("blocked_by", by.to_sdk()), message);
        }
        let mut worker = match place::on_machine(home, machine) {
            // its Sandboxes run in this process: the native walk covers them
            Ok(Executor::Local) => return,
            Ok(Executor::Worker(worker)) => worker,
            Err(refusal) => return self.refused(machine, refusal),
        };
        let known = known(home, machine);
        let args = SdkValue::map([
            ("leftovers", SdkValue::Bool(true)),
            ("known", SdkValue::List(known)),
            (
                "timeout_ms",
                SdkValue::Int(BOUNDS.within.as_millis() as i64),
            ),
        ]);
        match worker.attempt("doctor.sandbox", args, &mut |_| {}) {
            Ok(Ok(walked)) => self.walked(home, machine, &walked),
            Ok(Err(refusal)) => self.refused(machine, refusal),
            Err(unreached) => self.not_listed(
                machine,
                ("reason", SdkValue::text("timeout")),
                format!(
                    "machine {machine}'s worker did not answer: {}",
                    unreached.0.message
                ),
            ),
        }
    }

    /// Machine `machine` answered, or its records were read, and that was
    /// refused: `busy` for an operation that holds it.
    fn refused(&mut self, machine: &str, refusal: SdkRefusal) {
        let reason = match refusal.code.as_str() {
            "Busy" => "busy",
            _ => "refused",
        };
        let message = format!("{}: {}", refusal.code, refusal.message);
        self.not_listed(machine, ("reason", SdkValue::text(reason)), message);
    }
}

/// The Sandboxes `home` records on `machine`: a machine's Sandboxes are
/// recorded in the home before their worker records them.
fn known(home: &Path, machine: &str) -> Vec<SdkValue> {
    let sandboxes = layerfs_sandbox::record::sandboxes(home).unwrap_or_default();
    (sandboxes.iter())
        .filter(|sandbox| sandbox.machine() == machine)
        .map(|sandbox| SdkValue::text(sandbox.name()))
        .collect()
}

/// Whether a live process holds an operation on the Store a staged
/// snapshot `<store>/<file>` belongs to: its own `backup.create`, for one.
fn backup_runs(home: &Path, name: &str) -> bool {
    let store = name.split_once('/').map_or(name, |(store, _)| store);
    let record = StoreRecord::read(home, store).ok().flatten();
    (record.and_then(|record| record.op())).is_some_and(|op: Op| op.standing() == Standing::Running)
}

/// `home.leftovers`: `{ items, not_listed, observed_at }` for `native` and
/// every recorded machine, or for `machine` alone; changes nothing. A home
/// that is not a current-layout home has none.
pub fn list(home: &Path, machine: Option<&str>) -> Outcome {
    if let Some(machine) = machine {
        layerfs_home::names::name("machine", machine)
            .map_err(|why| refused("InvalidInput", why))?;
        let built_in = matches!(machine, "native" | "docker");
        if !built_in && !paths::machine_dir(home, machine).is_dir() {
            return Err(refused(
                "NotFound",
                format!("no machine named {machine}; `layerfs machine list` lists them"),
            ));
        }
    }
    let mut list = List {
        copies: named_copies(home),
        items: Vec::new(),
        not_listed: Vec::new(),
    };
    if let Ok(Layout::Current(_)) = detect(home) {
        let asked = |name: &str| machine.is_none_or(|machine| machine == name);
        if asked("native") && layerfs_machine::runs_here(home, "native").unwrap_or(false) {
            let walked = leftovers::walk(home, &[], BOUNDS);
            list.walked(home, "native", &walked);
        }
        if asked("native") {
            list.exports(home);
        }
        let mut machines = paths::names(&home.join("machine")).unwrap_or_default();
        if let Some(only) = machine.filter(|only| !machines.iter().any(|name| name == only)) {
            machines.push(only.to_owned());
        }
        for name in machines
            .iter()
            .filter(|name| *name != "native" && asked(name))
        {
            list.machine(home, name);
        }
    }
    Ok(SdkValue::map([
        ("items", SdkValue::List(list.items)),
        ("not_listed", SdkValue::List(list.not_listed)),
        ("observed_at", SdkValue::Text(iso(now_ms()))),
    ]))
}
