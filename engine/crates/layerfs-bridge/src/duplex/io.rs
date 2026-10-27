//! Complete partial byte progress, with no retry after an I/O error.
use super::{ChannelError, ChannelResult, ChannelWork};
use std::{
    io::{self, Read, Write},
    net::TcpStream,
};

pub(super) fn read(
    stream: &mut TcpStream,
    mut bytes: &mut [u8],
    work: &mut ChannelWork,
) -> ChannelResult<()> {
    while !bytes.is_empty() {
        work.io_attempts = work.io_attempts.saturating_add(1);
        let n = stream.read(bytes)?;
        if n == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
        }
        work.io_calls = work.io_calls.saturating_add(1);
        work.wire_bytes = work.wire_bytes.saturating_add(n as u64);
        bytes = &mut bytes[n..];
    }
    Ok(())
}
pub(super) fn write(
    stream: &mut TcpStream,
    mut bytes: &[u8],
    work: &mut ChannelWork,
) -> ChannelResult<()> {
    while !bytes.is_empty() {
        work.io_attempts = work.io_attempts.saturating_add(1);
        let n = stream.write(bytes)?;
        if n == 0 {
            return Err(io::Error::from(io::ErrorKind::WriteZero).into());
        }
        work.io_calls = work.io_calls.saturating_add(1);
        work.wire_bytes = work.wire_bytes.saturating_add(n as u64);
        bytes = &bytes[n..];
    }
    Ok(())
}
pub(super) fn read_record(
    stream: &mut TcpStream,
    limit: usize,
    sealed: &mut Vec<u8>,
    work: &mut ChannelWork,
) -> ChannelResult<()> {
    work.record_io_attempts = work.record_io_attempts.saturating_add(1);
    work.zeroed_bytes = work.zeroed_bytes.saturating_add(2);
    let mut header = [0; 2];
    read(stream, &mut header, work)?;
    let length = usize::from(u16::from_be_bytes(header));
    if length < 16 || length > limit {
        return Err(ChannelError::Invalid("encrypted record length"));
    }
    work.zeroed_bytes = work
        .zeroed_bytes
        .saturating_add(length.saturating_sub(sealed.len()) as u64);
    sealed.resize(length, 0);
    read(stream, sealed, work)
}
pub(super) fn write_record(
    stream: &mut TcpStream,
    sealed: &[u8],
    work: &mut ChannelWork,
) -> ChannelResult<()> {
    work.record_io_attempts = work.record_io_attempts.saturating_add(1);
    let length = u16::try_from(sealed.len()).map_err(|_| ChannelError::RecordLimit)?;
    write(stream, &length.to_be_bytes(), work)?;
    write(stream, sealed, work)
}
