//! A running command's record (compute-plane 2.0 §5.2, D-10):
//! `<sandbox folder>/workspace/<workspace id>/exec/<exec id>.json`, beside
//! the Workspace's own record `workspace/<workspace id>.json`, on the side
//! that keeps that record (the home on `native` and for a Sandbox it reaches
//! directly, a worker's root on every other machine).
//!
//! `{ v, id, workspace, path, name, owner, route, stop, started, cancel? }`:
//! its owner process (pid, kernel start time, boot, as an `op` names its
//! owner), its route (`local`, or `machine:<name>` when the command runs in
//! that machine's worker) and its stop handle (`{ cgroup: { path } }` or
//! `{ group: { pgid, leader } }`, null until known). A cancel marks it
//! (`cancel`, when) before it signals. It is created new under the Sandbox
//! lock when the command starts (a duplicate id is refused without a scan)
//! and removed when the command ends ([`Running`]). A record whose owner is
//! gone is stale.
use crate::exec::id::ExecId;
use crate::record::lock_in;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::process::LocalProcess;
use layerfs_home::home::{now_ms, read_record, write_entity};
use layerfs_home::outcome::{failed, refused};
use layerfs_sandbox_backend::Stop;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Where a command runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// In this record's side's own kernel.
    Local,
    /// In machine `name`'s worker (a Sandbox whose daemon the home reaches
    /// directly, #535 G4): a cancel travels in `sandbox.host` (D-11).
    Machine(String),
}

/// One running command's record.
#[derive(Clone, Debug, PartialEq)]
pub struct ExecRecord {
    /// Its exec id.
    pub id: String,
    /// Its Workspace's id (64 hex digits).
    pub workspace: String,
    /// Its Workspace's kernel path.
    pub path: String,
    /// Its Workspace's name, if it has one.
    pub name: Option<String>,
    /// Its owner process's identity fields.
    pub owner: SdkValue,
    /// Where it runs.
    pub route: Route,
    /// Its stop handle, once known.
    pub stop: Option<Stop>,
    /// When it started (ms since the epoch).
    pub started: i64,
    /// When a cancel marked it, if one did.
    pub cancel: Option<i64>,
}

/// The folder of Workspace `workspace`'s records in Sandbox folder `dir`.
pub fn workspace_dir(dir: &Path, workspace: &str) -> PathBuf {
    dir.join("workspace").join(workspace)
}

/// The record of command `id` of Workspace `workspace` in Sandbox folder `dir`.
pub fn file(dir: &Path, workspace: &str, id: &str) -> PathBuf {
    workspace_dir(dir, workspace)
        .join("exec")
        .join(format!("{id}.json"))
}

impl ExecRecord {
    /// A record of command `id`, owned by this process, started now.
    pub fn new(id: &ExecId, workspace: &str, (path, name): (&str, Option<&str>)) -> Self {
        Self {
            id: id.to_string(),
            workspace: workspace.to_owned(),
            path: path.to_owned(),
            name: name.map(str::to_owned),
            owner: Self::new_owner(),
            route: Route::Local,
            stop: None,
            started: now_ms(),
            cancel: None,
        }
    }

    /// This process's identity fields, as a record names its owner.
    pub fn new_owner() -> SdkValue {
        SdkValue::Map(
            (layerfs_home::process::identity().into_iter())
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        )
    }

    /// Whether its owner process is the one it names and still runs.
    pub fn owned(&self) -> bool {
        layerfs_home::process::recorded_alive(&self.owner)
    }

    /// The record as written.
    pub fn encode(&self) -> SdkValue {
        let text = |text: &str| SdkValue::text(text);
        let route = match &self.route {
            Route::Local => "local".to_owned(),
            Route::Machine(name) => format!("machine:{name}"),
        };
        let mut fields = vec![
            ("id", text(&self.id)),
            ("workspace", text(&self.workspace)),
            ("path", text(&self.path)),
            ("name", SdkValue::opt_text(self.name.clone())),
            ("owner", self.owner.clone()),
            ("route", SdkValue::Text(route)),
            (
                "stop",
                self.stop.as_ref().map_or(SdkValue::Null, stop_value),
            ),
            ("started", SdkValue::Int(self.started)),
        ];
        fields.extend(self.cancel.map(|at| ("cancel", SdkValue::Int(at))));
        SdkValue::Map(fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect())
    }

