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
    handshake(stream, private, expected_peer, true)
}
/// Authenticates the expected initiator on one accepted socket.
pub fn accept(
    stream: TcpStream,
    private: &[u8; 32],
    expected_peer: [u8; 32],
) -> ChannelResult<Connection> {
    handshake(stream, private, expected_peer, false)
}
fn handshake(
    mut stream: TcpStream,
    private: &[u8; 32],
    expected: [u8; 32],
    initiator: bool,
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
    let mut work = ChannelWork::default();
    if initiator {
        let n = state.write_message(&[], &mut output)?;
        io::write_record(&mut stream, &output[..n], &mut work)?;
        io::read_record(&mut stream, 256, &mut input, &mut work)?;
        if state.read_message(&input, &mut payload)? != 0 {
            return Err(ChannelError::Invalid("handshake payload"));
        }
    } else {
        io::read_record(&mut stream, 256, &mut input, &mut work)?;
        if state.read_message(&input, &mut payload)? != 0 {
            return Err(ChannelError::Invalid("handshake payload"));
        }
        let n = state.write_message(&[], &mut output)?;
        io::write_record(&mut stream, &output[..n], &mut work)?;
    }
    if !state.is_handshake_finished() || state.get_remote_static() != Some(expected.as_slice()) {
        return Err(ChannelError::Invalid("authenticated peer"));
    }
    Connection::new(
        stream,
        state.into_stateless_transport_mode()?,
        VerifiedPeer(expected),
        work,
    )
}
