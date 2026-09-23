//! Validated sequential input remains incomplete until exact EOF.
use std::{io::Read, time::Instant};
pub(crate) struct Exact<'a> {
    inner: &'a mut dyn Read,
    remaining: u64,
    deadline: Instant,
}
impl Read for Exact<'_> {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        if b.is_empty() {
            return Ok(0);
        }
        if Instant::now() >= self.deadline {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        let n = self.inner.read(b)?;
        if n as u64 > self.remaining || (n == 0 && self.remaining != 0) {
            return Err(std::io::Error::other("exact input length"));
        }
        self.remaining -= n as u64;
        Ok(n)
    }
}

impl<'a> Exact<'a> {
    pub(crate) fn new(inner: &'a mut dyn Read, remaining: u64, deadline: Instant) -> Self {
        Self {
            inner,
            remaining,
            deadline,
        }
    }
}
