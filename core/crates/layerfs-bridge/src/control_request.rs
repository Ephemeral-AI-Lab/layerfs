//! One correlated bounded control request, never an immutable-content upload.
use crate::{
    control::{Call, ControlError, Request, HISTORY_WINDOW},
    control_history::*,
    wire::{Reader, Writer},
};
use layerfs_history::{
    BranchId, CommitHistoryRequest, ForkRequest, ForkSource, HistoryName, LayerId, LayerStackId,
    WorkspaceId,
};
pub(crate) const MAGIC: &[u8] = b"LFSC\x01";
impl Call {
    /// Encodes one checked request. Failure before send has no remote effect.
    pub fn encode(&self) -> Result<Vec<u8>, ControlError> {
        if self.id == 0 {
            return Err(ControlError("control correlation"));
        }
        let request = self.request.without_observation()?;
        let mut out = Writer::new(MAGIC);
        out.put(&self.id.to_be_bytes())?;
        if let Request::Observed { scope, .. } = &self.request {
            out.byte(12)?;
            out.put(scope)?;
        }
        match request {
            Request::Observed { .. } => return Err(ControlError("invalid observed operation")),
            Request::Cleanup(token) => {
                out.byte(13)?;
                put_token(&mut out, *token)?;
            }
            Request::EndSession => out.byte(8)?,
            Request::Hello(value) => {
                out.byte(7)?;
                crate::daemon_wire::put_request(&mut out, value)?;
            }
            Request::Mount { workspace, branch } => {
                out.byte(1)?;
                out.put(&workspace.to_bytes())?;
                out.put(&branch.to_bytes())?;
            }
            Request::Commit(token) => {
                out.byte(2)?;
                put_token(&mut out, *token)?;
            }
            Request::Status(token) => {
                out.byte(3)?;
                put_token(&mut out, *token)?;
            }
            Request::Unmount(token) => {
                out.byte(4)?;
                put_token(&mut out, *token)?;
            }
            Request::Fork(value) => {
                out.byte(5)?;
                out.put(&value.stack.to_bytes())?;
                out.put(&value.branch.to_bytes())?;
                out.blob(value.name.as_str().as_bytes())?;
                match value.source {
                    ForkSource::Layer(id) => {
                        out.byte(1)?;
                        out.put(&id.to_bytes())?;
                    }
                    ForkSource::Commit { branch, commit } => {
                        out.byte(2)?;
                        out.put(&branch.to_bytes())?;
                        out.put(&commit.to_bytes())?;
                    }
                }
            }
            Request::Attach(token) => {
                out.byte(9)?;
                put_token(&mut out, *token)?;
            }
            Request::Locate(workspace) => {
                out.byte(10)?;
                out.put(&workspace.to_bytes())?;
            }
            Request::ForceUnmount {
                token,
                relinquish_unknown,
            } => {
                out.byte(11)?;
                put_token(&mut out, *token)?;
                out.byte(u8::from(*relinquish_unknown))?;
            }
            Request::History(value) => {
                window(value.limit)?;
                out.byte(6)?;
                out.put(&value.branch.to_bytes())?;
                put_head(&mut out, value.start)?;
                put_cursor(&mut out, &value.cursor)?;
                out.put(&value.limit.to_be_bytes())?;
            }
        }
        Ok(out.0)
    }
    /// Decodes exactly one original request. Unknown tags/versions are refused.
    pub fn decode(bytes: &[u8]) -> Result<Self, ControlError> {
        let mut input = Reader::new(bytes, MAGIC)?;
        let id = u64::from_be_bytes(input.array()?);
        if id == 0 {
            return Err(ControlError("control correlation"));
        }
        let tag = input.byte()?;
        let observed = tag == 12;
        let scope = if observed {
            let scope = input.array::<32>()?;
            if scope == [0; 32] {
                return Err(ControlError("zero observation scope"));
            }
            Some(scope)
        } else {
            None
        };
        let tag = if observed {
            let inner = input.byte()?;
            if matches!(inner, 7 | 8 | 12) {
                return Err(ControlError("invalid observed operation"));
            }
            inner
        } else {
            tag
        };
        let request = match tag {
            13 => Request::Cleanup(token(&mut input)?),
            8 => Request::EndSession,
            7 => Request::Hello(crate::daemon_wire::request(&mut input)?),
            1 => Request::Mount {
                workspace: history(WorkspaceId::from_authority(input.array()?))?,
                branch: history(BranchId::from_bytes(input.array()?))?,
            },
            2 => Request::Commit(token(&mut input)?),
            3 => Request::Status(token(&mut input)?),
            4 => Request::Unmount(token(&mut input)?),
            9 => Request::Attach(token(&mut input)?),
            10 => Request::Locate(history(WorkspaceId::from_authority(input.array()?))?),
            11 => Request::ForceUnmount {
                token: token(&mut input)?,
                relinquish_unknown: boolean(&mut input)?,
            },
            5 => {
                let stack = history(LayerStackId::from_bytes(input.array()?))?;
                let branch = history(BranchId::from_bytes(input.array()?))?;
                let name = history(HistoryName::new(&input.text(63)?))?;
                let source = match input.byte()? {
                    1 => ForkSource::Layer(history(LayerId::from_bytes(input.array()?))?),
                    2 => ForkSource::Commit {
                        branch: history(BranchId::from_bytes(input.array()?))?,
                        commit: history(layerfs_history::CommitId::from_bytes(input.array()?))?,
                    },
                    _ => return Err(ControlError("fork source")),
                };
                Request::Fork(ForkRequest {
                    stack,
                    branch,
                    name,
                    source,
                })
            }
            6 => {
                let branch = history(BranchId::from_bytes(input.array()?))?;
                let start = head(&mut input)?;
                let cursor = cursor(&mut input)?;
                let limit = u16::from_be_bytes(input.array()?);
                window(limit)?;
                Request::History(CommitHistoryRequest {
                    branch,
                    start,
                    cursor,
                    limit,
                })
            }
            _ => return Err(ControlError("control operation")),
        };
        input.finish()?;
        let request = match scope {
            Some(scope) => Request::Observed {
                scope,
                request: Box::new(request),
            },
            None => request,
        };
        Ok(Self { id, request })
    }
}
fn window(limit: u16) -> Result<(), ControlError> {
    if limit == 0 || limit > HISTORY_WINDOW {
        Err(ControlError("history processing window"))
    } else {
        Ok(())
    }
}
