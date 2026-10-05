//! Cooperative logical input capability, independent of a transport adapter.
use std::{
    io,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

/// Stable input which stops on the absolute deadline or cancellation.
/// Native pipe input enforces this boundary with poll.
pub trait Source: Send {
    fn read(
        &mut self,
        buffer: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize>;
}

impl Source for &[u8] {
    fn read(
        &mut self,
        buffer: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize> {
        if cancel.load(Ordering::Acquire) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if Instant::now() >= deadline {
            return Err(io::ErrorKind::TimedOut.into());
        }
        io::Read::read(self, buffer)
    }
}
