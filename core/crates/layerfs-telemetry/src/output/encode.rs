//! Fixed, bounded diagnostic envelopes, separate from mandatory product output.
use crate::{observation::Observation, operation::OperationReport};
use std::io::Write;
/// Explicit process/clock namespace. PID alone is never an incarnation.
#[derive(Clone, Copy, Debug)]
pub struct Identity {
    /// Configured run identifier, shared by participating processes.
    pub run: u128,
    /// Process identifier in this role's namespace.
    pub pid: u32,
    /// Configured role: 1 service, 2 daemon, 3 collector.
    pub role: u8,
    /// Configured host/container namespace, unique within this run.
    pub namespace: u64,
}
/// Encodes an operation and a valid clipped timer projection within the cap.
pub fn encode_operation(
    identity: Identity,
    report: &OperationReport,
    maximum: usize,
) -> Option<Vec<u8>> {
    if !(1024..=16384).contains(&maximum) {
        return None;
    }
    let mut prefix = Vec::with_capacity(256);
    write!(prefix,"LFT1 {{\"v\":1,\"namespace\":{},\"run\":\"{:032x}\",\"pid\":{},\"role\":{},\"kind\":\"operation\",\"key\":{},\"timing\":",identity.namespace,identity.run,identity.pid,identity.role,report.key()).ok()?;
    let mut bytes = Vec::with_capacity(1024);
    write!(
        bytes,
        ",\"success\":{},\"resource_status\":\"{}\"",
        report.success(),
        report.resource_status()
    )
    .ok()?;
    if let Some(w) = report.window() {
        write!(
            bytes,
            ",\"samples\":{},\"gaps\":{},\"concurrency\":{},\"cpu_shared_ns\":",
            w.samples, w.gaps, w.concurrency
        )
        .ok()?;
        if let Some((user, system)) = w.cpu_delta() {
            write!(bytes, "[{user},{system}]").ok()?;
        } else {
            bytes.extend_from_slice(b"null");
        }
        bytes.extend_from_slice(
            b",\"scope\":\"process-shared\",\"clock\":\"local-monitor\",\"first_ns\":",
        );
        number(&mut bytes, w.first.map(|s| s.at_ns));
        bytes.extend_from_slice(b",\"opened_ns\":");
        number(&mut bytes, w.opened_ns);
        bytes.extend_from_slice(b",\"closed_ns\":");
        number(&mut bytes, w.closed_ns);
        bytes.extend_from_slice(b",\"last_ns\":");
        number(&mut bytes, w.last.map(|s| s.at_ns));
        bytes.extend_from_slice(b",\"incarnation\":");
        number(&mut bytes, w.first.map(|s| s.incarnation));
        write!(bytes, ",\"largest_gap_ns\":{}", w.largest_gap_ns).ok()?;
        if let Some(sample) = w.last {
            write!(
                bytes,
                ",\"source\":\"{}\",\"last_probe_ns\":",
                sample.source.as_str()
            )
            .ok()?;
            number(&mut bytes, sample.probe_ns);
        }
        bytes.extend_from_slice(b",\"selected\":");
        number(&mut bytes, w.last.map(|s| u64::from(s.selected)));
        bytes.extend_from_slice(b",\"sampled_max_rss\":");
        if let Some(rss) = w.sampled_max_rss {
            write!(bytes, "{rss}").ok()?;
        } else {
            bytes.extend_from_slice(b"null");
        }
    }
    bytes.extend_from_slice(b"}\n");
    let available = maximum.checked_sub(prefix.len().checked_add(bytes.len())?)?;
    let timing = report.timing().encode_bounded(available).ok()??;
    let mut output = Vec::with_capacity(maximum);
    output.extend_from_slice(&prefix);
    output.extend(
        timing
            .iter()
            .copied()
            .filter(|b| *b != b'\n' && *b != b'\r'),
    );
    output.extend_from_slice(&bytes);
    Some(output)
}
/// Constant-size periodic process observation; no raw-ring serialization.
pub fn encode_resource(identity: Identity, s: Observation) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(512);
    write!(bytes,"LFT1 {{\"v\":1,\"namespace\":{},\"run\":\"{:032x}\",\"pid\":{},\"role\":{},\"kind\":\"resource\",\"scope\":\"process-shared\",\"clock\":\"local-monitor\",\"at_ns\":{},\"incarnation\":{},\"user_ns\":",identity.namespace,identity.run,identity.pid,identity.role,s.at_ns,s.incarnation).expect("Vec write");
    number(&mut bytes, s.user_ns);
    write!(bytes, ",\"source\":\"{}\",\"probe_ns\":", s.source.as_str()).expect("Vec write");
    number(&mut bytes, s.probe_ns);
    write!(bytes, ",\"selected\":{}", s.selected).expect("Vec write");
    bytes.extend_from_slice(b",\"system_ns\":");
    number(&mut bytes, s.system_ns);
    bytes.extend_from_slice(b",\"rss\":");
    number(&mut bytes, s.rss);
    bytes.extend_from_slice(b"}\n");
    bytes
}
fn number(bytes: &mut Vec<u8>, n: Option<u64>) {
    if let Some(n) = n {
        write!(bytes, "{n}").expect("Vec write");
    } else {
        bytes.extend_from_slice(b"null");
    }
}
