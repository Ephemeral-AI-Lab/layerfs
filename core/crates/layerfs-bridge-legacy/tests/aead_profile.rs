//! The negotiated authenticated suite must match the backend this build compiled.
//!
//! AES-GCM is only selected where it is accelerated (x86_64 at runtime, aarch64
//! with the `aes_armv8`/`polyval_armv8` cfgs). An aarch64 build without them keeps
//! ChaCha20-Poly1305, because soft AES-GCM measures 0.111 GB/s against 1.17 GB/s.
use layerfs_bridge::adapters::native::connection::NOISE;

const AES_GCM: &str = "Noise_KK_25519_AESGCM_SHA256";
const CHACHA: &str = "Noise_KK_25519_ChaChaPoly_BLAKE2s";

#[test]
fn negotiated_suite_matches_the_compiled_backend() {
    assert!(
        NOISE == AES_GCM || NOISE == CHACHA,
        "unexpected authenticated suite: {NOISE}"
    );
    #[cfg(all(target_arch = "aarch64", not(all(aes_armv8, polyval_armv8))))]
    assert_eq!(
        NOISE, CHACHA,
        "an aarch64 build without the ARMv8 AES backend must not select AES-GCM"
    );
    #[cfg(any(
        target_arch = "x86_64",
        target_arch = "x86",
        all(target_arch = "aarch64", aes_armv8, polyval_armv8)
    ))]
    assert_eq!(NOISE, AES_GCM);
    println!("negotiated suite: {NOISE}");
}
