//! What a machine call reports when it does not succeed: one code, the
//! reason, what the host lacks, and whether the provider's VM exists.
use std::fmt;

/// The outcome class of a refused or failed machine call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Code {
    /// The VM, disk or host is already there under that name.
    Exists,
    /// The machine still holds something that must end first.
    InUse,
    /// This host or kind cannot do it; `missing` names what is lacking.
    Unsupported,
    /// A provider or guest that should answer does not.
    Unavailable,
    /// The host refused the login or the host key.
    Denied,
    /// The call ran once and failed; nothing was replayed.
    Failed,
    /// The call's outcome is not known (it timed out or lost its answer);
    /// everything it may have made is kept.
    Unknown,
    /// The caller's spec or bundle is not usable.
    InvalidInput,
    /// The provider holds no VM of that name.
    NotFound,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exists => "Exists",
            Self::InUse => "InUse",
            Self::Unsupported => "Unsupported",
            Self::Unavailable => "Unavailable",
            Self::Denied => "Denied",
            Self::Failed => "Failed",
            Self::Unknown => "Unknown",
            Self::InvalidInput => "InvalidInput",
            Self::NotFound => "NotFound",
        }
    }
}

/// A machine call's failure. `vm_exists` is true once the provider's VM was
/// made: the caller keeps it and records the machine as `failed`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineError {
    pub code: Code,
    pub message: String,
    pub missing: Vec<String>,
    pub vm_exists: bool,
}

impl MachineError {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            missing: Vec::new(),
            vm_exists: false,
        }
    }

    /// `Unsupported`, naming what the host lacks (`tool:<cli>`, `host:<fact>`).
    pub fn missing(message: impl Into<String>, missing: Vec<String>) -> Self {
        Self {
            missing,
            ..Self::new(Code::Unsupported, message)
        }
    }

    /// The same failure, raised after the provider's VM exists.
    pub fn kept(mut self) -> Self {
        self.vm_exists = true;
        self
    }
}

impl fmt::Display for MachineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.message)?;
        if !self.missing.is_empty() {
            write!(f, " (missing: {})", self.missing.join(", "))?;
        }
        Ok(())
    }
}

impl std::error::Error for MachineError {}

/// Marks every error of `result` as raised after the VM exists.
pub(crate) fn kept<T>(result: Result<T, MachineError>) -> Result<T, MachineError> {
    result.map_err(MachineError::kept)
}
