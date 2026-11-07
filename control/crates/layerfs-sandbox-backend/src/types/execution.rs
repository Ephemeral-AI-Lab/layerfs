//! Process observation is independent of pipe EOF and filesystem drain.
#[cfg(unix)]
use super::RuntimeError;
/// Explicit nonroot identity for ordinary commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandIdentity {
    pub uid: u32,
    pub gid: u32,
}
impl CommandIdentity {
    /// Read by the backends only, which are Unix only.
    #[cfg(unix)]
    pub(crate) fn check(self) -> Result<(), RuntimeError> {
        if self.uid == 0 || self.gid == 0 {
            Err(RuntimeError::Protocol("nonroot command uid/gid"))
        } else {
            Ok(())
        }
    }
}
