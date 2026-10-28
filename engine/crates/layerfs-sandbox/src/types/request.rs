//! Provider-independent ordinary command metadata and validation.
use crate::{CommandIdentity, RuntimeError};
use std::fmt;
/// Caller-owned ordinary command metadata. No hidden shell/classifier/preparation.
#[derive(Clone)]
pub struct ExecRequest {
    pub identity: CommandIdentity,
    pub arguments: Vec<String>,
    pub environment: Vec<String>,
    pub directory: String,
    pub stdin: bool,
}
impl fmt::Debug for ExecRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecRequest")
            .field("identity", &self.identity)
            .field("arguments_count", &self.arguments.len())
            .field("environment_count", &self.environment.len())
            .field("directory", &self.directory)
            .field("stdin", &self.stdin)
            .finish()
    }
}
impl ExecRequest {
    pub(crate) fn check(&self) -> Result<(), RuntimeError> {
        self.identity.check()?;
        if self.arguments.is_empty()
            || self.arguments[0].is_empty()
            || !std::path::Path::new(&self.directory).is_absolute()
            || self
                .arguments
                .iter()
                .chain(&self.environment)
                .chain(std::iter::once(&self.directory))
                .any(|v| v.as_bytes().contains(&0))
        {
            return Err(RuntimeError::Protocol("ordinary command fields"));
        }
        if self
            .environment
            .iter()
            .any(|v| !v.contains('=') || v.starts_with('='))
        {
            return Err(RuntimeError::Protocol("ordinary environment field"));
        }
        Ok(())
    }
}
