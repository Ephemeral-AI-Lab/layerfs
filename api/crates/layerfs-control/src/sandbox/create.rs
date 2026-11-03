//! `sandbox.create`: the name claimed under the home lock, the Sandbox started on its machine.
use super::record::SandboxRecord;
use crate::common::context::Context;
use crate::common::home;
use crate::common::outcome::{refused, Outcome, View};
use crate::inventory;
use crate::sandbox::route::on_machine;
use crate::store::record::StoreRef;
use crate::store::resolve;
use layerfs_bridge::control::SdkValue;

/// `sandbox.create(project, name, machine, { image })`.
pub(super) fn create(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let home = context.home.clone();
    let project = args.get_text("project").unwrap_or("");
    let name = args.get_text("name").unwrap_or("");
    let machine = args.get_text("machine").unwrap_or("");
    crate::common::names::SandboxName::parse(name)?;
    if matches!(name, "native" | "docker") {
        return Err(refused(
            "InvalidInput",
            format!("a Sandbox is not named {name}: that names a built-in machine (H §7.6)"),
        ));
    }
    // the Store's place, a move and the name are checked under the lock
    // that claims the name, so a store.move cannot pass between them
    let _lock = home::lock::take(&home)?;
    let target = resolve::target(&home, project)?;
    target.record.moving()?;
    // refused too while the machine is being removed
    #[cfg(feature = "machine")]
    {
        // where a Sandbox of this Store's kind may run is the support
        // table's answer, word for word (#535 §7.3)
        use layerfs_machine::{sandbox_allowed, HostOs};
        let platform = crate::machine::machine_of(&home, machine)?.platform()?;
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
    if crate::machine::record::MachineRecord::read(&home, machine)?.is_none() {
        return Err(refused("NotFound", format!("no machine named {machine}")));
    }
    if let StoreRef::Postgres { .. } = target.store_ref {
        // a PostgreSQL Store is on no machine; its daemon dials the server.
        // A build without machines (the guest worker's) knows only these two.
        #[cfg(not(feature = "machine"))]
        if !matches!(machine, "native" | "docker") {
            return Err(refused(
                "Unsupported",
                format!("a Sandbox of PostgreSQL Store {} runs on native or docker; on VMs and ssh hosts it comes later (O3)", target.store),
            ));
        }
    } else if target.store_ref.machine()? != machine {
        let store_machine = target.store_ref.machine()?;
        return Err(refused(
            "Unsupported",
            format!("Store {} is on machine {store_machine}; a Sandbox of {project} runs there (`store move` moves the Store)", target.store),
        ));
    }
    // claim the name across the home; the creator's pid lets a close tell
    // a create in flight from one whose process is gone
    if inventory::sandbox(&home, name)?.is_some() {
        return Err(refused("Exists", format!("a Sandbox named {name} exists")));
    }
    SandboxRecord::creating(name, project, machine).write(&home)?;
    drop(_lock);
    let mut more = vec![];
    if let Some(image) = args.get_text("image") {
        more.push(("image", SdkValue::text(image)));
    }
    let record = SandboxRecord::read(&home, machine, name)?
        .unwrap_or_else(|| SandboxRecord::from_record(machine, name, SdkValue::Null));
    let started = on_machine(context, &record, "sandbox.create", more);
    let _lock = home::lock::take(&home)?;
    match started {
        Ok(value) => {
            let record = record.started(&value);
            record.write(&home)?;
            Ok(record.to_sdk(View::Full))
        }
        Err(refusal) if refusal.code == "Unknown" => {
            record.unknown().write(&home)?;
            Err(refusal)
        }
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(record.dir(&home));
            Err(refusal)
        }
    }
}
