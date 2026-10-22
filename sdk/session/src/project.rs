//! `project.init(path, { name, home, storePath })`: host Project Init into a
//! sealed Store, installed into the project volume.
//!
//! The Store is Disposable and sealed under `home/<name>/`, then installed in
//! this process at `storePath`, the project volume's Store file that every
//! Sandbox opens (#373: no Sandbox is started to install it). Init's first
//! Branch is named `main` and the SDK never shows it: `main` is the line of
//! Layers.
use crate::outcome::{failed, refused, Outcome};
use crate::registry::{lock, random, text, Project, INITS};
use layerfs_api::{InitCounters, InitRequest, InitStep, ProjectApi};
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::sdk_values::hex;
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Where the daemon opens the installed Store inside its Sandbox.
pub(crate) const LOCATOR: &str = "/layerfs-store/global/store.sqlite";
/// Init's own deadline.
const INIT_DEADLINE: Duration = Duration::from_secs(60);

fn sixteen(bytes: [u8; 32]) -> [u8; 16] {
    let mut out = [0; 16];
    out.copy_from_slice(&bytes[..16]);
    out
}

pub(crate) fn init(args: &SdkValue) -> Outcome {
    let name = text(args, "name")?;
    let source = PathBuf::from(text(args, "path")?);
    let home = crate::discovery::home(args)?;
    // checked first: a refused policy or missing Store path leaves no project entry
    let policy = policy(args)?;
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
        deadline: Instant::now() + INIT_DEADLINE,
    };
    let counters = Arc::new(InitCounters::new());
    lock(&INITS)?.insert(name.to_owned(), Arc::clone(&counters));
    let running = Running(name);
    let (sealed, _) = Timing::disabled("project.init", |timer| {
        ProjectApi::new().init_observed(request, timer, counters)
    });
    drop(running);
    let sealed = sealed.map_err(failed)?;
    let answer = SdkValue::map([
        ("kind", SdkValue::text("Initialized")),
        ("project", SdkValue::text(name)),
        (
            "layer",
            SdkValue::text(hex(sealed.branch.branch.base_layer.as_slice())),
        ),
        (
            "root",
            SdkValue::text(hex(sealed.branch.effective_root.as_bytes())),
        ),
        (
            "store",
            SdkValue::text(directory.join("sealed.sqlite").display().to_string()),
        ),
    ]);
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
    Ok(answer)
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

/// Removes a finished Init's counts, whichever way it ended.
struct Running<'a>(&'a str);
impl Drop for Running<'_> {
    fn drop(&mut self) {
        if let Ok(mut inits) = INITS.lock() {
            inits.remove(self.0);
        }
    }
}

/// `operator.project.progress`: how far the project's running Init has come
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
