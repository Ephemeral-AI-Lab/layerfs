//! Exact real-file draft selection and checked native lifecycle, before Save effects.
use super::{
    construction::Construction,
    error::{content, storage},
};
use layerfs_bridge::contract::{Code, Failure, Operation, Request};
use layerfs_content::{
    file::edit::NoDraft,
    policy::{ConstructionPolicy, Representation},
    ObjectId,
};
use layerfs_storage::construction_state::ScratchSession;

/// Explicit actual final-shape selection; None never substitutes for an edit owner.
pub(crate) enum FileAuthority {
    None,
    NoDraft(NoDraft),
    Native(ScratchSession),
}
/// Select before body/Save effects from actual Store policy and declared final shape.
pub(crate) fn begin(
    construction: &Construction,
    request: &Request,
    policy: ConstructionPolicy,
) -> Result<FileAuthority, Failure> {
    let policy = policy.validated().map_err(content)?;
    if let Operation::SaveFile {
        base: Some(base),
        base_length,
        length,
        ..
    }
    | Operation::SaveFileV2 {
        base: Some(base),
        base_length,
        length,
        ..
    } = &request.operation
    {
        if policy.representation(*length) != Representation::Chunked {
            return NoDraft::new(
                policy,
                ObjectId::from_bytes(base).map_err(content)?,
                *base_length,
                *length,
            )
            .map(FileAuthority::NoDraft)
            .map_err(content);
        }
        return construction
            .begin_file(request)?
            .map(FileAuthority::Native)
            .ok_or_else(|| Code::Ownership.into());
    }
    Ok(FileAuthority::None)
}
impl FileAuthority {
    pub(crate) fn take_failure(&mut self) -> Option<layerfs_storage::StorageError> {
        match self {
            Self::Native(state) => state.take_failure(),
            _ => None,
        }
    }
}

pub(super) fn selection(request: &Request) -> Result<Option<[u8; 32]>, Failure> {
    let (base, base_length, length, extents, replacement) = match &request.operation {
        Operation::SaveFile {
            base: Some(base),
            base_length,
            length,
            extents,
            replacement,
        }
        | Operation::SaveFileV2 {
            base: Some(base),
            base_length,
            length,
            extents,
            replacement,
        } => (base, base_length, length, extents, replacement),
        _ => return Ok(None),
    };
    let mut context = [0u8; 160];
    let mut used = 0;
    let id = request.id.to_be_bytes();
    let generation = request.generation.to_be_bytes();
    let store = request.store.to_be_bytes();
    let opcode = [request.operation.opcode()];
    let base_length = base_length.to_be_bytes();
    let length = length.to_be_bytes();
    let extents = extents.to_be_bytes();
    let replacement = replacement.to_be_bytes();
    for part in [
        b"layerfs/server-file-drafts/v1\0".as_slice(),
        id.as_slice(),
        generation.as_slice(),
        store.as_slice(),
        opcode.as_slice(),
        base.as_slice(),
        base_length.as_slice(),
        length.as_slice(),
        extents.as_slice(),
        replacement.as_slice(),
    ] {
        context[used..used + part.len()].copy_from_slice(part);
        used += part.len();
    }
    Ok(Some(*ObjectId::for_bytes(&context[..used]).as_bytes()))
}

/// Each actual owner decides its own custody; Unknown metadata is retained on Drop.
pub(crate) fn close<T>(
    state: &mut FileAuthority,
    result: Result<T, Failure>,
) -> Result<T, Failure> {
    let state = match state {
        FileAuthority::None => return result,
        FileAuthority::NoDraft(state) => {
            if result.is_err() {
                state.abandon();
                return result;
            }
            return if state.completed() {
                result
            } else {
                Err(Code::Ownership.into())
            };
        }
        FileAuthority::Native(state) => state,
    };
    if state.is_quarantined() {
        let mut error = result.err().unwrap_or_else(|| Code::Unknown.into());
        error.unknown = true;
        return Err(error);
    }
    let disposition = if result.is_ok() {
        state.return_to_idle()
    } else {
        state.release()
    };
    match (result, disposition.map_err(storage)) {
        (Ok(value), Ok(())) => Ok(value),
        (Ok(_), Err(cleanup)) => Err(cleanup),
        (Err(error), Ok(())) => Err(error),
        (Err(mut error), Err(cleanup)) => {
            error.cleanup.get_or_insert(cleanup.code);
            error.unknown |= cleanup.unknown;
            Err(error)
        }
    }
}
