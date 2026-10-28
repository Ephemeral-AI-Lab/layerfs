//! `project.init(path, { name, home, storePath, timeoutMs })`: host Project
//! Init into a sealed Store, installed into the project volume. `timeoutMs`
//! (required, a positive whole number) bounds Init from when this call
//! starts it; the caller owns its default (#431: @layerfs/agent's 180 s).
//!
//! The Store is Disposable and sealed under `home/<name>/`, then installed in
//! this process at `storePath`, the project volume's Store file that every
//! Sandbox opens (#373: no Sandbox is started to install it). Once the
//! install is recorded, the sealed copy is deleted (owner 2026-10-23):
//! nothing reads it again. A failed or uncertain install keeps it for
//! diagnosis; a delete that fails is reported in the answer's `leftover`
//! and leaves the project usable. Init's first
//! Branch is named `main` and the SDK never shows it: `main` is the line of
//! Layers.
use crate::job::{failed, refused, Outcome};
use crate::registry::{lock, random, text, Project, INITS};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_project::store::{InitError, InitFailure, InitRequest, ProjectApi};
use layerfs_project::{InitCounters, InitStep, ProjectError, SourceRefusal};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Where the daemon opens the installed Store inside its Sandbox.
pub(crate) const LOCATOR: &str = "/layerfs-store/global/store.sqlite";
fn sixteen(bytes: [u8; 32]) -> [u8; 16] {
    let mut out = [0; 16];
    out.copy_from_slice(&bytes[..16]);
    out
}

pub(crate) fn init(args: &SdkValue) -> Outcome {
    let name = text(args, "name")?;
    let source = PathBuf::from(text(args, "path")?);
    let home = crate::discovery::home(args)?;
    // checked first: a refused policy, timeout or Store path leaves no project entry
    let policy = policy(args)?;
    let deadline = deadline(args)?;
    let store_path =
        match args.get_text("storePath") {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            _ => return Err(refused(
                "InvalidInput",
                "project.init installs the Store at the client's storePath (LAYERFS_STORE_PATH)",
            )),
        };
    // the Store's directory is made by the install; its volume must carry the Store's locks
    let global = store_path.parent().unwrap_or(&store_path);
    layerfs_store_service::kernel_guard::check(global)
        .map_err(|reason| refused("Unsupported", reason.0))?;
    let directory = home.join(name);
    // The home entry is the project: creating it is the one existence check,
    // so a name whose entry was removed can be initialized again.
    std::fs::create_dir(&directory).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            refused("Exists", format!("project {name} already initialized"))
        } else {
            failed(error)
        }
    })?;
    #[cfg(unix)]
    std::fs::set_permissions(&directory, {
        use std::os::unix::fs::PermissionsExt;
        std::fs::Permissions::from_mode(0o700)
    })
    .map_err(failed)?;
    let _guard = crate::discovery::guard(&directory)?;
    let history = |text: &str| HistoryName::new(text).map_err(failed);
    let request = InitRequest {
        source,
        store: PersistenceConfig::sqlite(directory.join("sealed.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        locator: LOCATOR.into(),
        policy,
        catalog: HistoryCatalogConfig {
            binding_key: random()?.to_vec(),
            cursor_key: random()?,
            incarnation: 1,
        },
        stack: LayerStackId::from_authority(sixteen(random()?)),
        stack_name: history(name)?,
        branch: BranchId::from_authority(sixteen(random()?)),
        branch_name: history("main")?,
        scope_seed: random()?,
        deadline,
    };
    let counters = Arc::new(InitCounters::new());
    lock(&INITS)?.insert(name.to_owned(), Arc::clone(&counters));
    let running = Running(name);
    let (sealed, _) = Timing::disabled("project.init", |timer| {
        ProjectApi::new().init_observed(request, timer, counters)
    });
    drop(running);
    let sealed = sealed.map_err(|failure| init_failure(*failure, &directory))?;
    let layer = SdkValue::text(hex(sealed.branch.branch.base_layer.as_slice()));
    let root = SdkValue::text(hex(sealed.branch.effective_root.as_bytes()));
    let mut project = Project {
        store: sealed.store,
        manifest: sealed.manifest,
        installed: None,
    };
    crate::discovery::save_project(&directory, &project)?;
    let installed = layerfs_store_service::install::install_local(
        &project.store.path,
        &project.manifest,
        &store_path,
    )
    .map_err(|failure| refused("Failed", failure.to_string()))?;
    project.installed = Some(installed);
    crate::discovery::save_project(&directory, &project)?;
    let mut answer = vec![
        ("kind", SdkValue::text("Initialized")),
        ("project", SdkValue::text(name)),
        ("layer", layer),
        ("root", root),
        ("store", SdkValue::text(store_path.display().to_string())),
    ];
    if let Some(leftover) = remove_sealed(&project.store.path) {
        answer.push(("leftover", leftover));
    }
    Ok(SdkValue::Map(
        answer
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    ))
}

/// Deletes the sealed Store after its install is recorded, once: the file
/// and any SQLite side file. A delete that fails is `{ path, message }` for
/// the answer; the project stays usable.
fn remove_sealed(sealed: &Path) -> Option<SdkValue> {
    let mut failures = Vec::new();
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut path = sealed.as_os_str().to_owned();
        path.push(suffix);
        let path = PathBuf::from(path);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !suffix.is_empty() => {}
            Err(error) => failures.push(format!("{}: {error}", path.display())),
        }
    }
    (!failures.is_empty()).then(|| {
        SdkValue::map([
            ("path", SdkValue::text(sealed.display().to_string())),
            (
                "message",
                SdkValue::text(format!(
                    "the project is installed, but its sealed copy was not deleted: {}",
                    failures.join("; ")
                )),
            ),
        ])
    })
}

