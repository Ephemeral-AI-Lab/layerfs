//! `sandbox.create`: the name claimed under the home lock with the create's
//! `op` intent (#536 §7.1), the Sandbox started on its machine, and its
//! result recorded with the intent dropped in one write.
use super::direct;
use super::record::{self, SandboxRecord};
use crate::host::{Call, Message};
use crate::route::{on_machine, Asked};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths};
use layerfs_home::op::Op;
use layerfs_home::outcome::refused;
use layerfs_store::record::StoreRef;
use layerfs_store::resolve;
use std::path::Path;

/// `sandbox.create(project, name, machine?, { image })`.
#[derive(Clone, Debug, Default)]
pub struct CreateRequest {
    /// The project, `<store>/<project>`.
    pub project: String,
    /// The Sandbox's name.
    pub name: String,
    /// The machine it names; none (or empty) chooses one.
    pub machine: Option<String>,
    /// The image a namespaces Sandbox runs.
    pub image: Option<String>,
}

/// A Sandbox just started, as recorded.
pub struct Created {
    /// Its record.
    pub record: SandboxRecord,
    /// Why its daemon's forward did not open (#535 G4), when it did not:
    /// its Workspace calls go through the worker.
    pub forward_failed: Option<String>,
}

/// Starts the Sandbox: with no `machine`, on a sqlite Store's machine, or
/// `native` for a PostgreSQL Store (#536 §10).
pub fn create(
    home: &Path,
    events: &mut Events<'_>,
    request: &CreateRequest,
) -> Result<Created, SdkRefusal> {
    let home = home.to_path_buf();
    let project = request.project.as_str();
    let name = request.name.as_str();
    let named = request.machine.as_deref().filter(|m| !m.is_empty());
    layerfs_home::names::SandboxName::parse(name)?;
    if matches!(name, "native" | "docker") {
        return Err(refused(
            "InvalidInput",
            format!("a Sandbox is not named {name}: that names a built-in machine (H §7.6)"),
        ));
    }
    // the Store's place, a move and the name are checked under the lock
    // that claims the name, so a store.push or pull cannot pass between them
    let _lock = home::lock::take(&home)?;
    let target = resolve::target(&home, project)?;
    target.record.unblocked()?;
    let machine = match (named, &target.store_ref) {
        (Some(machine), _) => machine.to_owned(),
        (None, StoreRef::Sqlite { .. }) => target.store_ref.machine()?.to_owned(),
        (None, _) => "native".to_owned(),
    };
    let machine = machine.as_str();
    // refused too while the machine is being removed
    #[cfg(feature = "machine")]
    {
        // where a Sandbox of this Store's kind may run is the support
        // table's answer, word for word (#535 §7.3)
        use layerfs_machine::platform::{sandbox_allowed, HostOs};
        let platform = layerfs_machine::machine_of(&home, machine)?.platform()?;
        sandbox_allowed(platform, HostOs::current(), target.store_ref.kind()).map_err(
            |reason| {
                refused(
                    "Unsupported",
                    format!(
                        "a Sandbox of {} Store {} cannot run on machine {machine} ({}): {reason}",
                        target.store_ref.kind().as_str(),
                        target.store,
                        platform.name()
                    ),
                )
            },
        )?;
    }
    #[cfg(not(feature = "machine"))]
    if layerfs_machine::record::MachineRecord::read(&home, machine)?.is_none() {
        return Err(refused("NotFound", format!("no machine named {machine}")));
    }
    // a PostgreSQL Store is on no machine: its daemon dials the server from
    // wherever it runs (G3), and the create's reach check proves it can
    if let StoreRef::Sqlite { .. } = target.store_ref {
        let store_machine = target.store_ref.machine()?;
        if store_machine != machine {
            return Err(refused(
                "Unsupported",
                format!("Store {} is on machine {store_machine}; a Sandbox of {project} runs there (`store pull` and `store push` move the Store)", target.store),
            ));
        }
    }
    // claim the name across the home (one folder per name) with the
    // create's intent: while its process lives, every other operation on
    // the Sandbox is refused `Busy`
    let dir = paths::sandbox_dir(&home, name);
    if dir.exists() {
        return Err(refused("Exists", format!("a Sandbox named {name} exists")));
    }
    let op = Op::begin("sandbox.create", "claim", SdkValue::Null)?;
    home::private_dir(&dir)?;
    let record = SandboxRecord::creating(name, project, machine, &op);
    let claimed = record::lock(&home, name).and_then(|_sandbox| record.write(&home));
    if let Err(refusal) = claimed {
        // the folder this call made holds no record: the name stays free
        remove_folder(&dir)?;
        return Err(refusal);
    }
    drop(_lock);
    let op = op.at("start");
    {
        let _sandbox = record::lock(&home, name)?;
        record.clone().with_op(&op).write(&home)?;
    }
    let asked = Asked {
        call: Call::Create { platform: None },
        message: Message::Absent,
        image: request.image.clone(),
    };
    let started = on_machine(&home, events, &record, asked, crate::dispatch::call);
    // a worker's Sandbox whose daemon this host reaches directly (#535 G4):
    // its forward opens before the locks are taken
    let opened = (started.as_ref().ok()).map(|value| direct::open(&home, &record, value));
    let _lock = home::lock::take(&home)?;
    let sandbox = record::lock(&home, name)?;
    // read again: on `native` the Sandbox's own side added to the record
    let record = SandboxRecord::read(&home, name)?.unwrap_or(record);
    match started {
        Ok(value) => {
            let mut record = record.started(&value);
            if let Some(opened) = &opened {
                record = direct::save(&home, record, opened)?;
            }
            record.write(&home)?;
            Ok(Created {
                record,
                forward_failed: opened.and_then(|opened| opened.failed),
            })
        }
        // its outcome is not known: the intent stays, uncertain, until
        // `sandbox.close` clears it
        Err(refusal) if refusal.code == "Unknown" => {
            let op = op.as_uncertain();
            record.with_op(&op).write(&home)?;
            op.log_uncertain(&home, "sandbox", name, &refusal.message);
            Err(refusal)
        }
        Err(refusal) => {
            remove_folder(&record.dir(&home))?;
            drop(sandbox);
            Err(refusal)
        }
    }
}

/// Removes a Sandbox's folder; one its own side removed already is gone.
pub fn remove_folder(dir: &Path) -> Result<(), SdkRefusal> {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(layerfs_home::outcome::failed(error))
        }
        _ => Ok(()),
    }
}
