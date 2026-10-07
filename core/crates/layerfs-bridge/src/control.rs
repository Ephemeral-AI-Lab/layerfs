//! Bounded authenticated install control records, with no object/Save RPCs.
use crate::{
    provision::StoreManifest,
    wire::{Reader, Writer},
};
use std::fmt;
/// Final sender marker, emitted only after exact sealed length and EOF checks.
pub const INSTALL_FINISH: &[u8] = b"LFSF\x01";
const MAGIC: &[u8] = b"LFSR\x01";
/// Invalid bounded control input; the native channel owns authentication errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ControlError(pub &'static str);
impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ControlError {}
/// Original boundary of an installation attempt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum InstallPhase {
    /// Read/check the authenticated manifest.
    Request = 1,
    /// Exclusively create the deterministic temporary file.
    Claim = 2,
    /// Stream the declared file and final marker.
    Transfer = 3,
    /// Publish the temporary file with one rename.
    Publish = 4,
    /// Open and check the initial Store binding.
    Open = 5,
    /// Deliver the original installation acknowledgement.
    Reply = 6,
}
/// Refusal knowledge, never a retry instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RefusalKind {
    /// Invalid before-effect input or protocol.
    Invalid = 1,
    /// Destination or installation temporary already exists.
    Exists = 2,
    /// Typed before-effect provider contention.
    Busy = 3,
    /// Definite failure with retained partial/output custody.
    Failed = 4,
    /// Original outcome is uncertain; never infer success from a later read.
    Unknown = 5,
}
/// Original remote failure knowledge. The daemon also retains the typed cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallRefusal {
    /// Original phase.
    pub phase: InstallPhase,
    /// Typed knowledge category.
    pub kind: RefusalKind,
    /// True only after the original create_new returned its file handle.
    pub claim_acknowledged: bool,
    /// Bytes acknowledged by the daemon's original file writes.
    pub written: u64,
    /// True only after the original rename returned success.
    pub published: bool,
    /// Original operating-system error code when one exists.
    pub os_code: Option<i32>,
    /// Bounded original cause description; not parsed to decide knowledge.
    pub detail: String,
}
/// One reply to the original install request/stream, without replay tokens.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InstallReply {
    /// Temporary ownership was acquired, so the sender may stream.
    Ready,
    /// Installed and opened with the actual daemon SQLite version.
    Installed(StoreManifest),
    /// Original refusal; retained files are not removed automatically.
    Refused(InstallRefusal),
}
impl InstallReply {
    /// Encodes one bounded native control record.
    pub fn encode(&self) -> Result<Vec<u8>, ControlError> {
        let mut out = Writer::new(MAGIC);
        match self {
            Self::Ready => out.byte(1)?,
            Self::Installed(manifest) => {
                out.byte(2)?;
                out.blob(&manifest.encode()?)?;
            }
            Self::Refused(failure) => {
                if failure.detail.len() > 2048 {
                    return Err(ControlError("failure detail limit"));
                }
                out.put(&[
                    3,
                    failure.phase as u8,
                    failure.kind as u8,
                    u8::from(failure.published),
                    u8::from(failure.claim_acknowledged),
                ])?;
                out.put(&failure.written.to_be_bytes())?;
                out.byte(u8::from(failure.os_code.is_some()))?;
                if let Some(code) = failure.os_code {
                    out.put(&code.to_be_bytes())?;
                }
                out.blob(failure.detail.as_bytes())?;
            }
        }
        Ok(out.0)
    }
    /// Decodes exactly one reply. Unknown tags/booleans are refused.
    pub fn decode(bytes: &[u8]) -> Result<Self, ControlError> {
        let mut input = Reader::new(bytes, MAGIC)?;
        let result = match input.byte()? {
            1 => Self::Ready,
            2 => Self::Installed(StoreManifest::decode(input.blob(crate::wire::LIMIT)?)?),
            3 => {
                let phase = match input.byte()? {
                    1 => InstallPhase::Request,
                    2 => InstallPhase::Claim,
                    3 => InstallPhase::Transfer,
                    4 => InstallPhase::Publish,
                    5 => InstallPhase::Open,
                    6 => InstallPhase::Reply,
                    _ => return Err(ControlError("install phase")),
                };
                let kind = match input.byte()? {
                    1 => RefusalKind::Invalid,
                    2 => RefusalKind::Exists,
                    3 => RefusalKind::Busy,
                    4 => RefusalKind::Failed,
                    5 => RefusalKind::Unknown,
                    _ => return Err(ControlError("refusal kind")),
                };
                let published = boolean(input.byte()?)?;
                let claim_acknowledged = boolean(input.byte()?)?;
                let written = u64::from_be_bytes(input.array()?);
                let os_code = if boolean(input.byte()?)? {
                    Some(i32::from_be_bytes(input.array()?))
                } else {
                    None
                };
                let detail = input.text(2048)?;
                Self::Refused(InstallRefusal {
                    phase,
                    kind,
                    claim_acknowledged,
                    written,
                    published,
                    os_code,
                    detail,
                })
            }
            _ => return Err(ControlError("install reply")),
        };
        input.finish()?;
        Ok(result)
    }
}
fn boolean(value: u8) -> Result<bool, ControlError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(ControlError("control boolean")),
    }
}
