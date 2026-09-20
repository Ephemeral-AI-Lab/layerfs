//! Closed terminal result encoding.
use super::{Decoder, Encoder};
use crate::contract::*;
pub fn encode_response(r: &Response) -> Result<Vec<u8>, Failure> {
    let mut e = Encoder::default();
    match r {
        Response::Read { length } => {
            e.u8(1)?;
            e.u64(*length)?;
        }
        Response::Saved {
            root,
            length,
            inserted,
            reused,
        } => {
            e.u8(2)?;
            e.put(root)?;
            e.u64(*length)?;
            e.u64(*inserted)?;
            e.u64(*reused)?;
        }
        Response::FilesystemSaved {
            root,
            inserted,
            reused,
        } => {
            e.u8(7)?;
            e.put(root)?;
            e.u64(*inserted)?;
            e.u64(*reused)?;
        }
        Response::File {
            length,
            representation,
        } => {
            e.u8(3)?;
            e.u64(*length)?;
            e.u8(*representation)?;
        }
        Response::Stat {
            serial,
            kind,
            references,
            content,
            metadata,
            mode,
            mtime,
            nanoseconds,
        } => {
            e.u8(4)?;
            e.u64(*serial)?;
            e.u8(*kind)?;
            e.u64(*references)?;
            e.put(content)?;
            e.put(metadata)?;
            e.u32(*mode)?;
            e.u64(*mtime as u64)?;
            e.u32(*nanoseconds)?;
        }
        Response::List {
            entries,
            continuation,
        } => {
            if entries.len() > 128 {
                return Err(Code::Capacity.into());
            }
            e.u8(5)?;
            e.blob(continuation.as_deref().unwrap_or(&[]))?;
            e.count(entries.len())?;
            for (name, serial) in entries {
                e.blob(name)?;
                e.u64(*serial)?;
            }
        }
        Response::Link(bytes) => {
            e.u8(6)?;
            e.blob(bytes)?;
        }
        Response::History(result) => {
            e.u8(8)?;
            put_history(&mut e, result)?;
        }
    }
    Ok(e.finish())
}

fn put_optional<const N: usize>(e: &mut Encoder, value: Option<&[u8; N]>) -> Result<(), Failure> {
    match value {
        Some(value) => {
            e.u8(1)?;
            e.put(value)
        }
        None => e.u8(0),
    }
}

fn put_stack(e: &mut Encoder, record: &StackWire) -> Result<(), Failure> {
    e.put(&record.stack)?;
    e.blob(&record.name)?;
    e.put(&record.scope)?;
    e.put(&record.profile)?;
    e.put(&record.head_layer)
}

fn put_branch(e: &mut Encoder, record: &BranchWire) -> Result<(), Failure> {
    e.put(&record.branch)?;
    e.put(&record.stack)?;
    e.blob(&record.name)?;
    e.put(&record.base_layer)?;
    put_optional(e, record.head_commit.as_ref())
}

fn put_commit(e: &mut Encoder, record: &CommitWire) -> Result<(), Failure> {
    e.put(&record.commit)?;
    e.put(&record.stack)?;
    e.put(&record.root)?;
    put_optional(e, record.parent.as_ref())?;
    e.put(&record.base_layer)
}

fn put_layer(e: &mut Encoder, record: &LayerWire) -> Result<(), Failure> {
    e.put(&record.layer)?;
    e.put(&record.stack)?;
    put_optional(e, record.parent.as_ref())?;
    e.put(&record.root)?;
    put_optional(e, record.source_branch.as_ref())?;
    put_optional(e, record.source_commit.as_ref())
}

fn put_stage(e: &mut Encoder, record: &StageWire) -> Result<(), Failure> {
    e.put(&record.workspace)?;
    e.u64(record.token)?;
    e.put(&record.stack)?;
    e.put(&record.branch)?;
    put_optional(e, record.expected_head.as_ref())?;
    e.put(&record.expected_base)?;
    e.put(&record.expected_root)?;
    e.put(&record.construction_base_root)?;
    e.put(&record.intended_commit_base)?;
    e.put(&record.candidate_root)?;
    e.put(&record.profile)?;
    e.put(&record.scope)?;
    e.u64(record.generation)
}

