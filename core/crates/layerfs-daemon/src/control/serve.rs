//! One authenticated request/result exchange, retaining original delivery failures.
use super::{Failure, Service, Success};
use crate::store::CommitError;
use layerfs_bridge::{
    control::{Answer, Call, ControlError, Reply},
    native::{ChannelError, Connection},
};
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_history::BranchSnapshot;
use layerfs_overlay::Capture;
use layerfs_storage::Save;
use std::fmt;
/// Original product operation, even when response encoding/delivery fails later.
#[derive(Debug)]
pub struct Served {
    pub call: Call,
    pub outcome: Result<Success, Failure>,
}
#[derive(Debug)]
pub enum ServeCause {
    Channel(ChannelError),
    Protocol(ControlError),
}
/// Failed native delivery is not rollback. A caller owns every retained receipt.
#[derive(Debug)]
pub struct ServeFailure {
    pub original: Option<Served>,
    pub received: Option<Vec<u8>>,
    pub cause: ServeCause,
    pub fence_error: Option<ChannelError>,
}
impl Service {
    /// Receives and executes one command once; no hidden serving/reconnect loop.
    pub fn serve_one(
        &self,
        connection: &mut Connection,
        construct: impl FnOnce(
            &Save<'_>,
            Capture,
            &BranchSnapshot,
        ) -> Result<FilesystemRootId, CommitError>,
    ) -> Result<Served, Box<ServeFailure>> {
        let mut original = None;
        let mut received = None;
        let result = (|| {
            let bytes = connection.receive.receive().map_err(ServeCause::Channel)?;
            let call = match Call::decode(bytes) {
                Ok(call) => call,
                Err(error) => {
                    received = Some(bytes.to_vec());
                    return Err(ServeCause::Protocol(error));
                }
            };
            let outcome = self.execute(&call.request, construct);
            original = Some(Served { call, outcome });
            let served = original.as_ref().expect("original operation");
            let reply = match &served.outcome {
                Ok(success) => success.reply.clone(),
                Err(failure) => Reply::Refused(failure.wire()),
            };
            let answer = Answer {
                id: served.call.id,
                reply,
            }
            .encode()
            .map_err(ServeCause::Protocol)?;
            connection.send.send(&answer).map_err(ServeCause::Channel)?;
            Ok(())
        })();
        match result {
            Ok(()) => Ok(original.expect("delivered original operation")),
            Err(cause) => {
                let fence_error = if matches!(cause, ServeCause::Channel(_)) {
                    None
                } else {
                    connection.send.close().err()
                };
                Err(Box::new(ServeFailure {
                    original,
                    received,
                    cause,
                    fence_error,
                }))
            }
        }
    }
}
impl fmt::Display for ServeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native control: {:?}", self.cause)
    }
}
impl std::error::Error for ServeFailure {}
