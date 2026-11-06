//! Refuse new work on disabled platforms before home creation, locks, or
//! dispatch. Read-only inspection and explicit teardown retain their routes.
//! This also runs in builds without machine adapters, so a recorded worker or
//! a forwarded daemon cannot bypass the product policy.
use crate::machine::record::MachineRecord;
use crate::sandbox::record::SandboxRecord;
use crate::store::record::{StoreKind, StoreRecord};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::refused;
use std::path::Path;

type Result = std::result::Result<(), SdkRefusal>;

fn platform(name: &str) -> Result {
    match layerfs_isolation::platform_policy::disabled_reason(name) {
        Some(reason) => Err(refused("Unsupported", reason)),
        None => Ok(()),
    }
}

fn machine(home: &Path, name: &str) -> Result {
    layerfs_home::names::MachineName::parse(name)?;
    if let Some(record) = MachineRecord::read(home, name)? {
        platform(record.platform_name().unwrap_or(""))?;
    }
    Ok(())
}

fn store(home: &Path, name: &str) -> Result {
    layerfs_home::names::StoreName::parse(name)?;
    if let Some(record) = StoreRecord::read(home, name)? {
        if record.store_ref()?.kind() == StoreKind::Sqlite {
            machine(home, record.store_ref()?.machine()?)?;
        }
    }
    Ok(())
}

fn project(home: &Path, name: &str) -> Result {
    if let Some((store_name, _)) = name.split_once('/') {
        store(home, store_name)?;
    }
    Ok(())
}

fn sandbox(home: &Path, name: &str) -> Result {
    layerfs_home::names::SandboxName::parse(name)?;
    if let Some(record) = SandboxRecord::read(home, name)? {
        // A saved daemon still names its platform if the machine record is gone.
        platform(record.to_record().get_text("platform").unwrap_or(""))?;
        machine(home, record.machine())?;
        if let Some(name) = record.project() {
            project(home, name)?;
        }
    }
    Ok(())
}

fn transfer(home: &Path, name: &str) -> Result {
    store(home, name)?;
    if let Some(record) = StoreRecord::read(home, name)? {
        // A new transfer must not implicitly retire a historical copy on a
        // disabled machine before it checks the newly selected destination.
        if let Some(left) = crate::store::transfer::resolve::leftover(&record) {
            machine(home, &left.from)?;
            machine(home, &left.to)?;
        }
    }
    Ok(())
}

/// Checks the fully validated public call, independent of front end or features.
pub(super) fn check(home: &Path, name: &str, args: &SdkValue) -> Result {
    let text = |field| args.get_text(field).unwrap_or("");
    match name {
        "machine.create" => platform(text("platform")),
        "machine.connect" => platform("ssh"),
        "machine.start" | "machine.update" => machine(home, text("name")),
        "sandbox.create" => {
            if let Some(name) = args.get_text("machine") {
                machine(home, name)?;
            }
            project(home, text("project"))
        }
        "workspace.mount" => sandbox(home, text("sandbox")),
        "workspace.exec" | "workspace.commit" => {
            if let Some((name, _)) = text("ws").split_once(':') {
                sandbox(home, name)?;
            }
            Ok(())
        }
        "store.push" => {
            machine(home, text("machine"))?;
            transfer(home, text("name"))
        }
        "store.pull" => transfer(home, text("name")),
        "backup.create" => store(home, text("store")),
        "project.init" | "branch.fork" | "branch.merge" | "branch.publish" | "file.write"
        | "file.edit" | "file.apply" | "file.rm" | "file.mkdir" | "file.mv" | "file.cp" => {
            project(home, text("project"))
        }
        _ => Ok(()),
    }
}
