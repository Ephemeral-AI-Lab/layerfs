//! Borrowed replies distinguish adapter outcomes, receipt knowledge and admission.
use super::{reader::Reader, records, RemoteFailure, SaveToken};
use crate::CompletionPhase;
use layerfs_bridge::contract::{FrameError, FrameResult};
use layerfs_content::ObjectId;
use layerfs_history::{CommitStagedOutcome, DiscardOutcome, StageRecord};
use layerfs_storage::{
    policy::{CANONICAL_LIMIT, READ_CANONICAL_BYTES_LIMIT, READ_OBJECT_LIMIT},
    save::WriteOutcome,
    StoragePolicy,
};
/// Original invocation scope of a remote failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureOrigin {
    /// Existing adapter was invoked once; error/publication custody remains exact.
    Dispatched,
    /// Input/admission refused before any adapter invocation.
    Admission,
}
/// Original Save receipt, separate from receipt-inspection authority failure.
#[derive(Debug)]
pub struct SaveCompletionView<'a> {
    /// Operation phase which produced the retained receipt.
    pub phase: CompletionPhase,
    /// Exact original counters or typed failure.
    pub outcome: Result<WriteOutcome, RemoteFailure<'a>>,
}
/// Original history attempts; None means unattempted, never catalog absence.
#[derive(Debug)]
pub struct HistoryView<'a> {
    /// Original stage admission/provider result.
    pub stage: Option<Result<StageRecord, RemoteFailure<'a>>>,
    /// Original exact-token transition result.
    pub commit: Option<Result<CommitStagedOutcome, RemoteFailure<'a>>>,
    /// Original explicit discard result.
    pub discard: Option<Result<DiscardOutcome, RemoteFailure<'a>>>,
}
/// Checked versioned result borrowing the original credited receive allocation.
#[derive(Debug)]
pub enum ReplyView<'a> {
    /// Header admission only; neither invocation nor publication success.
    Granted,
    /// Typed failure and exact scope of the original attempted operation.
    Failure {
        /// Original invocation scope.
        origin: FailureOrigin,
        /// Exact typed underlying failure tree.
        error: RemoteFailure<'a>,
    },
    /// Queued original request cancelled before invocation by an attachment fence.
    Unattempted,
    /// Exact persisted policy; malformed/unsupported policy never defaults.
    Policy(StoragePolicy),
    /// Original half-open allocated range.
    Serials {
        /// First serial.
        start: u64,
        /// Count.
        count: u64,
    },
    /// Fresh runtime Save identity.
    Begun(SaveToken),
    /// Exact acknowledged object identity.
    Accepted(ObjectId),
    /// Authenticated canonical values in original demand order.
    Objects(ObjectValues<'a>),
    /// Trusted saved length facts in demand order.
    Lengths(LengthValues<'a>),
    /// Existing Save response, with original completion or distinct inspection refusal.
    Completion {
        /// Original runtime Save.
        save: SaveToken,
        /// Receipt access does not change the attempted operation.
        receipt: Result<SaveCompletionView<'a>, RemoteFailure<'a>>,
    },
    /// Existing history response, with exact receipts or distinct inspection refusal.
    History {
        /// Original runtime Save.
        save: SaveToken,
        /// Exact retained knowledge.
        receipt: Result<HistoryView<'a>, RemoteFailure<'a>>,
    },
    /// Explicit known terminal acknowledgement released its registry slot.
    Released,
    /// Original host context and Branch expectations, without a Branch refresh.
    Binding(records::RemoteBinding),
}
impl<'a> ReplyView<'a> {
    /// Validates widths/windows, original canonical hashes, reserved fields and
    /// complete record consumption. No object/error vector mirror is allocated.
    pub fn decode(bytes: &'a [u8]) -> FrameResult<Self> {
        let mut r = Reader::new(bytes);
        let magic = r.take(4)?;
        let status = r.byte()?;
        let kind = r.byte()?;
        if r.take(2)? != [0, 0] {
            return Err(FrameError::Invalid("reply reserved bytes"));
        }
        if magic == b"LRG1" {
            if status != 1 || kind != 0 {
                return Err(FrameError::Invalid("grant grammar"));
            }
            r.end()?;
            return Ok(Self::Granted);
        }
        if magic != b"LRP1" {
            return Err(FrameError::Invalid("reply version"));
        }
        let value = match status {
            1 | 4 if kind == 0 => Self::Failure {
                origin: if status == 1 {
                    FailureOrigin::Dispatched
                } else {
                    FailureOrigin::Admission
                },
                error: r.failure()?,
            },
            3 if kind == 0 => Self::Unattempted,
            0 => match kind {
                1 => {
                    let format = r.byte()?;
                    let cutoff = r.u64()?;
                    let whole = r.byte()?;
                    let chunk = r.byte()?;
                    let metadata = r.byte()?;
                    Self::Policy(
                        StoragePolicy::new(format, cutoff, whole, chunk)
                            .with_metadata_depth(metadata)
                            .validated()
                            .map_err(|_| FrameError::Invalid("reply persisted policy"))?,
                    )
                }
                2 => {
                    let start = r.u64()?;
                    let count = r.u64()?;
                    if start == 0
                        || count == 0
                        || start
                            .checked_add(count)
                            .is_none_or(|end| end > i64::MAX as u64)
                    {
                        return Err(FrameError::Invalid("reply serial range"));
                    }
                    Self::Serials { start, count }
                }
                3 => Self::Begun(token(&mut r)?),
                4 => Self::Accepted(r.object()?),
                5 => {
                    let count = r.u32()?;
                    if count > READ_OBJECT_LIMIT {
                        return Err(FrameError::Invalid("reply demand count"));
                    }
                    let body = r.bytes;
                    let mut bytes = 0usize;
                    for _ in 0..count {
                        let id = r.object()?;
                        let canonical = r.blob()?;
                        bytes = bytes
                            .checked_add(canonical.len())
                            .ok_or(FrameError::Invalid("reply canonical size"))?;
                        if canonical.len() > CANONICAL_LIMIT || bytes > READ_CANONICAL_BYTES_LIMIT {
                            return Err(FrameError::Invalid("reply canonical window"));
                        }
                        if ObjectId::for_bytes(canonical) != id {
                            return Err(FrameError::IdentityMismatch);
                        }
                    }
                    Self::Objects(ObjectValues { bytes: body, count })
                }
                6 => {
                    let count = r.u32()?;
                    if count > READ_OBJECT_LIMIT {
                        return Err(FrameError::Invalid("length count"));
                    }
                    let body = r.take(
                        count
                            .checked_mul(40)
                            .ok_or(FrameError::Invalid("length bytes"))?,
                    )?;
                    Self::Lengths(LengthValues { bytes: body, count })
                }
                7 => {
                    let save = token(&mut r)?;
                    let receipt = if r.flag()? {
                        Err(r.failure()?)
                    } else {
                        let phase = match r.byte()? {
                            1 => CompletionPhase::Accept,
                            2 => CompletionPhase::Finish,
                            3 => CompletionPhase::Abort,
                            _ => return Err(FrameError::Invalid("Save receipt phase")),
                        };
                        Ok(SaveCompletionView {
                            phase,
                            outcome: if r.flag()? {
                                Err(r.failure()?)
                            } else {
                                Ok(records::write(&mut r)?)
                            },
                        })
                    };
                    Self::Completion { save, receipt }
                }
                8 => {
                    let save = token(&mut r)?;
                    let receipt = if r.flag()? {
                        Err(r.failure()?)
                    } else {
                        Ok(HistoryView {
                            stage: result(&mut r, records::stage)?,
                            commit: result(&mut r, records::commit)?,
                            discard: result(&mut r, |r| match r.byte()? {
                                1 => Ok(DiscardOutcome::Removed),
                                2 => Ok(DiscardOutcome::Absent),
                                _ => Err(FrameError::Invalid("discard outcome")),
                            })?,
                        })
                    };
                    Self::History { save, receipt }
                }
                9 => Self::Released,
                10 => Self::Binding(records::binding(&mut r)?),
                _ => return Err(FrameError::Invalid("reply result kind")),
            },
            _ => return Err(FrameError::Invalid("reply status")),
        };
        r.end()?;
        Ok(value)
    }
}
fn token(r: &mut Reader<'_>) -> FrameResult<SaveToken> {
    Ok(SaveToken::from_bytes(
        r.take(48)?.try_into().expect("fixed Save token"),
    ))
}
fn result<'a, T>(
    r: &mut Reader<'a>,
    read: impl FnOnce(&mut Reader<'a>) -> FrameResult<T>,
) -> FrameResult<Option<Result<T, RemoteFailure<'a>>>> {
    Ok(match r.byte()? {
        0 => None,
        1 => Some(Ok(read(r)?)),
        2 => Some(Err(r.failure()?)),
        _ => return Err(FrameError::Invalid("receipt attempt tag")),
    })
}
/// Forward borrowed object delivery; grammar/hashes were checked by ReplyView.
#[derive(Clone, Debug)]
pub struct ObjectValues<'a> {
    bytes: &'a [u8],
    count: usize,
}
impl<'a> Iterator for ObjectValues<'a> {
    type Item = (ObjectId, &'a [u8]);
    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            return None;
        }
        let mut r = Reader::new(self.bytes);
        let id = r.object().expect("validated ID");
        let bytes = r.blob().expect("validated object");
        self.bytes = r.bytes;
        self.count -= 1;
        Some((id, bytes))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.count, Some(self.count))
    }
}
impl ExactSizeIterator for ObjectValues<'_> {}
/// Forward borrowed trusted length facts; the whole fixed-width window is checked.
#[derive(Clone, Debug)]
pub struct LengthValues<'a> {
    bytes: &'a [u8],
    count: usize,
}
impl Iterator for LengthValues<'_> {
    type Item = (ObjectId, u64);
    fn next(&mut self) -> Option<Self::Item> {
        if self.count == 0 {
            return None;
        }
        let mut r = Reader::new(self.bytes);
        let id = r.object().expect("validated length ID");
        let length = r.u64().expect("validated length");
        self.bytes = r.bytes;
        self.count -= 1;
        Some((id, length))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.count, Some(self.count))
    }
}
impl ExactSizeIterator for LengthValues<'_> {}
