//! `sandbox.create`: the name claimed under the home lock with the create's
//! `op` intent (#536 §7.1), the Sandbox started on its machine, and its
//! result recorded with the intent dropped in one write. Its progress is
//! the image copy's phases when it names an image, then `start`, and
//! `mount` when the caller mounts a Branch next (`sandbox create --mount`, which the call surface runs
//! after this, with the Workspace calls).
use super::direct;
use super::record::{self, SandboxRecord};
use crate::host::{Call, Message, Started};
use crate::route::{on_machine, Asked};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::events::Events;
use layerfs_home::home::{self, paths};
use layerfs_home::op::Op;
use layerfs_home::outcome::shell_word;
use layerfs_home::outcome::{public_code, refused, with_fix, with_next, FIX};
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
    /// The image a namespaces Sandbox runs: a digest, Docker image Id or
    /// Docker reference, copied to the machine when it does not hold it.
    pub image: Option<String>,
    /// Whether the caller mounts a Branch once it started: its progress
    /// then counts a `mount` phase, which the caller reports.
    pub mount: bool,
}

/// A create's own phases, `start` and with `mount` `mount`, after the
/// image copy's (V19-8: `inspect, lock, export, send, unpack, start,
/// mount`).
fn total(mount: bool) -> i64 {
    if mount {
        2
    } else {
        1
    }
}

/// A Sandbox just started, as recorded.
pub struct Created {
    /// Its record.
    pub record: SandboxRecord,
    /// Why its daemon's forward did not open (#535 G4), when it did not:
    /// its Workspace calls go through the worker.
    pub forward_failed: Option<String>,
}

/// The machine a Sandbox of PostgreSQL Store `store` runs on when the call
/// names none: the one machine attached to it. None attached is refused
/// with the attach as its fix; several are listed in `next` and the call
/// is asked to name one.
fn only_attached(store: &str, attached: Vec<String>) -> Result<String, SdkRefusal> {
    match attached.as_slice() {
        [machine] => Ok(machine.clone()),
        [] => Err(with_fix(
            refused(
                "InvalidInput",
                format!("no machine is attached to PostgreSQL Store {store} to run a Sandbox on"),
            ),
            format!(
                "layerfs store attach {} --machine <machine>",
                shell_word(store)
            ),
        )),
        several => {
            let message = format!(
                "PostgreSQL Store {store} is attached to {} machines ({}): name one with --machine",
                several.len(),
                several.join(", ")
            );
            let names = several.iter().map(SdkValue::text).collect();
            Err(with_next(
                refused("InvalidInput", message),
                "machines",
                SdkValue::List(names),
            ))
        }
    }
}

/// Starts the Sandbox: with no `machine`, on a sqlite Store's machine, or
/// on the one machine attached to a PostgreSQL Store. A PostgreSQL Store's
/// Sandbox runs only on a machine attached to it (`store.attach`) whose
/// platform runs PostgreSQL Sandboxes.
pub fn create(
    home: &Path,
    events: &mut Events<'_>,
    request: &CreateRequest,
) -> Result<Created, SdkRefusal> {
    create_on(home, events, request, crate::dispatch::call)
}

