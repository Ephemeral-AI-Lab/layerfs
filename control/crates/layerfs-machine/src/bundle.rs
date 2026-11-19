//! The sealed Linux bundle (O16): the guest `layerfs` worker, the daemon,
//! the Sandbox helper and the Sandbox image archive, which every VM, ssh
//! host and `docker` on macOS needs. It lives in a per-build user cache
//! outside every home, `<cache>/layerfs/bundle/<version>/linux-<arch>/`,
//! with its hash in `bundle.json`; `tools/linux_bundle.py` builds and places
//! it. An installed runtime carries its own beside its command, which comes
//! first; `LAYERFS_BUNDLE` (internal) names another folder.
use layerfs_bridge::control::SdkRefusal;
use layerfs_home::outcome::refused;
#[cfg(feature = "machine")]
use layerfs_home::outcome::{shell_word, with_fix};
#[cfg(feature = "machine")]
use layerfs_platform::Bundle;
use std::path::PathBuf;

/// The Sandbox image archive (a docker-archive) a Linux bundle carries.
pub const IMAGE_ARCHIVE: &str = "sandbox-image.tar";

/// The sealed bundle's folder for `os` on this host's architecture (O16).
pub fn bundle_dir(os: &str) -> Result<PathBuf, SdkRefusal> {
    bundle_place(os).map(|(dir, _)| dir)
}

/// The sealed bundle's folder for `os` and where it was found: for Linux,
/// `LAYERFS_BUNDLE` (`env:LAYERFS_BUNDLE`); else the bundle an installed
/// runtime carries beside its command, `<runtime>/bin/layerfs` beside
/// `<runtime>/bundle/<os>-<arch>/` with its `bundle.json` (`runtime`); else
/// `<cache>/layerfs/bundle/<version>/<os>-<arch>/`, the cache being
/// `XDG_CACHE_HOME` or `~/.cache` (`cache`), which every build of this
/// version shares.
pub fn bundle_place(os: &str) -> Result<(PathBuf, &'static str), SdkRefusal> {
    if os == "linux" {
        if let Some(dir) = std::env::var_os("LAYERFS_BUNDLE").filter(|d| !d.is_empty()) {
            return Ok((PathBuf::from(dir), "env:LAYERFS_BUNDLE"));
        }
    }
    if let Some(dir) = runtime_place(os) {
        return Ok((dir, "runtime"));
    }
    Ok((cache_place(os)?, "cache"))
}

/// The bundle an installed runtime carries beside its command, when it
/// has a `bundle.json`.
fn runtime_place(os: &str) -> Option<PathBuf> {
    let command = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .ok()?;
    let runtime = command.parent()?.parent()?;
    let dir = runtime.join("bundle").join(folder(os));
    dir.join("bundle.json").is_file().then_some(dir)
}

/// The cache's bundle of this version, which every build of it shares.
fn cache_place(os: &str) -> Result<PathBuf, SdkRefusal> {
    let cache = cache_root()?;
    Ok((cache.join("layerfs").join("bundle"))
        .join(env!("CARGO_PKG_VERSION"))
        .join(folder(os)))
}

/// A bundle's folder name for `os` on this host's architecture.
fn folder(os: &str) -> String {
    format!("{os}-{}", std::env::consts::ARCH)
}

/// The `fix` of a Linux bundle at `refused` (found as `source`) sealed
/// from another commit than this build's (`ours`), and the sentence that
/// says where this build's is: the first other place [`bundle_place`]
/// looks that holds a sealed bundle of commit `ours`. Unsetting
/// `LAYERFS_BUNDLE` when it is the place found without it; else
/// `LAYERFS_BUNDLE` pointed at it. `None` when no place holds one: no
/// command fixes it then (RT5 note 4).
#[cfg(feature = "machine")]
fn own_bundle(refused: &std::path::Path, source: &str, ours: &str) -> Option<(String, String)> {
    // where it is looked for with LAYERFS_BUNDLE unset, in order
    let places: Vec<PathBuf> = (runtime_place("linux").into_iter())
        .chain(cache_place("linux").ok())
        .collect();
    let ours_at = |dir: &PathBuf| {
        let manifest = layerfs_home::home::read_record(&dir.join("bundle.json"));
        manifest.ok().flatten().is_some_and(|manifest| {
            let head = manifest.get("source").and_then(|s| s.get_text("head"));
            manifest.get_text("hash").is_some() && head == Some(ours)
        })
    };
    let (at, dir) = (places.iter().enumerate()).find(|(_, dir)| *dir != refused && ours_at(dir))?;
    let shown = dir.display();
    Some(match source == "env:LAYERFS_BUNDLE" && at == 0 {
        true => (
            "unset LAYERFS_BUNDLE".to_owned(),
            format!(
                "this build's bundle is at {shown}, which is used once LAYERFS_BUNDLE is unset"
            ),
        ),
        false => (
            format!(
                "export LAYERFS_BUNDLE={}",
                shell_word(&dir.to_string_lossy())
            ),
            format!("this build's bundle is at {shown}: point LAYERFS_BUNDLE at it"),
        ),
    })
}

/// The commit this build was compiled from, as the build tools name it
/// (`LAYERFS_BUILD_COMMIT`); `None` when they named none.
#[cfg(feature = "machine")]
fn build_commit() -> Option<&'static str> {
    option_env!("LAYERFS_BUILD_COMMIT")
        .filter(|commit| commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The bundle of this build, or `Unsupported` naming where it is expected.
/// One sealed from another commit is refused `Unavailable`, with the `fix`
/// that reaches this build's own when one is where LayerFS looks.
#[cfg(feature = "machine")]
pub fn bundle() -> Result<Bundle, SdkRefusal> {
    let (dir, source) = bundle_place("linux")?;
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
        // a bundle sealed from another commit speaks this protocol but not
        // this build's calls: a worker started from it answers them wrongly
        let sealed = manifest.get("source").and_then(|s| s.get_text("head"));
        if let (Some(ours), Some(theirs)) = (build_commit(), sealed) {
            if ours != theirs {
                let said = format!(
                    "the Linux bundle at {} was sealed from {}, this build from {}; a worker started from it would not answer this build's calls",
                    dir.display(),
                    theirs.get(..10).unwrap_or(theirs),
                    &ours[..10]
                );
                return Err(match own_bundle(&dir, source, ours) {
                    Some((fix, where_)) => {
                        with_fix(refused("Unavailable", format!("{said}; {where_}")), fix)
                    }
                    None => refused(
                        "Unavailable",
                        format!(
                            "{said}; no bundle of this build is where LayerFS looks (LAYERFS_BUNDLE, beside an installed runtime's command, or the cache): rebuild it with `python3 tools/linux_bundle.py`"
                        ),
                    ),
                });
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
