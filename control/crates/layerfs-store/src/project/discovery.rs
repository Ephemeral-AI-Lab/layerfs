//! A project's home entry and the private, permission-authorized discovery
//! records on the side that runs a call (#506, #536 §5): the home a call
//! names, a project's folder and record, the turn calls take on one folder,
//! and the record file format (moved from `sandbox/files.rs`,
//! layerfs-project#105).
use layerfs_bridge::{
    control::{Reply, SdkRefusal, SdkValue},
    provision::StoreManifest,
};
use layerfs_home::helpers;
use layerfs_home::outcome::{failed, refused};
use layerfs_persistence::{SealedStore, SqlitePersistenceProfile};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};
type Result<T> = std::result::Result<T, SdkRefusal>;

/// One project, read from or saved to its home entry.
pub struct Project {
    /// Its sealed Store file.
    pub store: SealedStore,
    /// Its Store manifest.
    pub manifest: StoreManifest,
    /// The installed manifest once Import installed the Store in the volume.
    pub installed: Option<StoreManifest>,
}

/// The home a call names, else `LAYERFS_HOME`: absolute, and a private
/// directory of this user's, made if absent.
pub fn home_of(home: Option<&Path>) -> Result<PathBuf> {
    let home = home
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("LAYERFS_HOME").map(PathBuf::from))
        .ok_or_else(|| refused("InvalidInput", "LAYERFS_HOME is required"))?;
    if !home.is_absolute() {
        return Err(refused("InvalidInput", "LAYERFS_HOME must be absolute"));
    }
    directory(&home)?;
    Ok(home)
}
/// `path` is this user's and nobody else may read or change it. A folder
/// may let others pass through by name (o+x), as the folders above a
/// namespaces Sandbox's local root must ([`layerfs_home::home::traversable`]).
pub fn private(path: &Path, dir: bool) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(failed)?;
    let others = if dir { 0o076 } else { 0o077 };
    if meta.file_type().is_symlink()
        || meta.is_dir() != dir
        || (!dir && !meta.is_file())
        || meta.uid() != layerfs_home::home::flock::euid()
        || meta.mode() & others != 0
    {
        return Err(refused(
            "Unauthorized",
            format!("{} must be owned by this user and private", path.display()),
        ));
    }
    Ok(())
}
pub fn directory(path: &Path) -> Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(failed(e)),
    }
    private(path, true)
}
/// A project's home entry (#506 Store-first): `<home>/<project>/` for a
/// call that names no `store`, and for the one project of a Store made by
/// an Init of its own (the Store has its project's name and no Store
/// record); `<home>/.stores/<store>/projects/<project>/` for every other
/// project of a Store.
pub fn project_dir(args: &SdkValue) -> Result<PathBuf> {
    let path = project_entry(args)?;
    private(&path, true)?;
    Ok(path)
}

/// [`project_dir`]'s path, not checked to exist.
pub fn project_entry(args: &SdkValue) -> Result<PathBuf> {
    let name = helpers::text(args, "project")?;
    layerfs_history::HistoryName::new(name).map_err(failed)?;
    let home = home_of(args.get_text("home").map(Path::new))?;
    // 2.0 (#533 step 4): every project is named with its Store
    let store = args.get_text("store").unwrap_or(name);
    crate::identity::store_dir(&home, store)?;
    Ok(layerfs_home::home::paths::project_dir(&home, store, name))
}
/// Calls take turns on one directory: in this process before its OS lock,
/// and between processes on the lock itself. The wait is readiness, bounded,
/// and nothing has been attempted before it ends.
// Unmount exchanges run outside this guard; one shared readiness deadline
// covers the in-process turn and the cross-process discovery lock.
const IN_PROCESS_WAIT: Duration = Duration::from_secs(60);
/// Between two looks at another process's lock.
const LOCK_PAUSE: Duration = Duration::from_millis(2);
static HELD: Mutex<std::collections::BTreeSet<PathBuf>> =
    Mutex::new(std::collections::BTreeSet::new());
static FREED: std::sync::Condvar = std::sync::Condvar::new();

/// One discovery mutation's ownership of a directory: this process's turn and
/// the OS lock. Dropping it releases both.
pub struct Guard {
    file: File,
    directory: PathBuf,
}
impl Drop for Guard {
    fn drop(&mut self) {
        // The lock belongs to the open file description, which a child forked
        // meanwhile shares until its exec: closing only this descriptor would
        // leave it held (F-053). Unlock explicitly; the close follows.
        layerfs_home::home::flock::unlock(&self.file);
        let mut held = HELD.lock().unwrap_or_else(|error| error.into_inner());
        held.remove(&self.directory);
        FREED.notify_all();
    }
}

