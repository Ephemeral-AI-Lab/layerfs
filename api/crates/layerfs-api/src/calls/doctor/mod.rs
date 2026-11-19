//! The answer the six doctor calls share (`answer`): `home`, `machine`,
//! `store`, `project`, `sandbox` and `backup doctor`, each an entry of its
//! own family, reached as `crate::doctor`. There is no `doctor` family
//! (call surface 2.0); the leftover repair it held is gone with
//! `home.prune` (spec v20): LayerFS removes leftovers itself.
pub(crate) mod answer;

// the six doctor entries live with their families; declared here, so each
// family's list names its doctor in one line
#[path = "../../backup/doctor.rs"]
pub(crate) mod backup;
#[path = "../../home/doctor.rs"]
pub(crate) mod home;
#[path = "../../machine/doctor.rs"]
pub(crate) mod machine;
#[path = "../../project/doctor.rs"]
pub(crate) mod project;
#[path = "../../sandbox/doctor.rs"]
pub(crate) mod sandbox;
#[path = "../../store/doctor.rs"]
pub(crate) mod store;