    /// The record `value` holds; `None` when it is not one.
    pub fn decode(value: &SdkValue) -> Option<Self> {
        let text = |name: &str| value.get_text(name).map(str::to_owned);
        let route = match value.get_text("route")? {
            "local" => Route::Local,
            other => Route::Machine(other.strip_prefix("machine:")?.to_owned()),
        };
        Some(Self {
            id: text("id")?,
            workspace: text("workspace")?,
            path: text("path")?,
            name: text("name"),
            owner: value.get("owner")?.clone(),
            route,
            stop: value.get("stop").and_then(stop_of),
            started: value.get_int("started")?,
            cancel: value.get_int("cancel"),
        })
    }
}

/// A stop handle as a record (and a worker's `exec_started` event) writes it.
pub fn stop_value(stop: &Stop) -> SdkValue {
    match stop {
        Stop::Cgroup { path } => SdkValue::map([(
            "cgroup",
            SdkValue::map([("path", SdkValue::text(path.display().to_string()))]),
        )]),
        Stop::Group { pgid, leader } => SdkValue::map([(
            "group",
            SdkValue::map([
                ("pgid", SdkValue::Int(i64::from(*pgid))),
                ("pid", SdkValue::Int(i64::from(leader.pid))),
                ("pid_start", SdkValue::text(leader.start.to_string())),
                (
                    "pid_boot",
                    SdkValue::opt_text(leader.boot.map(|boot| format!("{boot:032x}"))),
                ),
            ]),
        )]),
    }
}

/// The stop handle [`stop_value`] wrote, if `value` is one.
pub fn stop_of(value: &SdkValue) -> Option<Stop> {
    if let Some(cgroup) = value.get("cgroup") {
        return Some(Stop::Cgroup {
            path: PathBuf::from(cgroup.get_text("path")?),
        });
    }
    let group = value.get("group")?;
    let int = |name: &str| group.get_int(name).and_then(|n| u32::try_from(n).ok());
    Some(Stop::Group {
        pgid: int("pgid")?,
        leader: LocalProcess {
            pid: int("pid")?,
            start: group.get_text("pid_start")?.parse().ok()?,
            boot: (group.get_text("pid_boot")).and_then(|b| u128::from_str_radix(b, 16).ok()),
        },
    })
}

/// The record at `path`, if there is one; one that is not a record is
/// refused, never taken for an ended command.
pub fn read(path: &Path) -> Result<Option<ExecRecord>> {
    match read_record(path)? {
        None => Ok(None),
        Some(value) => ExecRecord::decode(&value).map(Some).ok_or_else(|| {
            refused(
                "Failed",
                format!("{} is not an exec record", path.display()),
            )
        }),
    }
}

/// Every command record in Sandbox folder `dir`, of Workspace `workspace`
/// or of all its Workspaces, each with its file. One removed while they
/// are listed is left out.
pub fn listed(dir: &Path, workspace: Option<&str>) -> Result<Vec<(PathBuf, ExecRecord)>> {
    let workspaces = match workspace {
        Some(workspace) => vec![workspace_dir(dir, workspace)],
        None => folders(&dir.join("workspace"))?,
    };
    let mut found = Vec::new();
    for folder in workspaces {
        for path in entries(&folder.join("exec"))? {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or(".");
            if name.starts_with('.') || !name.ends_with(".json") {
                continue;
            }
            found.extend(read(&path)?.map(|record| (path, record)));
        }
    }
    Ok(found)
}

fn entries(dir: &Path) -> Result<Vec<PathBuf>> {
    match fs::read_dir(dir) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(failed(error)),
        Ok(items) => (items.map(|item| item.map(|item| item.path())))
            .collect::<std::io::Result<_>>()
            .map_err(failed),
    }
}

fn folders(dir: &Path) -> Result<Vec<PathBuf>> {
    Ok(entries(dir)?.into_iter().filter(|p| p.is_dir()).collect())
}

