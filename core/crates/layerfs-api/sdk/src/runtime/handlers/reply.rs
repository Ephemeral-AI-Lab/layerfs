//! Original runtime results and receipt knowledge encoded under caller admission.
use super::{failure, writer::Writer};
use crate::runtime::{
    service::{Response, Service, ServiceCompletion, ServiceOutcome},
    Completion, CompletionPhase, HistoryReceipts, RuntimeError,
};
use layerfs_bridge::contract::{FrameError, FrameResult};
use layerfs_history::{CommitRecord, CommitStagedOutcome, DiscardOutcome};
use layerfs_storage::save::WriteOutcome;
/// Bounded owned reply ready for the separate native output owner.
/// Keep its ServiceCompletion alive on the host until send success/failure is fenced.
pub struct WireReply {
    /// Exact versioned bytes; allocation capacity is observed separately.
    pub bytes: Vec<u8>,
    /// Exact canonical payload bytes copied into this delivery buffer.
    pub canonical_copied_bytes: u64,
}
enum Source<'a> {
    Response(&'a Response),
    Failure(&'a RuntimeError),
    Unattempted,
    Completion(&'a Response, Result<&'a Completion, RuntimeError>),
    History(&'a Response, Result<&'a HistoryReceipts, RuntimeError>),
}
/// Encodes an original service outcome without consuming it or repeating its
/// operation. Receipt-inspection refusal has its own field and cannot be confused
/// with a refused Finish/transition. Count then allocate once avoids growing copies.
pub fn encode_reply(
    service: &Service<'_, '_>,
    completion: &ServiceCompletion,
    limit: usize,
) -> FrameResult<WireReply> {
    let source = match completion.outcome() {
        ServiceOutcome::Unattempted(_) => Source::Unattempted,
        ServiceOutcome::Dispatched(Err(error)) => Source::Failure(error),
        ServiceOutcome::Dispatched(Ok(response @ Response::Completion(_))) => {
            Source::Completion(response, service.save_completion(completion))
        }
        ServiceOutcome::Dispatched(Ok(response @ Response::History(_))) => {
            Source::History(response, service.history_receipts(completion))
        }
        ServiceOutcome::Dispatched(Ok(response)) => Source::Response(response),
    };
    let mut count = Writer::count(limit);
    encode(&mut count, &source)?;
    let mut writer = Writer::sized(count.len(), limit)?;
    let copied = encode(&mut writer, &source)?;
    if writer.len() != count.len() {
        return Err(FrameError::Invalid("reply size changed"));
    }
    Ok(WireReply {
        bytes: writer.bytes,
        canonical_copied_bytes: copied,
    })
}
/// Exact pre-dispatch refusal, separate from a dispatched adapter result.
/// The refused input and all capabilities remain with their original owners.
pub fn encode_refusal(error: &RuntimeError, limit: usize) -> FrameResult<WireReply> {
    let write = |writer: &mut Writer| {
        writer.raw(b"LRP1")?;
        writer.raw(&[4, 0, 0, 0])?;
        failure::encode(writer, error)
    };
    let mut count = Writer::count(limit);
    write(&mut count)?;
    let mut writer = Writer::sized(count.len(), limit)?;
    write(&mut writer)?;
    Ok(WireReply {
        bytes: writer.bytes,
        canonical_copied_bytes: 0,
    })
}
/// Header admission acknowledgement. Body receive may still fail; this grants
/// only input capacity/authority, never adapter invocation or publication.
pub fn encode_grant() -> WireReply {
    WireReply {
        bytes: b"LRG1\x01\x00\x00\x00".to_vec(),
        canonical_copied_bytes: 0,
    }
}
fn encode(w: &mut Writer, source: &Source<'_>) -> FrameResult<u64> {
    w.raw(b"LRP1")?;
    match source {
        Source::Failure(error) => {
            w.raw(&[1, 0, 0, 0])?;
            failure::encode(w, error)?;
            return Ok(0);
        }
        Source::Unattempted => {
            w.raw(&[3, 0, 0, 0])?;
            return Ok(0);
        }
        _ => (),
    }
    let response = match source {
        Source::Response(r) | Source::Completion(r, _) | Source::History(r, _) => r,
        _ => unreachable!(),
    };
    let code = match response {
        Response::Policy(_) => 1,
        Response::Serials { .. } => 2,
        Response::Begun(_) => 3,
        Response::Accepted(_) => 4,
        Response::Objects(_) => 5,
        Response::Lengths(_) => 6,
        Response::Completion(_) => 7,
        Response::History(_) => 8,
        Response::Released => 9,
        Response::Binding(_) => 10,
    };
    w.raw(&[0, code, 0, 0])?;
    let mut copied = 0u64;
    match response {
        Response::Binding(binding) => {
            w.raw(&binding.owner)?;
            w.raw(&binding.peer)?;
            w.raw(&binding.workspace.to_bytes())?;
            w.raw(binding.catalog.as_slice())?;
            w.u64(binding.incarnation)?;
            w.u64(binding.root_serial)?;
            let snapshot = &binding.snapshot;
            w.raw(&snapshot.branch.id.to_bytes())?;
            w.raw(&snapshot.branch.stack.to_bytes())?;
            w.blob(snapshot.branch.name.as_str().as_bytes())?;
            w.raw(&snapshot.branch.base_layer.to_bytes())?;
            w.byte(u8::from(snapshot.branch.head_commit.is_some()))?;
            if let Some(id) = snapshot.branch.head_commit {
                w.raw(&id.to_bytes())?;
            }
            w.byte(u8::from(snapshot.head_root.is_some()))?;
            if let Some(root) = snapshot.head_root {
                w.raw(root.as_bytes())?;
            }
            for root in [
                snapshot.base_root,
                snapshot.effective_root,
                snapshot.scope,
                snapshot.profile,
            ] {
                w.raw(root.as_bytes())?;
            }
        }
        Response::Policy(policy) => {
            w.byte(policy.format_profile())?;
            w.u64(policy.small_file_threshold_bytes())?;
            w.raw(&[
                policy.whole_file_delta_max_depth(),
                policy.chunk_delta_max_depth(),
                policy.metadata_delta_max_depth(),
            ])?;
        }
        Response::Serials { start, count } => {
            w.u64(*start)?;
            w.u64(*count)?;
        }
        Response::Begun(save) => w.raw(&save.token().bytes())?,
        Response::Accepted(id) => w.raw(id.as_bytes())?,
        Response::Objects(objects) => {
            w.u32(objects.len())?;
            for object in objects {
                w.raw(object.id.as_bytes())?;
                w.blob(&object.canonical)?;
                copied = copied.saturating_add(object.canonical.len() as u64);
            }
        }
        Response::Lengths(lengths) => {
            w.u32(lengths.len())?;
            for (id, length) in lengths {
                w.raw(id.as_bytes())?;
                w.u64(*length)?;
            }
        }
        Response::Completion(save) => {
            w.raw(&save.token().bytes())?;
            if let Source::Completion(_, receipt) = source {
                match receipt {
                    Err(error) => {
                        w.byte(1)?;
                        failure::encode(w, error)?;
                    }
                    Ok(completion) => {
                        w.byte(0)?;
                        w.byte(match completion.phase() {
                            CompletionPhase::Accept => 1,
                            CompletionPhase::Finish => 2,
                            CompletionPhase::Abort => 3,
                        })?;
                        match completion.outcome() {
                            Ok(outcome) => {
                                w.byte(0)?;
                                write_outcome(w, outcome)?;
                            }
                            Err(error) => {
                                w.byte(1)?;
                                failure::encode(w, error)?;
                            }
                        }
                    }
                }
            }
        }
        Response::History(save) => {
            w.raw(&save.token().bytes())?;
            if let Source::History(_, receipt) = source {
                match receipt {
                    Err(error) => {
                        w.byte(1)?;
                        failure::encode(w, error)?;
                    }
                    Ok(receipts) => {
                        w.byte(0)?;
                        result(w, receipts.stage(), failure::stage_record)?;
                        result(w, receipts.commit(), commit_outcome)?;
                        result(w, receipts.discard(), |w, outcome| {
                            w.byte(match outcome {
                                DiscardOutcome::Removed => 1,
                                DiscardOutcome::Absent => 2,
                            })
                        })?;
                    }
                }
            }
        }
        Response::Released => (),
    }
    Ok(copied)
}
fn result<T>(
    w: &mut Writer,
    result: Option<&Result<T, RuntimeError>>,
    write: impl FnOnce(&mut Writer, &T) -> FrameResult<()>,
) -> FrameResult<()> {
    match result {
        None => w.byte(0),
        Some(Ok(value)) => {
            w.byte(1)?;
            write(w, value)
        }
        Some(Err(error)) => {
            w.byte(2)?;
            failure::encode(w, error)
        }
    }
}
fn write_outcome(w: &mut Writer, outcome: &WriteOutcome) -> FrameResult<()> {
    for value in [
        outcome.inserted,
        outcome.reused,
        outcome.full_records,
        outcome.prefix_records,
        outcome.packs,
        outcome.canonical_bytes,
        outcome.pool.leaves,
        outcome.pool.reused_values,
        outcome.pool.new_values,
        outcome.pool.groups,
        outcome.pool.delta_leaves,
        outcome.pool.full_leaves,
        outcome.pool.trials,
        outcome.pool.work_exceeded,
    ] {
        w.u64(value)?;
    }
    Ok(())
}
fn commit_outcome(w: &mut Writer, outcome: &CommitStagedOutcome) -> FrameResult<()> {
    match outcome {
        CommitStagedOutcome::Committed(commit) => {
            w.byte(1)?;
            commit_record(w, commit)
        }
        CommitStagedOutcome::UpToDate { head, root } => {
            w.byte(2)?;
            w.byte(u8::from(head.is_some()))?;
            if let Some(id) = head {
                w.raw(&id.to_bytes())?;
            }
            w.raw(root.as_bytes())
        }
    }
}
fn commit_record(w: &mut Writer, commit: &CommitRecord) -> FrameResult<()> {
    w.raw(&commit.id.to_bytes())?;
    w.raw(&commit.stack.to_bytes())?;
    w.raw(commit.root.as_bytes())?;
    w.byte(u8::from(commit.parent.is_some()))?;
    if let Some(parent) = commit.parent {
        w.raw(&parent.to_bytes())?;
    }
    w.raw(&commit.base_layer.to_bytes())
}
