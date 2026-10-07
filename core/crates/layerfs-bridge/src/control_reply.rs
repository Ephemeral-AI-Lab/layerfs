//! Exact known control results and explicitly scoped status observations.
use crate::{
    control::{
        Activity, Answer, ControlCode, ControlError, ControlRefusal, LocalObservation, Reply,
        WorkspaceStatus, HISTORY_WINDOW,
    },
    control_history::*,
    wire::{Reader, Writer},
};
use layerfs_history::PageResult;
const MAGIC: &[u8] = b"LFSA\x01";
impl Answer {
    /// Encodes one bounded original result; publication is independent of delivery.
    pub fn encode(&self) -> Result<Vec<u8>, ControlError> {
        if self.id == 0 {
            return Err(ControlError("control correlation"));
        }
        let mut out = Writer::new(MAGIC);
        out.put(&self.id.to_be_bytes())?;
        match &self.reply {
            Reply::SessionEnded => out.byte(9)?,
            Reply::Hello(value) => {
                out.byte(8)?;
                crate::daemon_wire::put_status(&mut out, value)?;
            }
            Reply::Bound { token, binding } => {
                out.byte(1)?;
                put_token(&mut out, *token)?;
                put_binding(&mut out, binding)?;
            }
            Reply::Committed(value) => {
                out.byte(2)?;
                put_outcome(&mut out, value)?;
            }
            Reply::Status(value) => {
                out.byte(3)?;
                put_status(&mut out, value)?;
            }
            Reply::Unmounted(token) => {
                out.byte(4)?;
                put_token(&mut out, *token)?;
            }
            Reply::Forked(value) => {
                out.byte(5)?;
                put_binding(&mut out, value)?;
            }
            Reply::History(value) => {
                if value.records.len() > HISTORY_WINDOW as usize {
                    return Err(ControlError("history result window"));
                }
                out.byte(6)?;
                out.put(&(value.records.len() as u16).to_be_bytes())?;
                for row in &value.records {
                    put_commit(&mut out, row)?;
                }
                put_cursor(&mut out, &value.continuation)?;
            }
            Reply::Refused(value) => {
                out.byte(7)?;
                put_refusal(&mut out, value)?;
            }
        }
        Ok(out.0)
    }
    /// Decodes exactly one correlated reply without interpreting later Store state.
    pub fn decode(bytes: &[u8]) -> Result<Self, ControlError> {
        let mut input = Reader::new(bytes, MAGIC)?;
        let id = u64::from_be_bytes(input.array()?);
        if id == 0 {
            return Err(ControlError("control correlation"));
        }
        let reply = match input.byte()? {
            9 => Reply::SessionEnded,
            8 => Reply::Hello(crate::daemon_wire::status(&mut input)?),
            1 => Reply::Bound {
                token: token(&mut input)?,
                binding: binding(&mut input)?,
            },
            2 => Reply::Committed(outcome(&mut input)?),
            3 => Reply::Status(Box::new(status(&mut input)?)),
            4 => Reply::Unmounted(token(&mut input)?),
            5 => Reply::Forked(binding(&mut input)?),
            6 => {
                let count = u16::from_be_bytes(input.array()?);
                if count > HISTORY_WINDOW {
                    return Err(ControlError("history result window"));
                }
                let mut records = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    records.push(commit(&mut input)?);
                }
                Reply::History(PageResult {
                    records,
                    continuation: cursor(&mut input)?,
                })
            }
            7 => Reply::Refused(refusal(&mut input)?),
            _ => return Err(ControlError("control reply")),
        };
        input.finish()?;
        Ok(Self { id, reply })
    }
}
fn put_refusal(out: &mut Writer, value: &ControlRefusal) -> Result<(), ControlError> {
    if value.phase.len() > 64
        || value.detail.len() > 2048
        || (value.code == ControlCode::HeadMoved) != value.moved.is_some()
    {
        return Err(ControlError("control refusal fields"));
    }
    out.byte(match value.code {
        ControlCode::Busy => 1,
        ControlCode::HeadMoved => 2,
        ControlCode::Missing => 3,
        ControlCode::Capacity => 4,
        ControlCode::Invalid => 5,
        ControlCode::Failed => 6,
        ControlCode::Unknown => 7,
    })?;
    out.blob(value.phase.as_bytes())?;
    out.byte(u8::from(value.moved.is_some()))?;
    if let Some(moved) = &value.moved {
        put_moved(out, moved)?;
    }
    put_publication(out, &value.published)?;
    out.blob(value.detail.as_bytes())
}
fn refusal(input: &mut Reader<'_>) -> Result<ControlRefusal, ControlError> {
    let code = match input.byte()? {
        1 => ControlCode::Busy,
        2 => ControlCode::HeadMoved,
        3 => ControlCode::Missing,
        4 => ControlCode::Capacity,
        5 => ControlCode::Invalid,
        6 => ControlCode::Failed,
        7 => ControlCode::Unknown,
        _ => return Err(ControlError("control refusal code")),
    };
    let phase = input.text(64)?;
    let moved = if boolean(input)? {
        Some(moved(input)?)
    } else {
        None
    };
    if (code == ControlCode::HeadMoved) != moved.is_some() {
        return Err(ControlError("control conflict fields"));
    }
    Ok(ControlRefusal {
        code,
        phase,
        moved,
        published: publication(input)?,
        detail: input.text(2048)?,
    })
}
fn put_status(out: &mut Writer, value: &WorkspaceStatus) -> Result<(), ControlError> {
    put_token(out, value.token)?;
    put_binding(out, &value.binding)?;
    out.byte(match value.activity {
        Activity::Idle => 1,
        Activity::Committing => 2,
        Activity::Closing => 3,
        Activity::Uncertain => 4,
        Activity::LocalFailure => 5,
    })?;
    out.put(&value.epoch.to_be_bytes())?;
    out.byte(u8::from(value.epoch_saturated))?;
    put_publication(out, &value.published)?;
    if value.local.is_some() == value.local_failure.is_some() {
        return Err(ControlError("engine observation disposition"));
    }
    out.byte(u8::from(value.local.is_some()))?;
    let Some(local) = &value.local else {
        return put_refusal(
            out,
            value
                .local_failure
                .as_ref()
                .expect("checked engine refusal"),
        );
    };
    out.put(&local.revision.to_be_bytes())?;
    out.put(&local.active.to_be_bytes())?;
    put_generation(out, local.captured)?;
    put_generation(out, local.captured_revision)?;
    out.put(&local.base_root)?;
    out.put(&local.dirty_inodes.to_be_bytes())?;
    out.put(&local.dirty_directory_entries.to_be_bytes())?;
    out.byte(u8::from(local.closed))?;
    out.put(&local.base_readers.to_be_bytes())
}
fn status(input: &mut Reader<'_>) -> Result<WorkspaceStatus, ControlError> {
    let token = token(input)?;
    let binding = binding(input)?;
    let activity = match input.byte()? {
        1 => Activity::Idle,
        2 => Activity::Committing,
        3 => Activity::Closing,
        4 => Activity::Uncertain,
        5 => Activity::LocalFailure,
        _ => return Err(ControlError("control activity")),
    };
    let epoch = u64::from_be_bytes(input.array()?);
    let epoch_saturated = boolean(input)?;
    let published = publication(input)?;
    let (local, local_failure) = if boolean(input)? {
        (
            Some(LocalObservation {
                revision: i64::from_be_bytes(input.array()?),
                active: i64::from_be_bytes(input.array()?),
                captured: generation(input)?,
                captured_revision: generation(input)?,
                base_root: input.array()?,
                dirty_inodes: u64::from_be_bytes(input.array()?),
                dirty_directory_entries: u64::from_be_bytes(input.array()?),
                closed: boolean(input)?,
                base_readers: u64::from_be_bytes(input.array()?),
            }),
            None,
        )
    } else {
        (None, Some(refusal(input)?))
    };
    Ok(WorkspaceStatus {
        token,
        binding,
        activity,
        epoch,
        epoch_saturated,
        published,
        local,
        local_failure,
    })
}
fn put_generation(out: &mut Writer, value: Option<i64>) -> Result<(), ControlError> {
    out.byte(u8::from(value.is_some()))?;
    if let Some(value) = value {
        out.put(&value.to_be_bytes())?;
    }
    Ok(())
}
fn generation(input: &mut Reader<'_>) -> Result<Option<i64>, ControlError> {
    if boolean(input)? {
        Ok(Some(i64::from_be_bytes(input.array()?)))
    } else {
        Ok(None)
    }
}
