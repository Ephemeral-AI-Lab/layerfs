use layerfs_layerstack_store::{Result, StoreError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourcePolicy {
    pub max_spool_bytes: u64,
    pub max_final_delta_memory_bytes: u64,
}

impl Default for ResourcePolicy {
    fn default() -> Self {
        Self {
            max_spool_bytes: 1024 * 1024 * 1024,
            max_final_delta_memory_bytes: 8 * 1024 * 1024,
        }
    }
}

impl ResourcePolicy {
    pub(crate) fn check(self, spool_bytes: u64) -> Result<()> {
        if spool_bytes <= self.max_spool_bytes {
            Ok(())
        } else {
            Err(StoreError::InvalidInput("workspace spool limit"))
        }
    }

    pub(crate) fn check_final_delta(self, memory_bytes: u64) -> Result<()> {
        if memory_bytes <= self.max_final_delta_memory_bytes {
            Ok(())
        } else {
            Err(StoreError::InvalidInput("workspace final-delta limit"))
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
}
