//! Exact-length stream adapters; a valid END_INPUT is the only logical EOF.
use super::{
    connection::{Receiver, Sender},
    protocol::{Frame, InputState, Kind},
};
use crate::contract::*;
use std::io::{self, Read, Write};
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
}
impl<'a> Output<'a> {
    pub fn new(send: &'a mut Sender, id: u64, maximum: u64) -> Self {
        Self {
            send,
            id,
            maximum,
            bytes: 0,
            frames: 0,
        }
    }
}
impl Write for Output<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        for part in b.chunks(FRAME_BYTES) {
            self.bytes = self
                .bytes
                .checked_add(part.len() as u64)
                .filter(|n| *n <= self.maximum)
                .ok_or_else(|| io::Error::other("response bound"))?;
            self.frames += 1;
            if self.frames >= frame_budget(self.maximum) {
                return Err(io::Error::other("response frame bound"));
            }
            self.send
                .write(&Frame {
                    kind: Kind::ResultData,
                    id: self.id,
                    bytes: part.to_vec(),
                })
                .map_err(io::Error::other)?;
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
