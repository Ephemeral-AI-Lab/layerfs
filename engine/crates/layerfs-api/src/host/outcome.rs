//! What one host operation answers: a value, or a refusal with a stable
//! code. Moved from layerfs-node's job.rs (#533 step 2).
use layerfs_bridge::control::{SdkRefusal, SdkValue};

/// What one operation answers.
pub type Outcome = Result<SdkValue, SdkRefusal>;

/// A refusal with a stable code.
pub fn refused(code: &str, message: impl Into<String>) -> SdkRefusal {
    SdkRefusal {
        code: code.to_owned(),
        message: message.into(),
        detail: SdkValue::Null,
    }
}

/// A failure that has no SDK code of its own.
pub fn failed(error: impl std::fmt::Debug) -> SdkRefusal {
    refused("Failed", format!("{error:?}"))
}
