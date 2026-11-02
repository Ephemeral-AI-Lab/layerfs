//! Where a Sandbox's daemon, image and Store come from (#533 step 12): the
//! settings the host Sandbox code reads as `deploy`, resolved on the side
//! that runs the call: this process from the home's records and the sealed
//! bundle in the user cache (O16), or a machine's worker from its own
//! bundle folder (a guest never reads the home, H §7.7).
use crate::home::{paths, read_record};
use crate::host::outcome::{failed, refused};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The Sandbox image archive (a docker-archive) a Linux bundle carries.
pub(crate) const IMAGE_ARCHIVE: &str = "sandbox-image.tar";

/// Where a guest holds a machine's own Sandbox image archive, copied in at
/// start (`layerfs_sandbox::machine::IMAGE`).
const GUEST_IMAGES: &str = "/layerfs/image";

/// The sealed bundle's folder for `os` on this host's architecture (O16):
/// `<cache>/layerfs/bundle/<version>/<os>-<arch>/`, the cache being
/// `XDG_CACHE_HOME` or `~/.cache`; for Linux, `LAYERFS_BUNDLE` (internal)
/// names another folder.
pub(crate) fn bundle_dir(os: &str) -> Result<PathBuf> {
    if os == "linux" {
        if let Some(dir) = std::env::var_os("LAYERFS_BUNDLE").filter(|d| !d.is_empty()) {
            return Ok(PathBuf::from(dir));
        }
    }
    let cache = match std::env::var_os("XDG_CACHE_HOME").filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".cache"))
            .ok_or_else(|| {
                refused(
                    "InvalidInput",
                    "HOME is unset, so the user cache is unknown",
                )
            })?,
    };
    Ok(cache
        .join("layerfs")
        .join("bundle")
        .join(env!("CARGO_PKG_VERSION"))
        .join(format!("{os}-{}", std::env::consts::ARCH)))
}

/// Where a `native` Sandbox's daemon executable is: the native machine
/// record's `daemon` (`import-env` writes it from 1.1.0's `LAYERFS_DAEMON`),
/// else this host's bundle's.
pub(super) fn daemon(home: &Path) -> Result<String> {
    let record = read_record(&paths::machine_record(home, "native"))?.unwrap_or_default();
    if let Some(path) = record.get_text("daemon") {
        return Ok(path.to_owned());
    }
    let path = bundle_dir(std::env::consts::OS)?.join("layerfs-daemon");
    if !path.is_file() {
        return Err(refused(
            "Unavailable",
            format!(
                "no LayerFS daemon at {}; install the bundle",
                path.display()
            ),
        ));
    }
    Ok(path.display().to_string())
}

/// The docker machine's settings for a Sandbox of `store`: the image (the
/// call's, else the record's), where its Store folder comes from, and for a
/// worker the host it dials. The side that runs the call adds its own
/// Docker socket and Linux daemon ([`docker_here`], [`docker_in_worker`]).
pub(super) fn docker_settings(
    home: &Path,
    store: &str,
    image: Option<&str>,
    postgresql: bool,
    worker: bool,
) -> Result<SdkValue> {
    let record = read_record(&paths::machine_record(home, "docker"))?.unwrap_or_default();
    let image = image.or(record.get_text("image")).ok_or_else(|| {
        refused(
            "InvalidInput",
            "the docker machine names no Sandbox image; `layerfs machine update docker --image sha256:<digest>`",
        )
    })?;
    let mut fields = vec![("image", SdkValue::text(image))];
    if !postgresql {
        let source = if worker {
            // the worker's data volume, the Store's folder in it
            format!("{}-data/store/{store}", docker_vm(home, &record)?)
        } else {
            paths::store_dir(home, store).display().to_string()
        };
        fields.push(("volume", SdkValue::text(source)));
    }
    if worker {
        // the worker dials a Sandbox container's loopback-published port as
        // 1.1.0's macOS worker did
        fields.push((
            "daemonHost",
            SdkValue::text(layerfs_sandbox::docker::DESKTOP_HOST),
        ));
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// The docker machine's worker container name.
#[cfg(feature = "machine")]
pub(super) fn docker_vm(home: &Path, record: &SdkValue) -> Result<String> {
    Ok(match record.get_text("vm_name") {
        Some(name) => name.to_owned(),
        None => crate::machine::vm_name(&crate::home::open(home)?.id, "docker"),
    })
}

#[cfg(not(feature = "machine"))]
pub(super) fn docker_vm(_: &Path, _: &SdkValue) -> Result<String> {
    Err(refused(
        "Unsupported",
        "a docker machine's worker is reached from a host build",
    ))
}

/// Completes the docker settings in this process: the docker machine's
/// socket and the Linux daemon it uploads (the record's `daemon`, else on
/// Linux this host's own, else the sealed bundle's). A Store folder bind
/// mounted from here must be the container's root user's, as the daemon
/// requires of it, so only a root caller binds one.
pub(super) fn docker_here(home: &Path, settings: SdkValue) -> Result<SdkValue> {
    let record = read_record(&paths::machine_record(home, "docker"))?.unwrap_or_default();
    if settings.get("volume").is_some() && crate::host::discovery::user().0 != 0 {
        return Err(refused(
            "Unsupported",
            "a docker Sandbox of a sqlite Store opened in this process bind mounts the Store's folder, which its daemon (root in the container) must own; run as root, or keep the Store on native",
        ));
    }
    let socket = record
        .get_text("docker")
        .unwrap_or("/var/run/docker.sock")
        .to_owned();
    let daemon = match record.get_text("daemon") {
        Some(path) => path.to_owned(),
        None if cfg!(target_os = "linux") => daemon(home)?,
        None => bundle_daemon()?,
    };
    let SdkValue::Map(mut fields) = settings else {
        return Err(refused("InvalidInput", "docker settings are a map"));
    };
    fields.push(("docker".to_owned(), SdkValue::text(socket)));
    fields.push(("daemon".to_owned(), SdkValue::text(daemon)));
    Ok(SdkValue::Map(fields))
}

/// The sealed bundle's Linux daemon.
#[cfg(feature = "machine")]
pub(super) fn bundle_daemon() -> Result<String> {
    let bundle = crate::machine::bundle()?;
    Ok(bundle.dir.join("layerfs-daemon").display().to_string())
}

#[cfg(not(feature = "machine"))]
pub(super) fn bundle_daemon() -> Result<String> {
    Err(refused(
        "Unsupported",
        "no Linux daemon for a docker Sandbox; set the docker machine's daemon",
    ))
}

/// Completes the docker settings in the worker: the Engine socket mounted
/// into its container and the daemon beside its own executable (the
/// bundle's). The Store's folder in the data volume is made the container
/// root user's alone first, as the daemon requires.
pub(super) fn docker_in_worker(root: &Path, store: &str, settings: &SdkValue) -> Result<SdkValue> {
    use std::os::unix::fs::PermissionsExt;
    let daemon = std::env::current_exe()
        .map_err(failed)?
        .with_file_name("layerfs-daemon");
    let folder = paths::store_dir(root, store);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).map_err(failed)?;
    let file = paths::store_file(root, store);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).map_err(failed)?;
    let SdkValue::Map(mut fields) = settings.clone() else {
        return Err(refused("InvalidInput", "docker settings are a map"));
    };
    fields.retain(|(name, _)| matches!(name.as_str(), "image" | "volume" | "daemonHost"));
    fields.push(("docker".to_owned(), SdkValue::text("/var/run/docker.sock")));
    fields.push((
        "daemon".to_owned(),
        SdkValue::text(daemon.display().to_string()),
    ));
    Ok(SdkValue::Map(fields))
}

