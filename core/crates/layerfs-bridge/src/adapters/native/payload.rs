//! Exact-length stream adapters; a valid END_INPUT is the only logical EOF.
use super::{
    connection::{Receiver, Sender},
    protocol::{Frame, InputState, Kind},
};
use crate::contract::*;
use std::{
    io::{self, Read, Write},
    time::Instant,
};
// Match the existing frame budget's byte quantum without changing wire limits.
pub(super) const COALESCE_BYTES: usize = 1024;
pub struct Input<'a> {
    receive: &'a mut Receiver,
    state: InputState,
    buffer: Vec<u8>,
    offset: usize,
}
impl<'a> Input<'a> {
    pub fn new(receive: &'a mut Receiver, id: u64, expected: u64) -> Self {
        Self {
            receive,
            state: InputState::new(id, expected),
            buffer: Vec::new(),
            offset: 0,
        }
    }
    pub fn complete(&self) -> bool {
        self.state.ended()
    }
}
impl Read for Input<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.state.ended() {
            return Ok(0);
        }
        if self.offset == self.buffer.len() {
            let frame = self.receive.read().map_err(io::Error::other)?;
            self.state.accept(&frame).map_err(io::Error::other)?;
            if self.state.ended() {
                return Ok(0);
            }
            self.buffer = frame.bytes;
            self.offset = 0;
        }
        let n = out.len().min(self.buffer.len() - self.offset);
        out[..n].copy_from_slice(&self.buffer[self.offset..self.offset + n]);
        self.offset += n;
        Ok(n)
    }
}
pub struct Output<'a> {
    send: &'a mut Sender,
    id: u64,
    maximum: u64,
    pub bytes: u64,
    frames: u64,
    deadline: Instant,
    pending: [u8; COALESCE_BYTES],
    pending_len: usize,
    failed: bool,
}
impl<'a> Output<'a> {
    pub fn new(send: &'a mut Sender, id: u64, maximum: u64, deadline: Instant) -> Self {
        Self {
            send,
            id,
            maximum,
            bytes: 0,
            frames: 0,
            deadline,
            pending: [0; COALESCE_BYTES],
            pending_len: 0,
            failed: false,
        }
    }
    fn ready(&mut self) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("previous response output failure"));
        }
        if Instant::now() >= self.deadline {
            self.failed = true;
            return Err(io::ErrorKind::TimedOut.into());
        }
        Ok(())
    }
    fn send_part(&mut self, bytes: Vec<u8>) -> io::Result<()> {
        self.frames += 1;
        if self.frames >= frame_budget(self.maximum) {
            return Err(io::Error::other("response frame bound"));
        }
        self.send
            .write(&Frame {
                kind: Kind::ResultData,
                id: self.id,
                bytes,
            })
            .map_err(io::Error::other)
    }
    fn send_pending(&mut self) -> io::Result<()> {
        if self.pending_len == 0 {
            return Ok(());
        }
        let bytes = self.pending[..self.pending_len].to_vec();
        self.pending_len = 0;
        self.send_part(bytes)
    }
    fn write_parts(&mut self, mut bytes: &[u8]) -> io::Result<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .filter(|n| *n <= self.maximum)
            .ok_or_else(|| io::Error::other("response bound"))?;
        while !bytes.is_empty() {
            if self.pending_len == 0 && bytes.len() >= COALESCE_BYTES {
                let length = bytes.len().min(FRAME_BYTES);
                self.send_part(bytes[..length].to_vec())?;
                bytes = &bytes[length..];
            } else {
                let length = bytes.len().min(COALESCE_BYTES - self.pending_len);
                self.pending[self.pending_len..self.pending_len + length]
                    .copy_from_slice(&bytes[..length]);
                self.pending_len += length;
                bytes = &bytes[length..];
                if self.pending_len == COALESCE_BYTES {
                    self.send_pending()?;
                }
            }
        }
        Ok(())
    }
}
impl Write for Output<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.ready()?;
        let result = self.write_parts(b).map(|()| b.len());
        self.failed = result.is_err();
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        self.ready()?;
        let result = self
            .send_pending()
            .and_then(|()| self.send.flush().map_err(io::Error::other));
        self.failed = result.is_err();
        result
    }
}