/// Removes `path`, then its `exec/` folder and its Workspace's folder when
/// they hold nothing more.
pub fn remove(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != ErrorKind::NotFound => return Err(failed(error)),
        _ => {}
    }
    if let Some(exec) = path.parent() {
        if fs::remove_dir(exec).is_ok() {
            let _ = exec.parent().map(fs::remove_dir);
        }
    }
    Ok(())
}

/// The record of one command this process runs, held while it runs: made
/// new under Sandbox `sandbox`'s lock in its folder `dir` ([`Running::start`]),
/// told the command's stop handle at spawn ([`Running::spawned`]) and
/// removed when it ends ([`Running::end`]), or when it is dropped on a
/// failure.
pub struct Running {
    dir: PathBuf,
    sandbox: String,
    path: PathBuf,
    ended: bool,
}

impl Running {
    /// Records `record` new. A live record of the same id is the same
    /// command id running: refused `Exists`. A stale one (its owner gone)
    /// is ended first and replaced ([`super::cancel::stale`]); one whose
    /// command runs on a machine is ended in its worker through `remote`
    /// ([`super::cancel::stale_remote`]), and while that end is not
    /// confirmed its id stays taken: refused `Exists`.
    pub fn start(
        dir: &Path,
        sandbox: &str,
        record: &ExecRecord,
        remote: &mut super::cancel::Remote<'_>,
    ) -> Result<Self> {
        let path = file(dir, &record.workspace, &record.id);
        let exists = |why: &str| {
            refused(
                "Exists",
                format!(
                    "Workspace {sandbox}:{} {why} exec_id {}: pick another exec_id, or cancel \
                     it with workspace.exec_cancel",
                    record.name.as_deref().unwrap_or(&record.path),
                    record.id
                ),
            )
        };
        let mut lock = lock_in(dir, sandbox)?;
        if let Some(found) = read(&path)? {
            match (found.owned(), &found.route) {
                (true, _) => return Err(exists("already runs a command with")),
                (false, Route::Local) => {
                    super::cancel::stale(dir, &path, &found)?;
                }
                (false, Route::Machine(_)) => {
                    let claimed = super::cancel::claim(&path, &found)?;
                    drop(lock);
                    let pair = (&found, &claimed);
                    let ended = super::cancel::stale_remote(dir, sandbox, &path, pair, remote)?;
                    if ended == super::cancel::Outcome::Unknown {
                        return Err(exists("has a stale command, not confirmed ended, with"));
                    }
                    lock = lock_in(dir, sandbox)?;
                    if read(&path)?.is_some() {
                        return Err(exists("already runs a command with"));
                    }
                }
            }
        }
        write_entity(&path, &record.encode())?;
        drop(lock);
        Ok(Self {
            dir: dir.to_owned(),
            sandbox: sandbox.to_owned(),
            path,
            ended: false,
        })
    }

    /// Records the command's stop handle once it is known: once the
    /// command runs in it, so a cancel before that finds no handle and is
    /// answered as one this owner carries out. Such a cancel, marked
    /// first, is refused here as interrupted: the runtime then ends the
    /// command at once.
    pub fn spawned(&self, stop: &Stop) -> std::io::Result<()> {
        let io = |refusal: SdkRefusal| std::io::Error::other(refusal.message);
        let _sandbox = lock_in(&self.dir, &self.sandbox).map_err(io)?;
        let mut record = read(&self.path)
            .map_err(io)?
            .ok_or_else(|| std::io::Error::other("the command's record was removed"))?;
        if record.cancel.is_some() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "the command was cancelled as it started",
            ));
        }
        if record.stop.as_ref() == Some(stop) {
            return Ok(());
        }
        record.stop = Some(stop.clone());
        write_entity(&self.path, &record.encode()).map_err(io)
    }

    /// Removes the record once the command ended: whether a cancel marked
    /// it.
    pub fn end(mut self) -> Result<bool> {
        self.ended = true;
        let _sandbox = lock_in(&self.dir, &self.sandbox)?;
        let cancelled = read(&self.path)?.is_some_and(|record| record.cancel.is_some());
        remove(&self.path)?;
        Ok(cancelled)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if !self.ended {
            if let Ok(_sandbox) = lock_in(&self.dir, &self.sandbox) {
                let _ = remove(&self.path);
            }
        }
    }
}
