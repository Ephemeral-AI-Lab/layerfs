//! Open an installed Store in this process and run the public Store table,
//! plus the Tier 1 Workspace calls over the shared session layer.

use crate::config::{Config, Deploy};
use crate::error::LayerFsError;
use crate::value;
use layerfs_bridge::control::{Answer as ControlAnswer, Reply, SdkValue};
use layerfs_bridge::provision::{StoreManifest, StoreProfile};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const RECORD_LIMIT: u64 = 65_535;
/// Bytes per spooled-output page while filling a sink.
const PAGE: usize = 64 * 1024;

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

struct Inner {
    config: Config,
    opening: tokio::sync::Mutex<()>,
    opened: AtomicBool,
    closed: AtomicBool,
}

/// One project's Store and Workspace service. The Store is opened lazily on
/// the first call; `close` disables new calls and releases it.
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
                opened: AtomicBool::new(false),
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
        let method = exact_method(&method)?;
        self.open_store().await?;
        let args = self.session_args(args)?;
        let project = self.inner.config.project.clone();
        tokio::task::spawn_blocking(move || {
            let args = with_project(args, &project)?;
            let value = layerfs_session::dispatch(method, args)?;
            Ok(value::to_json(&value))
        })
        .await
        .map_err(|error| LayerFsError::Join(error.to_string()))?
    }

    /// `workspace_mount({ branch })`: a new Workspace of the Branch.
    pub async fn workspace_mount(&self, args: Value) -> Result<Value, LayerFsError> {
        self.workspace_call("workspace.mount", args, true).await
    }

    /// `workspace_status({ sandbox, path })`.
    pub async fn workspace_status(&self, args: Value) -> Result<Value, LayerFsError> {
        self.workspace_call("workspace.status", args, true).await
    }

    /// `workspace_commit({ sandbox, path, message?, force? })`.
    pub async fn workspace_commit(&self, args: Value) -> Result<Value, LayerFsError> {
        self.workspace_call("workspace.commit", args, false).await
    }

    /// `workspace_unmount({ sandbox, path, discard? })`.
    pub async fn workspace_unmount(&self, args: Value) -> Result<Value, LayerFsError> {
        self.workspace_call("workspace.unmount", args, false).await
    }

    /// `workspace_exec({ sandbox, path, command, message?, sink? })`.
    ///
    /// Without `sink` the SDK result is returned unchanged. With `sink`
    /// (`{ stdout?: path, stderr?: path }`, absolute paths), the command
    /// spools its output, every page is written to the named file in order,
    /// and the result carries the byte counts. A slow sink holds the command
    /// back.
    pub async fn workspace_exec(&self, args: Value) -> Result<Value, LayerFsError> {
        self.ensure_open()?;
        self.ensure_deploy("workspace.exec")?;
        let (args, sink) = take_sink(args)?;
        let args = self.session_args(args)?;
        tokio::task::spawn_blocking(move || match sink {
            None => {
                let value = layerfs_session::dispatch("workspace.exec", args)?;
                Ok(value::to_json(&value))
            }
            Some(sink) => exec_into_sink(args, sink),
        })
        .await
        .map_err(|error| LayerFsError::Join(error.to_string()))?
    }

    /// Disables new calls and releases the Store when the last in-flight call
    /// ends. Idempotent.
    pub async fn close(&self) -> Result<(), LayerFsError> {
        let _opening = self.inner.opening.lock().await;
        self.inner.closed.store(true, Ordering::Release);
        if !self.inner.opened.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        let config = self.inner.config.clone();
        tokio::task::spawn_blocking(move || release_store(&config))
            .await
            .map_err(|error| LayerFsError::Join(error.to_string()))?
    }

    async fn workspace_call(
        &self,
        method: &'static str,
        args: Value,
        store: bool,
    ) -> Result<Value, LayerFsError> {
        self.ensure_open()?;
        self.ensure_deploy(method)?;
        if store {
            self.open_store().await?;
        }
        let args = self.session_args(args)?;
        tokio::task::spawn_blocking(move || {
            let value = layerfs_session::dispatch(method, args)?;
            Ok(value::to_json(&value))
        })
        .await
        .map_err(|error| LayerFsError::Join(error.to_string()))?
    }

    fn ensure_deploy(&self, method: &str) -> Result<(), LayerFsError> {
        if self.inner.config.deploy.is_none() {
            return Err(LayerFsError::invalid(format!(
                "{method} needs deploy.docker, deploy.image, deploy.volume and deploy.daemon"
            )));
        }
        Ok(())
    }

    async fn open_store(&self) -> Result<(), LayerFsError> {
        self.ensure_open()?;
        if self.inner.opened.load(Ordering::Acquire) {
            return Ok(());
        }
        let _opening = self.inner.opening.lock().await;
        self.ensure_open()?;
        if self.inner.opened.load(Ordering::Acquire) {
            return Ok(());
        }
        let config = self.inner.config.clone();
        tokio::task::spawn_blocking(move || open_store(&config))
            .await
            .map_err(|error| LayerFsError::Join(error.to_string()))??;
        self.inner.opened.store(true, Ordering::Release);
        Ok(())
    }

    fn ensure_open(&self) -> Result<(), LayerFsError> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(LayerFsError::invalid("layerfs service is closed"));
        }
        Ok(())
    }

    fn session_args(&self, args: Value) -> Result<SdkValue, LayerFsError> {
        let SdkValue::Map(mut fields) = value::to_sdk(args)? else {
            return Err(LayerFsError::invalid("arguments must be an object"));
        };
        let config = &self.inner.config;
        let fixed = [
            ("project", SdkValue::Text(config.project.clone())),
            ("home", SdkValue::Text(config.home.display().to_string())),
            (
                "storePath",
                SdkValue::Text(config.store_path.display().to_string()),
            ),
        ];
        for (name, value) in fixed {
            set(&mut fields, name, value);
        }
        if let Some(deploy) = &config.deploy {
            set(&mut fields, "deploy", deploy_value(deploy));
        }
        Ok(SdkValue::Map(fields))
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        if !self.opened.swap(false, Ordering::AcqRel) {
            return;
        }
        let config = self.config.clone();
        let job = move || {
            let _ = release_store(&config);
        };
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                drop(handle.spawn_blocking(job));
            }
            Err(_) => job(),
        }
    }
}