fn put_history(e: &mut Encoder, result: &HistoryResult) -> Result<(), Failure> {
    match result {
        HistoryResult::Stack(record) => {
            e.u8(1)?;
            put_stack(e, record)?;
        }
        HistoryResult::Stacks {
            continuation,
            records,
        } => {
            e.u8(2)?;
            e.blob(continuation)?;
            e.count(records.len())?;
            for record in records {
                put_stack(e, record)?;
            }
        }
        HistoryResult::BranchSnapshot(snapshot) => {
            e.u8(3)?;
            put_branch(e, &snapshot.branch)?;
            put_optional(e, snapshot.head_root.as_ref())?;
            e.put(&snapshot.base_root)?;
            e.put(&snapshot.effective_root)?;
            e.put(&snapshot.scope)?;
            e.put(&snapshot.profile)?;
        }
        HistoryResult::Branches {
            continuation,
            records,
        } => {
            e.u8(4)?;
            e.blob(continuation)?;
            e.count(records.len())?;
            for record in records {
                put_branch(e, record)?;
            }
        }
        HistoryResult::Commit(record) => {
            e.u8(6)?;
            put_commit(e, record)?;
        }
        HistoryResult::Commits {
            continuation,
            records,
        } => {
            e.u8(7)?;
            e.blob(continuation)?;
            e.count(records.len())?;
            for record in records {
                put_commit(e, record)?;
            }
        }
        HistoryResult::Layer(record) => {
            e.u8(8)?;
            put_layer(e, record)?;
        }
        HistoryResult::Layers {
            continuation,
            records,
        } => {
            e.u8(9)?;
            e.blob(continuation)?;
            e.count(records.len())?;
            for record in records {
                put_layer(e, record)?;
            }
        }
        HistoryResult::Stage(record) => {
            e.u8(10)?;
            put_stage(e, record)?;
        }
        HistoryResult::Stages {
            continuation,
            records,
        } => {
            e.u8(11)?;
            e.blob(continuation)?;
            e.count(records.len())?;
            for record in records {
                put_stage(e, record)?;
            }
        }
        HistoryResult::StackCreated(record) => {
            e.u8(12)?;
            put_stack(e, record)?;
        }
        HistoryResult::Committed(outcome) => {
            e.u8(13)?;
            match outcome {
                CommitOutcomeWire::Committed(record) => {
                    e.u8(0)?;
                    put_commit(e, record)?;
                }
                CommitOutcomeWire::UpToDate { head, root } => {
                    e.u8(1)?;
                    put_optional(e, head.as_ref())?;
                    e.put(root)?;
                }
            }
        }
        HistoryResult::Published(outcome) => {
            e.u8(14)?;
            match outcome {
                LayerOutcomeWire::Added(record) => {
                    e.u8(0)?;
                    put_layer(e, record)?;
                }
                LayerOutcomeWire::UpToDate { layer } => {
                    e.u8(1)?;
                    e.put(layer)?;
                }
                LayerOutcomeWire::NoChanges { head } => {
                    e.u8(2)?;
                    e.put(head)?;
                }
            }
        }
        HistoryResult::Discarded { removed } => {
            e.u8(15)?;
            e.u8(u8::from(*removed))?;
        }
        HistoryResult::Reservation {
            scope,
            start,
            count,
        } => {
            e.u8(16)?;
            e.put(scope)?;
            e.u64(*start)?;
            e.u64(*count)?;
        }
    }
    Ok(())
}

fn take_optional<const N: usize>(d: &mut Decoder<'_>) -> Result<Option<[u8; N]>, Failure> {
    match d.u8()? {
        0 => Ok(None),
        1 => Ok(Some(d.take(N)?.try_into().map_err(|_| Code::InvalidInput)?)),
        _ => Err(Code::InvalidInput.into()),
    }
}

fn take_stack(d: &mut Decoder<'_>) -> Result<StackWire, Failure> {
    Ok(StackWire {
        stack: d.take(17)?.try_into().map_err(|_| Code::InvalidInput)?,
        name: d.blob(NAME_MAX_BYTES)?,
        scope: d.root()?,
        profile: d.root()?,
        head_layer: d.take(33)?.try_into().map_err(|_| Code::InvalidInput)?,
    })
}

