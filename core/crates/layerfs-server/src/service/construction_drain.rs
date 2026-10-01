//! Explicit shutdown disposition for unique actual Store scratch domains.
use super::Service;
use layerfs_bridge::contract::{Code, Failure};
impl Service {
    /// Close/unlink known idle metadata owners after all callers have ended.
    /// Active or Unknown ownership remains retained; no global engine shutdown occurs.
    pub fn drain_construction_idle(&self) -> Result<usize, Failure> {
        let mut drained = 0usize;
        for (index, owner) in self.construction.iter().enumerate() {
            if self.construction[..index]
                .iter()
                .any(|earlier| std::sync::Arc::ptr_eq(earlier, owner))
            {
                continue;
            }
            drained = drained
                .checked_add(owner.drain_idle()?)
                .ok_or(Code::Capacity)?;
        }
        Ok(drained)
    }
}
