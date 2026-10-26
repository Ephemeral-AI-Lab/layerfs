//! Write bodies that follow one call's record on its connection.
//!
//! A file write's bytes once followed its call record, in records of at most
//! one channel plaintext (`streamed` lengths in `fs.write` and `fs.apply`).
//! Store calls are not served here (#352 F-020), so nothing reads them: the
//! bytes a call declares are read and dropped before its answer, a record at
//! a time, so the connection stays in step.
use layerfs_bridge::control::{Request, SdkValue};
use layerfs_bridge::duplex::{ChannelError, Receiver};

/// The declared bodies of the call being served.
pub(super) struct Bodies<'a> {
    receive: &'a mut Receiver,
    remaining: u64,
}

impl<'a> Bodies<'a> {
    /// The bodies `request` declares, read from `receive`.
    pub(super) fn of(request: &Request, receive: &'a mut Receiver) -> Self {
        Self {
            receive,
            remaining: declared(request),
        }
    }

    /// Reads and drops the declared bytes; the channel's failure, if any.
    pub(super) fn finish(mut self) -> Result<(), ChannelError> {
        while self.remaining > 0 {
            let size = self.receive.receive()?.len() as u64;
            if size == 0 || size > self.remaining {
                return Err(ChannelError::Invalid("streamed body record length"));
            }
            self.remaining -= size;
        }
        Ok(())
    }
}

/// Bytes the call's `streamed` write fields declare: `fs.write`'s own, or
/// each `fs.apply` write change's. Other calls declare none.
fn declared(request: &Request) -> u64 {
    let call = match request {
        Request::Sdk(call) => call,
        Request::Observed { request, .. } => match request.as_ref() {
            Request::Sdk(call) => call,
            _ => return 0,
        },
        _ => return 0,
    };
    let length = |change: &SdkValue| match change.get("streamed") {
        Some(SdkValue::Int(n)) if *n > 0 => *n as u64,
        _ => 0,
    };
    match call.method.as_str() {
        "fs.write" => length(&call.args),
        "fs.apply" => match call.args.get("changes") {
            Some(SdkValue::List(changes)) => changes
                .iter()
                .filter(|change| change.get_text("op") == Some("write"))
                .map(length)
                .fold(0u64, u64::saturating_add),
            _ => 0,
        },
        _ => 0,
    }
}
