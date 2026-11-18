//! Provider-independent ordinary command metadata; each backend checks it.
use crate::CommandIdentity;
use std::fmt;
/// Caller-owned ordinary command metadata. No hidden shell/classifier/preparation.
#[derive(Clone)]
pub struct ExecRequest {
    pub identity: CommandIdentity,
    pub arguments: Vec<String>,
    pub environment: Vec<String>,
    pub directory: String,
    /// The command's standard input, given whole; `None`: it reads
    /// `/dev/null`.
    pub stdin: Option<Vec<u8>>,
}
impl fmt::Debug for ExecRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExecRequest")
            .field("identity", &self.identity)
            .field("arguments_count", &self.arguments.len())
            .field("environment_count", &self.environment.len())
            .field("directory", &self.directory)
            .field("stdin_bytes", &self.stdin.as_ref().map(Vec::len))
            .finish()
    }
}
