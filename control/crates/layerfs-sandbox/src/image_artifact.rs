//! A machine's image cache, on the side that runs its Sandboxes (a worker,
//! or this process on native Linux): an image copied from Docker arrives as
//! one flat root filesystem tar in 1 MiB frames (`write`), staged as
//! `.recv-<hex>.part` beside the unpacked images and keyed by the Docker
//! image Id, then unpacked once into `<hex>.2` and deleted (`publish`). Each
//! unpacked image has a sidecar `<hex>.2.json` beside it: its Id, refs,
//! size, architecture, source, config and the time a Sandbox last used it.
//! The host keeps nothing; Docker is the source. A staged file is resumed
//! from its length when its bytes are a prefix of the next export
//! (`probe`'s `prefix_sha256`). Whether another copy is still writing it is
//! the copy's own lock in the home (`image_lock::ImageLock`), never the
//! staged file's age.
use crate::deploy::{self, Side};
use crate::host::Deploy;
use crate::images::{lowers, refused_with};
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::home::{self, paths};
use layerfs_home::outcome::{failed, refused, Outcome};
use crate::rootfs;
use crate::Image;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// The largest frame of image bytes one `write` carries.
pub const CHUNK: usize = 1 << 20;

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

/// The image cache under a home or worker root: `images/` of its
/// run root ([`paths::run_root`]), where the namespaces backend unpacks
/// images (`rootfs::cache` of the runtime root).
pub fn cache(root: &Path) -> PathBuf {
    rootfs::cache(&paths::run_root(root))
}

/// Whether `deploy`'s runtime root already holds `image` unpacked and published
/// (both `<hex>.2` and its sidecar `<hex>.2.json`): an image copied from
/// Docker needs no archive then, and an interrupted unpack without a sidecar is
/// not treated as ready.
pub fn unpacked(deploy: &Deploy, image: &Image) -> bool {
    deploy.local_dir.as_deref().is_some_and(|dir| {
        let cache = rootfs::cache(Path::new(dir));
        let hex = image.hex();
        cache.join(format!("{hex}.2")).is_dir() && matches!(Sidecar::read(&cache, hex), Ok(Some(_)))
    })
}

/// Whether `value` is a 64-character lowercase SHA-256 hex digest.
pub fn is_hex(value: &str) -> bool {
    hex(value)
}

/// The 64-character lowercase digest hex of a staged part filename
/// (`.recv-<hex>.part`).
pub fn part_hex(name: &str) -> Option<&str> {
    let digest_hex = name.strip_prefix(".recv-")?.strip_suffix(".part")?;
    hex(digest_hex).then_some(digest_hex)
}

