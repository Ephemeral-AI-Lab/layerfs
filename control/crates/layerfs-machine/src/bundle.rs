//! The sealed Linux bundle (O16): the guest `layerfs` worker, the daemon,
//! the Sandbox helper and the Sandbox image archive, which every VM, ssh
//! host and `docker` on macOS needs. It lives in a per-build user cache
//! outside every home, `<cache>/layerfs/bundle/<version>/linux-<arch>/`,
//! with its hash in `bundle.json`; `tools/linux_bundle.py` builds and places
//! it. `LAYERFS_BUNDLE` (internal) names another folder. A program that
//! names its build ([`this_build`], the `layerfs` command) first uses the
//! bundle of that build beside its executable, `<exe dir>/../bundle/<os>-<arch>/`,
//! as an installed runtime lays it out.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::refused;
#[cfg(feature = "machine")]
use layerfs_platform::Bundle;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The Sandbox image archive (a docker-archive) a Linux bundle carries.
pub const IMAGE_ARCHIVE: &str = "sandbox-image.tar";

/// The build this program says it is: its 40-digit commit and whether its
/// tree was dirty. Unset (a library caller that names none), no bundle
/// beside the executable is used.
static BUILD: OnceLock<(String, bool)> = OnceLock::new();

/// Names this program's build, once, before its first call: from then on
/// a bundle sealed from the same build beside the executable is used
/// before the cache ([`bundle_dir`]).
pub fn this_build(commit: &str, dirty: bool) {
    let _ = BUILD.set((commit.to_owned(), dirty));
}

/// Where a bundle folder was found: `LAYERFS_BUNDLE`, beside the
/// executable, or the per-build cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `LAYERFS_BUNDLE` names it (Linux only).
    Env,
    /// Beside the executable, sealed from this program's build.
    Beside,
    /// The per-build user cache.
    Cache,
}

impl Source {
    /// The word `home get` answers as the bundle's `source`.
    pub fn word(self) -> &'static str {
        match self {
            Source::Env => "env:LAYERFS_BUNDLE",
            Source::Beside => "beside",
            Source::Cache => "cache",
        }
    }
}

/// The sealed bundle's folder for `os` on this host's architecture (O16):
/// see [`located`].
pub fn bundle_dir(os: &str) -> Result<PathBuf, SdkRefusal> {
    located(os).map(|(dir, _)| dir)
}

/// The sealed bundle's folder for `os` on this host's architecture, and
/// where it was found: for Linux, `LAYERFS_BUNDLE` (internal) when set;
/// else, once the program named its build ([`this_build`]), the bundle
/// beside its executable ([`beside`]) when its seal names that build;
/// else `<cache>/layerfs/bundle/<version>/<os>-<arch>/`, the cache being
/// `XDG_CACHE_HOME` or `~/.cache`.
pub fn located(os: &str) -> Result<(PathBuf, Source), SdkRefusal> {
    if os == "linux" {
        if let Some(dir) = std::env::var_os("LAYERFS_BUNDLE").filter(|d| !d.is_empty()) {
            return Ok((PathBuf::from(dir), Source::Env));
        }
    }
    if let Some(dir) = BUILD.get().and_then(|build| beside(os, build)) {
        return Ok((dir, Source::Beside));
    }
    let cache = cache_root()?;
    let dir = cache
        .join("layerfs")
        .join("bundle")
        .join(env!("CARGO_PKG_VERSION"))
        .join(folder(os));
    Ok((dir, Source::Cache))
}

/// `<os>-<arch>`, a bundle's folder name.
fn folder(os: &str) -> String {
    format!("{os}-{}", std::env::consts::ARCH)
}

/// The bundle beside this program's executable (symlinks resolved):
/// `<exe dir>/../bundle/<os>-<arch>/`, used only when its `bundle.json`
/// names this version, this worker protocol (when it names one) and
/// `build`: the same source commit and the same dirty flag.
fn beside(os: &str, build: &(String, bool)) -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let dir = exe.parent()?.parent()?.join("bundle").join(folder(os));
    sealed_by(&dir, build).then_some(dir)
}

/// Whether the bundle in `dir` was sealed from `build` of this version.
pub fn sealed_by(dir: &Path, (commit, dirty): &(String, bool)) -> bool {
    let Ok(Some(manifest)) = layerfs_home::home::read_record(&dir.join("bundle.json")) else {
        return false;
    };
    let source = manifest.get("source");
    let protocol = manifest.get("protocol");
    manifest.get_text("version") == Some(env!("CARGO_PKG_VERSION"))
        && protocol.is_none_or(|p| p == &SdkValue::Int(crate::worker::PROTOCOL))
        && source.and_then(|s| s.get_text("head")) == Some(commit.as_str())
        && source.and_then(|s| s.get("dirty")) == Some(&SdkValue::Bool(*dirty))
}

