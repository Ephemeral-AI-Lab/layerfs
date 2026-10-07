//! One-attempt authenticated duplex channels with bounded borrowed records.
mod channel;
mod ending;
mod error;
mod handshake;
mod io;
mod profile;
mod types;

pub use channel::{CloseHandle, Connection, Receiver, Sender};
pub use ending::{PeerEndFailure, PeerEndPhase};
pub use error::{ChannelError, ChannelResult};
pub use handshake::{accept, accept_observed, initiate, initiate_observed, ChannelStart};
pub use profile::{generate_keypair, public_key, Keypair, MAX_PLAINTEXT_BYTES, NOISE};
pub use types::{ChannelWork, VerifiedPeer};
