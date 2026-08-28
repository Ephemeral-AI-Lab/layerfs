//! Authenticated legacy_full capture used only by frozen evaluator fixtures.

mod bootstrap;
mod directory;
mod hard_links;
mod identity;
mod metadata;
mod regular;
mod workflow;

pub(crate) use bootstrap::initialize_empty;
use directory::capture_directory;
pub(crate) use hard_links::live_hard_link_authority;
use identity::HardLink;
pub(crate) use identity::SemanticDigestCache;
pub(crate) use metadata::put_metadata_observed;
use regular::capture_regular;
pub(crate) use workflow::capture_workspace;
