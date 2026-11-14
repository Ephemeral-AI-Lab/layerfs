//! A machine's image cache, on the side that runs its Sandboxes (a worker,
//! or this process on native Linux): an image copied from Docker arrives as
//! one flat root filesystem tar in 1 MiB frames (`write`), staged as
//! `.recv-<hex>.part` beside the unpacked images and keyed by the Docker
//! image Id, then unpacked once into `<hex>.2` and deleted (`publish`). Each
//! unpacked image has a sidecar `<hex>.2.json` beside it: its Id, refs,
//! size, architecture, source, config and the time a Sandbox last used it.
//! The host keeps nothing; Docker is the source. A staged file is resumed
//! from its length when its bytes are a prefix of the next export
//! (`probe`'s `prefix_sha256`).
use crate::deploy::{self, Side};
use crate::host::Deploy;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
use layerfs_sandbox_backend::rootfs;
use layerfs_sandbox_backend::Image;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The largest frame of image bytes one `write` carries.
pub const CHUNK: usize = 1 << 20;
/// How recently a staged file must have grown for `probe` to say another
/// caller is still sending it.
const RECEIVING: Duration = Duration::from_secs(30);

/// Whether a value is a full lowercase digest, `sha256:<64 hex>`.
pub fn digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(hex)
}

fn hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The image cache under a home or worker root:
/// `<root>/machines/native/run/images`, where the namespaces backend
/// unpacks images (`rootfs::cache` of the runtime root).
pub fn cache(root: &Path) -> PathBuf {
    rootfs::cache(&paths::run_root(root))
}

/// Whether `deploy`'s runtime root already holds `image` unpacked: an image
/// copied from Docker needs no archive then.
pub fn unpacked(deploy: &Deploy, image: &Image) -> bool {
    deploy.local_dir.as_deref().is_some_and(|dir| {
        rootfs::cache(Path::new(dir))
            .join(format!("{}.2", image.hex()))
            .is_dir()
    })
}

/// The staged tar of image `hex` while it is received.
fn part(cache: &Path, hex: &str) -> PathBuf {
    cache.join(format!(".recv-{hex}.part"))
}

/// The sidecar of image `hex`'s unpacked copy.
pub fn sidecar_path(cache: &Path, hex: &str) -> PathBuf {
    cache.join(format!("{hex}.2.json"))
}

/// This machine's architecture as Docker names it (`arm64`, `amd64`).
pub fn architecture() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        other => other,
    }
}

/// Bytes free on the filesystem that holds `path` (or its nearest existing
/// parent), when it can be read.
pub fn free(path: &Path) -> Option<u64> {
    path.ancestors()
        .find(|p| p.exists())
        .and_then(|p| home::filesystem_available(p).ok())
}

/// One image's sidecar: what `machine.images` lists for it.
#[derive(Clone, Debug, PartialEq)]
pub struct Sidecar {
    /// `sha256:<64 hex>`: the Docker image Id, or an archive's config digest.
    pub id: String,
    /// Its references (`postgres:17-bookworm`), for display.
    pub refs: Vec<String>,
    /// Its size in bytes, when known.
    pub size: Option<u64>,
    /// Its architecture (`arm64`), when known.
    pub architecture: Option<String>,
    /// `imported` (from Docker), `bundle` or `machine` (an archive's).
    pub source: String,
    /// The image's config as Docker inspected it (`Env`, `WorkingDir`, ...).
    pub config: SdkValue,
    /// When a Sandbox was last created from it (UTC, RFC 3339).
    pub last_used: Option<String>,
}

impl Sidecar {
    /// Its JSON record.
    pub fn to_value(&self) -> SdkValue {
        let opt_text = |v: &Option<String>| v.clone().map_or(SdkValue::Null, SdkValue::Text);
        SdkValue::map([
            ("id", SdkValue::text(&self.id)),
            (
                "refs",
                SdkValue::List(self.refs.iter().cloned().map(SdkValue::Text).collect()),
            ),
            (
                "size",
                self.size.map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
            ),
            ("architecture", opt_text(&self.architecture)),
            ("source", SdkValue::text(&self.source)),
            ("config", self.config.clone()),
            ("last_used", opt_text(&self.last_used)),
        ])
    }

