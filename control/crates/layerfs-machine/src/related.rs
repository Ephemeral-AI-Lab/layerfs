//! What a machine holds that its own records do not say: its Stores and
//! Sandboxes, read from their records by `layerfs-control`, which passes
//! these reads in (layerfs-project#104). A machine call runs each at the
//! point it ran before, under the same locks.
use layerfs_bridge::control::SdkRefusal;
use std::path::Path;

/// A read of the names on machine `name` in home `home`.
type Names = fn(home: &Path, name: &str) -> Result<Vec<String>, SdkRefusal>;
/// An `InUse` check on machine `name` in home `home`.
type Check = fn(home: &Path, name: &str) -> Result<(), SdkRefusal>;

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
    /// (UI2-8): the bare `machine remove` it carries is refused `InUse`
    /// while things are recorded on the machine, so the chain that removes
    /// them first replaces it. The holders are read only here, on the
    /// refusal path; any other refusal, or one whose reads fail, is kept.
    pub fn nameless_fix(&self, home: &Path, name: &str, refusal: SdkRefusal) -> SdkRefusal {
        use crate::record::{no_vm_name_fix, Holders, MachineRecord};
        let bare = no_vm_name_fix(name, Holders::default());
        if refusal.detail.get_text("fix") != Some(bare.as_str()) {
            return refusal;
        }
        let read = || -> Result<_, SdkRefusal> {
            let nameless = MachineRecord::read(home, name)?.is_some_and(|r| r.nameless());
            let names = [self.sandboxes_on, self.stores_on, self.hosted_on];
            let found: Vec<Vec<String>> = match nameless {
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
                layerfs_home::outcome::with_fix(refusal, no_vm_name_fix(name, held))
            }
            _ => refusal,
        }
    }
}
