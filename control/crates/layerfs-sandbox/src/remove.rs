//! `sandbox.remove` (call surface 2.0; `sandbox.close` before it), and the
//! removal of a Sandbox whose create never recorded an id.
//!
//! A remove records its `op` intent (`sandbox.remove`, #536 §7.1) under the
//! home and Sandbox locks before it stops the runtime, and removes the
//! record at the end. A live intent of another operation refuses it
//! `Busy`; a gone owner's uncertain intent does not: `sandbox.remove` is the
//! call that clears it. Its progress is its op's phases: `cancel` (forced,
//! one event per command this home records), `stop` and `delete`.
use super::create::remove_folder;
use super::direct;
use super::family::named;
use super::list::handle;
use super::record::{self, SandboxRecord};
use crate::host::Call;
use crate::route::on_machine;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home;
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::{refused, with_next};
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The op's name a remove records.
pub const OP: &str = "sandbox.remove";

/// `sandbox.remove(sandbox, { force })`.
#[derive(Clone, Debug, Default)]
pub struct RemoveRequest {
    /// The Sandbox's name.
    pub name: String,
    /// Remove it even while Workspaces are mounted or commands run.
    pub force: bool,
}

/// How a remove ended.
pub enum Removed {
    /// Its runtime stopped and its record went: the Workspaces it ended
    /// (their handles), whose uncommitted changes ended with it.
    Stopped {
        /// The handles of the Workspaces the stop ended.
        unmounted: Vec<String>,
    },
    /// Its runtime was gone already, it never recorded one, or it was
    /// recorded under the retired docker backend; its record went.
    AlreadyGone {
        /// Each sentence about what may be left unrecorded.
        left: Vec<String>,
    },
}

/// The phases a remove reports, in order.
const PHASES: [&str; 3] = ["cancel", "stop", "delete"];

/// Sends progress event `phase` of the remove: `done` and `total` count
/// the phases, or for `cancel` the commands.
fn phase(events: &mut Events<'_>, phase: &str, counted: Option<(usize, usize)>) {
    let count = |n: usize| i64::try_from(n).ok();
    let (done, total) = counted.unwrap_or_else(|| {
        let at = PHASES.iter().position(|name| *name == phase).unwrap_or(0);
        (at, PHASES.len())
    });
    events.phase(phase, count(done), count(total), Vec::new());
}

/// The last phase ended: all are done.
fn done(events: &mut Events<'_>) {
    phase(events, "delete", Some((PHASES.len(), PHASES.len())));
}

/// The texts of list `items`.
fn texts(items: &[SdkValue]) -> impl Iterator<Item = &str> {
    items.iter().filter_map(|item| match item {
        SdkValue::Text(text) => Some(text.as_str()),
        _ => None,
    })
}

