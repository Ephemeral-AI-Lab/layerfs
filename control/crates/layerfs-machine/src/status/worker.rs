//! The host machine's worker as `machine.get` with `live` shows it: one
//! bounded probe of its control socket and the evidence of what answered.
//! The host doctor (`layerfs-inspect`) reads the same probe and evidence.
use crate::worker::{HostObservation, HostPaths, HostProbe};
use layerfs_home::home::with_field;
use layerfs_protocol::sdk::SdkValue;
use std::path::Path;
use std::time::{Duration, Instant};

/// How long one probe of the host worker may take.
pub const TIMEOUT: Duration = Duration::from_secs(2);

/// One probe of the host worker, within `timeout`.
pub fn observe(home: &Path, timeout: Duration) -> HostObservation {
    match HostPaths::new(home) {
        Ok(paths) => paths.observe(Instant::now() + timeout),
        Err(error) => HostObservation::Unreachable(HostProbe::Endpoint, error),
    }
}

/// The evidence of a probe: the worker's health and what its hello said.
pub fn evidence(observation: &HostObservation) -> SdkValue {
    use HostObservation::*;
    let (health, reply) = match observation {
        Absent => ("absent", None),
        Stale => ("stale", None),
        Owned => ("owned", None),
        Ready(reply) => ("ready", Some(reply)),
        Draining(hello) => ("draining", Some(hello)),
        Incompatible(error) => ("unreachable", error.detail.get("worker")),
        Unreachable(_, _) => ("unreachable", None),
    };
    let mut fields = vec![("health", SdkValue::text(health))];
    fields.extend(
        [
            "pid",
            "pid_start",
            "pid_boot",
            "build",
            "generation",
            "admitted",
            "queued",
            "stores",
            "postgresql",
            "last_close",
        ]
        .into_iter()
        .map(|key| {
            (
                key,
                reply.and_then(|v| v.get(key)).cloned().unwrap_or_default(),
            )
        }),
    );
    if let Incompatible(error) = observation {
        fields.push(("compatible", SdkValue::Bool(false)));
        fields.push((
            "expected_build",
            SdkValue::opt_text(
                error
                    .detail
                    .get("expected")
                    .and_then(|value| value.get_text("build")),
            ),
        ));
    }
    if let Unreachable(stage, error) = observation {
        fields.push(("code", SdkValue::text(&error.code)));
        fields.push((
            "stage",
            SdkValue::text(match stage {
                HostProbe::Endpoint => "endpoint",
                HostProbe::Connect => "connect",
                HostProbe::Peer => "peer",
                HostProbe::Hello => "hello",
                HostProbe::Ping => "ping",
            }),
        ));
    }
    layerfs_home::helpers::map_of(fields)
}

/// `machine.get`'s `answer` with its `live` section's `machine_state` and
/// `worker` evidence from one probe.
pub fn live(home: &Path, answer: SdkValue) -> SdkValue {
    let probe = observe(home, TIMEOUT);
    let live = answer.get("live").cloned().unwrap_or_default();
    let state = if answer.get_text("state") == Some("stopped") {
        "stopped"
    } else {
        "enabled"
    };
    let live = with_field(live, "machine_state", SdkValue::text(state));
    with_field(answer, "live", with_field(live, "worker", evidence(&probe)))
}
