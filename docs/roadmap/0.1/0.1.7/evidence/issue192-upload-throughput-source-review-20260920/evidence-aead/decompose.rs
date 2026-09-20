//! Raw-crate decomposition: cipher vs universal hash vs snow wrapper overhead.
use aes::cipher::BlockEncrypt;
use chacha20::cipher::{KeyIvInit, StreamCipher};
use std::time::Instant;

const MIB: usize = 1024 * 1024;
type Block16 = aes::cipher::generic_array::GenericArray<u8, aes::cipher::consts::U16>;

fn bench(label: &str, size: usize, total: u64, mut body: impl FnMut(&[u8])) {
    let data = vec![0x5au8; size];
    let messages = (total / size as u64) as usize;
    let started = Instant::now();
    for _ in 0..messages {
        body(&data);
    }
    let d = started.elapsed();
    println!(
        "{label:34} size={size:>6} B  {:>7.3} GB/s  ({:>8.2} us/msg)",
        (total as f64 / 1e9) / d.as_secs_f64(),
        d.as_secs_f64() * 1e6 / messages as f64
    );
}

fn main() {
    println!(
        "arch={} features: neon={} aes={} sha2={}",
        std::env::consts::ARCH,
        cfg!(target_feature = "neon"),
        cfg!(target_feature = "aes"),
        cfg!(target_feature = "sha2")
    );
    let total: u64 = 256 * MIB as u64;
    for size in [16 * 1024usize, 60 * 1024usize] {
        // chacha20 keystream only
        let mut c = chacha20::ChaCha20::new(&[7u8; 32].into(), &[3u8; 12].into());
        let mut out = vec![0u8; size];
        bench("chacha20 keystream (NEON?)", size, total, |data| {
            out.copy_from_slice(data);
            c.apply_keystream(&mut out);
        });
        // poly1305 only
        use poly1305::universal_hash::{KeyInit as _, UniversalHash};
        let key = poly1305::Key::from_slice(&[9u8; 32]).to_owned();
        let mut mac = poly1305::Poly1305::new(&key);
        bench("poly1305 (soft on aarch64)", size, total, |data| {
            mac.update_padded(data);
            let _ = mac.clone().finalize();
        });
        // chacha20poly1305 AEAD directly
        use chacha20poly1305::aead::{AeadInPlace, KeyInit as _};
        let aead = chacha20poly1305::ChaCha20Poly1305::new(&[1u8; 32].into());
        let mut buf = vec![0u8; size + 16];
        bench("chacha20poly1305 in place", size, total, |data| {
            buf[..size].copy_from_slice(data);
            let _ = aead.encrypt_in_place_detached(&[2u8; 12].into(), b"", &mut buf[..size]);
        });
        // aes-gcm AEAD directly
        let gcm = aes_gcm::Aes256Gcm::new(&[4u8; 32].into());
        let mut buf2 = vec![0u8; size + 16];
        bench("aes256gcm in place (armv8?)", size, total, |data| {
            buf2[..size].copy_from_slice(data);
            let _ = gcm.encrypt_in_place_detached(&[5u8; 12].into(), b"", &mut buf2[..size]);
        });
        // raw AES rounds
        let aes = aes::Aes128::new(&[6u8; 16].into());
        let mut block = Block16::default();
        let mut blocks: Vec<Block16> = vec![Block16::default(); size / 16];
        bench("aes128 ecb blocks (armv8?)", size, total, |_| {
            aes.encrypt_blocks(&mut blocks);
            block = blocks[0];
            std::hint::black_box(block);
        });
        println!();
    }
}
