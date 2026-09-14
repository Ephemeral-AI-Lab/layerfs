use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourcePolicy {
    pub max_spool_bytes: u64,
    pub max_final_delta_memory_bytes: u64,
    /// Host-private overlay admission. The existing two-word remote seed does
    /// not export these host-only capacities into container admission.
    pub overlay: OverlayPolicy,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            max_spool_bytes: 1024 * 1024 * 1024,
            max_final_delta_memory_bytes: 8 * 1024 * 1024,
            overlay: OverlayPolicy::default(),
        }
    }
}

/// Independent capacity ceilings, not allocations or performance targets. Disk
/// indexes reserve their data and ownership catalogs; their full disk quota is
/// never charged as resident RAM. One current spool plus one retained predecessor
/// have separate bounds, with relocation headroom added by the host factory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverlayPolicy {
    pub max_index_bytes: u64,
    pub max_payload_index_bytes: u64,
    pub max_retained_payload_bytes: u64,
    pub max_scratch_bytes: u64,
    pub max_memory_bytes: u64,
    pub max_transport_bytes: u64,
    pub max_replay_bytes: u64,
    pub max_roots: usize,
    pub max_payload_owners: usize,
    pub max_readers: usize,
    pub max_writers: usize,
    pub max_prepared: usize,
    pub max_requests: usize,
    pub max_replay_entries: usize,
    pub max_files: usize,
}
impl Default for OverlayPolicy {
    fn default() -> Self {
        Self {
            // Metadata/range graphs and payload ownership have separate files
            // and quotas; large directories do not imply a resident inode map.
            max_index_bytes: 4 * 1024 * 1024 * 1024,
            max_payload_index_bytes: 1024 * 1024 * 1024,
            max_retained_payload_bytes: 1024 * 1024 * 1024,
            max_scratch_bytes: 256 * 1024 * 1024,
            max_memory_bytes: 32 * 1024 * 1024,
            max_transport_bytes: 8 * 1024 * 1024,
            max_replay_bytes: 256 * 1024,
            // Root owners cover cursors/prepared paths; page bodies stay on disk.
            max_roots: 16_384,
            // Maximum *resident* payload owners (live handles plus queued
            // release tickets), not a ceiling on persisted tokens. A Workspace
            // may retain far more changed files than this; their payload
            // records and bytes are charged to the index and arena disk
            // quotas instead.
            max_payload_owners: 8192,
            max_readers: 32,
            max_writers: 4,
            max_prepared: 4,
            max_requests: 256,
            max_replay_entries: 256,
            // Two files per index, two payload arenas, two construction journals.
            max_files: 8,
        }
    }
}
impl OverlayPolicy {
    pub fn validate(self) -> Result<()> {
        if self.max_index_bytes == 0
            || self.max_payload_index_bytes == 0
            || self.max_scratch_bytes == 0
            || self.max_memory_bytes == 0
            || self.max_transport_bytes == 0
            || self.max_replay_bytes == 0
            || self.max_roots < 4
            || self.max_payload_owners == 0
            || self.max_readers == 0
            || self.max_writers == 0
            || self.max_prepared == 0
            || self.max_requests == 0
            || self.max_replay_entries == 0
            || self.max_files < 8
            || self.max_replay_entries > self.max_requests
            || self.max_replay_bytes > self.max_memory_bytes
            || self.max_transport_bytes > self.max_memory_bytes
            || [
                self.max_roots,
                self.max_payload_owners,
                self.max_readers,
                self.max_writers,
                self.max_prepared,
                self.max_requests,
                self.max_replay_entries,
                self.max_files,
            ]
            .into_iter()
            .any(|n| n > u32::MAX as usize)
        {
            return Err(Error::InvalidInput("workspace overlay policy"));
        }
        Ok(())
    }
    pub fn check_memory(self, bytes: u64) -> Result<()> {
        if bytes <= self.max_memory_bytes {
            Ok(())
        } else {
            Err(Error::InvalidInput("workspace overlay memory limit"))
        }
    }
    pub fn check_scratch(self, bytes: u64) -> Result<()> {
        if bytes <= self.max_scratch_bytes {
            Ok(())
        } else {
            Err(Error::InvalidInput("workspace overlay scratch limit"))
        }
    }
    pub fn check_transport(self, bytes: u64) -> Result<()> {
        if bytes <= self.max_transport_bytes {
            Ok(())
        } else {
            Err(Error::InvalidInput("workspace overlay transport limit"))
        }
    }
}

impl ResourcePolicy {
    pub fn check(self, spool_bytes: u64) -> Result<()> {
        if spool_bytes <= self.max_spool_bytes {
            Ok(())
        } else {
            Err(Error::InvalidInput("workspace spool limit"))
        }
    }

    pub fn check_final_delta(self, memory_bytes: u64) -> Result<()> {
        if memory_bytes <= self.max_final_delta_memory_bytes {
            Ok(())
        } else {
            Err(Error::InvalidInput("workspace final-delta limit"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spool_and_final_delta_limits_accept_exact_and_reject_plus_one() {
        let policy = ResourcePolicy::default();
        assert!(policy.check(policy.max_spool_bytes).is_ok());
        assert!(policy.check(policy.max_spool_bytes + 1).is_err());
        assert!(policy
            .check_final_delta(policy.max_final_delta_memory_bytes)
            .is_ok());
        assert!(policy
            .check_final_delta(policy.max_final_delta_memory_bytes + 1)
            .is_err());
    }
    #[test]
    fn overlay_capacity_checks_accept_exact_and_reject_plus_one() {
        let p = OverlayPolicy::default();
        p.validate().unwrap();
        assert!(p.check_memory(p.max_memory_bytes).is_ok());
        assert!(p.check_memory(p.max_memory_bytes + 1).is_err());
        assert!(p.check_scratch(p.max_scratch_bytes).is_ok());
        assert!(p.check_scratch(p.max_scratch_bytes + 1).is_err());
        assert!(p.check_transport(p.max_transport_bytes).is_ok());
        assert!(p.check_transport(p.max_transport_bytes + 1).is_err());
        assert!(OverlayPolicy { max_files: 7, ..p }.validate().is_err());
        assert!(OverlayPolicy {
            max_roots: usize::MAX,
            ..p
        }
        .validate()
        .is_err());
        assert_eq!(
            ResourcePolicy::default().max_spool_bytes,
            1024 * 1024 * 1024
        );
    }
}
