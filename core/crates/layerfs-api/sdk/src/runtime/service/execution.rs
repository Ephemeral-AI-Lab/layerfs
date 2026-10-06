//! One existing runtime call per job; no retry or delivery under provider locks.
use super::{ObjectValue, Request, Response, ServiceClass};
use crate::runtime::{
    Binding, LengthReply, ObjectReply, RuntimeError, RuntimeResult, SaveId, Sessions,
};
use layerfs_content::ObjectId;
use layerfs_storage::policy::{CANONICAL_LIMIT, READ_CANONICAL_BYTES_LIMIT, READ_OBJECT_LIMIT};
use layerfs_telemetry::timer::Timing;

impl Request {
    pub(super) fn class(&self) -> ServiceClass {
        match self {
            Self::Objects { .. } | Self::Lengths { .. } => ServiceClass::Demand,
            Self::Policy => ServiceClass::Policy,
            Self::ReserveInodes { .. } => ServiceClass::Serial,
            Self::Accept { .. } => ServiceClass::Accept,
            Self::Begin
            | Self::Finish { .. }
            | Self::Abort { .. }
            | Self::Completion { .. }
            | Self::Release { .. } => ServiceClass::Finish,
            _ => ServiceClass::History,
        }
    }
    pub(super) fn save(&self) -> Option<SaveId> {
        match self {
            Self::Accept { save, .. }
            | Self::Finish { save }
            | Self::Abort { save }
            | Self::Completion { save }
            | Self::Release { save }
            | Self::Stage { save, .. }
            | Self::Commit { save }
            | Self::Discard { save }
            | Self::History { save } => Some(*save),
            Self::Objects { save, .. } => *save,
            _ => None,
        }
    }
    pub(super) fn charge(&self, base: usize) -> Option<usize> {
        let extra = match self {
            Self::Accept { canonical, .. } => {
                if canonical.len() > CANONICAL_LIMIT {
                    return None;
                }
                canonical.capacity()
            }
            Self::Objects { ids, .. } => {
                if ids.len() > READ_OBJECT_LIMIT {
                    return None;
                }
                ids.capacity()
                    .checked_mul(std::mem::size_of::<ObjectId>())?
                    .checked_add(ids.len().checked_mul(std::mem::size_of::<ObjectValue>())?)?
                    .checked_add(
                        ids.len()
                            .checked_mul(CANONICAL_LIMIT)?
                            .min(READ_CANONICAL_BYTES_LIMIT)
                            .checked_mul(2)?,
                    )?
            }
            Self::Lengths { ids } => {
                if ids.len() > READ_OBJECT_LIMIT {
                    return None;
                }
                ids.capacity()
                    .checked_mul(std::mem::size_of::<ObjectId>())?
                    .checked_add(
                        ids.len()
                            .checked_mul(std::mem::size_of::<(ObjectId, u64)>() + 8)?,
                    )?
            }
            _ => 0,
        };
        base.checked_add(extra)
    }
}
struct Objects {
    values: Vec<ObjectValue>,
    bytes: usize,
}
impl ObjectReply for Objects {
    fn object(&mut self, id: ObjectId, canonical: &[u8]) -> RuntimeResult<()> {
        self.bytes = self
            .bytes
            .checked_add(canonical.len())
            .ok_or(RuntimeError::Invalid("reply byte overflow"))?;
        if self.bytes > READ_CANONICAL_BYTES_LIMIT {
            return Err(RuntimeError::Invalid("canonical reply window"));
        }
        self.values.push(ObjectValue {
            id,
            canonical: canonical.to_vec(),
        });
        Ok(())
    }
}
struct Lengths(Vec<(ObjectId, u64)>);
impl LengthReply for Lengths {
    fn file_length(&mut self, id: ObjectId, logical_len: u64) -> RuntimeResult<()> {
        self.0.push((id, logical_len));
        Ok(())
    }
}
pub(super) fn invoke(
    sessions: &mut Sessions<'_>,
    binding: &Binding,
    request: Request,
) -> (RuntimeResult<Response>, u64) {
    let mut copied = 0;
    let result = (|| match request {
        Request::Policy => sessions.policy(binding).map(Response::Policy),
        Request::ReserveInodes { count } => sessions
            .reserve_serials(binding, count)
            .map(|(start, count)| Response::Serials { start, count }),
        Request::Begin => sessions.begin(binding).map(Response::Begun),
        Request::Accept {
            save,
            id,
            role,
            canonical,
        } => Timing::disabled("runtime.service.accept", |timing| {
            sessions.accept(binding, save, id, role, canonical, timing.child("admit"))
        })
        .0
        .map(Response::Accepted),
        Request::Objects { save, ids } => {
            let mut reply = Objects {
                values: Vec::with_capacity(ids.len()),
                bytes: 0,
            };
            let read = sessions.read_objects(binding, save, &ids, &mut reply);
            copied = reply.bytes as u64;
            read?;
            Ok(Response::Objects(reply.values))
        }
        Request::Lengths { ids } => {
            let mut reply = Lengths(Vec::with_capacity(ids.len()));
            sessions.file_lengths(binding, &ids, &mut reply)?;
            Ok(Response::Lengths(reply.0))
        }
        Request::Finish { save } => sessions
            .finish(binding, save)
            .map(|_| Response::Completion(save)),
        Request::Abort { save } => sessions
            .abort(binding, save)
            .map(|_| Response::Completion(save)),
        Request::Completion { save } => sessions
            .completion(binding, save)
            .map(|_| Response::Completion(save)),
        Request::Release { save } => sessions.release(binding, save).map(|_| Response::Released),
        Request::Stage {
            save,
            root,
            generation,
        } => sessions
            .stage_saved(binding, save, root, generation)
            .map(|_| Response::History(save)),
        Request::Commit { save } => sessions
            .commit_saved(binding, save)
            .map(|_| Response::History(save)),
        Request::Discard { save } => sessions
            .discard_saved(binding, save)
            .map(|_| Response::History(save)),
        Request::History { save } => sessions
            .history_receipts(binding, save)
            .map(|_| Response::History(save)),
    })();
    (result, copied)
}
