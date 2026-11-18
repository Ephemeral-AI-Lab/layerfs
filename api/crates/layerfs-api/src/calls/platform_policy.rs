//! Refuse new work on removed platforms before home creation, locks, or
//! dispatch. Read-only inspection and explicit teardown retain their routes.
//! This also runs in builds without machine adapters, so a recorded worker or
//! a forwarded daemon cannot bypass the product policy. Each table entry
//! says which [`Policy`] checks it, so no call name is written here.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::refused;
use layerfs_machine::record::MachineRecord;
use layerfs_sandbox::record::SandboxRecord;
use layerfs_store::record::{StoreKind, StoreRecord};
use std::path::Path;

type Result = std::result::Result<(), SdkRefusal>;

/// What of a call must not be on a removed platform; its table entry says
/// which. Each variant names the argument that holds the thing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// Nothing: a read, or an explicit teardown.
    None,
    /// The platform this argument names.
    Platform(&'static str),
    /// The machine this argument names.
    Machine(&'static str),
    /// The Store this argument names, and its machine.
    Store(&'static str),
    /// The project this argument names: its Store and that Store's machine.
    Project(&'static str),
    /// The Sandbox this argument names: its platform, machine and project.
    Sandbox(&'static str),
    /// The Sandbox of the Workspace (`<sandbox>:<workspace>`) this argument names.
    Workspace(&'static str),
    /// A new Sandbox: its `machine` when given, and its `project`.
    NewSandbox,
    /// A Store's move between machines: the Store this argument names and
    /// the machines of a move it left unfinished.
    Transfer(&'static str),
    /// A Store's move to a machine: [`Policy::Transfer`], and the `machine`
    /// it goes to.
    TransferTo(&'static str),
}

fn platform(name: &str) -> Result {
    match layerfs_sandbox_backend::platform_policy::disabled_reason(name) {
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
        platform(record.platform().unwrap_or(""))?;
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
        // machine of a removed platform before it checks the newly selected destination.
        if let Some(left) = layerfs_store::sqlite::transfer::resolve::leftover(&record) {
            machine(home, &left.from)?;
            machine(home, &left.to)?;
        }
    }
    Ok(())
}

/// Checks the fully validated public call, independent of front end or features.
pub(super) fn check(home: &Path, policy: Policy, args: &SdkValue) -> Result {
    let text = |field| args.get_text(field).unwrap_or("");
    match policy {
        Policy::None => Ok(()),
        Policy::Platform(field) => platform(text(field)),
        Policy::Machine(field) => machine(home, text(field)),
        Policy::Store(field) => store(home, text(field)),
        Policy::Project(field) => project(home, text(field)),
        Policy::Sandbox(field) => sandbox(home, text(field)),
        Policy::Workspace(field) => match text(field).split_once(':') {
            Some((name, _)) => sandbox(home, name),
            None => Ok(()),
        },
        Policy::NewSandbox => {
            if let Some(name) = args.get_text("machine") {
                machine(home, name)?;
            }
            project(home, text("project"))
        }
        Policy::Transfer(field) => transfer(home, text(field)),
        Policy::TransferTo(field) => {
            machine(home, text("machine"))?;
            transfer(home, text(field))
        }
    }
}