fn open_store(config: &Config) -> Result<(), LayerFsError> {
    read_record(config)?;
    let args = base_args(config);
    layerfs_session::acquire(&args)?;
    Ok(())
}

fn release_store(config: &Config) -> Result<(), LayerFsError> {
    let args = base_args(config);
    layerfs_session::release(&args)?;
    Ok(())
}

fn base_args(config: &Config) -> SdkValue {
    let mut fields = vec![
        ("project".to_owned(), SdkValue::Text(config.project.clone())),
        (
            "home".to_owned(),
            SdkValue::Text(config.home.display().to_string()),
        ),
        (
            "storePath".to_owned(),
            SdkValue::Text(config.store_path.display().to_string()),
        ),
    ];
    if let Some(deploy) = &config.deploy {
        fields.push(("deploy".to_owned(), deploy_value(deploy)));
    }
    SdkValue::Map(fields)
}

fn deploy_value(deploy: &Deploy) -> SdkValue {
    let mut fields = vec![
        (
            "docker".to_owned(),
            SdkValue::Text(deploy.docker.display().to_string()),
        ),
        ("image".to_owned(), SdkValue::Text(deploy.image.clone())),
        ("volume".to_owned(), SdkValue::Text(deploy.volume.clone())),
        (
            "daemon".to_owned(),
            SdkValue::Text(deploy.daemon.display().to_string()),
        ),
    ];
    if let Some(host) = &deploy.daemon_host {
        fields.push(("daemonHost".to_owned(), SdkValue::Text(host.clone())));
    }
    if let Some(prefix) = &deploy.container_prefix {
        fields.push(("containerPrefix".to_owned(), SdkValue::Text(prefix.clone())));
    }
    SdkValue::Map(fields)
}

