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

/// What the Store calls read from the Sandboxes and machines that use a
/// Store, implemented by the crate that owns them (the Sandbox crate's
/// `users::STORE`).
pub trait StoreUsers {
    /// The checks over a Store's users.
    fn related(&self) -> Related;
    /// Machine `machine`'s state as `machine get` derives it from the
    /// records, so a Store's `blocked_by` says what `machine get` says of
    /// its machine.
    fn machine_state(&self, home: &Path, machine: &str) -> Result<State, SdkRefusal>;
    /// The Sandboxes on machine `machine` (a refusal's fix counts them).
    fn sandboxes_on(&self, home: &Path, machine: &str) -> Result<Vec<String>, SdkRefusal>;
    /// `store.detach`: refused while a Sandbox of Store `store`'s projects
    /// is on `machine`.
    fn store_idle_on(&self, home: &Path, store: &str, machine: &str) -> Result<(), SdkRefusal>;
    /// `refusal` with the fix of machine `machine` when it has no VM name,
    /// for a check dialled from it.
    fn nameless_fix(&self, home: &Path, machine: &str, refusal: SdkRefusal) -> SdkRefusal;
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
