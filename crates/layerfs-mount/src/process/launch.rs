use super::encoding::hex_digit;
use layerfs_workspace::BranchId;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

pub(super) fn parse_arguments() -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    let values = std::env::args().skip(1).collect::<Vec<_>>();
    if values.len() % 2 != 0 {
        return Err(usage().into());
    }
    let mut arguments = HashMap::new();
    for pair in values.chunks_exact(2) {
        let key = pair[0].strip_prefix("--").ok_or_else(usage)?;
        if arguments.insert(key.to_owned(), pair[1].clone()).is_some() {
            return Err(format!("duplicate --{key}").into());
        }
    }
    Ok(arguments)
}

fn usage() -> String {
    "usage: layerfs-mount --store WORKING_ROOT --branch 64_HEX --mount PATH --receipt PATH --integrity trusted|verified [--uid N] [--gid N] [--control-request PATH --control-receipt PATH]".to_owned()
}

pub(super) fn prepare_public_mount(
    mount: &Path,
    working: &Path,
    receipt: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    std::fs::create_dir_all(mount)?;
    let mount = std::fs::canonicalize(mount)?;
    if std::fs::read_dir(&mount)?.next().is_some()
        || mount.starts_with(working)
        || receipt.starts_with(&mount)
    {
        return Err("published mount must be empty and outside WorkingStore/receipt".into());
    }
    Ok(mount)
}

pub(super) fn move_mount(from: &Path, to: &Path) -> Result<(), String> {
    let output = std::process::Command::new("mount")
        .arg("--move")
        .arg(from)
        .arg(to)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

pub(super) fn branch_id(value: &str) -> Result<BranchId, Box<dyn std::error::Error>> {
    if value.len() != 64 {
        return Err("BranchId must contain exactly 64 hexadecimal characters".into());
    }
    let mut bytes = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        bytes[index] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    Ok(BranchId::from_bytes(bytes))
}

pub(super) fn required_path(
    arguments: &HashMap<String, String>,
    name: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    arguments
        .get(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing --{name}").into())
}

pub(super) fn number(
    arguments: &HashMap<String, String>,
    name: &str,
    default: u32,
) -> Result<u32, Box<dyn std::error::Error>> {
    arguments
        .get(name)
        .map(|value| value.parse().map_err(Into::into))
        .unwrap_or(Ok(default))
}

pub(super) fn hash_file(path: &Path) -> Result<[u8; 32], Box<dyn std::error::Error>> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut hasher = blake3::Hasher::new();
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(*hasher.finalize().as_bytes());
        }
        hasher.update(&buffer[..read]);
    }
}
