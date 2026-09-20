//! Explicit native telemetry ownership; disabled startup has no side effects.
use super::{Monitor, MonitorConfig};
use crate::{
    operation::{Diagnostic, OperationRecorder},
    output::{encode_operation, Identity, Output, OutputConfig},
    timer::RecordingLimits,
};
use std::{io, sync::Arc, time::Duration};
/// Native assembly configuration; the library never reads environment variables.
pub struct Configuration {
    /// Master switch, checked before every other field or allocation.
    pub enabled: bool,
    /// Independently selected timing.
    pub timing: bool,
    /// Process collection and window limits.
    pub monitor: MonitorConfig,
    /// Independent diagnostics destination and retention.
    pub output: OutputConfig,
    /// Local process and run namespace.
    pub identity: Identity,
}
struct Inner {
    monitor: Option<Monitor>,
    output: Output,
    identity: Identity,
    record_bytes: usize,
}
/// Lifetime owner shared only by explicit application assembly.
#[derive(Clone)]
pub struct Runtime {
    inner: Option<Arc<Inner>>,
    recorder: OperationRecorder,
}
impl Runtime {
    /// Master-off without configuration validation, observations, workers or storage.
    pub const fn disabled() -> Self {
        Self {
            inner: None,
            recorder: OperationRecorder::disabled(),
        }
    }
    /// Starts selected facilities. Invalid optional setup can be reported separately
    /// by assembly without changing any already known product outcome.
    pub fn start(config: Configuration) -> io::Result<Self> {
        if !config.enabled {
            return Ok(Self::disabled());
        }
        config.output.validate()?;
        config.monitor.validate()?;
        // Validate hostile configuration arithmetic before any native allocation.
        let charge =
            || -> Option<usize> {
                (4usize * 1024 * 1024)
                    .checked_add(config.output.queue_bytes)?
                    .checked_add(config.output.queue_count.checked_mul(64)?)?
                    .checked_add(config.output.record_bytes.checked_mul(16)?)?
                    .checked_add(config.monitor.history.checked_mul(std::mem::size_of::<
                        Option<crate::observation::Observation>,
                    >())?)?
                    .checked_add(
                        config
                            .monitor
                            .windows
                            .checked_mul(std::mem::size_of::<crate::observation::Window>() + 128)?,
                    )?
                    .checked_add(65536)
            };
        if charge().is_none_or(|owned| owned > 8 * 1024 * 1024) {
            return Err(io::Error::other("aggregate telemetry ownership"));
        }
        let record_bytes = config.output.record_bytes;
        let output = Output::start(config.output)?;
        let monitor = match Monitor::start_with_output(
            config.monitor,
            Some((output.clone(), config.identity)),
        ) {
            Ok(m) => m,
            Err(e) => {
                output.shutdown(Duration::ZERO);
                return Err(e);
            }
        };
        let limits = RecordingLimits::new(256, 32, 256 * 1024)
            .ok_or_else(|| io::Error::other("recording limits"))?;
        let mut recorder = OperationRecorder::new(limits, 8, 4 * 1024 * 1024)
            .ok_or_else(|| io::Error::other("recording pool"))?
            .with_timing(config.timing);
        if let Some(monitor) = &monitor {
            recorder = recorder.with_windows(monitor.windows());
        }
        Ok(Self {
            inner: Some(Arc::new(Inner {
                monitor,
                output,
                identity: config.identity,
                record_bytes,
            })),
            recorder,
        })
    }
    /// Shared optional recorder, with no extra sampler or exporter.
    pub fn recorder(&self) -> OperationRecorder {
        self.recorder.clone()
    }
    /// Encodes and queues completed detail only; no synchronous output in handlers.
    pub fn publish(&self, diagnostic: Diagnostic) {
        let Some(inner) = &self.inner else {
            return;
        };
        let report = match diagnostic {
            Diagnostic::Report(report) => report,
            Diagnostic::Omitted => {
                inner.output.omit();
                return;
            }
            Diagnostic::Disabled => return,
        };
        if let Some(bytes) = encode_operation(inner.identity, &report, inner.record_bytes) {
            inner.output.submit(bytes);
        } else {
            inner.output.omit();
        }
    }
}
impl Drop for Inner {
    fn drop(&mut self) {
        self.monitor.take();
        let loss = self.output.loss();
        self.output.submit(format!("LFT1 {{\"v\":1,\"kind\":\"run-summary\",\"run\":\"{:032x}\",\"pid\":{},\"role\":{},\"namespace\":{},\"dropped\":{},\"failed\":{},\"overflow\":{},\"delivery\":\"best-effort\"}}\n",self.identity.run,self.identity.pid,self.identity.role,self.identity.namespace,loss.dropped,loss.failed,loss.overflow).into_bytes());
        self.output.shutdown(Duration::from_secs(2));
    }
}
