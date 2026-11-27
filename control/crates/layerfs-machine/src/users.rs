//! What a machine holds that its own records do not say: its Stores and
//! Sandboxes, read from their records by the crates above this one, which
//! pass these reads in (layerfs-project#104), and the steps a machine call
//! takes on them ([`MachineUsers`]). A machine call runs each at the point
//! it ran before, under the same locks.
#[cfg(feature = "machine")]
use layerfs_home::reply::outcome::Outcome;
use layerfs_protocol::sdk::SdkRefusal;
use std::path::Path;

/// A read of the names on machine `name` in home `home`.
type Names = fn(home: &Path, name: &str) -> Result<Vec<String>, SdkRefusal>;
/// An `InUse` check on machine `name` in home `home`.
type Check = fn(home: &Path, name: &str) -> Result<(), SdkRefusal>;

/// Settles, in each Store on machine `machine`, the mount intents an
/// earlier worker incarnation left, with incarnation `incarnation`.
type Reconcile = fn(home: &Path, machine: &str, incarnation: &str) -> Result<(), SdkRefusal>;
/// Forgets removed machine `name` where the records of its users name it.
type Forget = fn(home: &Path, name: &str) -> Result<(), SdkRefusal>;
/// A forced stop of machine `name` and its Sandboxes, its machine lock
/// held, reporting on `phases`.
#[cfg(feature = "machine")]
type ForceStop = fn(home: &Path, name: &str, phases: crate::Phases<'_, '_>) -> Outcome;

/// What a machine call reads from and does to the Stores and Sandboxes on
/// the machine, passed in by the crates that own them.
#[derive(Clone, Copy)]
pub struct MachineUsers {
    /// The reads and `InUse` checks.
    pub related: Related,
    /// `machine.start`: settles a previous incarnation's mount intents.
    pub reconcile: Reconcile,
    /// `machine.remove`: forgets the machine's PostgreSQL Store
    /// attachments and its image copy locks.
    pub forget: Forget,
    /// `machine.stop` of the host machine: what still holds it (Sandboxes
    /// not stopped, and Stores another process or a call here holds), each
    /// named; this process's idle Store copies are closed first.
    pub host_holders: Names,
    /// `machine.stop --force`.
    #[cfg(feature = "machine")]
    pub force_stop: ForceStop,
}

/// The reads and `InUse` checks over a machine's Stores and Sandboxes.
#[derive(Clone, Copy)]
pub struct Related {
    /// The Stores whose record names the machine.
    pub stores_on: Names,
    /// The Sandboxes whose record names the machine.
    pub sandboxes_on: Names,
    /// The old Firecracker machine records naming the machine as their
    /// host; read only for a record with no `vm_name`, whose fix removes
    /// them first.
    pub hosted_on: Names,
    /// `machine.remove`: refused while the machine
    /// holds Stores or Sandboxes.
    pub can_remove: Check,
    /// `machine.stop`: refused while Sandboxes run on it.
    pub can_stop: Check,
    /// `machine.update`: refused while a running Sandbox uses its settings.
    pub can_change: Check,
}

impl Related {
    /// `refusal` of a call that needs machine `name`'s VM, with the fix
    /// every view of its state shows when its record has no `vm_name`
    /// (UI2-8) or its VM is gone (area 10): the bare `machine remove` it carries is refused `InUse`
    /// while things are recorded on the machine, so the chain that removes
    /// them first replaces it. The holders are read only here, on the
    /// refusal path; any other refusal, or one whose reads fail, is kept.
    pub fn nameless_fix(&self, home: &Path, name: &str, refusal: SdkRefusal) -> SdkRefusal {
        use crate::record::machine::{no_vm_name_fix, Holders, MachineRecord};
        let bare = no_vm_name_fix(name, Holders::default());
        if refusal.detail.get_text("fix") != Some(bare.as_str()) {
            return refusal;
        }
        let read = || -> Result<_, SdkRefusal> {
            let gone = MachineRecord::read(home, name)?.is_some_and(|r| r.gone());
            let names = [self.sandboxes_on, self.stores_on, self.hosted_on];
            let found: Vec<Vec<String>> = match gone {
                true => names
                    .iter()
                    .map(|on| on(home, name))
                    .collect::<Result<_, _>>()?,
                false => Vec::new(),
            };
            Ok(found)
        };
        match read().as_deref() {
            Ok([sandboxes, stores, hosted]) => {
                let held = Holders {
                    sandboxes,
                    stores,
                    hosted,
                };
                layerfs_home::reply::outcome::with_fix(refusal, no_vm_name_fix(name, held))
            }
            _ => refusal,
        }
    }
}
