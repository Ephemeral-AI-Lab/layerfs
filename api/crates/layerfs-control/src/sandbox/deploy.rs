//! Where a Sandbox's daemon, image and Store come from (#533 step 12, #535
//! phase 4): the settings the host Sandbox code reads as `deploy`. The
//! caller reads what the machine's record chooses ([`choose`]: the backend,
//! the image, the machine's own image archive, the docker worker's data
//! volume) and sends that to the side that runs the call, which resolves
//! everything that depends on where it runs, once ([`resolve`]): its daemon,
//! its Engine socket, where its image archive and the Store's folder are.
//! This process resolves from the home's records and the sealed bundle in
//! the user cache (O16); a machine's worker from its own bundle folder (a
//! guest never reads the home, H §7.7).
use crate::common::home::{paths, read_record};
use crate::common::outcome::{failed, refused};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The Sandbox image archive (a docker-archive) a Linux bundle carries.
pub(crate) const IMAGE_ARCHIVE: &str = "sandbox-image.tar";

/// Where a guest holds a machine's own Sandbox image archive, copied in at
/// start (`layerfs_machine::IMAGE`).
const GUEST_IMAGES: &str = "/layerfs/image";

/// How a Sandbox runs, with the settings the side running the call resolved:
/// a daemon process (`native` on macOS, namespaces on Linux) or a container.
pub(super) enum Deploy {
    /// The daemon, and for namespaces the image archive and digest.
    Native(Vec<(String, SdkValue)>),
    /// The container's image, Engine socket, daemon and Store volume.
    Docker(SdkValue),
}

