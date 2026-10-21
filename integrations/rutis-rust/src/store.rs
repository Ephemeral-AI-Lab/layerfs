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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const RECORD_LIMIT: u64 = 65_535;

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

impl Drop for Opened {
    fn drop(&mut self) {
        if let Some(service) = self.service.take() {
            let _ = service.close();
        }
        let _ = std::fs::remove_dir_all(&self.overlay);
    }
}

struct Inner {
    config: Config,
    opening: tokio::sync::Mutex<()>,
    state: Mutex<Option<Arc<Opened>>>,
    closed: AtomicBool,
}

/// One project's Store service. The Store is opened lazily on the first call;
/// `close` disables new calls and releases the Store. No Store registry or
/// process-global mutable state is kept.
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

    /// The service name and project. Does not open the Store.
    pub async fn info(&self) -> Info {
        Info {
            project: self.inner.config.project.clone(),
        }
    }

    /// Runs one public Store method once.
    pub async fn call(&self, method: String, args: Value) -> Result<Value, LayerFsError> {
        self.ensure_open()?;
        let method = exact_method(&method)?.to_owned();
        let args = with_project(value::to_sdk(args)?, &self.inner.config.project)?;
        let opened = self.service().await?;
        tokio::task::spawn_blocking(move || {
            let service = opened
                .service
                .as_ref()
                .ok_or_else(|| LayerFsError::invalid("layerfs service is closed"))?;
            let value = layerfs_api::call(service, &SdkCall { method, args })?;
            value::to_json(&value)
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
            tokio::task::spawn_blocking(move || drop(opened))
                .await
                .map_err(|error| LayerFsError::Join(error.to_string()))?;
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
        self.ensure_open()?;
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
        if let Ok(mut state) = self.state.lock() {
            drop(state.take());
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
    let overlay = unique_overlay();
    make_private_dir(&overlay)?;
    let template = overlay.join("overlay.sqlite");
    match StoreService::open_direct(
        &config.store_path,
        &record.manifest.binding,
        record.manifest.cursor_key,
        &template,
    ) {
        Ok(service) => Ok(Arc::new(Opened {
            service: Some(service),
            overlay,
        })),
        Err(refusal) => {
            let _ = std::fs::remove_dir_all(&overlay);
            Err(match refusal {
                OpenRefusal::Unsupported(reason) => LayerFsError::unsupported(reason.0),
                OpenRefusal::Missing(reason) => {
                    LayerFsError::refused("NoStore", reason, Value::Null)
                }
                OpenRefusal::Failed(reason) => LayerFsError::refused("Failed", reason, Value::Null),
            })
        }
    }
}

fn unique_overlay() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("layerfs-rutis-{}-{}", std::process::id(), nanos))
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
        return Err(LayerFsError::Control(
            "project record is not an SDK value".into(),
        ));
    };
    let SdkValue::Map(fields) = value else {
        return Err(LayerFsError::Control("project record is not a map".into()));
    };
    let manifest = decode_manifest(field_bytes(&fields, "manifest")?)?;
    manifest
        .check()
        .map_err(|error| LayerFsError::Control(error.into()))?;
    if manifest.profile != StoreProfile::Disposable {
        return Err(LayerFsError::invalid("Disposable Store required"));
    }
    let installed = match fields
        .iter()
        .find(|(name, _)| name == "installed")
        .map(|(_, value)| value)
    {
        Some(SdkValue::Bytes(bytes)) if !bytes.is_empty() => decode_manifest(bytes)?,
        _ => {
            return Err(LayerFsError::refused(
                "NoStore",
                "the project has no installed Store",
                Value::Null,
            ));
        }
    };
    installed
        .check()
        .map_err(|error| LayerFsError::Control(error.into()))?;
    if installed.daemon_sqlite.is_none()
        || installed.binding != manifest.binding
        || installed.cursor_key != manifest.cursor_key
    {
        return Err(LayerFsError::Control(
            "installed manifest does not match the project".into(),
        ));
    }
    Ok(Record { manifest })
}

fn decode_manifest(bytes: &[u8]) -> Result<StoreManifest, LayerFsError> {
    StoreManifest::decode(bytes).map_err(|error| LayerFsError::Control(error.to_string()))
}

fn field_bytes<'a>(fields: &'a [(String, SdkValue)], name: &str) -> Result<&'a [u8], LayerFsError> {
    match fields
        .iter()
        .find(|(field, _)| field == name)
        .map(|(_, value)| value)
    {
        Some(SdkValue::Bytes(bytes)) => Ok(bytes),
        _ => Err(LayerFsError::Control(format!(
            "project record has no {name}"
        ))),
    }
}

fn read_private(path: &Path) -> Result<Vec<u8>, LayerFsError> {
    let meta = std::fs::symlink_metadata(path)?;
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
