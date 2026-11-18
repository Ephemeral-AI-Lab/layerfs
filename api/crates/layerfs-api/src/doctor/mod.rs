//! The `doctor` family: its one call, `doctor.fix`, which removes a
//! leftover (layerfs-project#180), and the answer the six doctor calls
//! share (`answer`): `home`, `machine`, `store`, `project`, `sandbox` and
//! `backup doctor`, each an entry of its own family.
pub(crate) mod answer;
mod fix;

// the six doctor entries live with their families; declared here, so each
// family's list names its doctor in one line
#[path = "../backup/doctor.rs"]
pub(crate) mod backup;
#[path = "../home/doctor.rs"]
pub(crate) mod home;
#[path = "../machine/doctor.rs"]
pub(crate) mod machine;
#[path = "../project/doctor.rs"]
pub(crate) mod project;
#[path = "../sandbox/doctor.rs"]
pub(crate) mod sandbox;
#[path = "../store/doctor.rs"]
pub(crate) mod store;

use crate::calls::Calls;

/// Its calls, in the order help lists them.
pub(crate) fn calls() -> Calls {
    &[fix::CALL]
}