/// The staged tar of image `hex` while it is received.
pub fn part(cache: &Path, hex: &str) -> PathBuf {
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
                self.size
                    .map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
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

/// Extended sidecar record keeping optional `variant` alongside [`Sidecar`].
#[derive(Clone, Debug, PartialEq)]
pub struct SidecarInfo {
    /// Core sidecar metadata stored in `<hex>.2.json`.
    pub sidecar: Sidecar,
    /// Image variant (`v8`, ...), recorded only and never applied.
    pub variant: Option<String>,
}

impl SidecarInfo {
    /// Its JSON record, including optional `variant` and `config`.
    pub fn to_value(&self) -> SdkValue {
        let opt_text = |v: &Option<String>| v.clone().map_or(SdkValue::Null, SdkValue::Text);
        home::with_field(self.sidecar.to_value(), "variant", opt_text(&self.variant))
    }

    /// Reads one from its JSON record; `None` when it names no valid Id.
    pub fn from_value(value: &SdkValue) -> Option<Self> {
        let sidecar = Sidecar::from_value(value)?;
        let variant = value.get_text("variant").map(str::to_owned);
        Some(Self { sidecar, variant })
    }

    /// The extended sidecar of image `hex` in `cache`, if there is one.
    pub fn read(cache: &Path, hex: &str) -> Result<Option<Self>, SdkRefusal> {
        Ok(home::read_record(&sidecar_path(cache, hex))?
            .as_ref()
            .and_then(Self::from_value))
    }

    /// Writes it beside its image's directory (one rename).
    pub fn write(&self, cache: &Path) -> Result<(), SdkRefusal> {
        let hex = &self.sidecar.id["sha256:".len()..];
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
    let mut info = match SidecarInfo::read(&cache, hex)? {
        Some(info) => info,
        None => SidecarInfo {
            sidecar: from_archive(side, archive, image)?.unwrap_or(Sidecar {
                id: image.to_owned(),
                refs: Vec::new(),
                size: None,
                architecture: None,
                source: "bundle".to_owned(),
                config: SdkValue::Null,
                last_used: None,
            }),
            variant: None,
        },
    };
    info.sidecar.last_used = Some(utc(home::now_ms()));
    info.write(&cache)
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
            source: if archive.is_some() {
                "machine"
            } else {
                "bundle"
            }
            .to_owned(),
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
    Ok(layerfs_protocol::sdk::hex(&hasher.finalize()))
}

/// One step of an image copy on the machine (worker op `image.artifact`),
/// with no caller path accepted.
pub fn in_worker(root: &Path, args: &SdkValue) -> Outcome {
    step(&Side::Worker(root), args)
}

/// One step of an image copy on `side`: `probe`, `write`, `publish`,
/// `discard`, or `sweep` (what killed copies left, [`crate::image_sweep`]).
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
        // what killed copies left in this cache (spec v20)
        "sweep" => Ok(crate::image_sweep::temporaries(root, &cache, args)),
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
/// architecture, the image cache measured for free bytes and where it is,
/// and how much of `id` is staged (with its sha256 when `hash` asks).
fn probe(side: &Side<'_>, cache: &Path, id: Option<&str>, args: &SdkValue) -> Outcome {
    let mut out = vec![
        ("architecture".to_owned(), SdkValue::text(architecture())),
        (
            "free".to_owned(),
            free(cache).map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
        ),
        (
            "root".to_owned(),
            SdkValue::text(cache.to_string_lossy().as_ref()),
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
    let dir_ready = cache.join(format!("{hex}.2")).is_dir();
    let info = SidecarInfo::read(cache, hex)?;
    // an archive that cannot be found or read holds nothing
    let in_archive = deploy::archive(side, args.get_text("archive"))
        .ok()
        .and_then(|archive| rootfs::images(&archive).ok())
        .is_some_and(|entries| entries.iter().any(|e| e.digest.as_str() == id));
    // Interrupted publish (renamed `<hex>.2` without `<hex>.2.json`, and not from
    // an archive) stays unpublished so recovery removes the stranded dir and
    // republishes cleanly under lock.
    let present = (dir_ready && info.is_some()) || in_archive;
    out.push(("id".to_owned(), SdkValue::text(&id)));
    out.push(("present".to_owned(), SdkValue::Bool(present)));
    if dir_ready && info.is_none() && !in_archive {
        out.push(("stranded".to_owned(), SdkValue::Bool(true)));
    }
    // the size `machine.images` lists, so a reused load answers the same
    if let Some(info) = info {
        if let Some(size) = info.sidecar.size {
            out.push(("size".to_owned(), SdkValue::Int(size as i64)));
        }
        if let Some(variant) = info.variant {
            out.push(("variant".to_owned(), SdkValue::text(variant)));
        }
        if !matches!(info.sidecar.config, SdkValue::Null) {
            out.push(("config".to_owned(), info.sidecar.config));
        }
    }
    // the staged bytes are hashed only when the copy asks (`hash`): once per
    // load, on the probe it resumes from
    let staged = part(cache, hex);
    if let Ok(meta) = fs::symlink_metadata(&staged) {
        if meta.file_type().is_file() {
            out.push(("offset".to_owned(), SdkValue::Int(meta.len() as i64)));
            if args.get_bool("hash") {
                out.push((
                    "prefix_sha256".to_owned(),
                    SdkValue::text(sha256_file(&staged)?),
                ));
            }
        }
    }
    Ok(SdkValue::Map(out))
}

/// The image whose sidecar lists `reference` among its refs; refuses when
/// multiple cached images claim the same reference.
fn named(cache: &Path, reference: &str) -> Result<Option<String>, SdkRefusal> {
    let mut matches: Vec<String> = sidecars(cache)?
        .into_iter()
        .filter(|s| s.refs.iter().any(|r| r == reference))
        .map(|s| s.id)
        .collect();
    matches.sort();
    matches.dedup();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches.remove(0))),
        _ => Err(refused_with(
            "InvalidInput",
            format!(
                "image reference `{reference}` is ambiguous across cached images: {}; remove one with `machine.prune`",
                matches.join(", ")
            ),
            SdkValue::map([(
                "images",
                SdkValue::List(matches.into_iter().map(SdkValue::Text).collect()),
            )]),
        )),
    }
}

/// Every extended sidecar in `cache` whose image directory `<hex>.2` exists.
pub fn sidecar_infos(cache: &Path) -> Result<Vec<SidecarInfo>, SdkRefusal> {
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
        if let Some(info) = SidecarInfo::read(cache, hex)? {
            found.push(info);
        }
    }
    found.sort_by(|a, b| a.sidecar.id.cmp(&b.sidecar.id));
    Ok(found)
}

