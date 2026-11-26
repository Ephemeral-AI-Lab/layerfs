//! Host-worker identity, independent of the guest bundle handshake.
use super::HostPaths;
use layerfs_bridge::{
    control::{SdkRefusal, SdkValue},
    sdk_values::hex,
};
use layerfs_home::{helpers::random, outcome::refused};
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

/// Version of the host worker exchange.
pub const HOST_PROTOCOL: i64 = 1;
pub(super) const CORRELATION_LEN: usize = 32;

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

pub(super) fn build_identity() -> Result<String, SdkRefusal> {
    let executable = executable().and_then(fs::metadata).map_err(|error| {
        refused(
            "Unavailable",
            format!("cannot identify the host worker executable: {error}"),
        )
    })?;
    Ok(format!(
        "{}@{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}",
        env!("CARGO_PKG_VERSION"),
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
        if frame.get_bool("ready")
            && frame.get_text("kind") == Some("host")
            && frame.get_int("protocol") == Some(HOST_PROTOCOL)
            && frame.get_text("home_id") == Some(&paths.home_id)
            && frame.get_text("build") == Some(build)
        {
            if let Some(generation) = frame.get_text("generation").filter(|generation| {
                generation.len() == CORRELATION_LEN
                    && generation.bytes().all(|b| b.is_ascii_hexdigit())
            }) {
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
}
