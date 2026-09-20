//! Shared local/network input grammar and checked cumulative work.
use super::{Frame, Kind};
use crate::contract::*;
pub struct InputState {
    id: u64,
    expected: u64,
    seen: u64,
    frames: usize,
    ended: bool,
}
impl InputState {
    pub fn new(id: u64, expected: u64) -> Self {
        Self {
            id,
            expected,
            seen: 0,
            frames: 0,
            ended: false,
        }
    }
    pub const fn ended(&self) -> bool {
        self.ended
    }
    pub fn accept(&mut self, f: &Frame) -> Result<(), Failure> {
        self.frames += 1;
        if self.ended || f.id != self.id || self.frames > MAX_FRAMES {
            return Err(Code::InvalidInput.into());
        }
        match f.kind {
            Kind::Body => {
                if f.bytes.is_empty() || f.bytes.len() > FRAME_BYTES {
                    return Err(Code::InvalidInput.into());
                }
                self.seen = self
                    .seen
                    .checked_add(f.bytes.len() as u64)
                    .filter(|n| *n <= self.expected)
                    .ok_or(Code::InvalidInput)?;
            }
            Kind::EndInput => {
                let total = u64::from_be_bytes(
                    f.bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| Code::InvalidInput)?,
                );
                if total != self.expected || self.seen != total {
                    return Err(Code::InvalidInput.into());
                }
                self.ended = true;
            }
            _ => return Err(Code::InvalidInput.into()),
        }
        Ok(())
    }
}