/// The native machine's settings in this process: its daemon and, for a
/// namespaces Sandbox (Linux), the Sandbox image archive and digest: the
/// native record's `image_archive`, else the bundle's archive, and the
/// call's `image`, else the record's (a digest the archive holds).
pub(super) fn native_here(home: &Path, image: Option<&str>) -> Result<Vec<(String, SdkValue)>> {
    let record = read_record(&paths::machine_record(home, "native"))?.unwrap_or_default();
    let mut fields = vec![("daemon".to_owned(), SdkValue::text(daemon(home)?))];
    if cfg!(target_os = "linux") {
        let archive = match record.get_text("image_archive") {
            Some(path) => PathBuf::from(path),
            None => bundle_dir("linux")?.join(IMAGE_ARCHIVE),
        };
        fields.push((
            "imageArchive".to_owned(),
            SdkValue::text(archive.display().to_string()),
        ));
        if let Some(digest) = image.or(record.get_text("image")).filter(|i| is_digest(i)) {
            fields.push(("image".to_owned(), SdkValue::text(digest)));
        }
    }
    Ok(fields)
}

/// What a VM or ssh machine's worker needs for a namespaces Sandbox: the
/// image digest (the call's, else the machine record's when it is one) and,
/// when the record names its own archive file, the guest's copy of it.
pub(super) fn guest_settings(home: &Path, machine: &str, image: Option<&str>) -> Result<SdkValue> {
    let record = read_record(&paths::machine_record(home, machine))?.unwrap_or_default();
    let mut fields = Vec::new();
    let named = record.get_text("image");
    if let Some(digest) = image.or(named).filter(|i| is_digest(i)) {
        fields.push(("image".to_owned(), SdkValue::text(digest)));
    }
    let archive = record
        .get_text("image_archive")
        .or(named.filter(|i| i.starts_with('/')));
    if let Some(file) = archive.and_then(|a| Path::new(a).file_name()?.to_str()) {
        fields.push((
            "image_archive".to_owned(),
            SdkValue::text(format!("{GUEST_IMAGES}/{file}")),
        ));
    }
    Ok(SdkValue::Map(fields))
}

/// Completes a namespaces Sandbox's settings in a worker: the daemon beside
/// its own executable (the bundle's), the caller's digest, and the guest's
/// copy of the machine's archive, else the bundle's own archive.
pub(super) fn native_in_worker(settings: Option<&SdkValue>) -> Result<Vec<(String, SdkValue)>> {
    let bundle = std::env::current_exe().map_err(failed)?;
    let mut fields = vec![(
        "daemon".to_owned(),
        SdkValue::text(
            bundle
                .with_file_name("layerfs-daemon")
                .display()
                .to_string(),
        ),
    )];
    let archive = settings
        .and_then(|s| s.get_text("image_archive"))
        .filter(|a| a.starts_with(GUEST_IMAGES))
        .map(PathBuf::from)
        .unwrap_or_else(|| bundle.with_file_name(IMAGE_ARCHIVE));
    fields.push((
        "imageArchive".to_owned(),
        SdkValue::text(archive.display().to_string()),
    ));
    if let Some(digest) = settings
        .and_then(|s| s.get_text("image"))
        .filter(|i| is_digest(i))
    {
        fields.push(("image".to_owned(), SdkValue::text(digest)));
    }
    Ok(fields)
}

/// Whether `image` is an image digest, `sha256:<64 hex>`.
fn is_digest(image: &str) -> bool {
    image.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
