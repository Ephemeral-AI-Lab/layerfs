//! Native telemetry configuration supplied by the daemon assembly.
use layerfs_telemetry::{
    output::{Identity, OutputConfig, OutputMode},
    runtime::{Configuration, MonitorConfig, Runtime},
};
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

pub(crate) fn env(name: &str) -> Result<String, layerfs_bridge::contract::Failure> {
    use layerfs_bridge::contract::Code;
    let value = std::env::var(name).map_err(|_| Code::InvalidInput)?;
    if value.len() > 4096 {
        return Err(Code::Capacity.into());
    }
    Ok(value)
}

pub(crate) struct WorkspaceLaunch {
    pub config: layerfs_workspace::WorkspaceConfig,
    pub attach: layerfs_workspace::AttachOptions,
}

/// Explicit local startup selection; this is not a daemon control protocol.
/// Grammar: --mount-readonly ID INCARNATION STORE ROOT|branch:ID UID GID.
/// The authority supplies IDs. The execution host supplies paths and budgets.
pub(crate) fn workspace(
    args: Vec<String>,
) -> Result<Option<WorkspaceLaunch>, layerfs_bridge::contract::Failure> {
    use layerfs_bridge::{
        adapters::native::pipe::key,
        contract::{Code, Failure},
    };
    use layerfs_workspace::{AttachOptions, Base, WorkspaceConfig, DEFAULT_MEMORY_BUDGET_BYTES};
    if args.is_empty() {
        return Ok(None);
    }
    if args.len() != 7 || args[0] != "--mount-readonly" || args.iter().any(|s| s.len() > 256) {
        return Err(Code::InvalidInput.into());
    }
    let number = |text: &str| {
        text.parse::<u32>()
            .map_err(|_| Failure::from(Code::InvalidInput))
    };
    let positive = |text: &str| {
        text.parse::<usize>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| Failure::from(Code::InvalidInput))
    };
    let root = std::path::PathBuf::from(env("LAYERFS_WORKSPACE_ROOT")?);
    if !root.is_absolute() {
        return Err(Code::InvalidInput.into());
    }
    let memory = match std::env::var("LAYERFS_WORKSPACE_MEMORY_BUDGET_BYTES") {
        Ok(value) => positive(&value)?,
        Err(std::env::VarError::NotPresent) => DEFAULT_MEMORY_BUDGET_BYTES,
        Err(_) => return Err(Code::InvalidInput.into()),
    };
    // R allocates no writable backing. Reject malformed explicitly supplied W
    // configuration rather than interpreting zero or invalid input as unlimited.
    if let Some(value) = std::env::var_os("LAYERFS_WORKSPACE_DISK_BUDGET_BYTES") {
        positive(value.to_str().ok_or(Code::InvalidInput)?)?;
    }
    let base = if let Some(hex) = args[4].strip_prefix("branch:") {
        if hex.len() != 34 || !hex.is_ascii() {
            return Err(Code::InvalidInput.into());
        }
        let branch: Result<Vec<u8>, _> = (0..hex.len())
            .step_by(2)
            .map(|at| u8::from_str_radix(&hex[at..at + 2], 16))
            .collect();
        let branch = branch.map_err(|_| Code::InvalidInput)?;
        if branch[0] != 0x11 {
            return Err(Code::InvalidInput.into());
        }
        Base::Branch(branch.try_into().map_err(|_| Code::InvalidInput)?)
    } else {
        Base::Root(key(&args[4])?)
    };
    Ok(Some(WorkspaceLaunch {
        config: WorkspaceConfig {
            root,
            max_count: positive(&env("LAYERFS_WORKSPACE_MAX_COUNT")?)?,
            memory_budget_bytes: memory,
        },
        attach: AttachOptions {
            id: args[1].clone(),
            incarnation: key(&args[2])?,
            store: number(&args[3])?,
            base,
            owner_uid: number(&args[5])?,
            owner_gid: number(&args[6])?,
        },
    }))
}
