//! One daemon operator call over the control channel, and its failure.
//!
//! Its arguments and result are plain values, and a refusal carries a stable
//! code. Store calls are not daemon calls (#352 F-020).
use crate::{
    base::failure::exchange_with,
    internal::{Control, ControlCause, ControlPhase},
    OperationCause, OperationFailure,
};
use layerfs_bridge::control::{ControlError, Reply, Request, SdkCall, SdkRefusal, SdkValue};
use std::fmt;

/// The failure of one SDK call.
#[derive(Debug)]
pub enum SdkError {
    /// The daemon refused the call; its `code` decides.
    Refused(SdkRefusal),
    /// The exchange or the daemon's control layer failed.
    Operation(Box<OperationFailure>),
    /// A mount stopped at bind or attach, with what it acknowledged.
    Mount(Box<crate::MountFailure>),
}

/// The answer of one SDK call.
pub type SdkResult = Result<SdkValue, SdkError>;

impl From<Box<OperationFailure>> for SdkError {
    fn from(failure: Box<OperationFailure>) -> Self {
        let oversized = matches!(&failure.cause, OperationCause::Exchange(cause)
            if cause.phase == ControlPhase::Encode
                && !cause.attempted
                && cause.received.is_none()
                && cause.fence_error.is_none()
                && matches!(&cause.cause,
                    ControlCause::Protocol(ControlError("control record limit"))));
        if oversized {
            Self::Refused(SdkRefusal {
                code: "TooLarge".into(),
                message:
                    "SDK request exceeds one control record; use a smaller call or a Sandbox step"
                        .into(),
                detail: SdkValue::Null,
            })
        } else {
            Self::Operation(failure)
        }
    }
}

impl fmt::Display for SdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(refusal) => write!(f, "{}: {}", refusal.code, refusal.message),
            Self::Operation(failure) => failure.fmt(f),
            Self::Mount(failure) => failure.failure.fmt(f),
        }
    }
}
impl std::error::Error for SdkError {}

/// Calls one daemon operator method once; nothing is retried.
pub(crate) fn exchange_call(control: &mut Control, method: &str, args: SdkValue) -> SdkResult {
    let request = Request::Sdk(SdkCall {
        method: method.to_owned(),
        args,
    });
    match exchange_with(control, request.clone(), &[])? {
        Reply::Sdk(value) => Ok(value),
        Reply::SdkRefused(refusal) => Err(SdkError::Refused(refusal)),
        reply => Err(SdkError::Operation(OperationFailure::unexpected(
            request, reply,
        ))),
    }
}
