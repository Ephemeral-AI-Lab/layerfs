//! Private, admitted DirectoryRoots construction state and explicit custody.

mod adapter;
mod authority;
mod index;
mod native;
mod profile;
mod session;
mod status;

pub use adapter::ScratchAdapter;
pub use authority::ScratchAuthority;
pub use session::ScratchSession;
pub use status::{NativeIdentity, ScratchDisposition, ScratchOwnerStatus, ScratchProfile};
