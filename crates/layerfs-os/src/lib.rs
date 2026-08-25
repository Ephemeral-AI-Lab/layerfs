//! Host filesystem observations used by the Phase 0 evaluation harness.
//!
//! This crate deliberately exposes observations, not LayerFS identity or
//! projection policy. Platform-specific behavior stays behind this boundary.

#![deny(unsafe_code)]

#[cfg(target_os = "macos")]
pub mod apple;

#[cfg(target_os = "macos")]
pub type HostDriver = apple::AppleDriver;

#[cfg(not(target_os = "macos"))]
#[derive(Default)]
pub struct HostDriver;

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(not(target_os = "macos"))]
impl layerfs_vfs::driver::ProjectionDriver for HostDriver {
    fn open_workspace(
        &self,
        _path: &Path,
        _policy: layerfs_vfs::driver::WorkspacePolicy,
        _store_id: [u8; 32],
    ) -> layerfs_vfs::driver::Result<Box<dyn layerfs_vfs::driver::ProjectionWorkspace>> {
        Err(layerfs_vfs::driver::DriverError::Unsupported)
    }
}

pub fn host_driver() -> std::sync::Arc<dyn layerfs_vfs::driver::ProjectionDriver> {
    std::sync::Arc::new(HostDriver::default())
}

#[cfg(target_os = "macos")]
pub fn open_host_store(
    directory: &Path,
    mode: layerfs_engine::integrity::IntegrityMode,
) -> Result<layerfs_engine::Engine, layerfs_vfs::VfsError> {
    Ok(HostDriver::open_store_with_integrity(directory, mode)?)
}

#[cfg(not(target_os = "macos"))]
pub fn open_host_store(
    _directory: &Path,
    _mode: layerfs_engine::integrity::IntegrityMode,
) -> Result<layerfs_engine::Engine, layerfs_vfs::VfsError> {
    Err(layerfs_vfs::VfsError::Driver(
        layerfs_vfs::driver::DriverError::Unsupported,
    ))
}

#[cfg(target_os = "macos")]
pub fn compact_host_store(
    engine: layerfs_engine::Engine,
    directory: &Path,
) -> Result<layerfs_engine::Engine, layerfs_vfs::VfsError> {
    Ok(HostDriver::compact_store(engine, directory)?)
}

#[cfg(not(target_os = "macos"))]
pub fn compact_host_store(
    _engine: layerfs_engine::Engine,
    _directory: &Path,
) -> Result<layerfs_engine::Engine, layerfs_vfs::VfsError> {
    Err(layerfs_vfs::VfsError::Driver(
        layerfs_vfs::driver::DriverError::Unsupported,
    ))
}

pub const COMPONENT: &str = "layerfs-os";

#[derive(Debug, Clone)]
pub struct HostEnvironment {
    pub operating_system: String,
    pub os_version: Option<String>,
    pub architecture: Option<String>,
    pub filesystem_type: Option<String>,
    pub apfs_volume: Option<String>,
    pub case_behavior: Option<String>,
    pub cpu_model: Option<String>,
    pub logical_cpu_count: Option<u64>,
    pub memory_bytes: Option<u64>,
    pub sqlite_version: Option<String>,
    pub rust_version: Option<String>,
    pub journal_mode: &'static str,
    pub synchronous: &'static str,
    pub temp_store: &'static str,
    pub mmap_size: &'static str,
    pub probe_path: String,
}

#[derive(Debug)]
pub enum ProbeError {
    Io(io::Error),
    InvalidProbePath,
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "host probe I/O failed: {error}"),
            Self::InvalidProbePath => write!(f, "host probe path is not a directory"),
        }
    }
}

impl std::error::Error for ProbeError {}

impl From<io::Error> for ProbeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn probe(path: &Path) -> Result<HostEnvironment, ProbeError> {
    if !path.is_dir() {
        return Err(ProbeError::InvalidProbePath);
    }

    let path_text = path.to_string_lossy().into_owned();
    let filesystem_type = filesystem_type(path);
    let apfs_volume = filesystem_type
        .as_deref()
        .filter(|value| value.eq_ignore_ascii_case("apfs"))
        .and_then(|_| volume_name(path));
    let case_behavior = detect_case_behavior(path);

    Ok(HostEnvironment {
        operating_system: std::env::consts::OS.to_owned(),
        os_version: command_text("sw_vers", &["-productVersion"]),
        architecture: command_text("uname", &["-m"]),
        filesystem_type,
        apfs_volume,
        case_behavior,
        cpu_model: command_text("sysctl", &["-n", "hw.model"]),
        logical_cpu_count: command_text("sysctl", &["-n", "hw.logicalcpu"])
            .and_then(|value| value.parse().ok()),
        memory_bytes: command_text("sysctl", &["-n", "hw.memsize"])
            .and_then(|value| value.parse().ok()),
        sqlite_version: command_text("sqlite3", &["--version"])
            .and_then(|value| value.split_whitespace().next().map(str::to_owned)),
        rust_version: command_text("rustc", &["--version"]),
        journal_mode: "DELETE",
        synchronous: "FULL",
        temp_store: "FILE",
        mmap_size: "0",
        probe_path: path_text,
    })
}

fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

#[cfg(target_os = "macos")]
fn filesystem_type(path: &Path) -> Option<String> {
    let output = diskutil_info(path)?;
    output.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim() == "Type (Bundle)").then(|| value.trim().to_owned())
    })
}

#[cfg(target_os = "linux")]
fn filesystem_type(path: &Path) -> Option<String> {
    let path_text = path.to_string_lossy().into_owned();
    command_text("stat", &["-f", "-c", "%T", &path_text])
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn filesystem_type(_path: &Path) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn volume_name(path: &Path) -> Option<String> {
    let output = diskutil_info(path)?;

    output.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim() == "Volume Name").then(|| value.trim().to_owned())
    })
}

#[cfg(not(target_os = "macos"))]
fn volume_name(_path: &Path) -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn diskutil_info(path: &Path) -> Option<String> {
    let path_text = path.to_string_lossy().into_owned();
    let device = command_text("df", &["-P", &path_text])?
        .lines()
        .skip(1)
        .find_map(|line| line.split_whitespace().next().map(str::to_owned))?;
    command_text("diskutil", &["info", &device])
}

fn detect_case_behavior(parent: &Path) -> Option<String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let directory = parent.join(format!(
        ".layerfs-case-probe-{}-{}",
        std::process::id(),
        stamp
    ));
    fs::create_dir(&directory).ok()?;

    let upper = directory.join("LayerFsCaseProbe");
    let lower = directory.join("layerfscaseprobe");
    let result = (|| {
        fs::write(&upper, b"probe")?;
        match OpenOptions::new().write(true).create_new(true).open(&lower) {
            Ok(file) => {
                drop(file);
                Ok("case-sensitive".to_owned())
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Ok("case-insensitive".to_owned())
            }
            Err(error) => Err(error),
        }
    })();

    let _ = fs::remove_dir_all(&directory);
    result.ok()
}
