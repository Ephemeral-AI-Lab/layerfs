//! Open an installed Store in this process and run the public Store table.

use crate::config::Config;
use crate::error::LayerFsError;
use crate::value;
use layerfs_bridge::control::{Answer as ControlAnswer, Reply, SdkCall, SdkValue};
use layerfs_bridge::provision::{StoreManifest, StoreProfile};
use layerfs_store_service::{OpenRefusal, StoreService};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const RECORD_LIMIT: u64 = 65_535;
static OVERLAY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const STORE_METHODS: &[&str] = &[
    "project.list_branches",
    "fs.read",
    "fs.list",
    "fs.stat",
    "fs.grep",
    "fs.glob",
    "fs.diff_files",
    "fs.blame",
    "fs.write",
    "fs.edit",
    "fs.apply",
    "fs.mkdir",
    "fs.rm",
    "fs.cp",
    "fs.mv",
    "branch.fork",
    "branch.list_commits",
    "branch.diff_commits",
    "branch.diff_branches",
    "branch.merge",
    "branch.publish",
];

/// Public integration state for one project.
#[derive(Clone, Debug)]
pub struct Info {
    /// LayerFS project name.
    pub project: String,
}

struct Opened {
    service: Option<StoreService>,
    overlay: PathBuf,
}

impl Opened {
    fn close(mut self) -> Result<(), LayerFsError> {
        let service = self.service.take();
        let overlay = std::mem::take(&mut self.overlay);
        let mut result = Ok(());
        if let Some(service) = service {
            if let Err(error) = service.close() {
                result = Err(LayerFsError::refused(
                    "Failed",
                    format!("Store close: {error}"),
                    Value::Null,
                ));
            }
        }
        if let Err(error) = std::fs::remove_dir_all(&overlay) {
            if result.is_ok() {
                result = Err(LayerFsError::Io(error));
            }
        }
        result
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        if let Some(service) = self.service.take() {
            let _ = service.close();
        }
        if !self.overlay.as_os_str().is_empty() {
            let _ = std::fs::remove_dir_all(&self.overlay);
        }
    }
}

struct Inner {
    config: Config,
    opening: tokio::sync::Mutex<()>,
    state: Mutex<Option<Arc<Opened>>>,
    closed: AtomicBool,
}

/// One project's Store service. The Store is opened lazily on the first call;
/// `close` disables new calls and releases the Store. No shared Store registry
/// is kept; only a process-local temp-name sequence is global.
#[derive(Clone)]
pub struct LayerFs {
    inner: Arc<Inner>,
}

impl LayerFs {
    /// Validates configuration; performs no Store I/O.
    pub fn new(config: Config) -> Result<Self, LayerFsError> {
        config.validate()?;
        Ok(Self {
            inner: Arc::new(Inner {
                config,
                opening: tokio::sync::Mutex::new(()),
                state: Mutex::new(None),
                closed: AtomicBool::new(false),
            }),
        })
    }

    /// The project name. Does not open the Store and remains available after
    /// `close`.
    pub async fn info(&self) -> Info {
        Info {
            project: self.inner.config.project.clone(),
        }
    }

    /// Runs one public Store method once.
    pub async fn call(&self, method: String, args: Value) -> Result<Value, LayerFsError> {
        self.ensure_open()?;
        let method = exact_method(&method)?.to_owned();
        let opened = self.service().await?;
        let project = self.inner.config.project.clone();
        tokio::task::spawn_blocking(move || {
            let args = with_project(value::to_sdk(args)?, &project)?;
            let service = opened
                .service
                .as_ref()
                .ok_or_else(|| LayerFsError::invalid("layerfs service is closed"))?;
            let value = layerfs_api::call(service, &SdkCall { method, args })?;
            Ok(value::to_json(&value))
        })
        .await
        .map_err(|error| LayerFsError::Join(error.to_string()))?
    }

    /// Disables new calls and releases the Store when the last in-flight call
    /// ends. Idempotent.
    pub async fn close(&self) -> Result<(), LayerFsError> {
        self.inner.closed.store(true, Ordering::Release);
        let _opening = self.inner.opening.lock().await;
        let opened = {
            let mut state = self.state();
            state.take()
        };
        if let Some(opened) = opened {
            let result = tokio::task::spawn_blocking(move || match Arc::try_unwrap(opened) {
                Ok(opened) => opened.close(),
                // In-flight calls keep the Opened alive; their drop performs
                // the deferred cleanup.
                Err(_) => Ok(()),
            })
            .await
            .map_err(|error| LayerFsError::Join(error.to_string()))?;
            result?;
        }
        Ok(())
    }

