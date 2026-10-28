//! Private, permission-authorized project discovery on Windows: the Store
//! face of the unix `discovery` module. Sandbox discovery and daemon
//! operation are refused `Unsupported`; the native Store route serves
//! project, fs and branch calls in-process.
//!
//! Windows cannot express the unix entry's owner/mode privacy check; a
//! symlink and an unexpected entry kind are still refused, and hardening the
//! home tree's ACLs is an open owner question. The discovery turn keeps the
//! unix discipline: this process's calls take turns on one directory, and
//! between processes one named kernel mutex per directory is held, released
//! by the kernel when its holder exits.
use crate::{
    job::{failed, refused},
    registry::{self, Project},
};
use layerfs_bridge::{
    control::{Reply, SdkRefusal, SdkValue},
    provision::StoreManifest,
};
use layerfs_persistence::{SealedStore, SqlitePersistenceProfile};
use std::{
    collections::BTreeSet,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, SdkRefusal>;

pub(crate) fn home(args: &SdkValue) -> Result<PathBuf> {
    let home = args
        .get_text("home")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LAYERFS_HOME").map(PathBuf::from))
        .ok_or_else(|| refused("InvalidInput", "LAYERFS_HOME is required"))?;
    if !home.is_absolute() {
        return Err(refused("InvalidInput", "LAYERFS_HOME must be absolute"));
    }
    directory(&home)?;
    Ok(home)
}

/// A private entry on Windows: no symlink and the expected kind. Owner and
/// mode bits have no Windows reading here; hardening the home tree's ACLs is
/// an open owner question.
fn private(path: &Path, dir: bool) -> Result<()> {
    let meta = fs::symlink_metadata(path).map_err(failed)?;
    if meta.file_type().is_symlink() || meta.is_dir() != dir {
        return Err(refused(
            "Unauthorized",
            format!(
                "{} must be a private {} entry",
                path.display(),
                if dir { "directory" } else { "file" }
            ),
        ));
    }
    Ok(())
}

pub(crate) fn directory(path: &Path) -> Result<()> {
    match fs::DirBuilder::new().create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(failed(e)),
    }
    private(path, true)
}

pub(crate) fn project_dir(args: &SdkValue) -> Result<PathBuf> {
    let name = registry::text(args, "project")?;
    layerfs_history::HistoryName::new(name).map_err(failed)?;
    let path = home(args)?.join(name);
    private(&path, true)?;
    Ok(path)
}

/// Calls take turns on one directory: in this process before its mutex, and
/// between processes on the mutex itself. The wait is readiness, bounded,
/// and nothing has been attempted before it ends.
const IN_PROCESS_WAIT: Duration = Duration::from_secs(60);
static HELD: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());
static FREED: Condvar = Condvar::new();

/// One discovery mutation's ownership of a directory: this process's turn
/// and the named kernel mutex. Dropping it releases both.
pub(crate) struct Guard {
    handle: usize,
    directory: PathBuf,
}
impl Drop for Guard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::System::Threading::ReleaseMutex(self.handle as _);
            windows_sys::Win32::Foundation::CloseHandle(self.handle as _);
        }
        let mut held = HELD.lock().unwrap_or_else(|error| error.into_inner());
        held.remove(&self.directory);
        FREED.notify_all();
    }
}

/// One mutex name per directory; the canonical path identifies it between
/// processes. SHA-256 truncated, not `DefaultHasher`: the hasher's algorithm
/// may change between Rust releases and silently split one directory onto
/// two guards across builds (K3 review F1, with the persistence lane name).
fn mutex_name(directory: &Path) -> String {
    use sha2::{Digest, Sha256};
    let resolved = fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
    let digest = Sha256::digest(resolved.as_os_str().as_encoded_bytes());
    let mut tag = [0u8; 8];
    tag.copy_from_slice(&digest[..8]);
    format!("Local\\layerfs-discovery-{:016x}", u64::from_be_bytes(tag))
}

pub(crate) fn guard(directory: &Path) -> Result<Guard> {
    let deadline = Instant::now() + IN_PROCESS_WAIT;
    let mut held = registry::lock(&HELD)?;
    while held.contains(directory) {
        let remaining = deadline.saturating_duration_since(Instant::now());
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
        Ok(handle) => Ok(Guard {
            handle,
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

/// The named kernel mutex of a directory, waited for until `deadline`; a
/// holder process that exits releases it.
fn os_lock(directory: &Path, deadline: Instant) -> Result<usize> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
    let name: Vec<u16> = mutex_name(directory)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(refused("Failed", "the discovery mutex cannot open"));
    }
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            unsafe { CloseHandle(handle) };
            return Err(refused(
                "Busy",
                "another process owns this discovery mutation",
            ));
        }
        let waited = unsafe {
            WaitForSingleObject(handle, remaining.as_millis().min(u32::MAX as u128) as u32)
        };
        match waited {
            WAIT_OBJECT_0 | WAIT_ABANDONED => return Ok(handle as usize),
            WAIT_TIMEOUT => {
                unsafe { CloseHandle(handle) };
                return Err(refused(
                    "Busy",
                    "another process owns this discovery mutation",
                ));
            }
            _ => {
                unsafe { CloseHandle(handle) };
                return Err(refused("Failed", "the discovery mutex cannot be held"));
            }
        }
    }
}

pub(crate) fn save(path: &Path, value: SdkValue) -> Result<()> {
    let bytes = layerfs_bridge::control::Answer {
        id: 1,
        reply: Reply::Sdk(value),
    }
    .encode()
    .map_err(failed)?;
    let temp = path.with_extension(format!(
        "{}-{}",
        std::process::id(),
        layerfs_bridge::sdk_values::hex(&registry::random()?[..8])
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(failed)?;
    file.write_all(&bytes).map_err(failed)?;
    drop(file);
    fs::rename(temp, path).map_err(failed)
}

pub(crate) fn load(path: &Path) -> Result<SdkValue> {
    private(path, false)?;
    let file = OpenOptions::new().read(true).open(path).map_err(failed)?;
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

pub(crate) fn save_project(path: &Path, project: &Project) -> Result<()> {
    save(
        &path.join("project.record"),
        SdkValue::map([
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
        ]),
    )
}

pub(crate) fn project(args: &SdkValue) -> Result<Project> {
    let path = project_dir(args)?;
    let value = load(&path.join("project.record"))?;
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
