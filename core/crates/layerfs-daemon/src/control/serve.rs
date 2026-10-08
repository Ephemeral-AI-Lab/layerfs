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
        let call = receive_call(connection)?;
        let outcome = self.execute(&call.request, construct);
        answer_call(connection, call, outcome)
    }
    /// Receives one ordinary control without inventing a Commit producer.
    pub fn serve_one_control(
        &self,
        connection: &mut Connection,
    ) -> Result<Served, Box<ServeFailure>> {
        let call = receive_call(connection)?;
        self.serve_control_call(connection, call)
    }
    pub(crate) fn serve_control_call(
        &self,
        connection: &mut Connection,
        call: Call,
    ) -> Result<Served, Box<ServeFailure>> {
        let outcome = self.execute_control(&call.request);
        answer_call(connection, call, outcome)
    }
}
impl fmt::Display for ServeFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native control: {:?}", self.cause)
    }
}
impl std::error::Error for ServeFailure {}

pub(crate) fn receive_call(connection: &mut Connection) -> Result<Call, Box<ServeFailure>> {
    let mut received = None;
    let result = (|| {
        let bytes = connection.receive.receive().map_err(ServeCause::Channel)?;
        Call::decode(bytes).map_err(|cause| {
            received = Some(bytes.to_vec());
            ServeCause::Protocol(cause)
        })
    })();
    result.map_err(|cause| failed(connection, None, received, cause))
}
pub(crate) fn answer_call(
    connection: &mut Connection,
    call: Call,
    outcome: Result<Success, Failure>,
) -> Result<Served, Box<ServeFailure>> {
    let original = Served { call, outcome };
    let result = (|| {
        let reply = match &original.outcome {
            Ok(success) => success.reply.clone(),
            Err(Failure::Retained(custody)) => Reply::Retained(custody.clone()),
            Err(failure) => Reply::Refused(failure.wire()),
        };
        let answer = Answer {
            id: original.call.id,
            reply,
        }
        .encode()
        .map_err(ServeCause::Protocol)?;
        connection.send.send(&answer).map_err(ServeCause::Channel)
    })();
    match result {
        Ok(()) => Ok(original),
        Err(cause) => Err(failed(connection, Some(original), None, cause)),
    }
}
fn failed(
    connection: &mut Connection,
    original: Option<Served>,
    received: Option<Vec<u8>>,
    cause: ServeCause,
) -> Box<ServeFailure> {
    let fence_error = if matches!(cause, ServeCause::Channel(_)) {
        None
    } else {
        connection.send.close().err()
    };
    Box::new(ServeFailure {
        original,
        received,
        cause,
        fence_error,
    })
}
