//! A docker-archive as a bundle would carry it, built in the test: the format
//! `docker save` writes (`manifest.json`, a config blob named by the image's
//! digest, uncompressed layer tars). Written by hand so a test needs no
//! Engine, no Docker and no root.
use std::{
    fs,
    path::{Path, PathBuf},
};

/// The digest the fixtures' images use unless a test needs another.
pub const DIGEST: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
/// A second digest, for one cache that holds two images.
pub const OTHER: &str = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

/// The 64 hex digits of a digest.
pub fn hex(digest: &str) -> &str {
    digest.strip_prefix("sha256:").expect("a sha256 digest")
}

/// One tar archive, built entry by entry.
pub struct Tar {
    bytes: Vec<u8>,
}

impl Tar {
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    /// A regular file with `mode` and `body`.
    pub fn file(self, path: &str, mode: u32, body: &[u8]) -> Self {
        self.entry(path, mode, b'0', body, "")
    }

    /// A directory entry; `path` ends with `/` as tar writes it.
    pub fn directory(self, path: &str, mode: u32) -> Self {
        self.entry(path, mode, b'5', &[], "")
    }

    pub fn symlink(self, path: &str, target: &str) -> Self {
        self.entry(path, 0o777, b'2', &[], target)
    }

    pub fn hard_link(self, path: &str, target: &str) -> Self {
        self.entry(path, 0o644, b'1', &[], target)
    }

    /// Any entry type the writer must be able to produce, including the ones
    /// a Sandbox root refuses (a device node is type `3`).
    pub fn raw(self, path: &str, kind: u8, mode: u32, body: &[u8]) -> Self {
        self.entry(path, mode, kind, body, "")
    }

    fn entry(mut self, path: &str, mode: u32, kind: u8, body: &[u8], link: &str) -> Self {
        assert!(
            path.len() < 100 && link.len() < 100,
            "{path} or {link} needs a pax header"
        );
        let mut header = [0u8; 512];
        header[..path.len()].copy_from_slice(path.as_bytes());
        octal(&mut header[100..108], u64::from(mode));
        octal(&mut header[108..116], 0);
        octal(&mut header[116..124], 0);
        octal(&mut header[124..136], body.len() as u64);
        octal(&mut header[136..148], 0);
        header[156] = kind;
        header[157..157 + link.len()].copy_from_slice(link.as_bytes());
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[148..156].fill(b' ');
        let sum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
        header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        self.bytes.extend_from_slice(&header);
        self.bytes.extend_from_slice(body);
        let padding = (512 - body.len() % 512) % 512;
        self.bytes.resize(self.bytes.len() + padding, 0);
        self
    }

    /// The archive's bytes, ending with the two zero blocks that end a tar.
    pub fn finish(mut self) -> Vec<u8> {
        self.bytes.resize(self.bytes.len() + 1024, 0);
        self.bytes
    }
}

fn octal(field: &mut [u8], value: u64) {
    let text = format!("{value:0width$o}\0", width = field.len() - 1);
    field.copy_from_slice(text.as_bytes());
}

/// A docker-archive at `<directory>/<name>.tar` holding one image whose config
/// digest is `digest`, with `layers` applied in the order given.
pub fn archive(directory: &Path, name: &str, digest: &str, layers: Vec<Vec<u8>>) -> PathBuf {
    write(
        directory,
        name,
        &format!("blobs/sha256/{}", hex(digest)),
        layers,
    )
}

/// The same image as older writers name it: the config blob is `<hex>.json`.
pub fn legacy_archive(directory: &Path, name: &str, digest: &str, layers: Vec<Vec<u8>>) -> PathBuf {
    write(directory, name, &format!("{}.json", hex(digest)), layers)
}

fn write(directory: &Path, name: &str, config: &str, layers: Vec<Vec<u8>>) -> PathBuf {
    let mut manifest =
        format!("[{{\"Config\":\"{config}\",\"RepoTags\":[\"layerfs-test:latest\"],\"Layers\":[");
    for index in 0..layers.len() {
        if index > 0 {
            manifest.push(',');
        }
        manifest.push_str(&format!("\"{index}/layer.tar\""));
    }
    manifest.push_str("]}]");
    let mut tar = Tar::new()
        .file("manifest.json", 0o644, manifest.as_bytes())
        .file(config, 0o644, br#"{"architecture":"amd64","os":"linux"}"#);
    for (index, layer) in layers.iter().enumerate() {
        tar = tar.file(&format!("{index}/layer.tar"), 0o644, layer);
    }
    let path = directory.join(format!("{name}.tar"));
    fs::write(&path, tar.finish()).unwrap();
    path
}

/// A scratch directory one test owns and removes when it ends.
pub struct Scratch {
    pub base: PathBuf,
    pub cache: PathBuf,
}

impl Scratch {
    pub fn new(name: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("layerfs-rootfs-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let cache = base.join("images");
        Self { base, cache }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}
