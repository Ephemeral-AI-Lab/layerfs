use crate::{Presentation, Result};
use std::path::Path;
use std::time::Duration;

/// Presentation-specific mutation state. Portable candidate construction and
/// Branch publication deliberately remain outside this contract.
pub trait WorkspaceDriver: Send {
    fn presentation(&self) -> Presentation;
    fn view_path(&self) -> Option<&Path>;
    fn quiesce(&mut self, _timeout: Duration) -> Result<()> {
        Ok(())
    }
    fn freeze(&mut self) -> Result<()>;
    fn cleanup(&mut self) -> Result<()>;
}
