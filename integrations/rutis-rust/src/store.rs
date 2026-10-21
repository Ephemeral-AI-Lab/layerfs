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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const RECORD_LIMIT: u64 = 65_535;
static OVERLAY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Public integration state for one project.
#[derive(Clone, Debug)]
pub struct Info {
    /// LayerFS project name.
    pub project: String,
}

struct OpenStore {
    service: Arc<StoreService>,
    overlay: PathBuf,
}

struct Record {
    manifest: StoreManifest,
}

/// One project's Store service. The Store is opened lazily on the first call;
/// `Drop` releases it. No global state and no environment mutation.
pub struct LayerFs {
    config: Config,
    opening: tokio::sync::Mutex<()>,
    state: Mutex<Option<OpenStore>>,
}

impl LayerFs {
    /// Validates configuration; performs no Store I/O.
    pub fn new(config: Config) -> Result<Self, LayerFsError> {
        config.validate()?;
        Ok(Self {
            config,
            opening: tokio::sync::Mutex::new(()),
            state: Mutex::new(None),
        })
    }

    /// The service name and project. Does not open the Store.
    pub async fn info(&self) -> Info {
        Info {
            project: self.config.project.clone(),
        }
    }

    /// Runs one public Store method once.
    pub async fn call(&self, method: String, args: Value) -> Result<Value, LayerFsError> {
        if !allowed(&method) {
            return Err(LayerFsError::invalid(format!("UnknownMethod: {method}")));
        }
        let args = with_project(value::to_sdk(args)?, &self.config.project)?;
        let service = self.service().await?;
        let result = tokio::task::spawn_blocking(move || {
            layerfs_api::call(&service, &SdkCall { method, args })
        })
        .await
        .map_err(|error| LayerFsError::Join(error.to_string()))??;
        value::to_json(&result)
    }

    async fn service(&self) -> Result<Arc<StoreService>, LayerFsError> {
        if let Some(open) = self.state().as_ref() {
            return Ok(open.service.clone());
        }
        let _opening = self.opening.lock().await;
        if let Some(open) = self.state().as_ref() {
            return Ok(open.service.clone());
        }
        let config = self.config.clone();
        let opened = tokio::task::spawn_blocking(move || open_store(&config))
            .await
            .map_err(|error| LayerFsError::Join(error.to_string()))??;
        let service = opened.service.clone();
        *self.state() = Some(opened);
        Ok(service)
    }

    fn state(&self) -> std::sync::MutexGuard<'_, Option<OpenStore>> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Drop for LayerFs {
    fn drop(&mut self) {
        let Some(open) = self.state().take() else {
            return;
        };
        if Arc::strong_count(&open.service) == 1 {
            drop(open.service);
            let _ = std::fs::remove_dir_all(open.overlay);
        }
    }
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

fn allowed(method: &str) -> bool {
    method.starts_with("fs.") || method.starts_with("branch.") || method == "project.list_branches"
}

fn open_store(config: &Config) -> Result<OpenStore, LayerFsError> {
    let record = read_record(config)?;
    let overlay = std::env::temp_dir().join(format!(
        "layerfs-rutis-{}-{}",
        std::process::id(),
        OVERLAY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    make_private_dir(&overlay)?;
    let template = overlay.join("overlay.sqlite");
    let opened = StoreService::open_direct(
        &config.store_path,
        &record.manifest.binding,
        record.manifest.cursor_key,
        &template,
    );
    match opened {
        Ok(service) => Ok(OpenStore {
            service: Arc::new(service),
            overlay,
        }),
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
    let _installed = match fields
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
