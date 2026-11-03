//! Bounded private deployment configuration, kept out of container env/argv.
use crate::{
    control::ControlError,
    store::DaemonStore,
    support::KernelRoute,
    wire::{Reader, Writer},
};
use std::fmt;
const MAGIC: &[u8] = b"LFSD\x08";
/// Explicit daemon startup resources; these are windows/admission, not file caps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DaemonLimits {
    /// Ordinary control connections; one additional slot is reserved for startup.
    pub connections: u16,
    /// Maximum blocking I/O wait during authentication and the first metadata record.
    /// Cleared before product operations; never applies to commands.
    pub handshake_ms: u32,
    /// Shared immutable cache allowance, separate from whole-process memory.
    pub cache_bytes: u64,
    /// Explicit simultaneously admitted Workspace count.
    pub namespaces: u32,
    /// Explicit Overlay pager suggestion, not a resident ceiling.
    pub pager_kib: u32,
    /// Unconsumed local inode serials below which a create makes one early
    /// reservation attempt. Zero makes none before the range is exhausted.
    pub serial_low_water: u64,
}
/// Private configuration for one real daemon; caller supplies all policy/resource values.
#[derive(Clone, Eq, PartialEq)]
pub struct DaemonSetup {
    /// TCP listen address; selected explicitly by deployment.
    pub listen: String,
    /// Daemon Noise static secret, stored only in a protected file.
    pub private_key: [u8; 32],
    /// Authorized controller's static public key.
    pub control_peer: [u8; 32],
    /// The kernel filesystem route this daemon serves.
    pub route: KernelRoute,
    /// Separate per-daemon Overlay file.
    pub overlay: String,
    /// Workspace mount directory root; no per-command launcher exists.
    pub mounts: String,
    /// Ordinary command uid, distinct from daemon uid.
    pub command_uid: u32,
    /// Ordinary command gid, with no privileged supplementary groups injected.
    pub command_gid: u32,
    /// Explicit startup resource settings.
    pub limits: DaemonLimits,
    /// The Store this daemon opens.
    pub store: DaemonStore,
}
impl DaemonSetup {
    /// Encodes one bounded private config. No operation/flow byte limit is implied.
    pub fn encode(&self) -> Result<Vec<u8>, ControlError> {
        self.check()?;
        let mut out = Writer::new(MAGIC);
        for v in [&self.listen, &self.overlay, &self.mounts] {
            out.blob(v.as_bytes())?;
        }
        out.put(&self.private_key)?;
        out.put(&self.control_peer)?;
        out.put(&self.command_uid.to_be_bytes())?;
        out.put(&self.command_gid.to_be_bytes())?;
        let l = self.limits;
        out.put(&l.connections.to_be_bytes())?;
        out.put(&l.handshake_ms.to_be_bytes())?;
        out.put(&l.cache_bytes.to_be_bytes())?;
        for v in [l.namespaces, l.pager_kib] {
            out.put(&v.to_be_bytes())?;
        }
        out.put(&l.serial_low_water.to_be_bytes())?;
        out.byte(match self.route {
            KernelRoute::Fuse => 0,
            KernelRoute::Nfs => 1,
        })?;
        self.store.encode(&mut out)?;
        Ok(out.0)
    }
    /// Decodes exactly one config without fallback or profile selection.
    pub fn decode(bytes: &[u8]) -> Result<Self, ControlError> {
        let mut input = Reader::new(bytes, MAGIC)?;
        let listen = input.text(128)?;
        let overlay = input.text(4096)?;
        let mounts = input.text(4096)?;
        let private_key = input.array()?;
        let control_peer = input.array()?;
        let command_uid = u32::from_be_bytes(input.array()?);
        let command_gid = u32::from_be_bytes(input.array()?);
        let limits = DaemonLimits {
            connections: u16::from_be_bytes(input.array()?),
            handshake_ms: u32::from_be_bytes(input.array()?),
            cache_bytes: u64::from_be_bytes(input.array()?),
            namespaces: u32::from_be_bytes(input.array()?),
            pager_kib: u32::from_be_bytes(input.array()?),
            serial_low_water: u64::from_be_bytes(input.array()?),
        };
        let route = match input.byte()? {
            0 => KernelRoute::Fuse,
            1 => KernelRoute::Nfs,
            _ => return Err(ControlError("daemon route")),
        };
        let store = DaemonStore::decode(&mut input)?;
        input.finish()?;
        let value = Self {
            listen,
            private_key,
            control_peer,
            route,
            overlay,
            mounts,
            command_uid,
            command_gid,
            limits,
            store,
        };
        value.check()?;
        Ok(value)
    }
    fn check(&self) -> Result<(), ControlError> {
        if self.listen.parse::<std::net::SocketAddr>().is_err()
            || self.private_key == [0; 32]
            || self.control_peer == [0; 32]
            || self.command_uid == 0
            || self.command_gid == 0
        {
            return Err(ControlError("daemon address/identity"));
        }
        for v in [&self.overlay, &self.mounts] {
            let p = std::path::Path::new(v);
            if !p.is_absolute()
                || v.len() > 4096
                || v.as_bytes().contains(&0)
                || p.components().any(|c| {
                    matches!(
                        c,
                        std::path::Component::ParentDir | std::path::Component::CurDir
                    )
                })
            {
                return Err(ControlError("daemon path"));
            }
        }
        let l = self.limits;
        if l.handshake_ms == 0 || l.connections == 0 || l.namespaces == 0 || l.pager_kib == 0 {
            return Err(ControlError("daemon resources"));
        }
        self.store.valid()
    }
    /// Whether the daemon opens an existing Store at startup (a file or
    /// PostgreSQL), so it starts ready with no install.
    pub fn opens_existing(&self) -> bool {
        self.store.opens_existing()
    }
}
impl fmt::Debug for DaemonSetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DaemonSetup")
            .field("listen", &self.listen)
            .field("private_key", &"[redacted]")
            .field("control_peer", &self.control_peer)
            .field("route", &self.route)
            .field("overlay", &self.overlay)
            .field("mounts", &self.mounts)
            .field("command_uid", &self.command_uid)
            .field("command_gid", &self.command_gid)
            .field("limits", &self.limits)
            .field("store", &self.store)
            .finish()
    }
}
