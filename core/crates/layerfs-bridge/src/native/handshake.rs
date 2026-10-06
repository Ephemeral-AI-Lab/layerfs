//! Bounded KK authentication on one supplied native socket; no connection replay.
use super::{
    channel::Connection,
    error::ChannelResult,
    io,
    profile::{NOISE, PROLOGUE},
    types::{ChannelWork, VerifiedPeer},
    ChannelError,
};
use std::net::TcpStream;

/// Authenticates the expected responder on an already connected socket.
pub fn initiate(
    stream: TcpStream,
    private: &[u8; 32],
    expected_peer: [u8; 32],
) -> ChannelResult<Connection> {
    initiate_observed(stream, private, expected_peer).result
}
/// Authenticates the expected initiator on one accepted socket.
pub fn accept(
    stream: TcpStream,
    private: &[u8; 32],
    expected_peer: [u8; 32],
) -> ChannelResult<Connection> {
    accept_observed(stream, private, expected_peer).result
}
/// Original handshake success/failure and work completed before its stopping point.
/// Construction of the successful direction buffers is observed by their owners.
pub struct ChannelStart {
    /// Exact original native result, without reconnect/replay or error substitution.
    pub result: ChannelResult<Connection>,
    /// Actual handshake I/O/crypto/scratch work, including original failed attempts.
    pub work: ChannelWork,
}
/// Initiates once and retains handshake work even when authentication/I/O fails.
pub fn initiate_observed(
    stream: TcpStream,
    private: &[u8; 32],
    expected_peer: [u8; 32],
) -> ChannelStart {
    observed(stream, private, expected_peer, true)
}
/// Accepts once and retains handshake work even when authentication/I/O fails.
pub fn accept_observed(
    stream: TcpStream,
    private: &[u8; 32],
    expected_peer: [u8; 32],
) -> ChannelStart {
    observed(stream, private, expected_peer, false)
}
fn observed(
    stream: TcpStream,
    private: &[u8; 32],
    expected: [u8; 32],
    initiator: bool,
) -> ChannelStart {
    let mut work = ChannelWork::default();
    let result = handshake(stream, private, expected, initiator, &mut work);
    ChannelStart { result, work }
}
fn handshake(
    mut stream: TcpStream,
    private: &[u8; 32],
    expected: [u8; 32],
    initiator: bool,
    work: &mut ChannelWork,
) -> ChannelResult<Connection> {
    let builder = snow::Builder::new(NOISE.parse()?)
        .local_private_key(private)
        .remote_public_key(&expected)
        .prologue(PROLOGUE);
    let mut state = if initiator {
        builder.build_initiator()?
    } else {
        builder.build_responder()?
    };
    let mut output = [0; 256];
    let mut input = Vec::with_capacity(256);
    let mut payload = [0; 256];
    *work = ChannelWork {
        buffer_allocation_attempts: 1,
        requested_buffer_bytes: 256,
        stack_window_bytes: 512,
        zeroed_bytes: 512,
        ..Default::default()
    };
    if initiator {
        work.crypto_attempts += 1;
        let n = state.write_message(&[], &mut output)?;
        work.crypto_output_bytes += n as u64;
        io::write_record(&mut stream, &output[..n], work)?;
        io::read_record(&mut stream, 256, &mut input, work)?;
        work.crypto_attempts += 1;
        work.crypto_input_bytes += input.len() as u64;
        let read = state.read_message(&input, &mut payload)?;
        work.crypto_output_bytes += read as u64;
        if read != 0 {
            return Err(ChannelError::Invalid("handshake payload"));
        }
    } else {
        io::read_record(&mut stream, 256, &mut input, work)?;
        work.crypto_attempts += 1;
        work.crypto_input_bytes += input.len() as u64;
        let read = state.read_message(&input, &mut payload)?;
        work.crypto_output_bytes += read as u64;
        if read != 0 {
            return Err(ChannelError::Invalid("handshake payload"));
        }
        work.crypto_attempts += 1;
        let n = state.write_message(&[], &mut output)?;
        work.crypto_output_bytes += n as u64;
        io::write_record(&mut stream, &output[..n], work)?;
    }
    if !state.is_handshake_finished() || state.get_remote_static() != Some(expected.as_slice()) {
        return Err(ChannelError::Invalid("authenticated peer"));
    }
    Connection::new(
        stream,
        state.into_stateless_transport_mode()?,
        VerifiedPeer(expected),
        *work,
    )
}
