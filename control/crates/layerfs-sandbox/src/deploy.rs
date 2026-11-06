//! Where a Sandbox's daemon, image and Store come from (#533 step 12, #535
//! phase 4): the settings the host Sandbox code reads as `deploy`. The
//! caller reads what the machine's record chooses ([`choose`]: the backend,
//! the image, the machine's own image archive, the platform the docker
//! machine implies) and sends that to the side that runs the call, which
//! resolves everything that depends on where it runs, once ([`resolve`]):
//! its daemon and where its image archive is.
//! This process resolves from the home's records and the sealed bundle in
//! the user cache (O16); a machine's worker from its own bundle folder (a
//! guest never reads the home, H §7.7).
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{failed, refused};
use layerfs_machine::record::MachineRecord;
use layerfs_machine::{bundle_dir, IMAGE_ARCHIVE};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, SdkRefusal>;

/// Where a guest holds a machine's own Sandbox image archive, copied in at
/// start (`layerfs_machine::platform::IMAGE`).
const GUEST_IMAGES: &str = "/layerfs/image";

/// The side that runs a Sandbox call.
pub enum Side<'a> {
    /// This process, with the home at the path and its records.
    Home(&'a Path),
    /// A machine's worker, with its root at the path and its bundle beside
    /// its own executable.
    Worker(&'a Path),
}

/// What machine `machine`'s record chooses for a Sandbox, read by the
/// caller: `backend` `local` (a daemon process: native, or namespaces in a
/// Linux kernel, the docker machine's worker container's included, D-8),
/// the image digest (the call's `image`, else the record's) and the
/// machine's own image archive (`image_archive`, or an `image` that names a
/// file). On `docker` the record's image is its worker container's Docker
/// image, which names no image of the bundle's archive, so only the call's
/// is taken; the bundle's archive is the only one, and the choice names
/// the `docker` platform its Sandboxes keep showing.
pub fn choose(home: &Path, machine: &str, image: Option<&str>) -> Result<SdkValue> {
    let record = MachineRecord::read(home, machine)?
        .unwrap_or_else(|| MachineRecord::from_record(machine, SdkValue::Null));
    let docker = machine == "docker";
    let named = if docker {
        image
    } else {
        image.or(record.image())
    };
    let mut fields = vec![("backend", SdkValue::text("local"))];
    if let Some(digest) = named.filter(|i| is_digest(i)) {
        fields.push(("image", SdkValue::text(digest)));
    }
    if docker {
        fields.push(("platform", SdkValue::text("docker")));
    } else {
        let archive = record
            .text("image_archive")
            .or(record.image().filter(|i| i.starts_with('/')));
        if let Some(archive) = archive {
            fields.push(("archive", SdkValue::text(archive)));
        }
    }
    Ok(SdkValue::Map(
        fields.into_iter().map(|(n, v)| (n.to_owned(), v)).collect(),
    ))
}

/// The deploy settings for `choice` ([`choose`]) on `side`: a daemon
/// process (`native` on macOS, namespaces on Linux), and for namespaces
/// its image archive and digest.
pub fn resolve(side: &Side<'_>, choice: &SdkValue) -> Result<Vec<(String, SdkValue)>> {
    let mut fields = vec![("daemon".to_owned(), SdkValue::Text(daemon(side)?))];
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
    // the platform the machine implies for its Sandboxes (`docker`, D-8)
    if let Some(platform) = choice.get_text("platform") {
        fields.push(("platform".to_owned(), SdkValue::text(platform)));
    }
    Ok(fields)
}

/// Why built-in machine `machine`'s recorded daemon `path` cannot run (it
/// is not a file), and the call that records another: the Sandbox refusal
/// and `layerfs doctor` say the same.
pub fn missing_daemon(machine: &str, path: &str) -> String {
    format!(
        "machine {machine}'s recorded daemon {path} is not there; record another with \
         `layerfs machine update {machine} --daemon <path>`"
    )
}

/// The daemon a Sandbox runs, on `side`: in a worker, the one beside its
/// own executable (the bundle's); here (`native`), the machine record's
/// `daemon` (`import-env` writes it from 1.1.0's `LAYERFS_DAEMON`), else
/// this host's bundle's. A recorded daemon that is not there is refused
/// before any launch, naming the call that records another.
fn daemon(side: &Side<'_>) -> Result<String> {
    let machine = "native";
    let home = match side {
        Side::Worker(_) => return Ok(beside_self("layerfs-daemon")?.display().to_string()),
        Side::Home(home) => home,
    };
    let record = MachineRecord::read(home, machine)?;
    if let Some(path) = record.as_ref().and_then(MachineRecord::daemon) {
        if !Path::new(path).is_file() {
            return Err(refused("Unavailable", missing_daemon(machine, path)));
        }
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

/// Whether `image` is an image digest, `sha256:<64 hex>`.
fn is_digest(image: &str) -> bool {
    image.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
