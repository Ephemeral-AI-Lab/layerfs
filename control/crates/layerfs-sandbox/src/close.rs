//! `sandbox.close`, and the close of a Sandbox whose create never recorded an id.
//!
//! A close records its `op` intent (`sandbox.close`, #536 §7.1) under the
//! home and Sandbox locks before it stops the runtime, and removes the
//! record at the end. A live intent of another operation refuses it
//! `Busy`; a gone owner's uncertain intent does not: `sandbox.close` is the
//! call that clears it.
use super::create::remove_folder;
use super::direct;
use super::family::named;
use super::record::{self, SandboxRecord};
use crate::host::Call;
use crate::route::on_machine;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home;
use layerfs_home::op::{Op, Standing};
use layerfs_home::outcome::refused;
use std::path::Path;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// `sandbox.close(name, { force })`.
#[derive(Clone, Debug, Default)]
pub struct CloseRequest {
    /// The Sandbox's name.
    pub name: String,
    /// Stop it even while Workspaces are mounted.
    pub force: bool,
}

/// How a close ended.
pub enum Closed {
    /// Its runtime stopped: what its side answered.
    Stopped(SdkValue),
    /// Its runtime was gone already; its record went too.
    WasGone,
    /// A Sandbox recorded under the retired docker backend, forgotten (D-8).
    Forgotten {
        /// Its container, left for the user to remove.
        container: String,
        /// What its machine's worker kept, when it kept its record.
        left: Option<String>,
    },
    /// A Sandbox with no id.
    Unstarted {
        /// The state it was in.
        was: &'static str,
        /// Each reason what it may have started is left unrecorded.
        left: Vec<String>,
    },
}

/// Stops the Sandbox and removes its record.
pub fn close(home: &Path, events: &mut Events<'_>, request: &CloseRequest) -> Result<Closed> {
    let home = home.to_path_buf();
    let name = request.name.as_str();
    if let Some(container) = named(&home, name)?.retired_container(&home)? {
        return forget(&home, events, name, &container);
    }
    let force = request.force;
    let op = Op::begin(
        "sandbox.close",
        "stop",
        SdkValue::map([("force", SdkValue::Bool(force))]),
    )?;
    let record = {
        let _lock = home::lock::take(&home)?;
        let _sandbox = record::lock(&home, name)?;
        let Some(record) = SandboxRecord::read(&home, name)? else {
            return Err(refused("NoSuchSandbox", format!("no Sandbox named {name}")));
        };
        clearable(&record)?;
        if record.id().is_none() {
            return close_unstarted(&home, &record);
        }
        let record = record.with_op(&op);
        record.write(&home)?;
        record
    };
    let mut held = None;
    // a Sandbox reached directly keeps its Workspace records here: its
    // worker is told which it holds (#535 G4)
    if direct::forwarded(&record) {
        match direct::held(&home, &record) {
            Ok(paths) => held = Some(paths),
            Err(refusal) => {
                let _lock = home::lock::take(&home)?;
                let _sandbox = record::lock(&home, name)?;
                if let Some(now) = SandboxRecord::read(&home, name)? {
                    now.without_op().write(&home)?;
                }
                return Err(refusal);
            }
        }
    }
    let closed = on_machine(
        &home,
        events,
        &record,
        Call::Close { force, held },
        crate::dispatch::call,
    );
    let _lock = home::lock::take(&home)?;
    // on `native` the Sandbox's own side removed the folder already
    let dir = record.dir(&home);
    let _sandbox = match dir.exists() {
        true => Some(record::lock(&home, name)?),
        false => None,
    };
    match closed {
        Ok(value) => {
            direct::release(&home, &record);
            remove_folder(&dir)?;
            Ok(Closed::Stopped(value))
        }
        // a Sandbox already gone loses its record too
        Err(refusal) if refusal.code == "NoSuchSandbox" => {
            direct::release(&home, &record);
            remove_folder(&dir)?;
            Ok(Closed::WasGone)
        }
        // its outcome is not known: the record stays with the intent,
        // uncertain, until another `sandbox.close` clears it
        Err(refusal) if refusal.code == "Unknown" => {
            let op = op.as_uncertain();
            if let Some(now) = SandboxRecord::read(&home, name)? {
                now.with_op(&op).write(&home)?;
            }
            op.log_uncertain(&home, "sandbox", name, &refusal.message);
            Err(refusal)
        }
        // a certain failure leaves the Sandbox as it was
        Err(refusal) => {
            if let Some(now) = SandboxRecord::read(&home, name)? {
                now.without_op().write(&home)?;
            }
            Err(refusal)
        }
    }
}

/// Forgets Sandbox `name`, recorded under the retired docker backend (D-8,
/// layerfs-project#111), whose container is `container`: its machine's
/// worker is asked once to forget its own record of it (a worker that
/// cannot be reached or refuses leaves that record, and the answer says
/// so), then the home's record goes. The container is left for the user
/// to remove; the answer names the command.
fn forget(home: &Path, events: &mut Events<'_>, name: &str, container: &str) -> Result<Closed> {
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
    let _lock = home::lock::take(home)?;
    let dir = record.dir(home);
    {
        let _sandbox = record::lock(home, name)?;
        direct::release(home, &record);
    }
    remove_folder(&dir)?;
    Ok(Closed::Forgotten {
        container: container.to_owned(),
        left: left.map(|left| {
            format!(
                "its machine {}'s worker keeps its record: {left}",
                record.machine()
            )
        }),
    })
}

/// Refuses a close while another operation's intent holds Sandbox
/// `record` (`Busy`); a gone owner's uncertain intent is what a close
/// clears, and a gone owner's certain one holds nothing.
fn clearable(record: &SandboxRecord) -> Result<()> {
    match record.op() {
        Some(op) if op.standing() == Standing::Unresolved => Ok(()),
        _ => record.free(),
    }
}

/// Closes Sandbox `record`, which has no id: its create's outcome was not
/// known, or its process ended before recording one. The folder goes and
/// the name is free. The caller holds the home and Sandbox locks and has
/// refused a live create.
fn close_unstarted(home: &Path, record: &SandboxRecord) -> Result<Closed> {
    let was = record.state(false);
    remove_folder(&record.dir(home))?;
    let mut left = Vec::new();
    if was == "unresolved" {
        left.push("its create's outcome was unknown: a Sandbox it may have started on its machine is not recorded here".to_owned());
    }
    // a layout 2 Sandbox whose runtime the conversion found ended
    left.extend(record.left().map(str::to_owned));
    Ok(Closed::Unstarted { was, left })
}