    async fn service(&self) -> Result<Arc<Opened>, LayerFsError> {
        self.ensure_open()?;
        if let Some(opened) = self.state().as_ref().cloned() {
            return Ok(opened);
        }
        let _opening = self.inner.opening.lock().await;
        self.ensure_open()?;
        if let Some(opened) = self.state().as_ref().cloned() {
            return Ok(opened);
        }
        let config = self.inner.config.clone();
        let opened = tokio::task::spawn_blocking(move || open_store(&config))
            .await
            .map_err(|error| LayerFsError::Join(error.to_string()))??;
        if let Err(error) = self.ensure_open() {
            let _ = tokio::task::spawn_blocking(move || drop(opened)).await;
            return Err(error);
        }
        *self.state() = Some(opened.clone());
        Ok(opened)
    }

    fn ensure_open(&self) -> Result<(), LayerFsError> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(LayerFsError::invalid("layerfs service is closed"));
        }
        Ok(())
    }

    fn state(&self) -> std::sync::MutexGuard<'_, Option<Arc<Opened>>> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(opened) = state.take() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn_blocking(move || drop(opened));
            } else {
                drop(opened);
            }
        }
    }
}

fn exact_method(method: &str) -> Result<&'static str, LayerFsError> {
    STORE_METHODS
        .iter()
        .find(|known| **known == method)
        .copied()
        .ok_or_else(|| {
            LayerFsError::refused(
                "UnknownMethod",
                format!("no Store method {method}"),
                Value::Null,
            )
        })
}

fn with_project(args: SdkValue, project: &str) -> Result<SdkValue, LayerFsError> {
    let SdkValue::Map(mut fields) = args else {
        return Err(LayerFsError::invalid("arguments must be an object"));
    };
    match fields.iter_mut().find(|(name, _)| name == "project") {
        Some((_, value)) => *value = SdkValue::Text(project.to_owned()),
        None => fields.push(("project".to_owned(), SdkValue::Text(project.to_owned()))),
    }
    Ok(SdkValue::Map(fields))
}

fn open_store(config: &Config) -> Result<Arc<Opened>, LayerFsError> {
    let record = read_record(config)?;
    let mut overlay = OverlayGuard::new(unique_overlay());
    make_private_dir(overlay.path())?;
    let canonical = std::fs::canonicalize(overlay.path())?;
    overlay.replace(canonical.clone());
    let template = canonical.join("overlay.sqlite");
    match StoreService::open_direct(
        &config.store_path,
        &record.manifest.binding,
        record.manifest.cursor_key,
        &template,
    ) {
        Ok(service) => Ok(Arc::new(Opened {
            service: Some(service),
            overlay: overlay.disarm(),
        })),
        Err(refusal) => Err(match refusal {
            OpenRefusal::Unsupported(reason) => LayerFsError::unsupported(reason.0),
            OpenRefusal::Missing(reason) => LayerFsError::refused("NoStore", reason, Value::Null),
            OpenRefusal::Failed(reason) => LayerFsError::refused("Failed", reason, Value::Null),
        }),
    }
}

struct OverlayGuard(Option<PathBuf>);

impl OverlayGuard {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }
    fn path(&self) -> &Path {
        self.0.as_deref().expect("overlay path")
    }
    fn replace(&mut self, path: PathBuf) {
        self.0 = Some(path);
    }
    fn disarm(mut self) -> PathBuf {
        self.0.take().expect("overlay path")
    }
}

impl Drop for OverlayGuard {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = std::fs::remove_dir_all(path);
        }
    }
}

fn unique_overlay() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let sequence = OVERLAY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "layerfs-rutis-{}-{}-{}",
        std::process::id(),
        nanos,
        sequence
    ))
}

struct Record {
    manifest: StoreManifest,
}

