//! Project init: creates and seals the Disposable Store and installs it into storePath.
use crate::discovery::{ensure_directory, home, save_project};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_history::{BranchId, HistoryCatalogConfig, HistoryName, LayerStackId};
use layerfs_persistence::{PersistenceConfig, SqlitePersistenceProfile};
use layerfs_project::store::{InitRequest, ProjectApi};
use layerfs_project::InitCounters;
use layerfs_storage::policy::FORMAT_PROFILE;
use layerfs_storage::StoragePolicy;
use layerfs_telemetry::timer::Timing;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn refused(code: &str, message: impl Into<String>) -> SdkRefusal {
    SdkRefusal {
        code: code.to_string(),
        message: message.into(),
        detail: SdkValue::Null,
    }
}

fn failed<E: std::fmt::Display>(e: E) -> SdkRefusal {
    SdkRefusal {
        code: "Failed".to_string(),
        message: e.to_string(),
        detail: SdkValue::Null,
    }
}

fn random_32() -> Result<[u8; 32], SdkRefusal> {
    layerfs_bridge::duplex::generate_keypair()
        .map(|pair| pair.public)
        .map_err(failed)
}

fn sixteen(bytes: [u8; 32]) -> [u8; 16] {
    let mut out = [0; 16];
    out.copy_from_slice(&bytes[..16]);
    out
}

fn policy(args: &SdkValue) -> Result<StoragePolicy, SdkRefusal> {
    let Some(policy_val) = args.get("policy") else {
        return Ok(StoragePolicy::default());
    };
    if matches!(policy_val, SdkValue::Null) {
        return Ok(StoragePolicy::default());
    }
    let SdkValue::Map(fields) = policy_val else {
        return Err(refused("InvalidInput", "policy must be a map"));
    };
    let default = StoragePolicy::default();
    let mut threshold = default.small_file_threshold_bytes();
    let mut whole_depth = default.whole_file_delta_max_depth();
    let mut chunk_depth = default.chunk_delta_max_depth();
    let mut meta_depth = default.metadata_delta_max_depth();
    for (k, v) in fields {
        match k.as_str() {
            "smallFileThreshold" => {
                if let SdkValue::Int(n) = v {
                    if *n < 0 {
                        return Err(refused(
                            "InvalidInput",
                            "policy.smallFileThreshold must be non-negative",
                        ));
                    }
                    threshold = *n as u64;
                }
            }
            "wholeFileDepth" => {
                if let SdkValue::Int(n) = v {
                    if !(0..=255).contains(n) {
                        return Err(refused(
                            "InvalidInput",
                            "policy.wholeFileDepth must be 0..255",
                        ));
                    }
                    whole_depth = *n as u8;
                }
            }
            "chunkDepth" => {
                if let SdkValue::Int(n) = v {
                    if !(0..=255).contains(n) {
                        return Err(refused("InvalidInput", "policy.chunkDepth must be 0..255"));
                    }
                    chunk_depth = *n as u8;
                }
            }
            "metadataDepth" => {
                if let SdkValue::Int(n) = v {
                    if !(0..=255).contains(n) {
                        return Err(refused(
                            "InvalidInput",
                            "policy.metadataDepth must be 0..255",
                        ));
                    }
                    meta_depth = *n as u8;
                }
            }
            _ => {}
        }
    }
    StoragePolicy::new(FORMAT_PROFILE, threshold, whole_depth, chunk_depth)
        .with_metadata_depth(meta_depth)
        .validated()
        .map_err(|e| refused("InvalidInput", format!("invalid policy: {e:?}")))
}

pub fn init(args: &SdkValue) -> Result<SdkValue, SdkRefusal> {
    let name = args
        .get_text("name")
        .ok_or_else(|| refused("InvalidInput", "name is required"))?;
    let path_str = args
        .get_text("path")
        .ok_or_else(|| refused("InvalidInput", "path is required"))?;
    let source = PathBuf::from(path_str);
    let home_dir = home(args)?;
    let store_path = args
        .get_text("storePath")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            refused(
                "InvalidInput",
                "project.init installs the Store at storePath",
            )
        })?;

    let timeout_ms = match args.get("timeoutMs") {
        Some(SdkValue::Int(n)) if *n > 0 => *n as u64,
        _ => 300_000,
    };
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let storage_policy = policy(args)?;

    let global = store_path.parent().unwrap_or(&store_path);
    layerfs_store_service::kernel_guard::check(global)
        .map_err(|reason| refused("Unsupported", reason.0))?;

    let directory = home_dir.join(name);
    if directory.exists() {
        return Err(refused("Exists", format!("project {name} already exists")));
    }
    ensure_directory(&directory)?;

    let history_name = |s: &str| HistoryName::new(s).map_err(failed);
    let request = InitRequest {
        source,
        store: PersistenceConfig::sqlite(directory.join("sealed.sqlite"))
            .with_sqlite_profile(SqlitePersistenceProfile::Disposable),
        locator: "/layerfs-store/global/store.sqlite".into(),
        policy: storage_policy,
        catalog: HistoryCatalogConfig {
            binding_key: random_32()?.to_vec(),
            cursor_key: random_32()?,
            incarnation: 1,
        },
        stack: LayerStackId::from_authority(sixteen(random_32()?)),
        stack_name: history_name(name)?,
        branch: BranchId::from_authority(sixteen(random_32()?)),
        branch_name: history_name("main")?,
        scope_seed: random_32()?,
        deadline,
    };

    let counters = Arc::new(InitCounters::new());
    let (sealed, _) = Timing::disabled("project.init", |timer| {
        ProjectApi::new().init_observed(request, timer, counters)
    });
    let sealed = sealed.map_err(|f| refused("Failed", format!("Init failed: {f:?}")))?;

    let layer = hex(sealed.branch.branch.base_layer.as_slice());
    let root = hex(sealed.branch.effective_root.as_bytes());

    save_project(&directory, &sealed.manifest, None)?;

    let installed = layerfs_store_service::install::install_local(
        &sealed.store.path,
        &sealed.manifest,
        &store_path,
    )
    .map_err(|e| refused("Failed", format!("install_local failed: {e}")))?;

    save_project(&directory, &sealed.manifest, Some(&installed))?;

    // Remove sealed sqlite and sidecars
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let mut p = sealed.store.path.as_os_str().to_owned();
        p.push(suffix);
        let _ = std::fs::remove_file(p);
    }

    Ok(SdkValue::map([
        ("kind", SdkValue::text("Initialized")),
        ("project", SdkValue::text(name)),
        ("layer", SdkValue::text(layer)),
        ("root", SdkValue::text(root)),
        ("store", SdkValue::text(store_path.display().to_string())),
    ]))
}
