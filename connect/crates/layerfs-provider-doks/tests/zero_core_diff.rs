//! #554 acceptance 6: adding the DOKS provider changes no core file.
//!
//! The fixture `tests/fixtures/core-files.sha256` is the SHA-256 of every
//! file under the core source folders, taken at the adapter's base commit
//! (`feat/551-b0@b5c5335b5`).  The test recomputes them; a changed, added or
//! removed core file fails, and so does a core file that names `doks` or
//! `digitalocean`.
#![cfg(unix)]

use std::path::{Path, PathBuf};

/// The folders that must not change for a new provider adapter; the storage
/// crates are included because they are core too.
const CORE: &[&str] = &[
    "api/crates/layerfs-control",
    "api/crates/layerfs-api",
    "connect/crates/layerfs-machine",
    "connect/crates/layerfs-sandbox",
    "engine/crates/layerfs-bridge",
    "core/crates",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("the crate is connect/crates/layerfs-provider-doks")
        .to_path_buf()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("a core folder") {
        let entry = entry.expect("a core entry");
        let path = entry.path();
        if entry.file_type().expect("a core type").is_dir() {
            collect(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn current() -> Vec<(String, String)> {
    let root = repo_root();
    let mut paths = Vec::new();
    for core in CORE {
        collect(&root.join(core), &mut paths);
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let relative = path
                .strip_prefix(&root)
                .expect("a path under the repository")
                .to_string_lossy()
                .replace('\\', "/");
            let hash = sha256(&std::fs::read(&path).expect("a readable core file"));
            (relative, hash)
        })
        .collect()
}

fn manifest() -> Vec<(String, String)> {
    include_str!("fixtures/core-files.sha256")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (hash, path) = line.split_once("  ").expect("a hash and a path");
            (path.to_owned(), hash.to_owned())
        })
        .collect()
}

#[test]
fn core_files_match_the_base_manifest() {
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let expected: std::collections::BTreeMap<String, String> = manifest().into_iter().collect();
    let got: std::collections::BTreeMap<String, String> = current().into_iter().collect();
    let mut differences = Vec::new();
    for (path, hash) in &expected {
        match got.get(path) {
            None => differences.push(format!("{path}: missing")),
            Some(found) if found != hash => {
                differences.push(format!("{path}: {hash} -> {found}"));
            }
            Some(_) => {}
        }
    }
    for path in got.keys() {
        if !expected.contains_key(path) {
            differences.push(format!("{path}: added"));
        }
    }
    assert!(
        differences.is_empty(),
        "core files changed since the adapter's base (feat/551-b0@b5c5335b5):\n{}",
        differences.join("\n")
    );
}

#[test]
fn no_core_file_names_the_provider() {
    let root = repo_root();
    for (relative, _) in current() {
        let text = std::fs::read_to_string(root.join(&relative)).unwrap_or_default();
        let lower = text.to_ascii_lowercase();
        assert!(!lower.contains("doks"), "{relative} names the provider");
        assert!(
            !lower.contains("digitalocean"),
            "{relative} names the provider"
        );
    }
}

#[test]
fn the_adapter_lives_outside_every_core_folder() {
    let root = repo_root();
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(manifest.starts_with(root.join("connect/crates/layerfs-provider-doks")));
    for core in CORE {
        assert!(!manifest.starts_with(root.join(core)), "{core}");
    }
}

fn sha256(input: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in message.chunks(64) {
        let mut words = [0u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let at = index * 4;
            *word = u32::from_be_bytes([chunk[at], chunk[at + 1], chunk[at + 2], chunk[at + 3]]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }
        let mut a = state[0];
        let mut b = state[1];
        let mut c = state[2];
        let mut d = state[3];
        let mut e = state[4];
        let mut f = state[5];
        let mut g = state[6];
        let mut h = state[7];
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
        state[5] = state[5].wrapping_add(f);
        state[6] = state[6].wrapping_add(g);
        state[7] = state[7].wrapping_add(h);
    }
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(64);
    for word in state {
        write!(hex, "{word:08x}").expect("writing to a String cannot fail");
    }
    hex
}
