//! Authenticated borrowed records and one-attempt fragment send progress.
use super::{ChannelError, ChannelWork, Receiver, Sender};
use crate::{
    codec,
    contract::{Envelope, Fragment, FrameError, MessageClass, MessageKind, MAX_RECORD_BYTES},
};
use std::{fmt, sync::Arc};
/// Original logical/native failure; send context records confirmed fragment
/// progress without claiming runtime publication or attempting a resend.
#[derive(Debug)]
pub enum FramingError {
    /// Original logical input failure; no native write was attempted.
    Frame(FrameError),
    /// Original authenticated channel failure.
    Native(ChannelError),
    /// A malformed authenticated record fenced the channel; close may also fail.
    Rejected {
        /// Exact original framing failure.
        frame: FrameError,
        /// Original close failure, if shutdown failed.
        close: Option<ChannelError>,
    },
    /// Original attempted write failure with only completed-fragment knowledge.
    Send {
        /// Immutable logical message identity/correlation.
        envelope: Envelope,
        /// Bytes whose complete records were acknowledged by local socket write.
        /// This does not assert remote adapter invocation/publication.
        completed_bytes: u64,
        /// Exact original native error, including any failed quarantine close.
        error: ChannelError,
    },
}
impl fmt::Display for FramingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "authenticated framing: {self:?}")
    }
}
impl std::error::Error for FramingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Frame(e) => Some(e),
            Self::Native(e) => Some(e),
            Self::Rejected { frame, .. } => Some(frame),
            Self::Send { error, .. } => Some(error),
        }
    }
}
/// Fixed actual framing observations; native encrypted I/O is separate.
#[derive(Clone, Copy, Debug, Default)]
pub struct FramingWork {
    /// Original codec calls, including checked input refusals before native I/O.
    pub encoding_attempts: u64,
    /// Successfully encoded fragments, including later native send failures.
    pub encoded_fragments: u64,
    /// Actual fixed header bytes written by successful fragment encoding.
    pub header_bytes: u64,
    /// Successfully sent logical fragments.
    pub fragments: u64,
    /// Actual body bytes copied into the native plaintext frame window.
    pub copied_bytes: u64,
    /// Current fixed frame allocation capacity, excluding native crypto buffers.
    pub frame_capacity: usize,
    /// Original fixed framing scratch allocation requests.
    pub allocation_attempts: u64,
    /// Requested framing scratch bytes, not allocator/OS residency.
    pub requested_buffer_bytes: u64,
    /// First-party frame bytes initialized by the successful scratch resize.
    pub zeroed_bytes: u64,
}
/// Unclonable original send progress, bound to one physical send owner.
pub struct SendProgress {
    envelope: Envelope,
    completed: u64,
    failed: bool,
    owner: Arc<()>,
}
impl SendProgress {
    /// Original identity/correlation.
    pub const fn envelope(&self) -> Envelope {
        self.envelope
    }
    /// Complete fragment progress; not remote operation knowledge.
    pub const fn completed_bytes(&self) -> u64 {
        self.completed
    }
    /// True only when the complete declared body was sent without error.
    pub fn complete(&self) -> bool {
        !self.failed && self.completed == self.envelope.total_bytes
    }
}
/// Single send worker's frame scratch, ID allocation and original progress.
/// Callers can interleave bounded fragments across active logical messages.
pub struct RecordSender {
    native: Sender,
    frame: Vec<u8>,
    owner: Arc<()>,
    next: u64,
    work: FramingWork,
}
impl RecordSender {
    /// Allocates fixed framing scratch before use, or returns native ownership.
    // Keep failure custody inline: an allocation refusal must not allocate a Box
    // merely to return the original socket owner and TryReserveError.
    #[allow(clippy::result_large_err)]
    pub fn new(native: Sender) -> Result<Self, (FrameError, Sender)> {
        let mut frame = Vec::new();
        if let Err(error) = frame.try_reserve_exact(MAX_RECORD_BYTES) {
            return Err((
                FrameError::Allocation {
                    requested_bytes: MAX_RECORD_BYTES,
                    error,
                },
                native,
            ));
        }
        frame.resize(MAX_RECORD_BYTES, 0);
        let capacity = frame.capacity();
        Ok(Self {
            native,
            frame,
            owner: Arc::new(()),
            next: 1,
            work: FramingWork {
                frame_capacity: capacity,
                allocation_attempts: 1,
                requested_buffer_bytes: MAX_RECORD_BYTES as u64,
                zeroed_bytes: MAX_RECORD_BYTES as u64,
                ..Default::default()
            },
        })
    }
    /// Mints and sends the first fragment once. First-record IDs therefore stay
    /// ordered even when completion/reply correlation order differs.
    pub fn begin(
        &mut self,
        kind: MessageKind,
        class: MessageClass,
        correlation: u64,
        total_bytes: u64,
        first: &[u8],
    ) -> Result<SendProgress, FramingError> {
        let message = self.next;
        self.next = message
            .checked_add(1)
            .ok_or(FramingError::Frame(FrameError::IdentityExhausted))?;
        let mut progress = SendProgress {
            envelope: Envelope {
                message,
                correlation,
                kind,
                class,
                total_bytes,
            },
            completed: 0,
            failed: false,
            owner: self.owner.clone(),
        };
        self.write(&mut progress, first)?;
        Ok(progress)
    }
    /// Sends one original subsequent fragment. Failed/completed/foreign progress
    /// cannot be reused; no automatic failed-operation replay exists.
    pub fn continue_message(
        &mut self,
        progress: &mut SendProgress,
        bytes: &[u8],
    ) -> Result<(), FramingError> {
        if !Arc::ptr_eq(&self.owner, &progress.owner) || progress.failed || progress.complete() {
            return Err(FramingError::Frame(FrameError::Invalid(
                "send progress ownership/state",
            )));
        }
        self.write(progress, bytes)
    }
    fn write(&mut self, progress: &mut SendProgress, bytes: &[u8]) -> Result<(), FramingError> {
        let fragment = Fragment {
            envelope: progress.envelope,
            offset: progress.completed,
            bytes,
        };
        self.work.encoding_attempts = self.work.encoding_attempts.saturating_add(1);
        let n = codec::encode(fragment, &mut self.frame).map_err(FramingError::Frame)?;
        self.work.encoded_fragments = self.work.encoded_fragments.saturating_add(1);
        self.work.header_bytes = self
            .work
            .header_bytes
            .saturating_add(crate::contract::HEADER_BYTES as u64);
        self.work.copied_bytes = self.work.copied_bytes.saturating_add(bytes.len() as u64);
        if let Err(error) = self.native.send(&self.frame[..n]) {
            progress.failed = true;
            return Err(FramingError::Send {
                envelope: progress.envelope,
                completed_bytes: progress.completed,
                error,
            });
        }
        progress.completed += bytes.len() as u64;
        self.work.fragments = self.work.fragments.saturating_add(1);
        Ok(())
    }
    /// Actual plaintext framing copies and current scratch.
    pub const fn work(&self) -> FramingWork {
        self.work
    }
    /// Original native encrypted/partial socket work.
    pub fn native_work(&self) -> ChannelWork {
        self.native.work()
    }
    /// Explicit socket fence once, without interpreting runtime outcomes.
    pub fn close(&self) -> Result<(), FramingError> {
        self.native.close().map_err(FramingError::Native)
    }
    /// Returns the original native owner, for example after an unattempted worker
    /// start refusal. Previously minted progress cannot be used by a new wrapper.
    pub fn into_native(self) -> Sender {
        self.native
    }
}
/// I/O-worker receive owner; decoding borrows its existing native record window.
/// The SDK can inspect/authenticate operation admission before body reassembly.
pub struct RecordReceiver {
    native: Receiver,
}
impl RecordReceiver {
    /// Wraps existing authenticated ownership without allocating a body/cache.
    pub const fn new(native: Receiver) -> Self {
        Self { native }
    }
    /// Receives one authenticated fragment once; malformed framing quarantines
    /// both directions, and earlier reassembly/adapter outcomes stay with owners.
    pub fn receive(&mut self) -> Result<Fragment<'_>, FramingError> {
        let checked = {
            let record = self.native.receive().map_err(FramingError::Native)?;
            codec::decode(record).map(|fragment| (fragment.envelope, fragment.offset))
        };
        match checked {
            Ok((envelope, offset)) => Ok(Fragment {
                envelope,
                offset,
                bytes: &self.native.last_record()[crate::contract::HEADER_BYTES..],
            }),
            Err(frame) => Err(FramingError::Rejected {
                frame,
                close: self.native.close().err(),
            }),
        }
    }
    /// Actual native record and retained-buffer observations.
    pub fn work(&self) -> ChannelWork {
        self.native.work()
    }
    /// Explicit socket fence; joining I/O workers remains with the caller.
    pub fn close(&self) -> Result<(), FramingError> {
        self.native.close().map_err(FramingError::Native)
    }
}
