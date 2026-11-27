//! Host-worker identity, independent of the guest bundle handshake.
use super::HostPaths;
use layerfs_home::{helpers::random, outcome::refused};
use layerfs_protocol::{
    sdk::hex,
    sdk::{SdkRefusal, SdkValue},
};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Version of the host worker exchange.
pub const HOST_PROTOCOL: i64 = 2;
pub(super) const CORRELATION_LEN: usize = 32;

static RUNNING_BUILD: OnceLock<&'static str> = OnceLock::new();

/// Records the host CLI's compiled source identity before it serves or dials.
/// The install path can refer to a newer executable while this process runs.
pub fn set_host_build(build: &'static str) {
    let _ = RUNNING_BUILD.set(build);
}

/// The greeting of the authenticated endpoint.
pub struct HostHello {
    /// Stable ID from the home record.
    pub home_id: String,
    /// Incarnation token echoed in every request.
    pub generation: String,
    /// Identity of the installed executable.
    pub build: String,
}

pub(super) fn executable() -> std::io::Result<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        Ok(PathBuf::from("/proc/self/exe"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::current_exe()
    }
}

pub(super) fn management_cli() -> Result<Option<(PathBuf, String)>, SdkRefusal> {
    if RUNNING_BUILD.get().is_some() {
        return Ok(None);
    }
    let commit = option_env!("LAYERFS_BUILD_COMMIT")
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            refused(
                "Unavailable",
                "the SDK build identity is unavailable for host worker management",
            )
        })?;
    let dirty = option_env!("LAYERFS_BUILD_DIRTY").unwrap_or("false");
    let path = std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join("layerfs"))
                .find(|path| path.is_file())
        })
        .ok_or_else(|| {
            refused(
                "Unavailable",
                "host worker management requires the matching layerfs CLI on PATH",
            )
        })?;
    let path = path
        .canonicalize()
        .map_err(|e| refused("Unavailable", format!("host CLI: {e}")))?;
    let build = identity(fs::metadata(&path), &format!("{commit}/{dirty}"))?;
    Ok(Some((path, build)))
}

pub(super) fn build_identity() -> Result<String, SdkRefusal> {
    identity(
        executable().and_then(fs::metadata),
        RUNNING_BUILD.get().copied().unwrap_or("unmarked"),
    )
}

fn identity(metadata: std::io::Result<fs::Metadata>, source: &str) -> Result<String, SdkRefusal> {
    let executable = metadata.map_err(|error| {
        refused(
            "Unavailable",
            format!("cannot identify the host worker executable: {error}"),
        )
    })?;
    Ok(format!(
        "{}/{}@{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
        env!("CARGO_PKG_VERSION"),
        source,
        executable.dev(),
        executable.ino(),
        executable.len(),
        executable.mtime(),
        executable.mtime_nsec(),
        executable.ctime(),
        executable.ctime_nsec()
    ))
}

pub(super) fn token() -> Result<String, SdkRefusal> {
    Ok(hex(&random()?[..CORRELATION_LEN / 2]))
}

impl HostHello {
    /// Identifies this worker and gives it a fresh generation.
    pub fn own(paths: &HostPaths) -> Result<Self, SdkRefusal> {
        Ok(Self {
            home_id: paths.home_id.clone(),
            generation: token()?,
            build: build_identity()?,
        })
    }

    /// The server-first hello frame.
    pub fn to_frame(&self) -> SdkValue {
        SdkValue::map([
            ("ready", SdkValue::Bool(true)),
            ("kind", SdkValue::text("host")),
            ("protocol", SdkValue::Int(HOST_PROTOCOL)),
            ("home_id", SdkValue::text(&self.home_id)),
            ("generation", SdkValue::text(&self.generation)),
            ("build", SdkValue::text(&self.build)),
        ])
    }

    pub(super) fn generation_of(
        frame: &SdkValue,
        paths: &HostPaths,
        build: &str,
    ) -> Result<String, SdkRefusal> {
        if frame.get_int("protocol") == Some(HOST_PROTOCOL)
            && frame.get_text("build") == Some(build)
        {
            if let Some(generation) = Self::home_generation(frame, paths) {
                return Ok(generation.to_owned());
            }
        }
        let mut refusal = refused(
            "Unavailable",
            "host worker hello does not match this home, protocol or executable; wait for its idle exit",
        );
        refusal.detail = SdkValue::map([
            ("worker", frame.clone()),
            (
                "expected",
                SdkValue::map([
                    ("protocol", SdkValue::Int(HOST_PROTOCOL)),
                    ("home_id", SdkValue::text(&paths.home_id)),
                    ("build", SdkValue::text(build)),
                ]),
            ),
        ]);
        Err(refusal)
    }

    pub(super) fn home_generation<'a>(frame: &'a SdkValue, paths: &HostPaths) -> Option<&'a str> {
        let belongs = frame.get_bool("ready")
            && frame.get_text("kind") == Some("host")
            && frame.get_text("home_id") == Some(&paths.home_id)
            && frame.get_int("protocol").is_some()
            && frame
                .get_text("build")
                .is_some_and(|build| !build.is_empty());
        belongs.then_some(())?;
        frame.get_text("generation").filter(|generation| {
            generation.len() == CORRELATION_LEN && generation.bytes().all(|b| b.is_ascii_hexdigit())
        })
    }
}
