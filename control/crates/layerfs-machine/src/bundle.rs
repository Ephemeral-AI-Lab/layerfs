//! The sealed Linux bundle (O16): the guest `layerfs` worker, the daemon,
//! the Sandbox helper and the Sandbox image archive, which every VM, ssh
//! host and `docker` on macOS needs. It lives in a per-build user cache
//! outside every home, `<cache>/layerfs/bundle/<version>/linux-<arch>/`,
//! with its hash in `bundle.json`; `tools/linux_bundle.py` builds and places
//! it. `LAYERFS_BUNDLE` (internal) names another folder.
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::refused;
#[cfg(feature = "machine")]
use layerfs_platform::Bundle;
use std::path::PathBuf;

/// The Sandbox image archive (a docker-archive) a Linux bundle carries.
pub const IMAGE_ARCHIVE: &str = "sandbox-image.tar";

/// The sealed bundle's folder for `os` on this host's architecture (O16):
/// `<cache>/layerfs/bundle/<version>/<os>-<arch>/`, the cache being
/// `XDG_CACHE_HOME` or `~/.cache`; for Linux, `LAYERFS_BUNDLE` (internal)
/// names another folder.
pub fn bundle_dir(os: &str) -> Result<PathBuf, SdkRefusal> {
    if os == "linux" {
        if let Some(dir) = std::env::var_os("LAYERFS_BUNDLE").filter(|d| !d.is_empty()) {
            return Ok(PathBuf::from(dir));
        }
    }
    let cache = cache_root()?;
    Ok(cache
        .join("layerfs")
        .join("bundle")
        .join(env!("CARGO_PKG_VERSION"))
        .join(format!("{os}-{}", std::env::consts::ARCH)))
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
            refusal.detail = layerfs_bridge::control::SdkValue::map([(
                "missing",
                layerfs_bridge::control::SdkValue::List(vec![layerfs_bridge::control::SdkValue::text(
                    format!("bundle:{}", dir.display()),
                )]),
            )]);
            refusal
        })?;
    let manifest = layerfs_home::home::read_record(&dir.join("bundle.json"))?;
    if let Some(manifest) = manifest {
        if let Some(protocol) = manifest.get("protocol") {
            if protocol != &layerfs_bridge::control::SdkValue::Int(crate::worker::PROTOCOL) {
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
