//! A Sandbox image's identity: the digest a deployment names, which is also
//! the name its unpacked root is cached under. It is a value type on every
//! platform, so a record written where the image backend runs can still be
//! read where it cannot.
use super::RuntimeError;

/// The prefix every image digest carries.
const ALGORITHM: &str = "sha256:";

/// A Sandbox image's identity: `sha256:<64 lowercase hex>`, the same text the
/// Docker backend's `deploy.image` holds and a docker-archive names its config
/// blob by.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Image(String);

impl Image {
    /// Parses `sha256:<64 lowercase hex>`, the only form an archive's config
    /// digest is compared with.
    pub fn parse(text: &str) -> Result<Self, RuntimeError> {
        let hex = text.strip_prefix(ALGORITHM).unwrap_or("");
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(RuntimeError::Protocol(
                "Sandbox image must be an exact digest, sha256:<64 lowercase hex>",
            ));
        }
        Ok(Self(text.to_owned()))
    }

    /// The whole digest text, `sha256:<64 hex>`.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The 64 hex digits without the algorithm: the name this image's
    /// unpacked root is cached under.
    pub fn hex(&self) -> &str {
        &self.0[ALGORITHM.len()..]
    }
}
