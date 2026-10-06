//! Untrusted token decoding and credited request ownership transfer.
use super::super::service::Request;
use crate::{
    client::{Operation, RequestHeader, SaveToken, REQUEST_HEADER_BYTES},
    runtime::{RuntimeError, SaveId},
};
use layerfs_bridge::{
    codec::{Message, MessageLease},
    contract::{FrameError, FrameResult},
};
use layerfs_content::ObjectId;
/// Original decoded input with transport credit until downstream admission.
pub struct WireRequest {
    /// Existing typed runtime request, never invoked by decoding.
    pub request: Request,
    /// Original body allocation lease, released after admitted ownership transfer.
    pub lease: MessageLease,
    /// Exact payload bytes moved while removing the fixed Accept header.
    pub copied_bytes: u64,
}
impl SaveId {
    /// Exact opaque wire identity returned to the authenticated client. Receiving
    /// these bytes confers no authority without host registry/binding validation.
    pub fn token(self) -> SaveToken {
        let mut bytes = [0; 48];
        bytes[..32].copy_from_slice(&self.owner);
        bytes[32..40].copy_from_slice(&(self.slot as u64).to_be_bytes());
        bytes[40..].copy_from_slice(&self.serial.to_be_bytes());
        SaveToken::from_bytes(bytes)
    }
    pub(crate) fn from_token(token: SaveToken) -> Result<Self, RuntimeError> {
        let bytes = token.bytes();
        Ok(Self {
            owner: bytes[..32].try_into().expect("fixed owner"),
            slot: usize::try_from(u64::from_be_bytes(
                bytes[32..40].try_into().expect("fixed slot"),
            ))
            .map_err(|_| RuntimeError::StaleCapability)?,
            serial: u64::from_be_bytes(bytes[40..].try_into().expect("fixed serial")),
        })
    }
}
/// Decodes one original complete credited message. On any failure the original
/// body and lease are returned intact. Header authority is checked separately
/// before allocation and again by the existing service/runtime before invocation.
pub fn decode_request(message: Message) -> Result<WireRequest, (FrameError, Message)> {
    let prepared = (|| {
        if !message.complete() || message.bytes().len() < REQUEST_HEADER_BYTES {
            return Err(FrameError::Invalid("incomplete runtime input"));
        }
        let header = RequestHeader::decode(&message.bytes()[..REQUEST_HEADER_BYTES])?;
        let envelope = message.envelope();
        if envelope.kind != layerfs_bridge::contract::MessageKind::Request
            || envelope.class != header.operation.class()
            || envelope.total_bytes != header.total_bytes()?
        {
            return Err(FrameError::Invalid("runtime envelope/header mismatch"));
        }
        let save = header
            .save
            .map(SaveId::from_token)
            .transpose()
            .map_err(|_| FrameError::Invalid("Save platform identity"))?;
        let mut ids = Vec::new();
        if matches!(header.operation, Operation::Objects | Operation::Lengths) {
            let count = header.value as usize;
            ids.try_reserve_exact(count)
                .map_err(|error| FrameError::Allocation {
                    requested_bytes: count * std::mem::size_of::<ObjectId>(),
                    error,
                })?;
            for bytes in message.bytes()[REQUEST_HEADER_BYTES..].chunks_exact(32) {
                ids.push(
                    ObjectId::from_bytes(bytes)
                        .map_err(|_| FrameError::Invalid("demand identity"))?,
                );
            }
        }
        Ok((header, save, ids))
    })();
    let (header, save, ids) = match prepared {
        Ok(v) => v,
        Err(e) => return Err((e, message)),
    };
    let (_, mut body, lease) = message.into_parts();
    let mut copied_bytes = 0;
    let required = || save.expect("checked required Save");
    let request = match header.operation {
        Operation::Binding => Request::Binding,
        Operation::Policy => Request::Policy,
        Operation::ReserveInodes => Request::ReserveInodes {
            count: header.value,
        },
        Operation::Begin => Request::Begin,
        Operation::Accept => {
            let length = body.len() - REQUEST_HEADER_BYTES;
            body.copy_within(REQUEST_HEADER_BYTES.., 0);
            body.truncate(length);
            copied_bytes = length as u64;
            Request::Accept {
                save: required(),
                id: header.object.expect("checked object"),
                role: header.role.expect("checked role"),
                canonical: body,
            }
        }
        Operation::Objects => Request::Objects { save, ids },
        Operation::Lengths => Request::Lengths { ids },
        Operation::Finish => Request::Finish { save: required() },
        Operation::Abort => Request::Abort { save: required() },
        Operation::Completion => Request::Completion { save: required() },
        Operation::Release => Request::Release { save: required() },
        Operation::Stage => Request::Stage {
            save: required(),
            root: header.object.expect("checked root"),
            generation: header.generation,
        },
        Operation::Commit => Request::Commit { save: required() },
        Operation::Discard => Request::Discard { save: required() },
        Operation::History => Request::History { save: required() },
    };
    Ok(WireRequest {
        request,
        lease,
        copied_bytes,
    })
}
/// Checks untrusted first-fragment facts against their authenticated envelope.
/// No body allocation, Save attempt, provider read or namespace scan occurs here.
pub fn check_header(
    envelope: layerfs_bridge::contract::Envelope,
    bytes: &[u8],
) -> FrameResult<RequestHeader> {
    let header = RequestHeader::decode(bytes)?;
    if envelope.kind != layerfs_bridge::contract::MessageKind::Request
        || envelope.class != header.operation.class()
        || envelope.total_bytes != header.total_bytes()?
    {
        return Err(FrameError::Invalid("runtime envelope/header mismatch"));
    }
    Ok(header)
}
