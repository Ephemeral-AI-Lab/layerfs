//! One-attempt authenticated duplex channels with bounded borrowed records.
pub mod admission;
mod channel;
mod ending;
mod error;
mod handshake;
mod io;
pub(crate) mod peer;
mod profile;

pub use channel::{CloseHandle, Connection, Receiver, Sender};
pub use ending::{PeerEndFailure, PeerEndPhase};
pub use error::{ChannelError, ChannelResult};
pub use handshake::{accept, accept_observed, initiate, initiate_observed, ChannelStart};
pub use peer::{ChannelWork, VerifiedPeer};
pub use profile::{generate_keypair, public_key, Keypair, MAX_PLAINTEXT_BYTES, NOISE};