fn read_record(config: &Config) -> Result<Record, LayerFsError> {
    let path = config.home.join(&config.project).join("project.record");
    let bytes = read_private(&path)?;
    let answer =
        ControlAnswer::decode(&bytes).map_err(|error| LayerFsError::Control(error.to_string()))?;
    let Reply::Sdk(value) = answer.reply else {
        return Err(LayerFsError::invalid("project record is not an SDK value"));
    };
    let SdkValue::Map(fields) = value else {
        return Err(LayerFsError::invalid("project record is not a map"));
    };
    let manifest = decode_manifest(manifest_bytes(&fields)?)?;
    manifest
        .check()
        .map_err(|error| LayerFsError::Control(error.into()))?;
    if manifest.profile != StoreProfile::Disposable {
        return Err(LayerFsError::invalid("Disposable Store required"));
    }
    if manifest.daemon_sqlite.is_some() {
        return Err(LayerFsError::Control(
            "sealed manifest already names a daemon SQLite".into(),
        ));
    }
    let installed = match fields
        .iter()
        .find(|(name, _)| name == "installed")
        .map(|(_, value)| value)
    {
        None | Some(SdkValue::Null) => {
            return Err(LayerFsError::refused(
                "NoStore",
                "the project has no installed Store",
                Value::Null,
            ));
        }
        Some(SdkValue::Bytes(bytes)) if !bytes.is_empty() => decode_manifest(bytes)?,
        Some(SdkValue::Bytes(_)) => {
            return Err(LayerFsError::invalid(
                "the installed manifest is an empty value",
            ));
        }
        Some(_) => {
            return Err(LayerFsError::invalid(
                "the installed manifest has the wrong type",
            ));
        }
    };
    installed
        .check()
        .map_err(|error| LayerFsError::Control(error.into()))?;
    if installed.daemon_sqlite.is_none() {
        return Err(LayerFsError::Control(
            "the installed manifest has no daemon SQLite".into(),
        ));
    }
    let mut expected = manifest.clone();
    expected.daemon_sqlite = installed.daemon_sqlite.clone();
    if installed != expected {
        return Err(LayerFsError::Control(
            "installed manifest does not match the project".into(),
        ));
    }
    Ok(Record { manifest })
}

fn manifest_bytes(fields: &[(String, SdkValue)]) -> Result<&[u8], LayerFsError> {
    match fields
        .iter()
        .find(|(name, _)| name == "manifest")
        .map(|(_, value)| value)
    {
        Some(SdkValue::Bytes(bytes)) if !bytes.is_empty() => Ok(bytes),
        _ => Err(LayerFsError::refused(
            "NoSuchProject",
            "project record has no sealed manifest",
            Value::Null,
        )),
    }
}

fn decode_manifest(bytes: &[u8]) -> Result<StoreManifest, LayerFsError> {
    StoreManifest::decode(bytes).map_err(|error| LayerFsError::Control(error.to_string()))
}

fn read_private(path: &Path) -> Result<Vec<u8>, LayerFsError> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(LayerFsError::refused(
                "NoSuchProject",
                format!("no project record at {}", path.display()),
                Value::Null,
            ));
        }
        Err(error) => return Err(error.into()),
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Err(LayerFsError::invalid(format!(
            "{} must be a regular file",
            path.display()
        )));
    }
    if meta.len() > RECORD_LIMIT {
        return Err(LayerFsError::invalid("project record is too large"));
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        // SAFETY: geteuid has no preconditions.
        let owner = unsafe { libc::geteuid() };
        if meta.uid() != owner || meta.mode() & 0o077 != 0 {
            return Err(LayerFsError::refused(
                "Unauthorized",
                format!("{} must be private", path.display()),
                Value::Null,
            ));
        }
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        let mut bytes = Vec::new();
        file.by_ref()
            .take(RECORD_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > RECORD_LIMIT {
            return Err(LayerFsError::invalid("project record is too large"));
        }
        Ok(bytes)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(RECORD_LIMIT + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > RECORD_LIMIT {
            return Err(LayerFsError::invalid("project record is too large"));
        }
        Ok(bytes)
    }
}

#[cfg(target_os = "linux")]
fn make_private_dir(path: &Path) -> Result<(), LayerFsError> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(path)
        .map_err(LayerFsError::from)
}

#[cfg(not(target_os = "linux"))]
fn make_private_dir(path: &Path) -> Result<(), LayerFsError> {
    std::fs::create_dir(path).map_err(LayerFsError::from)
}