/// Another call of this process, or another process holding the OS lock,
/// is waited for until the bounded wait ends; then the call is refused Busy,
/// which is not retried.
pub fn guard(directory: &Path) -> Result<Guard> {
    let deadline = std::time::Instant::now() + IN_PROCESS_WAIT;
    let mut held = helpers::lock(&HELD)?;
    while held.contains(directory) {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(refused(
                "Busy",
                "another call of this process owns this discovery mutation",
            ));
        }
        held = FREED
            .wait_timeout(held, remaining)
            .map_err(|_| refused("Failed", "discovery turn poisoned"))?
            .0;
    }
    held.insert(directory.to_owned());
    drop(held);
    match os_lock(directory, deadline) {
        Ok(file) => Ok(Guard {
            file,
            directory: directory.to_owned(),
        }),
        Err(refusal) => {
            let mut held = HELD.lock().unwrap_or_else(|error| error.into_inner());
            held.remove(directory);
            FREED.notify_all();
            Err(refusal)
        }
    }
}
/// The file of a directory's discovery turn. It is not `lock`: on `native`
/// a Sandbox's directory is the home's `sandbox/<name>/`, where `lock` is
/// the Sandbox lock (#536 §8), which is short and never held across a
/// daemon call, while this turn is held across a start, a mount or a close.
pub const TURN: &str = "serve.lock";

/// The OS lock, waited for until `deadline`; it is released even if its
/// process exits.
fn os_lock(directory: &Path, deadline: std::time::Instant) -> Result<File> {
    let path = directory.join(TURN);
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(no_follow())
        .open(&path)
        .map_err(failed)?;
    private(&path, false)?;
    while !layerfs_home::home::flock::try_lock(&file, true).map_err(failed)? {
        if std::time::Instant::now() >= deadline {
            return Err(refused(
                "Busy",
                "another process owns this discovery mutation",
            ));
        }
        std::thread::sleep(LOCK_PAUSE);
    }
    Ok(file)
}
#[cfg(target_os = "macos")]
fn no_follow() -> i32 {
    0x100
}
#[cfg(target_os = "linux")]
fn no_follow() -> i32 {
    0x20000
}

pub fn save(path: &Path, value: SdkValue) -> Result<()> {
    let bytes = layerfs_bridge::control::Answer {
        id: 1,
        reply: Reply::Sdk(value),
    }
    .encode()
    .map_err(failed)?;
    let temp = path.with_extension(format!(
        "{}-{}",
        std::process::id(),
        layerfs_bridge::sdk_values::hex(&helpers::random()?[..8])
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(no_follow())
        .open(&temp)
        .map_err(failed)?;
    file.write_all(&bytes).map_err(failed)?;
    drop(file);
    fs::rename(temp, path).map_err(failed)
}
pub fn load(path: &Path) -> Result<SdkValue> {
    private(path, false)?;
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(no_follow())
        .open(path)
        .map_err(failed)?;
    if file.metadata().map_err(failed)?.len() > 65535 {
        return Err(refused("InvalidInput", "discovery record too large"));
    }
    let mut bytes = Vec::new();
    file.take(65536).read_to_end(&mut bytes).map_err(failed)?;
    match layerfs_bridge::control::Answer::decode(&bytes)
        .map_err(failed)?
        .reply
    {
        Reply::Sdk(value) => Ok(value),
        _ => Err(refused("InvalidInput", "discovery record kind")),
    }
}
pub fn save_project(path: &Path, project: &Project) -> Result<()> {
    // the record is read, changed and written whole under the lock of the
    // home it is in: `<home>/store/<store>/project/<name>/`
    let home = path
        .ancestors()
        .nth(4)
        .filter(|_| path.parent().and_then(Path::file_name) == Some("project".as_ref()));
    let _lock = home.map(layerfs_home::home::lock::take).transpose()?;
    let old = layerfs_home::home::read_record(&path.join("record.json"))?.unwrap_or_default();
    let mut record = SdkValue::map([
        (
            "manifest",
            SdkValue::Bytes(project.manifest.encode().map_err(failed)?),
        ),
        (
            "installed",
            project
                .installed
                .as_ref()
                .map(|m| m.encode().map(SdkValue::Bytes))
                .transpose()
                .map_err(failed)?
                .unwrap_or_default(),
        ),
    ]);
    // the record's other fields (its name and Store) are kept
    if let SdkValue::Map(fields) = old {
        for (name, value) in fields {
            if name != "manifest" && name != "installed" {
                record = layerfs_home::home::with_field(record, &name, value);
            }
        }
    }
    layerfs_home::home::write_entity(&path.join("record.json"), &record)
}
pub fn project(args: &SdkValue) -> Result<Project> {
    let path = project_dir(args)?;
    let value = layerfs_home::home::read_record(&path.join("record.json"))?
        .ok_or_else(|| refused("NoSuchProject", "no project record"))?;
    let decode = |name| match value.get(name) {
        Some(SdkValue::Bytes(bytes)) => StoreManifest::decode(bytes).map_err(failed),
        _ => Err(refused("NoSuchProject", "no complete project manifest")),
    };
    let manifest = decode("manifest")?;
    if manifest.profile != layerfs_bridge::provision::StoreProfile::Disposable {
        return Err(refused("InvalidInput", "Disposable required"));
    }
    let installed = if matches!(value.get("installed"), Some(SdkValue::Bytes(_))) {
        Some(decode("installed")?)
    } else {
        None
    };
    Ok(Project {
        store: SealedStore {
            path: path.join("sealed.sqlite"),
            profile: SqlitePersistenceProfile::Disposable,
            sqlite_version: manifest.host_sqlite.clone(),
            bytes: manifest.bytes,
        },
        manifest,
        installed,
    })
}
