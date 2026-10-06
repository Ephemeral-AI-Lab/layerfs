//! Pinned architecture-specific Noise/AEAD profile; no runtime suite fallback.
use super::{ChannelError, ChannelResult};

#[cfg(all(target_arch = "aarch64", not(all(aes_armv8, polyval_armv8)), not(doc)))]
compile_error!("aarch64 requires root ARMv8 AEAD flags: --cfg aes_armv8 --cfg polyval_armv8 --cfg chacha20_force_neon -C target-feature=+aes,+sha2");

/// Negotiated suite, selected by the target architecture and required build inputs.
#[cfg(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64"))]
pub const NOISE: &str = "Noise_KK_25519_AESGCM_SHA256";
/// Negotiated suite for other target architectures.
#[cfg(not(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64")))]
pub const NOISE: &str = "Noise_KK_25519_ChaChaPoly_BLAKE2s";
/// Fixed plaintext processing window: Noise's u16 record minus its authentication tag.
pub const MAX_PLAINTEXT_BYTES: usize = crate::contract::MAX_RECORD_BYTES;
pub(super) const PROLOGUE: &[u8] = b"layerfs/cluster-two/native-channel/v1\0";

/// Application-owned static keys; provisioning/rotation remain application work.
pub struct Keypair {
    /// Secret static Curve25519 key. Do not expose it to untrusted callers.
    pub private: [u8; 32],
    /// Public key which authenticates this endpoint to its peer.
    pub public: [u8; 32],
}
/// Uses the pinned resolver's operating-system randomness once to generate keys.
pub fn generate_keypair() -> ChannelResult<Keypair> {
    let pair = snow::Builder::new(NOISE.parse()?).generate_keypair()?;
    Ok(Keypair {
        private: pair
            .private
            .try_into()
            .map_err(|_| ChannelError::Invalid("private key width"))?,
        public: pair
            .public
            .try_into()
            .map_err(|_| ChannelError::Invalid("public key width"))?,
    })
}
/// Derives a provisioning public key; it does not create a VerifiedPeer.
pub fn public_key(private: &[u8; 32]) -> ChannelResult<[u8; 32]> {
    use snow::resolvers::CryptoResolver;
    let mut dh = snow::resolvers::DefaultResolver
        .resolve_dh(&snow::params::DHChoice::Curve25519)
        .ok_or(ChannelError::Invalid("Curve25519 resolver"))?;
    dh.set(private);
    dh.pubkey()
        .try_into()
        .map_err(|_| ChannelError::Invalid("public key width"))
}
