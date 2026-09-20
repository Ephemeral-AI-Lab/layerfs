//! Native telemetry configuration, separate from the operation handler.
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