/// Stops the Sandbox and removes its record.
pub fn remove(home: &Path, events: &mut Events<'_>, request: &RemoveRequest) -> Result<Removed> {
    let home = home.to_path_buf();
    let name = request.name.as_str();
    if let Some(container) = named(&home, name)?.retired_container() {
        return forget(&home, events, name, container.as_deref());
    }
    let force = request.force;
    // the commands this home records, which a forced remove cancels first
    let running = match force {
        true => crate::exec::record::running(&named(&home, name)?.dir(&home)).unwrap_or_default(),
        false => Vec::new(),
    };
    let first = if running.is_empty() { "stop" } else { "cancel" };
    let op = Op::begin(OP, first, SdkValue::map([("force", SdkValue::Bool(force))]))?;
    let record = {
        let _lock = home::lock::take(&home)?;
        let _sandbox = record::lock(&home, name)?;
        let Some(record) = SandboxRecord::read(&home, name)? else {
            return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
        };
        clearable(&record)?;
        if record.id().is_none() {
            return remove_unstarted(&home, &record);
        }
        let record = record.with_op(&op);
        record.write(&home)?;
        record
    };
    for (done, _) in running.iter().enumerate() {
        phase(events, "cancel", Some((done, running.len())));
    }
    // the Workspaces this home records, by kernel path, for their handles
    let known: Vec<(String, String)> = (crate::files::workspaces(&record.dir(&home))?)
        .into_iter()
        .filter_map(|(_, workspace)| {
            let path = workspace.path()?.to_owned();
            Some((path, handle(name, &workspace)?))
        })
        .collect();
    let handle_of = |path: &str| match known.iter().find(|(at, _)| at == path) {
        Some((_, handle)) => handle.clone(),
        None => format!("{name}:{path}"),
    };
    let mut held = None;
    // a Sandbox reached directly keeps its Workspace records here: its
    // worker is told which it holds (#535 G4), and its commands' records
    // are settled here (D-10: `Busy`, or cancelled first with `force`)
    if direct::forwarded(&record) {
        let settled = crate::exec::remote::settle(events, (&home, &record), None, force)
            .and_then(|()| direct::held(&home, &record));
        match settled {
            Ok(paths) => held = Some(paths),
            Err(refusal) => {
                let _lock = home::lock::take(&home)?;
                let _sandbox = record::lock(&home, name)?;
                if let Some(now) = SandboxRecord::read(&home, name)? {
                    now.without_op().write(&home)?;
                }
                return Err(shown(refusal, &handle_of));
            }
        }
    }
    if first == "cancel" {
        let _sandbox = record::lock(&home, name)?;
        if let Some(now) = SandboxRecord::read(&home, name)? {
            now.with_op(&op.clone().at("stop")).write(&home)?;
        }
    }
    phase(events, "stop", None);
    let stopped = if force && record.machine() == "native" {
        // a forced remove on `native` needs no Store or daemon discovery,
        // even when active mounts or a broken Store would block the
        // ordinary route
        crate::force::native_free(&home)
            .and_then(|()| crate::force::native_host(&home, name))
            .and_then(|host| crate::force::close(&host, held.as_deref().unwrap_or(&[])))
    } else {
        on_machine(
            &home,
            events,
            &record,
            Call::Close { force, held },
            crate::dispatch::call,
        )
    };
    let _lock = home::lock::take(&home)?;
    // on `native` the Sandbox's own side removed the folder already
    let dir = record.dir(&home);
    let _sandbox = match dir.exists() {
        true => Some(record::lock(&home, name)?),
        false => None,
    };
    match stopped {
        Ok(value) => {
            phase(events, "delete", None);
            direct::release(&home, &record);
            remove_folder(&dir)?;
            let unmounted = match value.get("unknown") {
                Some(SdkValue::List(paths)) => texts(paths).map(&handle_of).collect(),
                _ => Vec::new(),
            };
            done(events);
            Ok(Removed::Stopped { unmounted })
        }
        // a Sandbox already gone loses its record too
        Err(refusal) if refusal.code == "NoSuchSandbox" => {
            phase(events, "delete", None);
            direct::release(&home, &record);
            remove_folder(&dir)?;
            done(events);
            Ok(Removed::AlreadyGone { left: Vec::new() })
        }
        // its outcome is not known: the record stays with the intent,
        // uncertain, until another `sandbox.remove` clears it
        Err(refusal) if refusal.code == "Unknown" => {
            let op = op.at("stop").as_uncertain();
            if let Some(now) = SandboxRecord::read(&home, name)? {
                now.with_op(&op).write(&home)?;
            }
            op.log_uncertain(&home, "sandbox", name, &refusal.message);
            Err(shown(refusal, &handle_of))
        }
        // a certain failure leaves the Sandbox as it was, except a forced
        // remove whose runtime already stopped: the record says so
        Err(refusal) => {
            if let Some(now) = SandboxRecord::read(&home, name)? {
                let stopped = refusal.detail.get_bool(crate::force::RUNTIME_STOPPED);
                match stopped {
                    true => now.without_op().with_machine_stop(true),
                    false => now.without_op(),
                }
                .write(&home)?;
            }
            Err(shown(refusal, &handle_of))
        }
    }
}

