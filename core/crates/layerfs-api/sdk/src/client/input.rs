//! Borrowed original inputs streamed through bounded native fragments.
use super::{Operation, RequestHeader, SaveToken};
use layerfs_bridge::contract::{FrameError, FrameResult};
use layerfs_content::{ObjectId, ObjectRole};
/// Caller-owned data for one bounded adapter message.
#[derive(Clone, Copy, Debug)]
pub enum RequestBody<'a> {
    /// No body follows the fixed admission header.
    Empty,
    /// Original canonical input; no whole-object transport copy.
    Canonical(&'a [u8]),
    /// Original ordered demand IDs, encoded through a fixed record scratch.
    Ids(&'a [ObjectId]),
}
/// Original request facts plus borrowed bounded input.
#[derive(Clone, Copy, Debug)]
pub struct ClientRequest<'a> {
    /// Untrusted admission header.
    pub header: RequestHeader,
    /// Body whose ownership remains with the caller through refusal/uncertainty.
    pub body: RequestBody<'a>,
}
impl<'a> ClientRequest<'a> {
    /// Header-only operation. Use typed helpers for object-bearing operations.
    pub fn control(operation: Operation, save: Option<SaveToken>, value: u64) -> FrameResult<Self> {
        let value = Self {
            header: RequestHeader {
                operation,
                save,
                object: None,
                role: None,
                value,
                generation: 0,
            },
            body: RequestBody::Empty,
        };
        value.validate()?;
        Ok(value)
    }
    /// Original canonical object, admitted as one existing library object window.
    pub fn accept(save: SaveToken, id: ObjectId, role: ObjectRole, canonical: &'a [u8]) -> Self {
        Self {
            header: RequestHeader {
                operation: Operation::Accept,
                save: Some(save),
                object: Some(id),
                role: Some(role),
                value: canonical.len() as u64,
                generation: 0,
            },
            body: RequestBody::Canonical(canonical),
        }
    }
    /// Ordered canonical demand, with optional same-Save visibility.
    pub fn objects(save: Option<SaveToken>, ids: &'a [ObjectId]) -> Self {
        Self {
            header: RequestHeader {
                operation: Operation::Objects,
                save,
                object: None,
                role: None,
                value: ids.len() as u64,
                generation: 0,
            },
            body: RequestBody::Ids(ids),
        }
    }
    /// Ordered saved-file length demand.
    pub fn lengths(ids: &'a [ObjectId]) -> Self {
        Self {
            header: RequestHeader {
                operation: Operation::Lengths,
                save: None,
                object: None,
                role: None,
                value: ids.len() as u64,
                generation: 0,
            },
            body: RequestBody::Ids(ids),
        }
    }
    /// Stage uses exact expectations from the host binding, never client fields.
    pub fn stage(save: SaveToken, root: ObjectId, generation: u64) -> Self {
        Self {
            header: RequestHeader {
                operation: Operation::Stage,
                save: Some(save),
                object: Some(root),
                role: None,
                value: 0,
                generation,
            },
            body: RequestBody::Empty,
        }
    }
    /// Checks exact body type/length before any socket write.
    pub fn validate(&self) -> FrameResult<()> {
        let expected = self.header.payload_bytes()?;
        let actual = match (self.header.operation, self.body) {
            (Operation::Accept, RequestBody::Canonical(bytes)) => bytes.len(),
            (Operation::Objects | Operation::Lengths, RequestBody::Ids(ids)) => ids
                .len()
                .checked_mul(32)
                .ok_or(FrameError::Invalid("request ID bytes"))?,
            (_, RequestBody::Empty) if expected == 0 => 0,
            _ => return Err(FrameError::Invalid("request body type")),
        };
        if actual != expected {
            return Err(FrameError::Invalid("request body length"));
        }
        Ok(())
    }
}