fn set(fields: &mut Vec<(String, SdkValue)>, name: &str, value: SdkValue) {
    match fields.iter_mut().find(|(known, _)| known == name) {
        Some((_, current)) => *current = value,
        None => fields.push((name.to_owned(), value)),
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
    set(&mut fields, "project", SdkValue::Text(project.to_owned()));
    Ok(SdkValue::Map(fields))
}

struct Record;

fn read_record(config: &Config) -> Result<Record, LayerFsError> {
    let project = config.home.join(&config.project);
    let path = project.join("project.record");
    let bytes = read_private(&path)?;
    crate::config::private_dir(&config.home)?;
    crate::config::private_dir(&project)?;
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
    Ok(Record)
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
        return Err(LayerFsError::refused(
            "Unauthorized",
            format!("{} must be a regular file", path.display()),
            Value::Null,
        ));
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
        std::io::Read::by_ref(&mut file)
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

/// One exec sink: absolute files that receive the streams in order.
struct Sink {
    stdout: Option<PathBuf>,
    stderr: Option<PathBuf>,
}

fn take_sink(args: Value) -> Result<(Value, Option<Sink>), LayerFsError> {
    let Value::Object(mut fields) = args else {
        return Err(LayerFsError::invalid("arguments must be an object"));
    };
    let Some(sink) = fields.remove("sink") else {
        return Ok((Value::Object(fields), None));
    };
    if sink.is_null() {
        return Ok((Value::Object(fields), None));
    }
    let Value::Object(streams) = sink else {
        return Err(LayerFsError::invalid(
            "sink must be an object with stdout/stderr paths",
        ));
    };
    let path = |name: &str| -> Result<Option<PathBuf>, LayerFsError> {
        match streams.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(path)) => {
                let path = PathBuf::from(path);
                if !path.is_absolute() {
                    return Err(LayerFsError::invalid(format!(
                        "sink.{name} must be absolute"
                    )));
                }
                Ok(Some(path))
            }
            Some(_) => Err(LayerFsError::invalid(format!("sink.{name} must be a path"))),
        }
    };
    let sink = Sink {
        stdout: path("stdout")?,
        stderr: path("stderr")?,
    };
    if sink.stdout.is_none() && sink.stderr.is_none() {
        return Err(LayerFsError::invalid("sink needs a stdout or stderr path"));
    }
    Ok((Value::Object(fields), Some(sink)))
}

fn exec_into_sink(args: SdkValue, sink: Sink) -> Result<Value, LayerFsError> {
    let SdkValue::Map(mut fields) = args else {
        return Err(LayerFsError::invalid("arguments must be an object"));
    };
    set(&mut fields, "spool", SdkValue::Bool(true));
    let result = layerfs_session::dispatch("workspace.exec", SdkValue::Map(fields))?;
    let SdkValue::Map(fields) = result else {
        return Err(LayerFsError::invalid("workspace.exec returned no result"));
    };
    let output = match fields.iter().find(|(name, _)| name == "output") {
        Some((_, SdkValue::Text(output))) => Some(output.clone()),
        _ => None,
    };
    let size = |name: &str| match fields
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
    {
        Some(SdkValue::Int(value)) => u64::try_from(*value).ok(),
        _ => None,
    };
    let mut streams = Vec::new();
    for (name, path, inline) in [
        ("stdout", sink.stdout, "stdout"),
        ("stderr", sink.stderr, "stderr"),
    ] {
        let Some(path) = path else {
            continue;
        };
        let inline = match fields.iter().find(|(key, _)| key == inline) {
            Some((_, SdkValue::Text(text))) => Some(text.as_str()),
            _ => None,
        };
        let bytes = write_stream(&path, inline, output.as_deref(), name, size(name))?;
        streams.push((
            name.to_owned(),
            SdkValue::map([
                ("path", SdkValue::text(path.display().to_string())),
                (
                    "bytes",
                    SdkValue::Int(i64::try_from(bytes).unwrap_or(i64::MAX)),
                ),
            ]),
        ));
    }
    if let Some(output) = output {
        let release = SdkValue::map([("output", SdkValue::Text(output))]);
        layerfs_session::dispatch("workspace.exec_release", release)?;
    }
    let field = |name: &str| {
        fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    let mut answer = Vec::new();
    for name in ["kind", "exitCode", "message", "truncated"] {
        if let Some(value) = field(name) {
            answer.push((name.to_owned(), value));
        }
    }
    for name in ["stdoutBytes", "stderrBytes"] {
        if let Some(value) = field(name) {
            answer.push((name.to_owned(), value));
        }
    }
    answer.push(("sink".to_owned(), SdkValue::Map(streams)));
    Ok(value::to_json(&SdkValue::Map(answer)))
}

fn write_stream(
    path: &Path,
    inline: Option<&str>,
    output: Option<&str>,
    stream: &str,
    size: Option<u64>,
) -> Result<u64, LayerFsError> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    if let Some(text) = inline {
        file.write_all(text.as_bytes())?;
        return Ok(text.len() as u64);
    }
    let Some(output) = output else {
        return Ok(0);
    };
    let size = size.unwrap_or(0);
    let mut offset = 0u64;
    let mut page = [0u8; PAGE];
    while offset < size {
        let args = SdkValue::map([
            ("output", SdkValue::text(output)),
            ("stream", SdkValue::text(stream)),
            ("offset", SdkValue::Int(offset as i64)),
        ]);
        let answer = layerfs_session::exec_output(&args, &mut page)?;
        let length = answer
            .get_int("length")
            .and_then(|length| usize::try_from(length).ok())
            .ok_or_else(|| LayerFsError::invalid("spooled output page has no length"))?;
        if length == 0 {
            break;
        }
        file.write_all(&page[..length])?;
        offset += length as u64;
    }
    Ok(offset)
}
