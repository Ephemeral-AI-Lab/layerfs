//! The VM kinds (the port of `api/runtime/src/platform/vm/`): one VM
//! per machine, named `<prefix><machine>`, with one data disk at
//! `/layerfs/data` holding many Stores and one worker. No guest mounts the
//! host home. `crate::setup::vm` holds what the adapters share;
//! `crate::setup::forward` is the detached `ssh -L` that `lima` and
//! `firecracker` use. `native` and `docker` are the built-in kinds.
pub(crate) mod apple_container;
pub(crate) mod docker;
pub(crate) mod firecracker;
pub(crate) mod lima;
pub(crate) mod native;