/// The side that runs a Sandbox call.
pub(super) enum Side<'a> {
    /// This process, with the home at the path and its records.
    Home(&'a Path),
    /// A machine's worker, with its root at the path and its bundle beside
    /// its own executable.
    Worker(&'a Path),
}

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

/// What machine `machine`'s record chooses for a Sandbox of a Store
/// (`postgresql` or sqlite), read by the caller: `backend` (`docker` or
/// `local`), the image (the call's `image`, else the record's; required on
/// `docker`), and for `local` the machine's own image archive (`image_archive`,
/// or an `image` that names a file); for a sqlite Store on `docker`, the
/// docker worker's data volume.
pub(super) fn choose(
    home: &Path,
    machine: &str,
    image: Option<&str>,
    postgresql: bool,
) -> Result<SdkValue> {
    let record = read_record(&paths::machine_record(home, machine))?.unwrap_or_default();
    let named = image.or(record.get_text("image"));
    let mut fields = Vec::new();
    if machine == "docker" {
        let image = named.ok_or_else(|| {
            refused(
                "InvalidInput",
                "the docker machine names no Sandbox image; `layerfs machine update docker --image sha256:<digest>`",
            )
        })?;
        fields.push(("backend", SdkValue::text("docker")));
        fields.push(("image", SdkValue::text(image)));
        if !postgresql {
            let volume = format!("{}-data", docker_vm(home, &record)?);
            fields.push(("data_volume", SdkValue::Text(volume)));
        }
    } else {
        fields.push(("backend", SdkValue::text("local")));
        if let Some(digest) = named.filter(|i| is_digest(i)) {
            fields.push(("image", SdkValue::text(digest)));
        }
        let archive = record
            .get_text("image_archive")
            .or(record.get_text("image").filter(|i| i.starts_with('/')));
        if let Some(archive) = archive {
            fields.push(("archive", SdkValue::text(archive)));
        }
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// The deploy settings for `choice` ([`choose`]) on `side`, for a Sandbox
/// of Store `store`.
pub(super) fn resolve(side: &Side<'_>, store: &str, choice: &SdkValue) -> Result<Deploy> {
    if choice.get_text("backend") == Some("docker") {
        return docker(side, store, choice).map(Deploy::Docker);
    }
    let mut fields = vec![("daemon".to_owned(), SdkValue::Text(daemon(side, "native")?))];
    // a namespaces Sandbox (Linux): its image archive and digest
    if cfg!(target_os = "linux") {
        let archive = archive(side, choice.get_text("archive"))?;
        fields.push((
            "imageArchive".to_owned(),
            SdkValue::text(archive.display().to_string()),
        ));
        if let Some(digest) = choice.get_text("image").filter(|i| is_digest(i)) {
            fields.push(("image".to_owned(), SdkValue::text(digest)));
        }
    }
    Ok(Deploy::Native(fields))
}

/// A docker Sandbox's settings on `side`: its image, the Engine socket and
/// the Linux daemon it uploads, and for a sqlite Store the Store's folder of
/// the worker's data volume. A worker makes that folder the container root
/// user's alone first, as the daemon requires, and dials the Sandbox's
/// loopback-published port as 1.1.0's macOS worker did. This process runs
/// only a PostgreSQL Store's docker Sandbox: a sqlite Store's goes through
/// the docker machine's worker on every host (G8).
fn docker(side: &Side<'_>, store: &str, choice: &SdkValue) -> Result<SdkValue> {
    let image = choice
        .get_text("image")
        .ok_or_else(|| refused("InvalidInput", "a docker Sandbox's image"))?;
    let mut fields = vec![("image", SdkValue::text(image))];
    let volume = choice.get_text("data_volume");
    let socket = match side {
        Side::Home(_) if volume.is_some() => {
            return Err(refused(
                "Unsupported",
                "a docker Sandbox of a sqlite Store runs through the docker machine's worker",
            ))
        }
        Side::Home(home) => read_record(&paths::machine_record(home, "docker"))?
            .and_then(|record| record.get_text("docker").map(str::to_owned))
            .unwrap_or_else(|| "/var/run/docker.sock".to_owned()),
        Side::Worker(_) => {
            fields.push((
                "daemonHost",
                SdkValue::text(layerfs_sandbox::docker::DESKTOP_HOST),
            ));
            "/var/run/docker.sock".to_owned()
        }
    };
    if let (Some(volume), Side::Worker(root)) = (volume, side) {
        private_store(root, store)?;
        fields.push(("volume", SdkValue::Text(format!("{volume}/store/{store}"))));
    }
    fields.push(("docker", SdkValue::Text(socket)));
    fields.push(("daemon", SdkValue::Text(daemon(side, "docker")?)));
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// Makes Store `store`'s folder and file under the worker's root the
/// container root user's alone (0700, 0600), as a docker Sandbox's daemon
/// requires of them.
fn private_store(root: &Path, store: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let folder = paths::store_dir(root, store);
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).map_err(failed)?;
    let file = paths::store_file(root, store);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).map_err(failed)
}

/// The daemon a Sandbox on `machine` (`native` or `docker`) runs, on `side`:
/// in a worker, the one beside its own executable (the bundle's); here, the
/// machine record's `daemon` (`import-env` writes native's from 1.1.0's
/// `LAYERFS_DAEMON`), else for `native` this host's bundle's, and for
/// `docker` (a Linux daemon) native's on Linux, else the sealed Linux
/// bundle's.
fn daemon(side: &Side<'_>, machine: &str) -> Result<String> {
    let home = match side {
        Side::Worker(_) => return Ok(beside_self("layerfs-daemon")?.display().to_string()),
        Side::Home(home) => home,
    };
    let record = read_record(&paths::machine_record(home, machine))?.unwrap_or_default();
    if let Some(path) = record.get_text("daemon") {
        return Ok(path.to_owned());
    }
    if machine == "docker" && !cfg!(target_os = "linux") {
        return bundle_daemon();
    }
    if machine == "docker" {
        return daemon(side, "native");
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

/// The namespaces image archive on `side`: the machine's own archive
/// (`chosen`; in a guest, its copy under [`GUEST_IMAGES`]), else the
/// bundle's.
fn archive(side: &Side<'_>, chosen: Option<&str>) -> Result<PathBuf> {
    Ok(match (side, chosen) {
        (Side::Home(_), Some(path)) => PathBuf::from(path),
        (Side::Home(_), None) => bundle_dir("linux")?.join(IMAGE_ARCHIVE),
        (Side::Worker(_), Some(path)) => match Path::new(path).file_name() {
            Some(file) => Path::new(GUEST_IMAGES).join(file),
            None => beside_self(IMAGE_ARCHIVE)?,
        },
        (Side::Worker(_), None) => beside_self(IMAGE_ARCHIVE)?,
    })
}

/// File `name` beside this process's executable: a worker's bundle.
fn beside_self(name: &str) -> Result<PathBuf> {
    Ok(std::env::current_exe()
        .map_err(failed)?
        .with_file_name(name))
}

/// The docker machine's worker container name.
#[cfg(feature = "machine")]
fn docker_vm(home: &Path, record: &SdkValue) -> Result<String> {
    Ok(match record.get_text("vm_name") {
        Some(name) => name.to_owned(),
        None => crate::machine::vm_name(&crate::common::home::open(home)?.id, "docker"),
    })
}

#[cfg(not(feature = "machine"))]
fn docker_vm(_: &Path, _: &SdkValue) -> Result<String> {
    Err(refused(
        "Unsupported",
        "a docker machine's worker is reached from a host build",
    ))
}

/// The sealed bundle's Linux daemon.
#[cfg(feature = "machine")]
fn bundle_daemon() -> Result<String> {
    let bundle = crate::machine::bundle()?;
    Ok(bundle.dir.join("layerfs-daemon").display().to_string())
}

#[cfg(not(feature = "machine"))]
fn bundle_daemon() -> Result<String> {
    Err(refused(
        "Unsupported",
        "no Linux daemon for a docker Sandbox; set the docker machine's daemon",
    ))
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
