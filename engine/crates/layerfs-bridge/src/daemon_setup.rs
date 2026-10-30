//! Bounded private deployment configuration, kept out of container env/argv.
use crate::{
    control::ControlError,
    provision::StoreManifest,
    wire::{Reader, Writer},
};
use std::fmt;
const MAGIC: &[u8] = b"LFSD\x06";
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
    /// Seconds with no Workspace and no control call after which the daemon
    /// ends its process (#373 idle close). Zero never ends it.
    pub idle_exit_s: u32,
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
    /// Exact shared Store file on the named in-VM volume.
    pub store: String,
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
    /// None selects the original one-time installer. Some selects explicit existing
    /// Store open using retained authority; never selected after an install error.
    pub existing_store: Option<StoreManifest>,
    /// Some opens a PostgreSQL Store (#506) instead of the Store file; the
    /// daemon then starts ready, with no install and no Store volume.
    pub postgresql: Option<DaemonPostgres>,
}
/// A PostgreSQL Store a daemon opens. The URL may hold a password: it lives
/// only in this private config, never in env, argv, records or `Debug`.
#[derive(Clone, Eq, PartialEq)]
pub struct DaemonPostgres {
    /// libpq-style connection string or URL, as the daemon dials it.
    pub url: String,
    /// Schema holding the Store: `[a-z_][a-z0-9_]*`, at most 63 bytes.
    pub schema: String,
    /// `off`, `local`, `remote_write`, `on` or `remote_apply`.
    pub synchronous_commit: String,
    /// The Store's binding key, from the home's Store record.
    pub binding: Vec<u8>,
    /// The Store's cursor key, from the home's Store record.
    pub cursor_key: [u8; 32],
}
impl DaemonPostgres {
    fn check(&self) -> Result<(), ControlError> {
        let schema = self.schema.as_bytes();
        let valid = !self.url.is_empty()
            && self.url.len() <= 4096
            && !schema.is_empty()
            && schema.len() <= 63
            && !schema[0].is_ascii_digit()
            && schema
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'_')
            && matches!(
                self.synchronous_commit.as_str(),
                "off" | "local" | "remote_write" | "on" | "remote_apply"
            )
            && !self.binding.is_empty()
            && self.binding.len() <= 128
            && self.cursor_key != [0; 32];
        valid
            .then_some(())
            .ok_or(ControlError("daemon PostgreSQL Store"))
    }
}
impl DaemonSetup {
    /// Encodes one bounded private config. No operation/flow byte limit is implied.
    pub fn encode(&self) -> Result<Vec<u8>, ControlError> {
        self.check()?;
        let mut out = Writer::new(MAGIC);
        for v in [&self.listen, &self.store, &self.overlay, &self.mounts] {
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
        out.put(&l.idle_exit_s.to_be_bytes())?;
        out.byte(u8::from(self.existing_store.is_some()))?;
        if let Some(value) = &self.existing_store {
            out.blob(&value.encode()?)?;
        }
        out.byte(u8::from(self.postgresql.is_some()))?;
        if let Some(p) = &self.postgresql {
            for v in [&p.url, &p.schema, &p.synchronous_commit] {
                out.blob(v.as_bytes())?;
            }
            out.blob(&p.binding)?;
            out.put(&p.cursor_key)?;
        }
        Ok(out.0)
    }
    /// Decodes exactly one config without fallback or profile selection.
    pub fn decode(bytes: &[u8]) -> Result<Self, ControlError> {
        let mut input = Reader::new(bytes, MAGIC)?;
        let listen = input.text(128)?;
        let store = input.text(4096)?;
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
            idle_exit_s: u32::from_be_bytes(input.array()?),
        };
        let existing_store = match input.byte()? {
            0 => None,
            1 => Some(StoreManifest::decode(input.blob(crate::wire::LIMIT)?)?),
            _ => return Err(ControlError("daemon config boolean")),
        };
        let postgresql = match input.byte()? {
            0 => None,
            1 => Some(DaemonPostgres {
                url: input.text(4096)?,
                schema: input.text(63)?,
                synchronous_commit: input.text(16)?,
                binding: input.blob(128)?.to_vec(),
                cursor_key: input.array()?,
            }),
            _ => return Err(ControlError("daemon config boolean")),
        };
        input.finish()?;
        let value = Self {
            listen,
            private_key,
            control_peer,
            store,
            overlay,
            mounts,
            command_uid,
            command_gid,
            limits,
            existing_store,
            postgresql,
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
        for v in [&self.store, &self.overlay, &self.mounts] {
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
        if let Some(m) = &self.existing_store {
            m.check().map_err(ControlError)?;
            if m.locator != self.store || m.profile != crate::provision::StoreProfile::Disposable {
                return Err(ControlError("explicit existing Disposable Store"));
            }
        }
        if let Some(p) = &self.postgresql {
            p.check()?;
            if self.existing_store.is_some() {
                return Err(ControlError("one Store: a file or PostgreSQL"));
            }
        }
        Ok(())
    }
    /// Whether the daemon opens an existing Store at startup (a file or
    /// PostgreSQL), so it starts ready with no install.
    pub fn opens_existing(&self) -> bool {
        self.existing_store.is_some() || self.postgresql.is_some()
    }
}
impl fmt::Debug for DaemonSetup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DaemonSetup")
            .field("listen", &self.listen)
            .field("private_key", &"[redacted]")
            .field("control_peer", &self.control_peer)
            .field("store", &self.store)
            .field("overlay", &self.overlay)
            .field("mounts", &self.mounts)
            .field("command_uid", &self.command_uid)
            .field("command_gid", &self.command_gid)
            .field("limits", &self.limits)
            .field("existing_store", &self.existing_store)
            .field("postgresql", &self.postgresql)
            .finish()
    }
}
impl fmt::Debug for DaemonPostgres {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DaemonPostgres")
            .field("url", &"[redacted]")
            .field("schema", &self.schema)
            .field("synchronous_commit", &self.synchronous_commit)
            .field("cursor_key", &"[redacted]")
            .finish()
    }
}
