//! Workspace facade borrows the single authenticated control owner.
use crate::control::Control;
use layerfs_bridge::control::{Reply, Request};

/// Explicit filesystem operations on one caller-owned sequential control channel.
/// Concurrent callers use separate authenticated channels. This owns no process.
pub struct WorkspaceApi<'a> {
    pub(crate) control: &'a mut Control,
    observation_scope: Option<&'a [u8; 32]>,
}
impl<'a> WorkspaceApi<'a> {
    /// Borrows an existing authenticated channel without reconnect or correlation reset.
    pub fn new(control: &'a mut Control) -> Self {
        Self {
            control,
            observation_scope: None,
        }
    }
    /// Borrows a channel with explicit numeric diagnostics for this facade's
    /// calls. The daemon retains no preference and returns the ordinary replies.
    pub fn with_observations(control: &'a mut Control, scope: &'a [u8; 32]) -> Self {
        Self {
            control,
            observation_scope: Some(scope),
        }
    }
    pub(crate) fn exchange(
        &mut self,
        request: Request,
    ) -> Result<Reply, Box<crate::OperationFailure>> {
        let request = match self.observation_scope {
            Some(scope) => Request::Observed {
                scope: *scope,
                request: Box::new(request),
            },
            None => request,
        };
        crate::operation::exchange(self.control, request)
    }
}