fn take_branch(d: &mut Decoder<'_>) -> Result<BranchWire, Failure> {
    Ok(BranchWire {
        branch: d.take(17)?.try_into().map_err(|_| Code::InvalidInput)?,
        stack: d.take(17)?.try_into().map_err(|_| Code::InvalidInput)?,
        name: d.blob(NAME_MAX_BYTES)?,
        base_layer: d.take(33)?.try_into().map_err(|_| Code::InvalidInput)?,
        head_commit: take_optional::<33>(d)?,
    })
}

fn take_commit(d: &mut Decoder<'_>) -> Result<CommitWire, Failure> {
    Ok(CommitWire {
        commit: d.take(33)?.try_into().map_err(|_| Code::InvalidInput)?,
        stack: d.take(17)?.try_into().map_err(|_| Code::InvalidInput)?,
        root: d.root()?,
        parent: take_optional::<33>(d)?,
        base_layer: d.take(33)?.try_into().map_err(|_| Code::InvalidInput)?,
    })
}

fn take_layer(d: &mut Decoder<'_>) -> Result<LayerWire, Failure> {
    Ok(LayerWire {
        layer: d.take(33)?.try_into().map_err(|_| Code::InvalidInput)?,
        stack: d.take(17)?.try_into().map_err(|_| Code::InvalidInput)?,
        parent: take_optional::<33>(d)?,
        root: d.root()?,
        source_branch: take_optional::<17>(d)?,
        source_commit: take_optional::<33>(d)?,
    })
}

fn take_stage(d: &mut Decoder<'_>) -> Result<StageWire, Failure> {
    Ok(StageWire {
        workspace: d.take(32)?.try_into().map_err(|_| Code::InvalidInput)?,
        token: d.u64()?,
        stack: d.take(17)?.try_into().map_err(|_| Code::InvalidInput)?,
        branch: d.take(17)?.try_into().map_err(|_| Code::InvalidInput)?,
        expected_head: take_optional::<33>(d)?,
        expected_base: d.take(33)?.try_into().map_err(|_| Code::InvalidInput)?,
        expected_root: d.root()?,
        construction_base_root: d.root()?,
        intended_commit_base: d.take(33)?.try_into().map_err(|_| Code::InvalidInput)?,
        candidate_root: d.root()?,
        profile: d.root()?,
        scope: d.root()?,
        generation: d.u64()?,
    })
}

