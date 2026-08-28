use super::super::contract::{EvalResult, BUFFER_BYTES};
use super::super::error::{display_error, io_error};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

pub(in crate::stage1_materialize) fn ascii_argument<'a>(
    value: &'a OsStr,
    name: &str,
) -> EvalResult<&'a str> {
    value
        .to_str()
        .filter(|value| {
            !value.is_empty()
                && value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
        .ok_or_else(|| format!("{name} must be a nonempty ASCII identifier"))
}

pub(in crate::stage1_materialize) fn append_manifest_line(
    output: &mut Vec<u8>,
    path: &str,
    bytes: usize,
    sha256: &str,
    blake3: &str,
) {
    output.extend_from_slice(path.as_bytes());
    output.push(0);
    output.extend_from_slice(bytes.to_string().as_bytes());
    output.push(0);
    output.extend_from_slice(sha256.as_bytes());
    output.push(0);
    output.extend_from_slice(blake3.as_bytes());
    output.push(b'\n');
}

pub(in crate::stage1_materialize) fn is_product_source(path: &str) -> bool {
    path == "Cargo.toml"
        || path == "Cargo.lock"
        || [
            "crates/layerfs-core/",
            "crates/layerfs-storage/",
            "crates/layerfs-working-store/",
            "crates/layerfs-durable-store/",
            "crates/layerfs-sync/",
            "crates/layerfs-workspace/",
            "crates/layerfs-mount/",
            "crates/layerfs-materialization/",
            "crates/layerfs-sdk/",
            "crates/layerfs-service/",
        ]
        .iter()
        .any(|prefix| path.starts_with(prefix))
}

pub(in crate::stage1_materialize) fn sha256_bytes(bytes: &[u8]) -> EvalResult<String> {
    let mut child = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(io_error)?;
    child
        .stdin
        .take()
        .ok_or_else(|| "shasum stdin unavailable".to_owned())?
        .write_all(bytes)
        .map_err(io_error)?;
    let output = child.wait_with_output().map_err(io_error)?;
    if !output.status.success() {
        return Err("shasum failed".to_owned());
    }
    String::from_utf8(output.stdout)
        .map_err(display_error)?
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .ok_or_else(|| "shasum returned no digest".to_owned())
}

pub(in crate::stage1_materialize) fn sha256_file(path: &Path) -> EvalResult<String> {
    let output = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err("shasum failed".to_owned());
    }
    String::from_utf8(output.stdout)
        .map_err(display_error)?
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .ok_or_else(|| "shasum returned no digest".to_owned())
}

pub(in crate::stage1_materialize) fn command_version(command: &str) -> EvalResult<String> {
    let output = Command::new(command)
        .arg("--version")
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(format!("{command} --version failed"));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(display_error)
}

pub(in crate::stage1_materialize) fn digest_file(path: &Path) -> EvalResult<String> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    loop {
        let count = file.read(&mut buffer).map_err(io_error)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