/// A remove's refusal with the fields the caller's next call needs: a
/// `Busy` or `Unknown` names the commands `running` (their exec ids), an
/// `InUse` (`MountsActive`) the Workspaces mounted, by handle.
fn shown(refusal: SdkRefusal, handle_of: &dyn Fn(&str) -> String) -> SdkRefusal {
    let listed = |field: &str| match refusal.detail.get(field) {
        Some(SdkValue::List(items)) => Some(items.clone()),
        _ => None,
    };
    match refusal.code.as_str() {
        "Busy" | "Unknown" => match listed("execs") {
            Some(execs) => with_next(refusal, "running", SdkValue::List(execs)),
            None => refusal,
        },
        "MountsActive" => match listed("workspaces") {
            Some(paths) => {
                let handles = texts(&paths).map(handle_of).map(SdkValue::Text).collect();
                with_next(refusal, "workspaces", SdkValue::List(handles))
            }
            None => refusal,
        },
        _ => refusal,
    }
}

/// Removes the record of Sandbox `name`, recorded under the retired docker
/// backend (D-8, layerfs-project#111), whose record names its container
/// `container` when it keeps the name: its
/// machine's worker is asked once to drop its own record of it (a worker
/// that cannot be reached or refuses leaves that record, and `left` says
/// so), then the home's record goes. The container is not touched; `left`
/// names it.
fn forget(
    home: &Path,
    events: &mut Events<'_>,
    name: &str,
    container: Option<&str>,
) -> Result<Removed> {
    let record = {
        let _lock = home::lock::take(home)?;
        let _sandbox = record::lock(home, name)?;
        let Some(record) = SandboxRecord::read(home, name)? else {
            return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
        };
        clearable(&record)?;
        record
    };
    let left = match record.machine() {
        // a `native` Sandbox's record is the folder removed below
        "native" => None,
        _ => on_machine(
            home,
            events,
            &record,
            Call::Close {
                force: true,
                held: None,
            },
            crate::dispatch::call,
        )
        .err()
        .filter(|refusal| refusal.code != "NoSuchSandbox")
        .map(|refusal| refusal.message),
    };
    phase(events, "delete", None);
    let _lock = home::lock::take(home)?;
    let dir = record.dir(home);
    {
        let _sandbox = record::lock(home, name)?;
        direct::release(home, &record);
    }
    remove_folder(&dir)?;
    done(events);
    let kept = left.map(|left| {
        format!(
            "its machine {}'s worker keeps its record: {left}",
            record.machine()
        )
    });
    let container = format!(
        "its container of the retired docker backend was not touched: {}",
        super::reopen::container_fix(container)
    );
    Ok(Removed::AlreadyGone {
        left: std::iter::once(container).chain(kept).collect(),
    })
}

/// Refuses a remove while another operation's intent holds Sandbox
/// `record` (`Busy`); a gone owner's uncertain intent is what a remove
/// clears, and a gone owner's certain one holds nothing.
fn clearable(record: &SandboxRecord) -> Result<()> {
    match record.op() {
        Some(op) if op.standing() == Standing::Unresolved => Ok(()),
        _ => record.free(),
    }
}

/// Removes Sandbox `record`, which has no id: its create's outcome was not
/// known, or its process ended before recording one. The folder goes and
/// the name is free. The caller holds the home and Sandbox locks and has
/// refused a live create.
fn remove_unstarted(home: &Path, record: &SandboxRecord) -> Result<Removed> {
    let unresolved = (record.op()).is_some_and(|op| op.standing() == Standing::Unresolved);
    remove_folder(&record.dir(home))?;
    let mut left = Vec::new();
    if unresolved {
        left.push("its create's outcome was unknown: a Sandbox it may have started on its machine is not recorded here".to_owned());
    }
    Ok(Removed::AlreadyGone { left })
}
