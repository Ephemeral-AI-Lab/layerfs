//! The VM and ssh kinds (the port of `api/runtime/src/platform/vm/`): one VM
//! per machine, named `<prefix><machine>`, with one data disk at
//! `/layerfs/data` holding many Stores and one worker. No guest mounts the
//! host home. `shared` holds what the adapters share; `forward` is the
//! detached `ssh -L` that `ssh` and `firecracker` use.
pub(crate) mod apple_container;
pub(crate) mod firecracker;
mod forward;
pub(crate) mod lima;
pub(crate) mod microsandbox;
mod shared;
pub(crate) mod ssh;