fn take_history(d: &mut Decoder<'_>) -> Result<HistoryResult, Failure> {
    Ok(match d.u8()? {
        1 => HistoryResult::Stack(take_stack(d)?),
        2 => {
            let continuation = d.blob(CURSOR_BYTES)?;
            let count = d.count(PAGE_RECORDS as usize, 100)?;
            let mut records = Vec::with_capacity(count);
            for _ in 0..count {
                records.push(take_stack(d)?);
            }
            HistoryResult::Stacks {
                continuation,
                records,
            }
        }
        3 => HistoryResult::BranchSnapshot(BranchSnapshotWire {
            branch: take_branch(d)?,
            head_root: take_optional::<32>(d)?,
            base_root: d.root()?,
            effective_root: d.root()?,
            scope: d.root()?,
            profile: d.root()?,
        }),
        4 => {
            let continuation = d.blob(CURSOR_BYTES)?;
            let count = d.count(PAGE_RECORDS as usize, 120)?;
            let mut records = Vec::with_capacity(count);
            for _ in 0..count {
                records.push(take_branch(d)?);
            }
            HistoryResult::Branches {
                continuation,
                records,
            }
        }
        6 => HistoryResult::Commit(take_commit(d)?),
        7 => {
            let continuation = d.blob(CURSOR_BYTES)?;
            let count = d.count(PAGE_RECORDS as usize, 100)?;
            let mut records = Vec::with_capacity(count);
            for _ in 0..count {
                records.push(take_commit(d)?);
            }
            HistoryResult::Commits {
                continuation,
                records,
            }
        }
        8 => HistoryResult::Layer(take_layer(d)?),
        9 => {
            let continuation = d.blob(CURSOR_BYTES)?;
            let count = d.count(PAGE_RECORDS as usize, 120)?;
            let mut records = Vec::with_capacity(count);
            for _ in 0..count {
                records.push(take_layer(d)?);
            }
            HistoryResult::Layers {
                continuation,
                records,
            }
        }
        10 => HistoryResult::Stage(take_stage(d)?),
        11 => {
            let continuation = d.blob(CURSOR_BYTES)?;
            let count = d.count(PAGE_RECORDS as usize, 90)?;
            let mut records = Vec::with_capacity(count);
            for _ in 0..count {
                records.push(take_stage(d)?);
            }
            HistoryResult::Stages {
                continuation,
                records,
            }
        }
        12 => HistoryResult::StackCreated(take_stack(d)?),
        13 => HistoryResult::Committed(match d.u8()? {
            0 => CommitOutcomeWire::Committed(take_commit(d)?),
            1 => {
                let head = take_optional::<33>(d)?;
                CommitOutcomeWire::UpToDate {
                    head,
                    root: d.root()?,
                }
            }
            _ => return Err(Code::Unsupported.into()),
        }),
        14 => HistoryResult::Published(match d.u8()? {
            0 => LayerOutcomeWire::Added(take_layer(d)?),
            1 => LayerOutcomeWire::UpToDate {
                layer: d.take(33)?.try_into().map_err(|_| Code::InvalidInput)?,
            },
            2 => LayerOutcomeWire::NoChanges {
                head: d.take(33)?.try_into().map_err(|_| Code::InvalidInput)?,
            },
            _ => return Err(Code::Unsupported.into()),
        }),
        15 => HistoryResult::Discarded {
            removed: match d.u8()? {
                0 => false,
                1 => true,
                _ => return Err(Code::InvalidInput.into()),
            },
        },
        16 => HistoryResult::Reservation {
            scope: d.root()?,
            start: d.u64()?,
            count: d.u64()?,
        },
        _ => return Err(Code::Unsupported.into()),
    })
}
pub fn decode_response(b: &[u8]) -> Result<Response, Failure> {
    let mut d = Decoder::new(b)?;
    let r = match d.u8()? {
        1 => Response::Read { length: d.u64()? },
        2 => Response::Saved {
            root: d.root()?,
            length: d.u64()?,
            inserted: d.u64()?,
            reused: d.u64()?,
        },
        3 => Response::File {
            length: d.u64()?,
            representation: d.u8()?,
        },
        4 => Response::Stat {
            serial: d.u64()?,
            kind: d.u8()?,
            references: d.u64()?,
            content: d.root()?,
            metadata: d.root()?,
            mode: d.u32()?,
            mtime: d.u64()? as i64,
            nanoseconds: d.u32()?,
        },
        5 => {
            let after = d.blob(255)?;
            let continuation = (!after.is_empty()).then_some(after);
            let n = d.count(128, 10)?;
            let mut entries = Vec::with_capacity(n);
            for _ in 0..n {
                entries.push((d.blob(255)?, d.u64()?));
            }
            Response::List {
                entries,
                continuation,
            }
        }
        6 => Response::Link(d.blob(4096)?),
        7 => Response::FilesystemSaved {
            root: d.root()?,
            inserted: d.u64()?,
            reused: d.u64()?,
        },
        8 => Response::History(Box::new(take_history(&mut d)?)),
        _ => return Err(Code::Unsupported.into()),
    };
    d.finish()?;
    Ok(r)
}
pub fn encode_failure(f: Failure) -> [u8; 3] {
    [
        f.code as u8,
        u8::from(f.unknown),
        f.cleanup.map_or(0, |c| c as u8),
    ]
}
pub fn decode_failure(b: &[u8]) -> Result<Failure, Failure> {
    if b.len() != 3 || b[1] > 1 {
        return Err(Code::InvalidInput.into());
    }
    if b[0] == Code::Unknown as u8 && b[1] == 0 {
        return Err(Code::InvalidInput.into());
    }
    Ok(Failure {
        code: code(b[0])?,
        unknown: b[1] != 0,
        cleanup: if b[2] == 0 { None } else { Some(code(b[2])?) },
    })
}
fn code(n: u8) -> Result<Code, Failure> {
    Ok(match n {
        1 => Code::InvalidInput,
        2 => Code::Unsupported,
        3 => Code::Denied,
        4 => Code::Capacity,
        5 => Code::Ownership,
        6 => Code::MissingObject,
        7 => Code::PathNotFound,
        8 => Code::Provider,
        9 => Code::Integrity,
        10 => Code::Io,
        11 => Code::Deadline,
        12 => Code::Unknown,
        13 => Code::Busy,
        14 => Code::NotFound,
        15 => Code::HeadMoved,
        16 => Code::StageChanged,
        17 => Code::ContinuityUnavailable,
        _ => return Err(Code::InvalidInput.into()),
    })
}
