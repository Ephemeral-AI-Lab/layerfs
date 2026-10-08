//! Publication before reply, and ticket release only after the reply attempt.
//!
//! A mutation takes effect at the commit of its single owner job. Its reply is
//! computed from that job's published values, so it is never older than the
//! state it reports and never older than an earlier mutation's attempted
//! reply: the one SQL owner orders every publication. A read-class reply is
//! computed by an owner job that runs after its request was received. The
//! kernel discards an attribute reply sampled before a newer inode version;
//! that discard, not reply order on the wire, is what the daemon relies on.
use crate::{
    operations::{MutationFailure, NativeMutation},
    RequestDisposition,
};

/// Called once the reply method returned. The pinned library reports no send
/// result, so this is a reply *attempt*: the ticket is released whether or not
/// the kernel received it, and a published mutation is never undone.
pub async fn attempted(mutation: NativeMutation) -> RequestDisposition {
    match mutation.replied().await {
        Ok(()) => RequestDisposition::Complete,
        Err(failure) => retained(failure),
    }
}
/// A mutation that stopped before or after publication keeps every owner it
/// still has; nothing is released, resent or rolled back on a guess.
pub fn retained(failure: MutationFailure) -> RequestDisposition {
    RequestDisposition::Retained(Box::new(failure))
}
