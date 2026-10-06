//! Independent nonce/record ownership per direction and shared quarantine.
use super::{io, ChannelError, ChannelResult, ChannelWork, VerifiedPeer, MAX_PLAINTEXT_BYTES};
use snow::StatelessTransportState;
use std::{
    net::{Shutdown, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

struct Shared {
    noise: StatelessTransportState,
    failed: AtomicBool,
}
impl Shared {
    fn ready(&self) -> ChannelResult<()> {
        if self.failed.load(Ordering::Acquire) {
            Err(ChannelError::Quarantined)
        } else {
            Ok(())
        }
    }
    fn fail(&self, stream: &TcpStream, error: ChannelError) -> ChannelError {
        if !self.failed.swap(true, Ordering::AcqRel) {
            if let Err(close) = stream.shutdown(Shutdown::Both) {
                return ChannelError::CloseFailed {
                    original: Box::new(error),
                    close,
                };
            }
        }
        error
    }
}
/// Authenticated duplex ownership. Each direction can move to its own I/O worker.
pub struct Connection {
    /// Single receive owner, with no clone/replay API.
    pub receive: Receiver,
    /// Single send owner, with no clone/replay API.
    pub send: Sender,
    /// Completed authenticated remote static identity.
    pub peer: VerifiedPeer,
    /// Handshake wire work, distinct from subsequent record counters.
    pub handshake_work: ChannelWork,
}
impl Connection {
    /// Creates an independent explicit fence owner before moving directions to
    /// blocking I/O workers. Shutdown wakes their socket calls; callers still
    /// join both workers and retain their original partial/error reports.
    pub fn close_handle(&self) -> ChannelResult<CloseHandle> {
        Ok(CloseHandle {
            stream: self.receive.stream.try_clone()?,
            shared: self.receive.shared.clone(),
        })
    }
    pub(super) fn new(
        stream: TcpStream,
        noise: StatelessTransportState,
        peer: VerifiedPeer,
        handshake_work: ChannelWork,
    ) -> ChannelResult<Self> {
        stream.set_nodelay(true)?;
        let sending = stream.try_clone()?;
        let shared = Arc::new(Shared {
            noise,
            failed: AtomicBool::new(false),
        });
        Ok(Self {
            receive: Receiver {
                stream,
                shared: shared.clone(),
                nonce: 0,
                sealed: Vec::with_capacity(u16::MAX as usize),
                plain: Vec::with_capacity(MAX_PLAINTEXT_BYTES),
                work: ChannelWork {
                    buffer_allocation_attempts: 2,
                    requested_buffer_bytes: (u16::MAX as usize + MAX_PLAINTEXT_BYTES) as u64,
                    ..Default::default()
                },
            },
            send: Sender {
                stream: sending,
                shared,
                nonce: 0,
                sealed: Vec::with_capacity(u16::MAX as usize),
                work: ChannelWork {
                    buffer_allocation_attempts: 1,
                    requested_buffer_bytes: u16::MAX as u64,
                    ..Default::default()
                },
            },
            peer,
            handshake_work,
        })
    }
}
/// Application-owned socket fence, independent of either I/O worker's borrow.
/// Dropping it does not close the channel or decide a runtime publication.
pub struct CloseHandle {
    stream: TcpStream,
    shared: Arc<Shared>,
}
impl CloseHandle {
    /// Explicitly quarantines and shuts down both directions once. A repeated
    /// close returns Quarantined and never repeats a failed shutdown attempt.
    pub fn close(&self) -> ChannelResult<()> {
        if self.shared.failed.swap(true, Ordering::AcqRel) {
            return Err(ChannelError::Quarantined);
        }
        self.stream.shutdown(Shutdown::Both).map_err(Into::into)
    }
}
/// Sender-owned encrypted scratch and monotonically increasing nonce.
pub struct Sender {
    stream: TcpStream,
    shared: Arc<Shared>,
    nonce: u64,
    sealed: Vec<u8>,
    work: ChannelWork,
}
impl Sender {
    /// Creates an explicit fence owner before transferring this direction to a
    /// blocking output worker. It carries no nonce or send/replay capability.
    pub fn close_handle(&self) -> ChannelResult<CloseHandle> {
        Ok(CloseHandle {
            stream: self.stream.try_clone()?,
            shared: self.shared.clone(),
        })
    }
    /// Sends one bounded plaintext record once; larger flows use multiple records.
    /// A failed attempt quarantines both directions, without inferred rollback.
    pub fn send(&mut self, plaintext: &[u8]) -> ChannelResult<()> {
        self.work.channel_calls = self.work.channel_calls.saturating_add(1);
        self.shared.ready()?;
        if plaintext.len() > MAX_PLAINTEXT_BYTES {
            return Err(ChannelError::RecordLimit);
        }
        let result = (|| {
            let next = self
                .nonce
                .checked_add(1)
                .ok_or(ChannelError::NonceExhausted)?;
            self.work.zeroed_bytes = self
                .work
                .zeroed_bytes
                .saturating_add((plaintext.len() + 16).saturating_sub(self.sealed.len()) as u64);
            self.sealed.resize(plaintext.len() + 16, 0);
            self.work.crypto_attempts = self.work.crypto_attempts.saturating_add(1);
            self.work.crypto_input_bytes = self
                .work
                .crypto_input_bytes
                .saturating_add(plaintext.len() as u64);
            let n = self
                .shared
                .noise
                .write_message(self.nonce, plaintext, &mut self.sealed)?;
            self.work.crypto_output_bytes = self.work.crypto_output_bytes.saturating_add(n as u64);
            io::write_record(&mut self.stream, &self.sealed[..n], &mut self.work)?;
            self.nonce = next;
            self.work.records = self.work.records.saturating_add(1);
            self.work.plaintext_bytes = self
                .work
                .plaintext_bytes
                .saturating_add(plaintext.len() as u64);
            Ok(())
        })();
        result.map_err(|error| self.shared.fail(&self.stream, error))
    }
    /// Actual completed/partial I/O work and current retained scratch.
    pub fn work(&self) -> ChannelWork {
        ChannelWork {
            sealed_capacity: self.sealed.capacity(),
            ..self.work
        }
    }
    /// Explicitly fences this channel's socket directions once.
    pub fn close(&self) -> ChannelResult<()> {
        if self.shared.failed.swap(true, Ordering::AcqRel) {
            return Err(ChannelError::Quarantined);
        }
        self.stream.shutdown(Shutdown::Both).map_err(Into::into)
    }
}
/// Receiver-owned ciphertext/plain scratch; replies borrow its current window.
pub struct Receiver {
    stream: TcpStream,
    shared: Arc<Shared>,
    nonce: u64,
    sealed: Vec<u8>,
    plain: Vec<u8>,
    work: ChannelWork,
}
impl Receiver {
    /// Receives/authenticates one record; no replay after read/crypto failure.
    pub fn receive(&mut self) -> ChannelResult<&[u8]> {
        self.work.channel_calls = self.work.channel_calls.saturating_add(1);
        self.shared.ready()?;
        let result = (|| {
            let next = self
                .nonce
                .checked_add(1)
                .ok_or(ChannelError::NonceExhausted)?;
            io::read_record(
                &mut self.stream,
                u16::MAX as usize,
                &mut self.sealed,
                &mut self.work,
            )?;
            self.work.zeroed_bytes = self
                .work
                .zeroed_bytes
                .saturating_add((self.sealed.len() - 16).saturating_sub(self.plain.len()) as u64);
            self.plain.resize(self.sealed.len() - 16, 0);
            self.work.crypto_attempts = self.work.crypto_attempts.saturating_add(1);
            self.work.crypto_input_bytes = self
                .work
                .crypto_input_bytes
                .saturating_add(self.sealed.len() as u64);
            let n = self
                .shared
                .noise
                .read_message(self.nonce, &self.sealed, &mut self.plain)?;
            self.work.crypto_output_bytes = self.work.crypto_output_bytes.saturating_add(n as u64);
            self.nonce = next;
            self.plain.truncate(n);
            self.work.records = self.work.records.saturating_add(1);
            self.work.plaintext_bytes = self.work.plaintext_bytes.saturating_add(n as u64);
            Ok(n)
        })();
        match result {
            Ok(n) => Ok(&self.plain[..n]),
            Err(error) => Err(self.shared.fail(&self.stream, error)),
        }
    }
    pub(super) fn last_record(&self) -> &[u8] {
        &self.plain
    }
    /// Explicitly fences both socket directions after a logical protocol refusal.
    /// Already quarantined ownership is not closed/retried a second time.
    pub fn close(&self) -> ChannelResult<()> {
        if self.shared.failed.swap(true, Ordering::AcqRel) {
            return Err(ChannelError::Quarantined);
        }
        self.stream.shutdown(Shutdown::Both).map_err(Into::into)
    }
    /// Actual completed/partial I/O work and current retained buffers.
    pub fn work(&self) -> ChannelWork {
        ChannelWork {
            sealed_capacity: self.sealed.capacity(),
            plain_capacity: self.plain.capacity(),
            ..self.work
        }
    }
}
