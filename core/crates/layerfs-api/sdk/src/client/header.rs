//! Fixed untrusted admission header, checked before declared body allocation.
use super::SaveToken;
use layerfs_bridge::contract::{FrameError, FrameResult, MessageClass};
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::policy::{CANONICAL_LIMIT, READ_OBJECT_LIMIT};
/// Versioned header bytes carried alone in the first logical request fragment.
pub const REQUEST_HEADER_BYTES: usize = 104;
/// Exact host adapter operation, independent of native transport correlation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Operation {
    /// Persisted policy.
    Policy = 1,
    /// Authority-owned serial range.
    ReserveInodes = 2,
    /// Admit Save.
    Begin = 3,
    /// Admit one canonical object.
    Accept = 4,
    /// Saved or same-Save canonical demand.
    Objects = 5,
    /// Trusted saved-file length facts.
    Lengths = 6,
    /// Attempt consuming SaveFinish.
    Finish = 7,
    /// Explicit producer abort.
    Abort = 8,
    /// Inspect original Save completion.
    Completion = 9,
    /// Acknowledge known terminal custody.
    Release = 10,
    /// Stage saved root under bound expectations.
    Stage = 11,
    /// Exact conditional stage transition.
    Commit = 12,
    /// Explicit exact-stage discard.
    Discard = 13,
    /// Inspect original history receipts.
    History = 14,
    /// Captured original binding; does not refresh the Branch.
    Binding = 15,
}
impl Operation {
    /// Checked protocol code, never an error-driven default operation.
    pub fn from_code(code: u8) -> FrameResult<Self> {
        Ok(match code {
            1 => Self::Policy,
            2 => Self::ReserveInodes,
            3 => Self::Begin,
            4 => Self::Accept,
            5 => Self::Objects,
            6 => Self::Lengths,
            7 => Self::Finish,
            8 => Self::Abort,
            9 => Self::Completion,
            10 => Self::Release,
            11 => Self::Stage,
            12 => Self::Commit,
            13 => Self::Discard,
            14 => Self::History,
            15 => Self::Binding,
            _ => return Err(FrameError::Invalid("runtime operation")),
        })
    }
    /// Required transport reserve class; a peer cannot relabel an Accept as control.
    pub const fn class(self) -> MessageClass {
        match self {
            Self::Accept => MessageClass::Save,
            Self::Objects | Self::Lengths => MessageClass::Demand,
            _ => MessageClass::Control,
        }
    }
}
/// Parsed fixed facts. Public fields remain untrusted until host authorization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestHeader {
    /// Owning host adapter.
    pub operation: Operation,
    /// Original remote Save identity when selected.
    pub save: Option<SaveToken>,
    /// Claimed canonical object or candidate root.
    pub object: Option<ObjectId>,
    /// Expected canonical role for Accept only.
    pub role: Option<ObjectRole>,
    /// Canonical bytes, demand ID count or requested serial count.
    pub value: u64,
    /// Captured generation for Stage only.
    pub generation: u64,
}
impl RequestHeader {
    /// Validates operation-specific fields and returns bounded payload bytes.
    /// These are per-message library windows, never total file/Save/flow caps.
    pub fn payload_bytes(&self) -> FrameResult<usize> {
        let required_save = matches!(
            self.operation,
            Operation::Accept
                | Operation::Finish
                | Operation::Abort
                | Operation::Completion
                | Operation::Release
                | Operation::Stage
                | Operation::Commit
                | Operation::Discard
                | Operation::History
        );
        if (required_save && self.save.is_none())
            || (!required_save && self.operation != Operation::Objects && self.save.is_some())
            || self.object.is_some()
                != matches!(self.operation, Operation::Accept | Operation::Stage)
            || self.role.is_some() != (self.operation == Operation::Accept)
            || (self.operation != Operation::Stage && self.generation != 0)
        {
            return Err(FrameError::Invalid("runtime header fields"));
        }
        let value = usize::try_from(self.value)
            .map_err(|_| FrameError::Invalid("runtime platform size"))?;
        match self.operation {
            Operation::Accept if value <= CANONICAL_LIMIT => Ok(value),
            Operation::Objects | Operation::Lengths if value <= READ_OBJECT_LIMIT => value
                .checked_mul(32)
                .ok_or(FrameError::Invalid("runtime ID bytes")),
            Operation::ReserveInodes if self.value != 0 => Ok(0),
            Operation::Accept
            | Operation::Objects
            | Operation::Lengths
            | Operation::ReserveInodes => Err(FrameError::Invalid("runtime request window")),
            _ if self.value == 0 => Ok(0),
            _ => Err(FrameError::Invalid("unused runtime value")),
        }
    }
    /// Exact total header plus bounded payload, checked before receive allocation.
    pub fn total_bytes(&self) -> FrameResult<u64> {
        Ok((REQUEST_HEADER_BYTES + self.payload_bytes()?) as u64)
    }
    /// Encodes into a fixed caller window without a payload copy/allocation.
    pub fn encode(&self) -> FrameResult<[u8; REQUEST_HEADER_BYTES]> {
        self.payload_bytes()?;
        let mut bytes = [0; REQUEST_HEADER_BYTES];
        bytes[..4].copy_from_slice(b"LRT1");
        bytes[4] = self.operation as u8;
        bytes[5] = self.role.map_or(0, ObjectRole::code);
        bytes[6] = u8::from(self.save.is_some());
        if let Some(save) = self.save {
            bytes[8..56].copy_from_slice(&save.0);
        }
        if let Some(object) = self.object {
            bytes[56..88].copy_from_slice(object.as_bytes());
        }
        bytes[88..96].copy_from_slice(&self.value.to_be_bytes());
        bytes[96..104].copy_from_slice(&self.generation.to_be_bytes());
        Ok(bytes)
    }
    /// Decodes the complete first-fragment header, refusing reserved/unused bytes.
    pub fn decode(bytes: &[u8]) -> FrameResult<Self> {
        if bytes.len() != REQUEST_HEADER_BYTES
            || &bytes[..4] != b"LRT1"
            || bytes[6] > 1
            || bytes[7] != 0
        {
            return Err(FrameError::Invalid("runtime header grammar"));
        }
        let operation = Operation::from_code(bytes[4])?;
        let save = if bytes[6] == 1 {
            Some(SaveToken(
                bytes[8..56].try_into().expect("fixed Save bytes"),
            ))
        } else {
            if bytes[8..56].iter().any(|b| *b != 0) {
                return Err(FrameError::Invalid("unused Save bytes"));
            }
            None
        };
        let object = if matches!(operation, Operation::Accept | Operation::Stage) {
            Some(
                ObjectId::from_bytes(&bytes[56..88])
                    .map_err(|_| FrameError::Invalid("runtime object ID"))?,
            )
        } else {
            if bytes[56..88].iter().any(|b| *b != 0) {
                return Err(FrameError::Invalid("unused object bytes"));
            }
            None
        };
        let header = Self {
            operation,
            save,
            object,
            role: if bytes[5] == 0 {
                None
            } else {
                Some(
                    ObjectRole::from_code(bytes[5])
                        .map_err(|_| FrameError::Invalid("runtime object role"))?,
                )
            },
            value: u64::from_be_bytes(bytes[88..96].try_into().expect("fixed value")),
            generation: u64::from_be_bytes(bytes[96..104].try_into().expect("fixed generation")),
        };
        header.payload_bytes()?;
        Ok(header)
    }
}
