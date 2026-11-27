//! What uses a Store from outside the Store's records: the Sandboxes of its
//! projects, read from their records by the crates above this one, which
//! pass these checks in (layerfs-project#105), with the machine reads the
//! Store calls take ([`StoreUsers`]). Each runs at the point it ran before,
//! under the same locks.
use crate::admin::open::lease::Owner;
use layerfs_home::state::State;
use layerfs_protocol::sdk::SdkRefusal;
use std::path::Path;

/// A related-use check on the Store or project `name` in home `home`.
type Check = fn(home: &Path, name: &str) -> Result<(), SdkRefusal>;
type Except = fn(home: &Path, name: &str, owner: &Owner) -> Result<(), SdkRefusal>;

/// The Sandboxes on machine `machine` in home `home`.
type Names = fn(home: &Path, machine: &str) -> Result<Vec<String>, SdkRefusal>;
/// A machine's records-only state.
type MachineState = fn(home: &Path, machine: &str) -> Result<State, SdkRefusal>;
/// Refused while a Sandbox of Store `store`'s projects is on `machine`.
type IdleOn = fn(home: &Path, store: &str, machine: &str) -> Result<(), SdkRefusal>;
/// `refusal` with the fix of machine `machine` when it has no VM name.
type Nameless = fn(home: &Path, machine: &str, refusal: SdkRefusal) -> SdkRefusal;

/// What the Store calls read from the Sandboxes and machines that use a
/// Store, passed in by the crates that own them.
#[derive(Clone, Copy)]
pub struct StoreUsers {
    /// The checks over a Store's users.
    pub related: Related,
    /// A machine's state as `machine get` derives it from the records, so a
    /// Store's `blocked_by` says what `machine get` says of its machine.
    pub machine_state: MachineState,
    /// The Sandboxes on a machine (a refusal's fix counts them).
    pub sandboxes_on: Names,
    /// `store.detach`: refused while a Sandbox of the Store is on the machine.
    pub store_idle_on: IdleOn,
    /// The fix of a machine with no VM name, for a check dialled from it.
    pub nameless_fix: Nameless,
}

/// The checks over a Store's users.
#[derive(Clone, Copy)]
pub struct Related {
    /// `store.move`, `store.remove`, `store.disconnect` and
    /// `backup.restore`: refused while another live process holds the Store
    /// or a Sandbox of one of its projects is recorded.
    pub store_idle: Check,
    /// The same check with an authenticated warm worker lease excluded.
    pub store_idle_except: Option<Except>,
    /// `project.delete`: refused `Busy` while a Sandbox of the project is
    /// recorded, even with nothing mounted.
    pub can_remove_project: Check,
}

impl Related {
    pub(crate) fn check_idle(
        self,
        home: &Path,
        store: &str,
        owner: Option<&Owner>,
    ) -> Result<(), SdkRefusal> {
        match (self.store_idle_except, owner) {
            (Some(check), Some(owner)) => check(home, store, owner),
            _ => (self.store_idle)(home, store),
        }
    }
}
