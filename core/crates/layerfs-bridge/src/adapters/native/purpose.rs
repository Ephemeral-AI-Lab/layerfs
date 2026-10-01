//! Closed authenticated HELLO purposes and their existing request surfaces.
use crate::contract::{Code, Failure, Operation, Request, METADATA_BYTES};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Purpose {
    General = 1,
    Catalog = 2,
    Control = 3,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hello {
    version: u16,
    purpose: Purpose,
}
impl Hello {
    pub const fn version(self) -> u16 {
        self.version
    }
    pub const fn purpose(self) -> Purpose {
        self.purpose
    }
    pub const fn legacy() -> Self {
        Self {
            version: 1,
            purpose: Purpose::General,
        }
    }
    pub const fn selected(purpose: Purpose) -> Self {
        Self {
            version: 2,
            purpose,
        }
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, Failure> {
        if bytes == [0, 1] {
            return Ok(Self::legacy());
        }
        if bytes.len() != 4 || bytes[..2] != [0, 2] || bytes[3] != 0 {
            return Err(Code::Unsupported.into());
        }
        let purpose = match bytes[2] {
            1 => Purpose::General,
            2 => Purpose::Catalog,
            3 => Purpose::Control,
            _ => return Err(Code::Unsupported.into()),
        };
        Ok(Self::selected(purpose))
    }
    pub fn encode(self) -> Vec<u8> {
        if self.version == 1 {
            vec![0, 1]
        } else {
            vec![0, 2, self.purpose as u8, 0]
        }
    }
}
impl Purpose {
    pub const fn frame_limit(self) -> usize {
        match self {
            Self::General | Self::Catalog => METADATA_BYTES,
            Self::Control => 8 * 1024,
        }
    }
    pub fn check(self, request: &Request) -> Result<(), Failure> {
        let allowed = match self {
            Self::General => true,
            Self::Catalog => {
                matches!(request.operation, Operation::HistoryQuery(_))
                    || request.operation.metadata_mutation()
            }
            Self::Control => matches!(
                request.operation,
                Operation::FileSaveCapabilities
                    | Operation::SandboxHello
                    | Operation::WorkspaceStatus { .. }
                    | Operation::WorkspaceViewStatus { .. }
                    | Operation::WorkspaceReleaseView { .. }
            ),
        };
        if !allowed || (self != Self::General && request.operation.input_length()? != 0) {
            return Err(Code::Denied.into());
        }
        Ok(())
    }
}
