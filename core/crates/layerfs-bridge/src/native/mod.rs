//! One-attempt authenticated duplex channels with bounded borrowed records.
mod channel;
mod error;
mod handshake;
mod io;
mod profile;
mod types;

pub use channel::{Connection, Receiver, Sender};
pub use error::{ChannelError, ChannelResult};
pub use handshake::{accept, initiate};
pub use profile::{generate_keypair, public_key, Keypair, MAX_PLAINTEXT_BYTES, NOISE};
pub use types::{ChannelWork, VerifiedPeer};
