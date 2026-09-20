//! Profile-selected history failure context; legacy failures keep three bytes.
use super::metadata::{put_optional, take_optional};
use super::response::{put_stage, take_stage};
use super::{decode_failure, encode_failure, Decoder, Encoder};
use crate::contract::*;

pub fn encode_request_failure(request: &Request, failure: &Failure) -> Result<Vec<u8>, Failure> {
    if request.profile != HISTORY_PROFILE {
        if failure.history.is_some() {
            return Err(Code::InvalidInput.into());
        }
        return Ok(encode_failure(failure.clone()).to_vec());
    }
    let mut e = Encoder::bounded(HISTORY_FAILURE_BYTES);
    e.put(&encode_failure(failure.clone()))?;
    e.u8(1)?;
    let empty = HistoryFailure::default();
    let history = failure.history.as_deref().unwrap_or(&empty);
    match &history.conflict {
        None => e.u8(0)?,
        Some(HistoryConflict::BranchMoved {
            expected_head,
            actual_head,
            expected_base,
            actual_base,
        }) => {
            e.u8(1)?;
            put_optional(&mut e, expected_head.as_ref())?;
            put_optional(&mut e, actual_head.as_ref())?;
            e.put(expected_base)?;
            e.put(actual_base)?;
        }
        Some(HistoryConflict::StackMoved { expected, actual }) => {
            e.u8(2)?;
            e.put(expected)?;
            e.put(actual)?;
        }
        Some(HistoryConflict::StageChanged { expected, actual }) => {
            e.u8(3)?;
            e.u64(*expected)?;
            put_optional(&mut e, actual.map(u64::to_be_bytes).as_ref())?;
        }
        Some(HistoryConflict::BaseMismatch {
            commit_base,
            branch_base,
        }) => {
            e.u8(4)?;
            e.put(commit_base)?;
            e.put(branch_base)?;
        }
    }
    match &history.stage {
        StageObservation::Unobserved => e.u8(0)?,
        StageObservation::Absent(workspace) => {
            e.u8(1)?;
            e.put(workspace)?;
        }
        StageObservation::Retained(stage) => {
            e.u8(2)?;
            put_stage(&mut e, stage)?;
        }
        StageObservation::AcknowledgedUnknown(stage) => {
            e.u8(3)?;
            put_stage(&mut e, stage)?;
        }
    }
    let bytes = e.finish();
    decode_request_failure(request, &bytes)?;
    Ok(bytes)
}

pub fn decode_request_failure(request: &Request, bytes: &[u8]) -> Result<Failure, Failure> {
    if request.profile != HISTORY_PROFILE {
        return decode_failure(bytes);
    }
    if !(6..=HISTORY_FAILURE_BYTES).contains(&bytes.len()) {
        return Err(Code::InvalidInput.into());
    }
    let mut d = Decoder::new(bytes)?;
    let mut failure = decode_failure(d.take(3)?)?;
    if d.u8()? != 1 {
        return Err(Code::Unsupported.into());
    }
    let conflict = match d.u8()? {
        0 => None,
        1 => Some(HistoryConflict::BranchMoved {
            expected_head: optional_id(&mut d, 0x12)?,
            actual_head: optional_id(&mut d, 0x12)?,
            expected_base: identity(&mut d, 0x32)?,
            actual_base: identity(&mut d, 0x32)?,
        }),
        2 => Some(HistoryConflict::StackMoved {
            expected: identity(&mut d, 0x32)?,
            actual: identity(&mut d, 0x32)?,
        }),
        3 => Some(HistoryConflict::StageChanged {
            expected: token(d.u64()?)?,
            actual: take_optional::<8>(&mut d)?
                .map(u64::from_be_bytes)
                .map(token)
                .transpose()?,
        }),
        4 => Some(HistoryConflict::BaseMismatch {
            commit_base: identity(&mut d, 0x32)?,
            branch_base: identity(&mut d, 0x32)?,
        }),
        _ => return Err(Code::InvalidInput.into()),
    };
    match (&conflict, failure.code) {
        (Some(HistoryConflict::StageChanged { .. }), Code::StageChanged)
        | (
            Some(
                HistoryConflict::BranchMoved { .. }
                | HistoryConflict::StackMoved { .. }
                | HistoryConflict::BaseMismatch { .. },
            ),
            Code::HeadMoved,
        ) => {}
        (None, code) if code != Code::HeadMoved && code != Code::StageChanged => {}
        _ => return Err(Code::InvalidInput.into()),
    }
    let stage = match d.u8()? {
        0 => StageObservation::Unobserved,
        1 => {
            let workspace = d.root()?;
            if workspace == [0; 32] || failure.unknown {
                return Err(Code::InvalidInput.into());
            }
            StageObservation::Absent(workspace)
        }
        2 if !failure.unknown => StageObservation::Retained(Box::new(take_stage(&mut d)?)),
        3 => StageObservation::AcknowledgedUnknown(Box::new(take_stage(&mut d)?)),
        _ => return Err(Code::InvalidInput.into()),
    };
    d.finish()?;
    if conflict.is_some() || stage != StageObservation::Unobserved {
        failure.history = Some(Box::new(HistoryFailure { conflict, stage }));
    }
    Ok(failure)
}

fn token(value: u64) -> Result<u64, Failure> {
    if value == 0 || value > i64::MAX as u64 {
        return Err(Code::InvalidInput.into());
    }
    Ok(value)
}
fn identity(d: &mut Decoder<'_>, tag: u8) -> Result<[u8; 33], Failure> {
    let value: [u8; 33] = d.take(33)?.try_into().map_err(|_| Code::InvalidInput)?;
    if value[0] != tag {
        return Err(Code::InvalidInput.into());
    }
    Ok(value)
}
fn optional_id(d: &mut Decoder<'_>, tag: u8) -> Result<Option<[u8; 33]>, Failure> {
    let value = take_optional::<33>(d)?;
    if value.is_some_and(|value| value[0] != tag) {
        return Err(Code::InvalidInput.into());
    }
    Ok(value)
}
