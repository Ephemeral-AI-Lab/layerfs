//! Workspace facade borrows the single authenticated control owner.
use crate::control::Control;

/// Explicit filesystem operations on one caller-owned sequential control channel.
/// Concurrent callers use separate authenticated channels. This owns no process.
pub struct WorkspaceApi<'a> {
    pub(crate) control: &'a mut Control,
}
impl<'a> WorkspaceApi<'a> {
    /// Borrows an existing authenticated channel without reconnect or correlation reset.
    pub fn new(control: &'a mut Control) -> Self {
        Self { control }
    }
}