/// Init's failure. A refused source entry is `InvalidInput` whose `detail`
/// names the check and the entry ({ reason, path, mode? }). Nothing was
/// published and Init already removed its own working state, so the project
/// entry this call made is removed and the name can be initialized again.
/// Every other failure keeps the entry and its Store as they are.
fn init_failure(failure: InitFailure, directory: &Path) -> SdkRefusal {
    let InitError::Project(ProjectError::InvalidInput { reason, path }) = &failure.error else {
        return failed(failure);
    };
    let mut message = format!("project.init refused {}: {reason}", path.display());
    let path = SdkValue::text(path.display().to_string());
    let detail = match reason {
        SourceRefusal::Mode(mode) => SdkValue::map([
            ("reason", SdkValue::text("Mode")),
            ("path", path),
            ("mode", SdkValue::text(format!("{mode:04o}"))),
        ]),
        other => SdkValue::map([
            ("reason", SdkValue::text(format!("{other:?}"))),
            ("path", path),
        ]),
    };
    // The entry is removed while this call still holds its discovery lock.
    if let Err(error) = std::fs::remove_dir_all(directory) {
        message = format!(
            "{message}; its entry {} was not removed: {error}",
            directory.display()
        );
    }
    SdkRefusal {
        code: "InvalidInput".to_owned(),
        message,
        detail,
    }
}

/// The Store policy Init records: the default, or the caller's `policy`
/// ({ smallFileThreshold, wholeFileDepth, chunkDepth, metadataDepth }, each
/// defaulting to the default's), validated before anything is made.
fn policy(args: &SdkValue) -> Result<StoragePolicy, layerfs_bridge::control::SdkRefusal> {
    let base = StoragePolicy::frozen_default();
    let Some(given) = args.get("policy") else {
        return Ok(base);
    };
    let field = |name: &str, default: u64| match given.get(name) {
        None | Some(SdkValue::Null) => Ok(default),
        Some(SdkValue::Int(n)) if *n >= 0 => Ok(*n as u64),
        Some(_) => Err(refused(
            "InvalidInput",
            format!("policy.{name} must be a whole number"),
        )),
    };
    let depth = |name: &str, default: u8| {
        u8::try_from(field(name, u64::from(default))?)
            .map_err(|_| refused("InvalidInput", format!("policy.{name} is out of range")))
    };
    StoragePolicy::new(
        base.format_profile(),
        field("smallFileThreshold", base.small_file_threshold_bytes())?,
        depth("wholeFileDepth", base.whole_file_delta_max_depth())?,
        depth("chunkDepth", base.chunk_delta_max_depth())?,
    )
    .with_metadata_depth(depth("metadataDepth", base.metadata_delta_max_depth())?)
    .validated()
    .map_err(|error| refused("InvalidInput", format!("policy: {error}")))
}

/// Init's deadline, counted from now: the caller's required `timeoutMs`, a
/// positive whole number of milliseconds. There is no default here.
fn deadline(args: &SdkValue) -> Result<Instant, layerfs_bridge::control::SdkRefusal> {
    let Some(SdkValue::Int(ms @ 1..)) = args.get("timeoutMs") else {
        return Err(refused(
            "InvalidInput",
            "timeoutMs is required: a positive whole number of milliseconds",
        ));
    };
    Instant::now()
        .checked_add(Duration::from_millis(*ms as u64))
        .ok_or_else(|| refused("InvalidInput", "timeoutMs is too large"))
}

/// Removes a finished Init's counts, whichever way it ended.
struct Running<'a>(&'a str);
impl Drop for Running<'_> {
    fn drop(&mut self) {
        if let Ok(mut inits) = INITS.lock() {
            inits.remove(self.0);
        }
    }
}

/// `runtime.project.get_init`: how far the project's running Init has come
/// (`step`, `entries`, `files`, `bytes`), or null when none runs.
pub(crate) fn progress(args: &SdkValue) -> Outcome {
    let name = text(args, "project")?;
    let Some(counters) = lock(&INITS)?.get(name).cloned() else {
        return Ok(SdkValue::Null);
    };
    let counts = counters.read();
    let step = match counts.step {
        InitStep::Scan => "scan",
        InitStep::Files => "files",
        InitStep::Tree => "tree",
        InitStep::Publish => "publish",
        InitStep::Seal => "seal",
    };
    let number = |n: u64| SdkValue::Int(i64::try_from(n).unwrap_or(i64::MAX));
    Ok(SdkValue::map([
        ("step", SdkValue::text(step)),
        ("entries", number(counts.entries)),
        ("files", number(counts.files)),
        ("bytes", number(counts.bytes)),
    ]))
}
