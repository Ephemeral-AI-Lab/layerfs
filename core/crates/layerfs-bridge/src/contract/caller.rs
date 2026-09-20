//! Authentication evidence shared by direct and transported operations.

/// A caller whose private-key possession was established by trusted entry code.
/// Request bytes and public-key bytes cannot construct this value.
#[derive(Clone, Copy, Debug)]
pub struct VerifiedPeer {
    public: [u8; 32],
}

impl VerifiedPeer {
    pub const fn public_key(&self) -> &[u8; 32] {
        &self.public
    }

    #[cfg(feature = "native")]
    pub(crate) const fn authenticated(public: [u8; 32]) -> Self {
        Self { public }
    }
}
