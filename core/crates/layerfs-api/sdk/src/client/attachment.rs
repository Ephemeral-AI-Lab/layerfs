//! Consumer attachment over one authenticated connection and shared reply credit.
use super::{
    Calls, ClientReceiver, ClientSender, RemoteLengths, RemoteObjects, RemoteSerials, SaveToken,
};
use layerfs_bridge::{
    codec::{Message, ReassemblyWork, ReceiveBudget},
    contract::{FrameError, MessageKind},
    native::{ChannelError, ChannelWork, Connection, FramingError, FramingWork},
};
use std::sync::Arc;

/// Unattempted consumer attachment refusal; original native ownership returns intact.
pub enum AttachmentError {
    /// Independent fence descriptor could not be created.
    Native(ChannelError),
    /// Shared reply admission/configuration or framing refusal.
    Frame(FrameError),
}
/// Exact local completion fence after any bounded call has released its driver.
/// It contains original partial replies; received caller-held Message credits
/// stay with those callers and are still visible through the shared receive budget.
pub struct ConsumerFence {
    /// Original explicit socket shutdown result, without inferred publication.
    pub close: Result<(), FramingError>,
    /// Original incomplete received replies, never interpreted as operation absence.
    pub partial: Vec<Message>,
    /// Original send framing observations.
    pub send_framing: FramingWork,
    /// Original native send observations, including failed partial I/O.
    pub send_native: ChannelWork,
    /// Exact first-party ID encoding copies.
    pub id_copied_bytes: u64,
    /// Original native receive observations.
    pub receive_native: ChannelWork,
    /// Shared partial/result allocation and copy ownership at the fence.
    pub receive: Result<ReassemblyWork, FrameError>,
}
/// Owning consumer connection with ordinary object/length/serial adapters.
/// Cloned Calls serialize one bounded exchange; they do not pipeline a connection
/// or lock it for a whole Save/Commit. Independent attachments use independent I/O.
pub struct Attachment {
    calls: Arc<Calls>,
    close: Option<Result<(), FramingError>>,
    joined: bool,
}
impl Attachment {
    /// Takes one authenticated connection once, retaining the caller's selected
    /// shared receive window. Refusal returns original directions/handshake work.
    #[allow(clippy::result_large_err)]
    pub fn new(
        connection: Connection,
        budget: ReceiveBudget,
    ) -> Result<Self, (AttachmentError, Connection)> {
        if budget.config().kind != MessageKind::Reply {
            return Err((
                AttachmentError::Frame(FrameError::Invalid("consumer reply budget kind")),
                connection,
            ));
        }
        let fence = match connection.close_handle() {
            Ok(fence) => fence,
            Err(error) => return Err((AttachmentError::Native(error), connection)),
        };
        let Connection {
            receive,
            send,
            peer,
            handshake_work,
        } = connection;
        let send = match ClientSender::new(send) {
            Ok(send) => send,
            Err((error, send)) => {
                return Err((
                    AttachmentError::Frame(error),
                    Connection {
                        receive,
                        send,
                        peer,
                        handshake_work,
                    },
                ))
            }
        };
        let receive = match ClientReceiver::new(receive, budget) {
            Ok(receive) => receive,
            Err((error, receive)) => {
                return Err((
                    AttachmentError::Frame(error),
                    Connection {
                        receive,
                        send: send.into_native(),
                        peer,
                        handshake_work,
                    },
                ))
            }
        };
        Ok(Self {
            calls: Arc::new(Calls::new(send, receive, peer, fence)),
            close: None,
            joined: false,
        })
    }
    /// Existing bounded call owner, including typed Save/history operations.
    pub fn calls(&self) -> Arc<Calls> {
        self.calls.clone()
    }
    /// One owning content demand operation, with optional exact same-Save visibility.
    pub fn objects(&self, save: Option<SaveToken>) -> RemoteObjects {
        RemoteObjects::new(self.calls(), save)
    }
    /// Saved-file length adapter over the existing attachment.
    pub fn lengths(&self) -> RemoteLengths {
        RemoteLengths::new(self.calls())
    }
    /// Authority-owned serial adapter over the existing attachment.
    pub fn serials(&self) -> RemoteSerials {
        RemoteSerials::new(self.calls())
    }
    /// Explicit socket fence once, independently of an in-flight call's mutex.
    /// The original call failure remains with its caller. No reconnect/resend occurs.
    pub fn fence(&mut self) {
        if self.close.is_none() {
            self.close = Some(self.calls.close());
        }
    }
    /// Checks local call completion without blocking. None means a call still
    /// owns its driver; explicit socket shutdown by itself is not completion.
    pub fn try_join(&mut self) -> Result<Option<ConsumerFence>, FrameError> {
        if self.joined {
            return Err(FrameError::Invalid("consumer fence already returned"));
        }
        if self.close.is_none() {
            return Err(FrameError::Invalid("consumer attachment not fenced"));
        }
        let Some(parts) = self.calls.try_fence()? else {
            return Ok(None);
        };
        self.joined = true;
        Ok(Some(ConsumerFence {
            close: self.close.take().expect("explicit original close"),
            partial: parts.partial,
            send_framing: parts.send_framing,
            send_native: parts.send_native,
            id_copied_bytes: parts.id_copied_bytes,
            receive_native: parts.receive_native,
            receive: parts.receive,
        }))
    }
}
impl Drop for Attachment {
    fn drop(&mut self) {
        if !self.joined {
            self.fence();
        }
    }
}

pub(super) struct FenceParts {
    pub partial: Vec<Message>,
    pub send_framing: FramingWork,
    pub send_native: ChannelWork,
    pub id_copied_bytes: u64,
    pub receive_native: ChannelWork,
    pub receive: Result<ReassemblyWork, FrameError>,
}
