//! Native configuration: telemetry and the history catalog binding.
//!
//! Nothing here has a default. A history catalog is configured with an explicit
//! binding key and an explicit mode, because the binding key is what ties the
//! catalog file to the content authority it describes, and "create" versus
//! "open read-only" is the difference between holding writable continuity and
//! being unable to allocate. An absent catalog simply leaves history
//! unconfigured; a partially configured one is refused.
use layerfs_bridge::contract::{Code, Failure};
use layerfs_history::sqlite::{create, open_read_only};
use layerfs_history::{HistoryCatalog, HistoryCatalogConfig};
use layerfs_telemetry::{
    output::{Identity, OutputConfig, OutputMode},
    runtime::{Configuration, MonitorConfig, Runtime},
};
/// Builds the configured history catalog, if the operator configured one.
///
/// `LAYERFS_HISTORY_CATALOG` names the file. `LAYERFS_HISTORY_BINDING` is the
/// stable authority binding key and is required with it. `LAYERFS_HISTORY_CREATE`
/// selects whether this process creates the catalog (and therefore owns writable
/// continuity) or opens an existing one read-only; `LAYERFS_HISTORY_INCARNATION`
/// is required when creating.
pub fn history() -> Result<Option<std::sync::Arc<dyn HistoryCatalog>>, Failure> {
    let Ok(path) = std::env::var("LAYERFS_HISTORY_CATALOG") else {
        return Ok(None);
    };
    if path.is_empty() || path.len() > 4096 {
        return Err(Code::InvalidInput.into());
    }
    let binding = std::env::var("LAYERFS_HISTORY_BINDING").map_err(|_| Code::InvalidInput)?;
    if binding.is_empty() || binding.len() > 128 {
        return Err(Code::InvalidInput.into());
    }
    let cursor_key = layerfs_bridge::adapters::native::pipe::key(
        &std::env::var("LAYERFS_HISTORY_CURSOR_KEY").map_err(|_| Code::InvalidInput)?,
    )?;
    let create_mode = std::env::var("LAYERFS_HISTORY_CREATE").map_err(|_| Code::InvalidInput)?;
    let catalog: std::sync::Arc<dyn HistoryCatalog> = match create_mode.as_str() {
        "1" => {
            let incarnation = std::env::var("LAYERFS_HISTORY_INCARNATION")
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|value| *value >= 1)
                .ok_or(Code::InvalidInput)?;
            std::sync::Arc::new(
                create(
                    std::path::Path::new(&path),
                    &HistoryCatalogConfig {
                        cursor_key,
                        binding_key: binding.into_bytes(),
                        incarnation,
                    },
                )
                .map_err(catalog_failure)?,
            )
        }
        "0" => std::sync::Arc::new(
            open_read_only(std::path::Path::new(&path), binding.as_bytes(), cursor_key)
                .map_err(catalog_failure)?,
        ),
        _ => return Err(Code::Unsupported.into()),
    };
    Ok(Some(catalog))
}

/// Maps a catalog failure onto the service's wire class.
pub fn catalog_failure(error: layerfs_history::HistoryError) -> Failure {
    crate::operation::history::failure(error)
}

pub fn telemetry(role: u8) -> Runtime {
    let mode = std::env::var("LAYERFS_TELEMETRY").unwrap_or_else(|_| "off".into());
    if mode == "off" {
        return Runtime::disabled();
    }
    let mut output = OutputConfig::forward();
    output.mode = match mode.as_str() {
        "forward" => OutputMode::Forward,
        "local" => OutputMode::Local,
        "both" => OutputMode::Both,
        _ => {
            layerfs_bridge::adapters::native::pipe::diagnostic(
                "telemetry initialization: unsupported mode\n",
            );
            return Runtime::disabled();
        }
    };
    if output.mode != OutputMode::Forward {
        output.directory = std::env::var_os("LAYERFS_TELEMETRY_DIRECTORY").map(Into::into);
    }
    let run = match std::env::var("LAYERFS_RUN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
    {
        Some(v) => v,
        None => {
            layerfs_bridge::adapters::native::pipe::diagnostic(
                "telemetry initialization: run identity required\n",
            );
            return Runtime::disabled();
        }
    };
    let namespace = match std::env::var("LAYERFS_NAMESPACE")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|n| *n != 0)
    {
        Some(v) => v,
        None => {
            layerfs_bridge::adapters::native::pipe::diagnostic(
                "telemetry initialization: namespace required\n",
            );
            return Runtime::disabled();
        }
    };
    match Runtime::start(Configuration {
        enabled: true,
        timing: true,
        monitor: MonitorConfig {
            cpu: true,
            memory: true,
            interval_ms: 100,
            history: 600,
            windows: 32,
        },
        output,
        identity: Identity {
            run,
            pid: std::process::id(),
            role,
            namespace,
        },
    }) {
        Ok(runtime) => runtime,
        Err(_) => {
            layerfs_bridge::adapters::native::pipe::diagnostic(
                "telemetry initialization failed; disabled\n",
            );
            Runtime::disabled()
        }
    }
}
