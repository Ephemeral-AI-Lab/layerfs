//! Every namespaces Sandbox's own network: its commands run in a network
//! namespace of their own, joined to the machine by one routed veth pair
//! (`lfs<id12>` on the machine, `eth0` inside) with one /31 of a /16 pool,
//! masqueraded out, siblings and LayerFS's own listening port closed by one
//! LayerFS-owned nftables table. The daemon stays on the machine's network.
//!
//! `pool` (the pool and slot) and `resolver` (the Sandbox's resolver file)
//! are pure rules, built and tested on every OS; the Linux steps that read
//! and change the kernel (`links`, `lock`, `netlink`, `nftables`, `setup`)
//! are Linux only and re-exported here.
#[cfg(target_os = "linux")]
pub(crate) mod links;
#[cfg(target_os = "linux")]
pub(crate) mod lock;
#[cfg(target_os = "linux")]
pub(crate) mod netlink;
#[cfg(target_os = "linux")]
pub(crate) mod nftables;
pub mod pool;
pub mod resolver;
#[cfg(target_os = "linux")]
pub(crate) mod setup;

#[cfg(target_os = "linux")]
pub use self::{
    links::Routes,
    lock::NetworkLock,
    nftables::TABLE,
    setup::{
        configure, connect, forward_policy, forwarding, inspect, plan as plan_slot, pool, prepare,
        private, table, Inspection, Wiring, GUEST, PREFIX,
    },
};

/// Whether this process may change the machine's network (it runs as root):
/// only then does it start namespaces Sandboxes on a Linux host.
#[cfg(target_os = "linux")]
pub fn privileged() -> bool {
    nix::unistd::Uid::effective().is_root()
}
