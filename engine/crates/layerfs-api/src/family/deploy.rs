//! Where a Sandbox's daemon, image and Store come from (#533 step 12): the
//! settings the host Sandbox code reads as `deploy`, resolved on the side
//! that runs the call. Moved from `sandboxes.rs` unchanged.
use crate::home::{paths, read_record};
use crate::host::outcome::{failed, refused};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Where a `native` Sandbox's daemon executable is: the native machine
/// record's `daemon` (`import-env` writes it from 1.1.0's `LAYERFS_DAEMON`),
/// else the bundle's.
pub(super) fn daemon(home: &Path) -> Result<String> {
    let record = read_record(&paths::machine_record(home, "native"))?.unwrap_or_default();
    if let Some(path) = record.get_text("daemon") {
        return Ok(path.to_owned());
    }
    let cache = std::env::var_os("HOME")
        .map(|h| {
            PathBuf::from(h)
                .join(".cache/layerfs/bundle")
                .join(env!("CARGO_PKG_VERSION"))
        })
        .ok_or_else(|| refused("InvalidInput", "HOME is unset, so the bundle is unknown"))?;
    let path = cache
        .join(format!(
            "{}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
        .join("layerfs-daemon");
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