/// Every sidecar in `cache` whose image is unpacked there.
pub fn sidecars(cache: &Path) -> Result<Vec<Sidecar>, SdkRefusal> {
    Ok(sidecar_infos(cache)?
        .into_iter()
        .map(|info| info.sidecar)
        .collect())
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
            "Failed",
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
            "Failed",
            "the staged image is not what was sent; it was kept for the next copy to resume or restart",
        ));
    }
    let image = Image::parse(id).map_err(|e| refused("InvalidInput", e.to_string()))?;
    let old = SidecarInfo::read(cache, hex)?;
    // Option B interrupted-publish recovery: `<hex>.2` without `<hex>.2.json`
    // means an earlier publish stopped after rename; remove the stranded tree
    // (unless in use) so `ensure_flat` unpacks cleanly before committing the sidecar.
    if old.is_none() && cache.join(format!("{hex}.2")).is_dir() {
        let users = lowers(cache.parent().unwrap_or(cache))?;
        let removed = rootfs::remove(&image, cache, |path| users.iter().any(|l| l == path))
            .map_err(failed)?;
        if !removed {
            let dir_path = cache.join(format!("{hex}.2"));
            let count = users.iter().filter(|l| **l == dir_path).count().max(1);
            return Err(refused_with(
                "InUse",
                format!("image {id} has a stranded directory in use on this machine"),
                SdkValue::map([("count", SdkValue::Int(count as i64))]),
            ));
        }
    }
    let (_, unpacked) = rootfs::ensure_flat(&staged, &image, cache)
        .map_err(|e| refused("InvalidInput", format!("image {id}: {e}")))?;
    let mut refs = texts(args.get("refs"));
    refs.extend(old.iter().flat_map(|s| s.sidecar.refs.clone()));
    refs.sort();
    refs.dedup();
    let variant = args
        .get_text("variant")
        .map(str::to_owned)
        .or_else(|| old.as_ref().and_then(|s| s.variant.clone()));
    let config = match args.get("config") {
        Some(c) if !matches!(c, SdkValue::Null) => c.clone(),
        _ => old
            .as_ref()
            .map(|s| s.sidecar.config.clone())
            .unwrap_or(SdkValue::Null),
    };
    strip_moved_refs(cache, hex, &refs)?;
    SidecarInfo {
        sidecar: Sidecar {
            id: id.to_owned(),
            refs,
            size: Some(size),
            architecture: args.get_text("architecture").map(str::to_owned),
            source: "imported".to_owned(),
            config: config.clone(),
            last_used: old.and_then(|s| s.sidecar.last_used),
        },
        variant: variant.clone(),
    }
    .write(cache)?;
    fs::remove_file(&staged).map_err(failed)?;
    let mut out = vec![("unpacked".to_owned(), SdkValue::Bool(unpacked))];
    if let Some(variant) = variant {
        out.push(("variant".to_owned(), SdkValue::text(variant)));
    }
    if !matches!(config, SdkValue::Null) {
        out.push(("config".to_owned(), config));
    }
    Ok(SdkValue::Map(out))
}

/// Removes any ref in `claimed` from other sidecars in `cache` so moved tags
/// never stay bound to stale images.
fn strip_moved_refs(
    cache: &Path,
    published_hex: &str,
    claimed: &[String],
) -> Result<(), SdkRefusal> {
    if claimed.is_empty() {
        return Ok(());
    }
    let entries = match fs::read_dir(cache) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(failed(e)),
    };
    for entry in entries {
        let name = entry.map_err(failed)?.file_name();
        let Some(other_hex) = name.to_str().and_then(|n| n.strip_suffix(".2.json")) else {
            continue;
        };
        if !hex(other_hex) || other_hex == published_hex {
            continue;
        }
        let Ok(Some(mut other)) = SidecarInfo::read(cache, other_hex) else {
            continue;
        };
        let before = other.sidecar.refs.len();
        other
            .sidecar
            .refs
            .retain(|r| !claimed.iter().any(|c| c == r));
        if other.sidecar.refs.len() != before {
            other.write(cache)?;
        }
    }
    Ok(())
}
