//! `sandbox.create`: the name claimed under the home lock with the create's
//! `op` intent (#536 §7.1), the Sandbox started on its machine, and its
//! result recorded with the intent dropped in one write.
use super::direct;
use super::record::{self, SandboxRecord};
use crate::common::context::Context;
use crate::common::home::{self, paths};
use crate::common::op::Op;
use crate::common::outcome::{refused, Outcome, View};
use crate::sandbox::route::on_machine;
use crate::store::record::StoreRef;
use crate::store::resolve;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_sandbox::{ExecMode, IsolationClass};

/// `sandbox.create(project, name, machine?, { image, isolation })`: with
/// no `machine`, a sqlite Store's machine; a PostgreSQL Store defaults to
/// `docker` (`container`) and `process` is opt-in (#551 B0-147).
pub(super) fn create(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let project = args.get_text("project").unwrap_or("");
    let name = args.get_text("name").unwrap_or("");
    let named = args.get_text("machine").filter(|m| !m.is_empty());
    crate::common::names::SandboxName::parse(name)?;
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
    // the isolation the caller asks for (#551 B0-147); `None` means it
    // follows the machine it named, or the default below
    let asked = match args.get_text("isolation").filter(|value| !value.is_empty()) {
        Some(name) => Some(IsolationClass::parse(name).ok_or_else(|| {
            refused(
                "InvalidInput",
                format!(
                    "isolation is one of {}, not {name}",
                    IsolationClass::ALL
                        .iter()
                        .map(|class| class.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        })?),
        None => None,
    };
    let (machine, wanted) = match (named, &target.store_ref) {
        (Some(machine), _) => (machine.to_owned(), asked),
        (None, StoreRef::Sqlite { .. }) => (target.store_ref.machine()?.to_owned(), asked),
        // a PostgreSQL Store is on no machine: the default is `docker`
        // where the host has it, and `process` is only ever opt-in
        (None, _) => {
            let wanted = asked.unwrap_or(IsolationClass::Container);
            let machine = match wanted {
                IsolationClass::Container => "docker",
                IsolationClass::Process => "native",
                IsolationClass::Vm | IsolationClass::Remote => {
                    return Err(unavailable(
                        wanted,
                        None,
                        format!(
                            "isolation {wanted} needs a machine; name one (`machine list` shows the machines)"
                        ),
                    ))
                }
            };
            (machine.to_owned(), Some(wanted))
        }
    };
    let machine = machine.as_str();
    // refused too while the machine is being removed
    #[cfg(feature = "machine")]
    let machine_record = Some(crate::machine::machine_of(&home, machine)?);
    #[cfg(not(feature = "machine"))]
    let machine_record = crate::machine::record::MachineRecord::read(&home, machine)?;
    #[cfg(feature = "machine")]
    {
        // where a Sandbox of this Store's kind may run is the support
        // table's answer, word for word (#535 §7.3)
        use layerfs_machine::{sandbox_allowed, HostOs};
        let platform = machine_record
            .as_ref()
            .expect("machine_of read the record")
            .platform()?;
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
    if machine_record.is_none() {
        return Err(refused("NotFound", format!("no machine named {machine}")));
    }
    // the class the machine actually provides (#551 B0-148); the caller can
    // ask for it explicitly or let it follow the machine it named
    let available = machine_record
        .as_ref()
        .and_then(|record| record.isolation_class());
    let Some(effective) = available else {
        return Err(unavailable(
            wanted.unwrap_or(IsolationClass::Process),
            None,
            format!("machine {machine} provides no Sandbox isolation on this host"),
        ));
    };
    let requested = wanted.unwrap_or(effective);
    if !effective.satisfies(requested) {
        let fix = match requested {
            IsolationClass::Process => {
                "name a process machine (`machine list`) or pass isolation: \"process\"".to_owned()
            }
            IsolationClass::Container => {
                "use a docker machine (`machine update docker ...`) or pass isolation: \"process\""
                    .to_owned()
            }
            IsolationClass::Vm => {
                "create a VM machine (`machine create ...`) and name it".to_owned()
            }
            IsolationClass::Remote => {
                "connect an ssh machine (`machine connect ...`) and name it".to_owned()
            }
        };
        return Err(unavailable(
            requested,
            Some(effective),
            format!("machine {machine} provides {effective} isolation, not {requested}; {fix}"),
        ));
    }
    let mode = machine_record
        .as_ref()
        .and_then(|record| record.exec_mode())
        .unwrap_or(ExecMode::Worker);
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
    let record = SandboxRecord::creating(name, project, machine, requested, effective, mode, &op);
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
    let mut more = vec![];
    if let Some(image) = args.get_text("image") {
        more.push(("image", SdkValue::text(image)));
    }
    let started = on_machine(context, &record, "sandbox.create", more);
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
            let shown = record.to_sdk(View::Full);
            // its Workspace calls go through the worker: say why
            Ok(match opened.and_then(|opened| opened.failed) {
                Some(why) => home::with_field(shown, "forward_failed", SdkValue::Text(why)),
                None => shown,
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

/// A requested isolation the machine cannot provide (#551 B0-148): the
/// code is always `Unavailable`, and the detail carries both classes so a
/// caller never has to parse the message. Never a downgraded Sandbox.
fn unavailable(
    requested: IsolationClass,
    available: Option<IsolationClass>,
    message: String,
) -> SdkRefusal {
    let mut refusal = refused("Unavailable", message);
    refusal.detail = SdkValue::map([
        ("requested", SdkValue::text(requested.as_str())),
        (
            "available",
            SdkValue::opt_text(available.map(|class| class.as_str())),
        ),
    ]);
    refusal
}

/// Removes a Sandbox's folder; one its own side removed already is gone.
pub(super) fn remove_folder(
    dir: &std::path::Path,
) -> Result<(), layerfs_bridge::control::SdkRefusal> {
    match std::fs::remove_dir_all(dir) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            Err(crate::common::outcome::failed(error))
        }
        _ => Ok(()),
    }
}
