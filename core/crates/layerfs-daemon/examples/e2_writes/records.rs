//! Source-scoped original outcomes. No unavailable value becomes a zero metric.
use super::{digest::hex, json::Json, observed, streams::Recorder};
use layerfs_daemon::{Completion, JobWork, OwnerWork, Response, ServiceClass};
use layerfs_overlay::{BaseSource, Inode, Publication, Resources, Route};
use layerfs_workspace::{ClientWork, JobOutcome, Need};
use std::io;

pub struct JobInfo {
    pub record_id: String,
    pub external_job_id: String,
    pub name: &'static str,
    pub class: ServiceClass,
    pub phase: &'static str,
    pub operation: Option<u64>,
    pub route: Option<Route>,
    pub source: Option<BaseSource>,
    pub opened: u64,
    pub publication: Option<Publication>,
    pub span_scope: &'static str,
}
pub fn publication(out: &mut Json, value: Publication) -> io::Result<()> {
    out.raw("{")?;
    out.field("namespace", value.route().namespace())?;
    out.raw(",")?;
    out.field("generation", value.generation.number())?;
    out.raw(",")?;
    out.field("revision", value.revision())?;
    out.raw("}")
}
fn inode(out: &mut Json, value: &Inode) -> io::Result<()> {
    out.raw("{")?;
    for (index, (name, value)) in [
        ("serial", value.serial),
        ("kind", value.kind as u64),
        ("mode", u64::from(value.mode)),
        ("nlink", value.nlink),
        ("size", value.size),
        ("born", value.born),
        ("entries", value.entries),
        ("inherited_cutoff", value.inherited_cutoff),
        ("mtime_nanoseconds", u64::from(value.mtime_nanoseconds)),
    ]
    .into_iter()
    .enumerate()
    {
        if index != 0 {
            out.raw(",")?;
        }
        out.field(name, value)?;
    }
    out.raw(",")?;
    out.field("mtime_seconds", value.mtime_seconds)?;
    out.raw("}")
}
pub fn optional_inode(out: &mut Json, value: &Option<Inode>) -> io::Result<()> {
    match value {
        Some(v) => inode(out, v),
        None => out.raw("null"),
    }
}
pub fn source(out: &mut Json, value: BaseSource) -> io::Result<()> {
    out.raw("{")?;
    out.field("namespace", value.route().namespace())?;
    out.raw(",")?;
    out.field("owner", value.owner())?;
    out.raw(",")?;
    out.text("root", &hex(&value.root()))?;
    out.raw("}")
}
// PathName is already canonical and bounded; retain every byte without hashing
// or imposing the digest helper's fixed 32-byte identity representation.
fn name_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        value.push(DIGITS[usize::from(byte >> 4)] as char);
        value.push(DIGITS[usize::from(byte & 15)] as char);
    }
    value
}
fn needs(out: &mut Json, values: &[Need]) -> io::Result<()> {
    out.raw("[")?;
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            out.raw(",")?;
        }
        match value {
            Need::Inode(serial) => {
                out.raw("{\"type\":\"inode\",")?;
                out.field("serial", serial)?;
                out.raw("}")?;
            }
            Need::Name(parent, name) => {
                out.raw("{\"type\":\"name\",")?;
                out.field("parent", parent)?;
                out.raw(",")?;
                out.text("name_hex", &name_hex(name.as_bytes()))?;
                out.raw("}")?;
            }
        }
    }
    out.raw("]")
}
fn namespace(out: &mut Json, outcome: &JobOutcome) -> io::Result<()> {
    match outcome {
        JobOutcome::Applied {
            publication: value,
            inode,
        } => {
            out.raw("{\"outcome\":\"Applied\",\"publication\":")?;
            publication(out, *value)?;
            out.raw(",\"inode\":")?;
            optional_inode(out, inode)?;
            out.raw("}")
        }
        JobOutcome::Unchanged { inode } => {
            out.raw("{\"outcome\":\"Unchanged\",\"inode\":")?;
            optional_inode(out, inode)?;
            out.raw("}")
        }
        JobOutcome::Needs(values) => {
            out.raw("{\"outcome\":\"Needs\",\"needs\":")?;
            needs(out, values)?;
            out.raw("}")
        }
        JobOutcome::Refused(value) => {
            out.raw("{\"outcome\":\"Refused\",")?;
            out.text("refusal", &format!("{value:?}"))?;
            out.raw("}")
        }
    }
}
pub fn resources(out: &mut Json, value: &Resources) -> io::Result<()> {
    out.raw(
        "{\"scope\":\"shared-pages-and-logical-counts-not-exclusive-physical\",\"allocation\":",
    )?;
    observed::allocation_state(out, value.allocation)?;
    out.raw(",")?;
    out.field("database_pages", value.database_pages)?;
    out.raw(",")?;
    out.field("free_pages", value.free_pages)?;
    out.raw(",")?;
    out.field("debt_upper_bytes", value.debt_upper_bytes)?;
    out.raw(",\"counts\":{")?;
    let c = value.counts;
    for (index, (name, value)) in [
        ("namespaces", c.namespaces),
        ("inode_rows", c.inode_rows),
        ("name_rows", c.name_rows),
        ("payload_cells", c.payload_cells),
        ("payload_bytes", c.payload_bytes),
        ("shrink_rows", c.shrink_rows),
        ("scratch_rows", c.scratch_rows),
        ("scratch_bytes", c.scratch_bytes),
        ("orphan_rows", c.orphan_rows),
        ("owner_rows", c.owner_rows),
        ("source_rows", c.source_rows),
        ("reply_tickets", c.reply_tickets),
        ("maintenance_targets", c.maintenance_targets),
        ("ready_targets", c.ready_targets),
        ("retire_targets", c.retire_targets),
        ("wait_refs", c.wait_refs),
        ("owner_details", c.owner_details),
    ]
    .into_iter()
    .enumerate()
    {
        if index != 0 {
            out.raw(",")?;
        }
        out.field(name, value)?;
    }
    out.raw("}}")
}
fn response(out: &mut Json, value: &Response) -> io::Result<&'static str> {
    match value {
        Response::Opened(route) => {
            out.raw("{")?;
            out.field("namespace", route.namespace())?;
            out.raw("}")?;
            Ok("Opened")
        }
        Response::BaseSource(value) => {
            source(out, *value)?;
            Ok("BaseSource")
        }
        Response::Namespace(value) => {
            namespace(out, value)?;
            Ok("Namespace")
        }
        Response::Done => {
            out.raw("null")?;
            Ok("Done")
        }
        Response::Inode(value) => {
            optional_inode(out, value)?;
            Ok("Inode")
        }
        Response::Dentry(value) => {
            out.raw("{\"present\":")?;
            out.raw(if value.is_some() { "true" } else { "false" })?;
            out.raw("}")?;
            Ok("Dentry")
        }
        Response::Names(value) => {
            out.raw("{")?;
            out.field("active_rows", value.active.len())?;
            out.raw(",")?;
            out.field("captured_rows", value.captured.len())?;
            out.raw("}")?;
            Ok("Names")
        }
        Response::Read(value) => {
            out.raw("{\"present\":")?;
            out.raw(if value.is_some() { "true" } else { "false" })?;
            if let Some(value) = value {
                out.raw(",")?;
                out.field("data_bytes", value.data.len())?;
                out.raw(",")?;
                out.field("data_capacity", value.data.capacity())?;
                out.raw(",")?;
                out.field("inherited_bytes", value.inherited.len())?;
                out.raw(",")?;
                out.field("inherited_capacity", value.inherited.capacity())?;
            }
            out.raw("}")?;
            Ok("Read")
        }
        Response::Cell(value) => {
            out.raw("{\"present\":")?;
            out.raw(if value.is_some() { "true" } else { "false" })?;
            out.raw("}")?;
            Ok("Cell")
        }
        Response::Resources(value) => {
            resources(out, value)?;
            Ok("Resources")
        }
        Response::CleanupState(value) => {
            out.raw("{")?;
            out.text("state", &format!("{value:?}"))?;
            out.raw("}")?;
            Ok("CleanupState")
        }
        Response::MaintenanceIdle(value) => {
            out.raw("{")?;
            out.field("idle", value)?;
            out.raw("}")?;
            Ok("MaintenanceIdle")
        }
        _ => {
            out.raw("{\"status\":\"UNAVAILABLE-unexpected-response-shape\"}")?;
            Ok("Unexpected")
        }
    }
}
pub fn work(out: &mut Json, value: &JobWork) -> io::Result<()> {
    out.raw("{\"sql_scope\":\"original-job-statement-total\",\"statement_family_status\":\"UNAVAILABLE\",\"sql\":")?;
    observed::statement(out, value.sql.total())?;
    out.raw(",\"payload\":")?;
    observed::payload(out, value.payload)?;
    out.raw(",\"allocation\":")?;
    observed::allocation(out, value.allocation)?;
    out.raw(",")?;
    out.field("parked_turns", value.parked_turns)?;
    out.raw(",")?;
    out.field("queue_wait_ns", value.queue_wait_ns)?;
    out.raw(",")?;
    out.field("service_ns", value.service_ns)?;
    out.raw("}")
}
pub fn job(
    recorder: &Recorder,
    index: u64,
    info: &JobInfo,
    completion: Option<&Completion>,
    disposition: &str,
    error: Option<&str>,
) -> io::Result<Vec<u8>> {
    let mut out = Json::new();
    recorder.header_with_id(&mut out, "owner-job", index, &info.record_id)?;
    out.raw(",")?;
    out.text("external_job_id", &info.external_job_id)?;
    out.raw(",")?;
    out.text("phase", info.phase)?;
    out.raw(",\"operation_index\":")?;
    match info.operation {
        Some(v) => out.raw(&v.to_string())?,
        None => out.raw("null")?,
    };
    out.raw(",")?;
    out.text("command_kind", info.name)?;
    out.raw(",")?;
    out.field("class", info.class as usize)?;
    out.raw(",\"route_ns\":")?;
    match info.route {
        Some(v) => out.raw(&v.namespace().to_string())?,
        None => out.raw("null")?,
    };
    out.raw(",\"source\":")?;
    match info.source {
        Some(v) => source(&mut out, v)?,
        None => out.raw("null")?,
    };
    out.raw(",\"publication\":")?;
    match info.publication {
        Some(value) => publication(&mut out, value)?,
        None => out.raw("null")?,
    };
    out.raw(",")?;
    out.field("opened_ns", info.opened)?;
    out.raw(",")?;
    out.field("closed_ns", recorder.at())?;
    out.raw(",")?;
    out.text("span_scope", info.span_scope)?;
    out.raw(",")?;
    out.text("disposition", disposition)?;
    out.raw(",\"original_result\":{")?;
    let result = completion.map(Completion::result);
    out.text(
        "status",
        if result.is_some_and(Result::is_ok) {
            "OK"
        } else {
            "FAILED"
        },
    )?;
    out.raw(",")?;
    out.text(
        "custody",
        if completion.is_some() {
            "original-completion-retained-during-record"
        } else {
            "original-unattempted-command-or-pending-retained"
        },
    )?;
    out.raw(",\"details\":")?;
    let response_kind = match result {
        Some(Ok(value)) => response(&mut out, value)?,
        _ => {
            out.raw("null")?;
            "None"
        }
    };
    out.raw(",")?;
    out.text("response_kind", response_kind)?;
    out.raw(",\"error\":")?;
    if let Some(error) = error {
        out.string(error)?;
    } else if let Some(Err(error)) = result {
        out.string(&format!("{error:?}"))?;
    } else {
        out.raw("null")?;
    }
    out.raw("},\"work\":")?;
    match completion {
        Some(v) => work(&mut out, v.work())?,
        None => out.raw("null")?,
    };
    out.raw(",\"copy_scope\":\"UNAVAILABLE-private-input-and-exclusive-copy-accounting\"}")?;
    out.finish()
}
pub fn owner(
    recorder: &mut Recorder,
    client: &layerfs_daemon::OwnerClient,
    endpoint: &str,
) -> io::Result<OwnerWork> {
    let index = recorder.probes.records;
    let id = recorder.id("owner-diagnostics", index);
    let work = client.diagnostics().map_err(io::Error::other)?;
    let mut out = Json::new();
    recorder.header_with_id(&mut out, "owner-diagnostics", index, &id)?;
    out.raw(",")?;
    out.text("endpoint", endpoint)?;
    out.raw(",")?;
    out.field("at_ns", recorder.at())?;
    out.raw(",\"status\":\"OBSERVED\",\"work\":")?;
    observed::owner(&mut out, work)?;
    out.raw("}")?;
    recorder.append("probes", out.finish()?)?;
    Ok(work)
}
pub fn client(out: &mut Json, value: ClientWork) -> io::Result<()> {
    out.raw("{")?;
    for (index, (name, value)) in [
        ("cache_hits", value.cache_hits),
        ("cache_misses", value.cache_misses),
        ("upstream_batches", value.upstream_batches),
        ("authenticated_bytes", value.authenticated_bytes),
        ("evictions", value.evictions),
        ("charged_cache_bytes", value.charged_cache_bytes as u64),
        ("cached_objects", value.cached_objects as u64),
    ]
    .into_iter()
    .enumerate()
    {
        if index != 0 {
            out.raw(",")?;
        }
        out.field(name, value)?;
    }
    out.raw("}")
}
