//! E1: per-core AEAD throughput with the exact crate versions in core/Cargo.lock.
//! Reuses buffers (pure crypto; no allocator or syscall effects).
use std::time::Instant;

const MIB: usize = 1024 * 1024;

fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

fn fill(buffer: &mut [u8], seed: u64) {
    let mut state = seed | 1;
    for chunk in buffer.chunks_exact_mut(8) {
        chunk.copy_from_slice(&xorshift(&mut state).to_le_bytes());
    }
}

fn keypair() -> snow::Keypair {
    let params: snow::params::NoiseParams = "Noise_KK_25519_ChaChaPoly_BLAKE2s".parse().unwrap();
    snow::Builder::new(params).generate_keypair().unwrap()
}

/// Runs a full KK handshake in memory and returns (initiator, responder) transports.
fn transports(suite: &str) -> (snow::StatelessTransportState, snow::StatelessTransportState) {
    let params: snow::params::NoiseParams = suite.parse().unwrap();
    let a = snow::Builder::new(params.clone()).generate_keypair().unwrap();
    let b = snow::Builder::new(params.clone()).generate_keypair().unwrap();
    let mut init = snow::Builder::new(params.clone())
        .local_private_key(&a.private)
        .remote_public_key(&b.public)
        .build_initiator()
        .unwrap();
    let mut resp = snow::Builder::new(params)
        .local_private_key(&b.private)
        .remote_public_key(&a.public)
        .build_responder()
        .unwrap();
    let mut m1 = [0u8; 1024];
    let mut m2 = [0u8; 1024];
    let mut out = [0u8; 1024];
    let n1 = init.write_message(&[], &mut m1).unwrap();
    assert_eq!(resp.read_message(&m1[..n1], &mut out).unwrap(), 0);
    let n2 = resp.write_message(&[], &mut m2).unwrap();
    assert_eq!(init.read_message(&m2[..n2], &mut out).unwrap(), 0);
    (
        init.into_stateless_transport_mode().unwrap(),
        resp.into_stateless_transport_mode().unwrap(),
    )
}

fn bench(suite: &str, message: usize, total: u64) {
    let (tx, rx) = transports(suite);
    let messages = (total / message as u64) as usize;
    let mut plain = vec![0u8; message];
    fill(&mut plain, 0x1234_5678_9abc_def0);
    // One ciphertext per message so the decrypt pass uses matching nonces.
    let mut stream = vec![0u8; messages * (message + 16)];
    let mut back = vec![0u8; message];

    let started = Instant::now();
    for i in 0..messages {
        let slot = &mut stream[i * (message + 16)..(i + 1) * (message + 16)];
        let n = tx.write_message(i as u64, &plain, slot).expect("encrypt");
        assert_eq!(n, message + 16);
    }
    let encrypt = started.elapsed();

    let started = Instant::now();
    for i in 0..messages {
        let slot = &stream[i * (message + 16)..(i + 1) * (message + 16)];
        let n = rx.read_message(i as u64, slot, &mut back).expect("decrypt");
        assert_eq!(n, message);
    }
    let decrypt = started.elapsed();
    assert_eq!(back[..16], plain[..16]);

    let gb = |d: std::time::Duration| (total as f64 / 1e9) / d.as_secs_f64();
    let us = |d: std::time::Duration| d.as_secs_f64() * 1e6 / messages as f64;
    println!(
        "{suite:42} msg={:>7} B  encrypt {:>6.3} GB/s ({:>7.2} us/msg)   decrypt {:>6.3} GB/s ({:>7.2} us/msg)",
        message, gb(encrypt), us(encrypt), gb(decrypt), us(decrypt)
    );
}

fn memcpy_control(message: usize, total: u64) {
    let messages = (total / message as u64) as usize;
    let src = vec![7u8; message];
    let mut dst = vec![0u8; message];
    let started = Instant::now();
    for _ in 0..messages {
        std::hint::black_box(&src);
        dst.copy_from_slice(&src);
        std::hint::black_box(&dst);
    }
    let d = started.elapsed();
    println!(
        "{:44} msg={:>7} B  copy    {:>7.3} GB/s",
        "memcpy control",
        message,
        (total as f64 / 1e9) / d.as_secs_f64()
    );
}

fn main() {
    println!(
        "arch={} os={} compile-time features: neon={} aes={} sha2={} avx2={}",
        std::env::consts::ARCH,
        std::env::consts::OS,
        cfg!(target_feature = "neon"),
        cfg!(target_feature = "aes"),
        cfg!(target_feature = "sha2"),
        cfg!(target_feature = "avx2")
    );
    let _ = keypair();
    let total: u64 = 256 * MIB as u64;
    for message in [4 * 1024usize, 16 * 1024usize, 60 * 1024usize] {
        bench("Noise_KK_25519_ChaChaPoly_BLAKE2s", message, total);
        bench("Noise_KK_25519_AESGCM_SHA256", message, total);
        memcpy_control(message, total);
    }
}
