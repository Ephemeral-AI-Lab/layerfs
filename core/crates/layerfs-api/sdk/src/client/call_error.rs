//! Formatting preserves structured original failures instead of reclassifying text.
use super::{CallError, CallFailure, ClientReceiveError, ClientSendError};
use std::fmt;
impl fmt::Debug for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frame(e) => f.debug_tuple("Frame").field(e).finish(),
            Self::Send(e) => f.debug_tuple("Send").field(e).finish(),
            Self::Receive(ClientReceiveError::Native(e)) => {
                f.debug_tuple("ReceiveNative").field(e).finish()
            }
            Self::Receive(ClientReceiveError::Frame(e)) => {
                f.debug_tuple("ReceiveFrame").field(e).finish()
            }
            Self::Receive(ClientReceiveError::Rejected {
                error,
                message,
                close,
            }) => f
                .debug_struct("Rejected")
                .field("error", error)
                .field("envelope", &message.envelope())
                .field("close", close)
                .finish(),
        }
    }
}
impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for CallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(match self {
            Self::Frame(e) => e,
            Self::Send(ClientSendError::Frame(e)) => e,
            Self::Send(ClientSendError::Native(e)) => e,
            Self::Receive(ClientReceiveError::Frame(e)) => e,
            Self::Receive(ClientReceiveError::Native(e)) => e,
            Self::Receive(ClientReceiveError::Rejected { error, .. }) => error,
        })
    }
}
impl fmt::Debug for CallFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CallFailure")
            .field("request", &self.request)
            .field("correlation", &self.correlation)
            .field("phase", &self.phase)
            .field("error", &self.error)
            .field("received", &self.received.as_ref().map(|m| m.envelope()))
            .field("close", &self.close)
            .finish()
    }
}
impl fmt::Display for CallFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "native runtime call: {self:?}")
    }
}
impl std::error::Error for CallFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}