/// [`create`], whose start runs through `local` on a machine whose calls
/// run in this process (`crate::dispatch::call` in [`create`]; a test may
/// stand in for the Sandbox's own side there).
pub fn create_on(
    home: &Path,
    events: &mut Events<'_>,
    request: &CreateRequest,
    local: crate::route::Local,
) -> Result<Created, SdkRefusal> {
    let home = home.to_path_buf();
    let project = request.project.as_str();
    let name = request.name.as_str();
    let named = request.machine.as_deref().filter(|m| !m.is_empty());
    layerfs_home::names::SandboxName::parse(name)?;
    if layerfs_home::names::BUILT_IN.contains(&name) {
        return Err(refused(
            "InvalidInput",
            format!("a Sandbox is not named {name}: that names a built-in machine"),
        ));
    }
    // `<name>.root` in the run folder is Sandbox `<name>`'s private root
    if name.ends_with(".root") {
        return Err(refused(
            "InvalidInput",
            format!(
                "a Sandbox is not named {name}: a name ending in `.root` is the run folder's \
                 name for another Sandbox's private root"
            ),
        ));
    }
    // the Store's place, a move and the name are checked under the lock
    // that claims the name, so a store.move cannot pass between them
    let _lock = home::lock::take(&home)?;
    let target = resolve::target(&home, project)?;
    target.record.unblocked()?;
    let machine = match (named, &target.store_ref) {
        (Some(machine), _) => machine.to_owned(),
        (None, StoreRef::Sqlite { .. }) => target.store_ref.machine()?.to_owned(),
        (None, _) => only_attached(&target.store, target.record.attached())?,
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
    // a PostgreSQL Store is on no machine: its Sandboxes run on each
    // machine attached to it, each dialling the URL and login this
    // computer saved; the create's reach check proves it can
    if let StoreRef::Postgres { .. } = target.store_ref {
        if !target.record.is_attached(machine) {
            let refusal = refused(
                "InvalidInput",
                format!(
                    "machine {machine} is not attached to PostgreSQL Store {}",
                    target.store
                ),
            );
            let attach = format!(
                "layerfs store attach {} --machine {}",
                shell_word(&target.store),
                shell_word(machine)
            );
            return Err(with_fix(refusal, attach));
        }
    }
    if let StoreRef::Sqlite { .. } = target.store_ref {
        let store_machine = target.store_ref.machine()?;
        if store_machine != machine {
            return Err(refused(
                "Unsupported",
                format!("Store {} is on machine {store_machine}; a Sandbox of {project} runs there (`store move` moves the Store)", target.store),
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
    let record = SandboxRecord::creating(&home, name, project, machine, &op)?;
    home::private_dir(&dir)?;
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
    // a worker keeps its own record of the Sandbox: it is sent the machine
    // and generation minted here, which the mounts it places then carry
    let place = (record.generation()).map(|g| (machine.to_owned(), g.to_owned()));
    let asked = Asked {
        call: Call::Create {
            platform: None,
            place,
        },
        message: Message::Absent,
        image: request.image.clone(),
        start: Some((0, total(request.mount))),
    };
    let started =
        on_machine(&home, events, &record, asked, local).map(|value| Started::read(&value));
    // a worker's Sandbox whose daemon this host reaches directly (#535 G4):
    // its forward opens before the locks are taken
    let opened = (started.as_ref().ok()).map(|started| direct::open(&home, &record, started));
    let _lock = home::lock::take(&home)?;
    let sandbox = record::lock(&home, name)?;
    // read again: on `native` the Sandbox's own side added to the record
    let record = SandboxRecord::read(&home, name)?.unwrap_or(record);
    match started {
        Ok(started) => {
            let mut record = record.started(&started);
            if let Some(opened) = &opened {
                record = direct::save(&home, record, opened)?;
            }
            record.write(&home)?;
            // the started record's newer generation proves the retired one's
            // mounts dead by itself: the name's older tombstone goes
            record::drop_retired(&home, &record);
            if !request.mount {
                events.phase("start", Some(1), Some(1), Vec::new());
            }
            Ok(Created {
                record,
                forward_failed: opened.and_then(|opened| opened.failed),
            })
        }
        // its outcome is not known: the intent stays, uncertain, until
        // `sandbox.remove` clears it
        Err(refusal) if refusal.code == "Unknown" => {
            let op = op.as_uncertain();
            record.with_op(&op).write(&home)?;
            op.log_uncertain(&home, "sandbox", name, &refusal.message);
            Err(refusal)
        }
        // a start that failed is the machine's to explain: its doctor is
        // the fix unless the refusal names a closer one
        Err(refusal) => {
            remove_folder(&record.dir(&home))?;
            drop(sandbox);
            let start_failed =
                public_code(&refusal.code) == "Failed" && refusal.detail.get(FIX).is_none();
            Err(match start_failed {
                true => with_fix(
                    refusal,
                    format!(
                        "layerfs machine doctor {machine}",
                        machine = shell_word(machine)
                    ),
                ),
                false => refusal,
            })
        }
    }
}

/// Removes a Sandbox's folder; one its own side removed already is gone.
pub fn remove_folder(dir: &Path) -> Result<(), SdkRefusal> {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(layerfs_home::outcome::failed(error))
        }
        _ => {
            layerfs_home::home::generation::moved_above(dir);
            Ok(())
        }
    }
}