    /// Reads one from its JSON record; `None` when it names no valid Id.
    pub fn from_value(value: &SdkValue) -> Option<Self> {
        let id = value.get_text("id").filter(|id| digest(id))?.to_owned();
        Some(Self {
            id,
            refs: texts(value.get("refs")),
            size: value.get_int("size").map(|n| n.max(0) as u64),
            architecture: value.get_text("architecture").map(str::to_owned),
            source: value.get_text("source").unwrap_or("imported").to_owned(),
            config: value.get("config").cloned().unwrap_or(SdkValue::Null),
            last_used: value.get_text("last_used").map(str::to_owned),
        })
    }

    /// The sidecar of image `hex` in `cache`, if there is one.
    pub fn read(cache: &Path, hex: &str) -> Result<Option<Self>, SdkRefusal> {
        Ok(home::read_record(&sidecar_path(cache, hex))?
            .as_ref()
            .and_then(Self::from_value))
    }

    /// Writes it beside its image's directory (one rename).
    pub fn write(&self, cache: &Path) -> Result<(), SdkRefusal> {
        let hex = &self.id["sha256:".len()..];
        home::write_record(&sidecar_path(cache, hex), &self.to_value())
    }
}

/// The text items of a list value.
pub fn texts(value: Option<&SdkValue>) -> Vec<String> {
    match value {
        Some(SdkValue::List(items)) => items
            .iter()
            .filter_map(|v| match v {
                SdkValue::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `ms` since 1970 as UTC RFC 3339 text, `2026-11-14T09:30:00Z`.
pub fn utc(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil from days (proleptic Gregorian), Howard Hinnant's algorithm
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// Marks `image` used now (a Sandbox was just created from it): its
/// sidecar's `last_used`. An image unpacked from an archive (the bundle's,
/// or a VM's own) gets its sidecar here the first time, from what the
/// archive says of it. Nothing is written for an image not unpacked here.
pub fn touch(side: &Side<'_>, archive: Option<&str>, image: &str) -> Result<(), SdkRefusal> {
    let Some(hex) = image.strip_prefix("sha256:").filter(|h| hex(h)) else {
        return Ok(());
    };
    let root = match side {
        Side::Home(root) | Side::Worker(root) => root,
    };
    let cache = cache(root);
    if !cache.join(format!("{hex}.2")).is_dir() {
        return Ok(());
    }
    let mut sidecar = match Sidecar::read(&cache, hex)? {
        Some(sidecar) => sidecar,
        None => from_archive(side, archive, image)?.unwrap_or(Sidecar {
            id: image.to_owned(),
            refs: Vec::new(),
            size: None,
            architecture: None,
            source: "bundle".to_owned(),
            config: SdkValue::Null,
            last_used: None,
        }),
    };
    sidecar.last_used = Some(utc(home::now_ms()));
    sidecar.write(&cache)
}

/// What the archive a Sandbox of `side` reads (`archive`, the machine's own,
/// else the bundle's) says of `image`, as a sidecar.
pub fn from_archive(
    side: &Side<'_>,
    archive: Option<&str>,
    image: &str,
) -> Result<Option<Sidecar>, SdkRefusal> {
    let path = deploy::archive(side, archive)?;
    let Ok(entries) = rootfs::images(&path) else {
        return Ok(None);
    };
    let single = entries.len() == 1;
    Ok(entries
        .into_iter()
        .find(|entry| entry.digest.as_str() == image)
        .map(|entry| Sidecar {
            id: image.to_owned(),
            refs: entry.tags,
            size: single
                .then(|| fs::metadata(&path).ok().map(|m| m.len()))
                .flatten(),
            architecture: entry.architecture,
            source: if archive.is_some() { "machine" } else { "bundle" }.to_owned(),
            config: SdkValue::Null,
            last_used: None,
        }))
}

/// The sha256 of `path`'s bytes, as hex.
fn sha256_file(path: &Path) -> Result<String, SdkRefusal> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(failed)?;
    let mut hasher = Sha256::new();
    let mut bytes = vec![0u8; 1 << 16];
    loop {
        let count = file.read(&mut bytes).map_err(failed)?;
        if count == 0 {
            break;
        }
        hasher.update(&bytes[..count]);
    }
    Ok(layerfs_bridge::sdk_values::hex(&hasher.finalize()))
}

/// One step of an image copy on the machine (worker op `image.artifact`),
/// with no caller path accepted.
pub fn in_worker(root: &Path, args: &SdkValue) -> Outcome {
    step(&Side::Worker(root), args)
}

/// One step of an image copy on `side`: `probe`, `write`, `publish` or
/// `discard`.
pub fn step(side: &Side<'_>, args: &SdkValue) -> Outcome {
    let root = match side {
        Side::Home(root) | Side::Worker(root) => *root,
    };
    let cache = cache(root);
    let id = args.get_text("id").filter(|id| digest(id));
    let hex = id.map(|id| &id["sha256:".len()..]);
    match args.get_text("step").unwrap_or("") {
        "probe" => probe(side, &cache, id, args),
        "write" => {
            let hex = hex.ok_or_else(|| refused("InvalidInput", "image id"))?;
            let offset = args
                .get_int("offset")
                .filter(|n| *n >= 0)
                .ok_or_else(|| refused("InvalidInput", "image frame offset"))?
                as u64;
            let bytes = match args.get("bytes") {
                Some(SdkValue::Bytes(bytes)) if bytes.len() <= CHUNK => bytes,
                _ => return Err(refused("InvalidInput", "image frame bytes")),
            };
            write(&cache, hex, offset, bytes)?;
            Ok(SdkValue::Null)
        }
        "publish" => {
            let id = id.ok_or_else(|| refused("InvalidInput", "image id"))?;
            publish(&cache, id, args)
        }
        "discard" => {
            let hex = hex.ok_or_else(|| refused("InvalidInput", "image id"))?;
            match fs::remove_file(part(&cache, hex)) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(failed(e)),
                _ => Ok(SdkValue::Null),
            }
        }
        _ => Err(refused("InvalidInput", "unknown image step")),
    }
}

/// Whether this machine holds image `id` (unpacked, or in the archive its
/// Sandboxes read), or which image a sidecar names `ref` by; its
/// architecture, free bytes, and how much of `id` is staged.
fn probe(side: &Side<'_>, cache: &Path, id: Option<&str>, args: &SdkValue) -> Outcome {
    let mut out = vec![
        ("architecture".to_owned(), SdkValue::text(architecture())),
        (
            "free".to_owned(),
            free(cache).map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
        ),
    ];
    let id = match (id, args.get_text("ref")) {
        (Some(id), _) => Some(id.to_owned()),
        (None, Some(reference)) => named(cache, reference)?,
        (None, None) => None,
    };
    let Some(id) = id else {
        out.push(("present".to_owned(), SdkValue::Bool(false)));
        return Ok(SdkValue::Map(out));
    };
    let hex = &id["sha256:".len()..];
    // an archive that cannot be found or read holds nothing
    let present = cache.join(format!("{hex}.2")).is_dir()
        || deploy::archive(side, args.get_text("archive"))
            .ok()
            .and_then(|archive| rootfs::images(&archive).ok())
            .is_some_and(|entries| entries.iter().any(|e| e.digest.as_str() == id));
    out.push(("id".to_owned(), SdkValue::text(&id)));
    out.push(("present".to_owned(), SdkValue::Bool(present)));
    let staged = part(cache, hex);
    if let Ok(meta) = fs::symlink_metadata(&staged) {
        if meta.file_type().is_file() {
            let recent = meta
                .modified()
                .ok()
                .and_then(|m| SystemTime::now().duration_since(m).ok())
                .is_some_and(|age| age < RECEIVING);
            out.push(("offset".to_owned(), SdkValue::Int(meta.len() as i64)));
            out.push((
                "prefix_sha256".to_owned(),
                SdkValue::text(sha256_file(&staged)?),
            ));
            out.push(("receiving".to_owned(), SdkValue::Bool(recent)));
        }
    }
    Ok(SdkValue::Map(out))
}

/// The image whose sidecar lists `reference` among its refs.
fn named(cache: &Path, reference: &str) -> Result<Option<String>, SdkRefusal> {
    Ok(sidecars(cache)?
        .into_iter()
        .find(|s| s.refs.iter().any(|r| r == reference))
        .map(|s| s.id))
}

/// Every sidecar in `cache` whose image is unpacked there.
pub fn sidecars(cache: &Path) -> Result<Vec<Sidecar>, SdkRefusal> {
    let mut found = Vec::new();
    let entries = match fs::read_dir(cache) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(found),
        Err(e) => return Err(failed(e)),
    };
    for entry in entries {
        let name = entry.map_err(failed)?.file_name();
        let Some(hex) = name.to_str().and_then(|n| n.strip_suffix(".2.json")) else {
            continue;
        };
        if !(self::hex(hex) && cache.join(format!("{hex}.2")).is_dir()) {
            continue;
        }
        if let Some(sidecar) = Sidecar::read(cache, hex)? {
            found.push(sidecar);
        }
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(found)
}

/// Appends one frame at `offset` to image `hex`'s staged tar; offset 0
/// starts it over. A frame whose offset is not the staged length is
/// refused, so a lost reply's resend never doubles bytes.
pub fn write(cache: &Path, hex: &str, offset: u64, bytes: &[u8]) -> Result<(), SdkRefusal> {
    cache_dir(cache)?;
    let staged = part(cache, hex);
    let mut options = OpenOptions::new();
    options
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600);
    if offset == 0 {
        options.create(true).truncate(true);
    }
    let mut file = options.open(&staged).map_err(failed)?;
    if file.metadata().map_err(failed)?.len() != offset {
        return Err(refused(
            "InvalidInput",
            "image frames are out of order: the staged length is not the frame's offset",
        ));
    }
    file.seek(SeekFrom::Start(offset)).map_err(failed)?;
    file.write_all(bytes).map_err(failed)
}

/// The cache directory, made as the namespaces backend makes it (0700) when
/// it is not there yet.
fn cache_dir(cache: &Path) -> Result<(), SdkRefusal> {
    if cache.is_dir() {
        return Ok(());
    }
    if let Some(parent) = cache.parent() {
        fs::create_dir_all(parent).map_err(failed)?;
    }
    match fs::DirBuilder::new().create(cache) {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => return Err(failed(e)),
        _ => {}
    }
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(cache, fs::Permissions::from_mode(0o700)).map_err(failed)
}

/// Checks the staged tar of `id` against the sender's `size` and `sha256`,
/// unpacks it once into the cache, writes its sidecar and deletes the
/// staged file: `{ unpacked }`, false when another copy was published first.
fn publish(cache: &Path, id: &str, args: &SdkValue) -> Outcome {
    let hex = &id["sha256:".len()..];
    let staged = part(cache, hex);
    let size = args
        .get_int("size")
        .filter(|n| *n >= 0)
        .ok_or_else(|| refused("InvalidInput", "image size"))? as u64;
    let length = fs::symlink_metadata(&staged).map_err(failed)?.len();
    let sum = sha256_file(&staged)?;
    if length != size || Some(sum.as_str()) != args.get_text("sha256") {
        return Err(refused(
            "InvalidInput",
            "the staged image is not what was sent; it was kept for the next copy to resume or restart",
        ));
    }
    let image = Image::parse(id).map_err(|e| refused("InvalidInput", e.to_string()))?;
    let (_, unpacked) = rootfs::ensure_flat(&staged, &image, cache)
        .map_err(|e| refused("InvalidInput", format!("image {id}: {e}")))?;
    let old = Sidecar::read(cache, hex)?;
    let mut refs = texts(args.get("refs"));
    refs.extend(old.iter().flat_map(|s| s.refs.clone()));
    refs.sort();
    refs.dedup();
    Sidecar {
        id: id.to_owned(),
        refs,
        size: Some(size),
        architecture: args.get_text("architecture").map(str::to_owned),
        source: "imported".to_owned(),
        config: args.get("config").cloned().unwrap_or(SdkValue::Null),
        last_used: old.and_then(|s| s.last_used),
    }
    .write(cache)?;
    fs::remove_file(&staged).map_err(failed)?;
    Ok(SdkValue::map([("unpacked", SdkValue::Bool(unpacked))]))
}