/// The bundle of this build, or `Unsupported` naming where it is expected.
#[cfg(feature = "machine")]
pub fn bundle() -> Result<Bundle, SdkRefusal> {
    let dir = bundle_dir("linux")?;
    let hash = crate::worker::sealed_hash(&dir)?.ok_or_else(|| {
            let mut refusal = refused(
                "Unsupported",
                format!(
                    "no sealed Linux bundle at {} (bundle.json with its hash); `python3 tools/linux_bundle.py` builds and places it (O16)",
                    dir.display()
                ),
            );
            refusal.detail = SdkValue::map([(
                "missing",
                SdkValue::List(vec![SdkValue::text(format!("bundle:{}", dir.display()))]),
            )]);
            refusal
        })?;
    let manifest = layerfs_home::home::read_record(&dir.join("bundle.json"))?;
    if let Some(manifest) = manifest {
        if let Some(protocol) = manifest.get("protocol") {
            if protocol != &SdkValue::Int(crate::worker::PROTOCOL) {
                return Err(refused("Unavailable", format!(
                    "the Linux bundle at {} has an incompatible worker protocol; rebuild it with `python3 tools/linux_bundle.py` before starting a machine (expected protocol {})",
                    dir.display(), crate::worker::PROTOCOL)));
            }
        }
        if let Some(platform) = manifest.get_text("platform") {
            if platform != format!("linux-{}", std::env::consts::ARCH) {
                return Err(refused(
                    "Unsupported",
                    format!(
                        "the Linux bundle is for {platform}, not linux-{}",
                        std::env::consts::ARCH
                    ),
                ));
            }
        }
    }
    Ok(Bundle { dir, hash })
}

/// The user's cache root, shared by all homes and bundle overrides.
pub fn cache_root() -> Result<PathBuf, SdkRefusal> {
    match std::env::var_os("XDG_CACHE_HOME").filter(|d| !d.is_empty()) {
        Some(dir) => Ok(PathBuf::from(dir)),
        None => std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".cache"))
            .ok_or_else(|| {
                refused(
                    "InvalidInput",
                    "HOME is unset, so the user cache is unknown",
                )
            }),
    }
}

/// A locally prepared, provider-specific OS image for this Sandbox content.
/// Preparation is explicit; an absent mapping keeps the provider's stock image.
/// An invalid or incompatible mapping is refused rather than silently bypassed.
#[cfg(feature = "machine")]
pub fn prepared_os_image(
    kind: layerfs_platform::Platform,
    bundle: &Bundle,
) -> Result<Option<String>, SdkRefusal> {
    use layerfs_bridge::control::SdkValue;
    use layerfs_home::home::read_record;
    use layerfs_platform::Platform;
    if !matches!(kind, Platform::Lima | Platform::AppleContainer) {
        return Ok(None);
    }
    let manifest = read_record(&bundle.dir.join("bundle.json"))?;
    let Some(content) = manifest.as_ref().and_then(|m| m.get_text("image")) else {
        return Ok(None);
    };
    let Some(hex) = content
        .strip_prefix("sha256:")
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
    else {
        return Err(refused("InvalidInput", "the bundle image ID is not sha256"));
    };
    let platform = format!("linux-{}", std::env::consts::ARCH);
    let path = cache_root()?
        .join("layerfs/machine-images")
        .join(kind.name())
        .join(&platform)
        .join(hex)
        .join("current.json");
    let Some(prepared) = read_record(&path)? else {
        return Ok(None);
    };
    if prepared.get_int("version") != Some(1)
        || prepared.get_int("protocol") != Some(crate::worker::PROTOCOL)
        || prepared.get_text("platform") != Some(platform.as_str())
        || prepared.get_text("image") != Some(content)
        || prepared.get_text("provider") != Some(kind.name())
        || prepared.get("sanitized") != Some(&SdkValue::Bool(true))
    {
        return Err(refused("Unavailable", format!("the prepared machine image mapping {} is incompatible or unsanitized; rebuild it with tools/machine_image.py", path.display())));
    }
    let image = prepared
        .get_text("os_image")
        .ok_or_else(|| refused("InvalidInput", "the prepared image mapping has no os_image"))?;
    if kind == Platform::Lima
        && (!std::path::Path::new(image).is_absolute() || !std::path::Path::new(image).is_file())
    {
        return Err(refused("Unsupported", format!("the prepared Lima image {image} is missing; rebuild it with tools/machine_image.py")));
    }
    Ok(Some(image.to_owned()))
}
