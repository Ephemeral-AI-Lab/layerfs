//! One startup, source-scoped diagnostics and explicit Stop; no route is invented.
use super::{json::Json, records, streams::Recorder};
use layerfs_daemon::{Owner, OwnerConfig, OwnerError, OwnerStart};
use layerfs_overlay::{CreationWork, ProfileConfig};
use std::{fmt, io, path::Path, sync::Arc, time::Instant};

pub struct Failure {
    pub cause: Box<dyn std::error::Error>,
    pub original_startup_error: Option<OwnerError>,
    pub original_stop_error: Option<OwnerError>,
    pub original_creation: Option<Arc<CreationWork>>,
    pub retained_owner: Option<Owner>,
    recorder: Option<Recorder>,
}
impl fmt::Debug for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = f.debug_struct("OriginalE01Failure");
        out.field("cause", &self.cause)
            .field("original_startup_error", &self.original_startup_error)
            .field("original_stop_error", &self.original_stop_error)
            .field("original_creation", &self.original_creation)
            .field("owner_retained", &self.retained_owner.is_some());
        if let Some(recorder) = &self.recorder {
            out.field("output", &recorder.root)
                .field("startup_acknowledged_bytes", &recorder.startup.bytes)
                .field("jobs_acknowledged_bytes", &recorder.jobs.bytes)
                .field("probes_acknowledged_bytes", &recorder.probes.bytes);
        }
        out.finish()
    }
}
impl Failure {
    fn before(cause: impl std::error::Error + 'static) -> Self {
        Self {
            cause: Box::new(cause),
            original_startup_error: None,
            original_stop_error: None,
            original_creation: None,
            retained_owner: None,
            recorder: None,
        }
    }
}
fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_nanos().min(u64::MAX as u128) as u64
}
fn inventory(out: &mut Json, path: &Path) -> io::Result<()> {
    out.raw("{\"scope\":\"separate-post-stop-artifact-stat\",")?;
    out.text(
        "path",
        path.to_str()
            .ok_or_else(|| io::Error::other("E01 path UTF-8"))?,
    )?;
    match std::fs::symlink_metadata(path) {
        Ok(v) => {
            out.raw(",\"status\":\"OBSERVED\",")?;
            out.field("logical_bytes", v.len())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                out.raw(",")?;
                out.field(
                    "allocated_bytes",
                    v.blocks()
                        .checked_mul(512)
                        .ok_or_else(|| io::Error::other("artifact allocation overflow"))?,
                )?;
                out.raw(",")?;
                out.field("device", v.dev())?;
                out.raw(",")?;
                out.field("inode", v.ino())?;
                out.raw(",")?;
                out.field("links", v.nlink())?;
            }
            #[cfg(not(unix))]
            out.raw(",\"allocated_bytes\":null,\"device\":null,\"inode\":null,\"links\":null")?;
        }
        Err(error) => {
            out.raw(
                ",\"status\":\"UNAVAILABLE\",\"logical_bytes\":null,\"allocated_bytes\":null,",
            )?;
            out.text("error", &format!("{error:?}"))?;
        }
    }
    out.raw("}")
}
fn diagnostics(
    recorder: &mut Recorder,
    owner: &Owner,
    label: &str,
    index: u64,
    start: Instant,
) -> io::Result<()> {
    let mut out = Json::new();
    recorder.header(&mut out, "owner-diagnostics", index)?;
    out.raw(",")?;
    out.text("endpoint", label)?;
    out.raw(",\"scope\":\"separate-post-ready-owner-diagnostics\",\"is_sql_command\":false,")?;
    out.field("at_ns", elapsed(start))?;
    match owner.client().diagnostics() {
        Ok(work) => {
            out.raw(",\"status\":\"OBSERVED\",\"work\":")?;
            records::owner(&mut out, work)?;
        }
        Err(error) => {
            out.raw(",\"status\":\"UNAVAILABLE\",\"work\":null,")?;
            out.text("error", &format!("{error:?}"))?;
        }
    }
    out.raw("}")?;
    recorder.probes.append(&out.finish()?)
}
fn finish(
    recorder: &mut Recorder,
    db: &Path,
    start: Instant,
    startup_status: &str,
    stop_status: &str,
    error: Option<&str>,
) -> io::Result<()> {
    let line = if startup_status == "READY" && stop_status == "STOPPED" {
        "E2_STARTUP_DIAGNOSTIC_RECORDED qualification=NOT_EVALUATED\n".to_string()
    } else {
        format!(
            "E2_STARTUP_FAILED {}\n",
            error.unwrap_or("original failure")
        )
    };
    if startup_status == "READY" && stop_status == "STOPPED" {
        recorder.stdout.append(line.as_bytes())?;
    } else {
        recorder.stderr.append(line.as_bytes())?;
    }
    let mut out = Json::new();
    recorder.header(&mut out, "outcomes", 3)?;
    out.raw(",")?;
    out.text("startup_status", startup_status)?;
    out.raw(",")?;
    out.text("stop_status", stop_status)?;
    out.raw(",\"route\":null,\"route_status\":\"NOT_IN_SCOPE\",\"source_owners\":null,\"reply_publications\":null,\"complete_command_ns\":null,\"complete_command_status\":\"UNAVAILABLE-external-command-wall-required\",\"collector_span_scope\":\"entry-through-stop-and-log-before-final-files\",")?;
    out.field("collector_span_ns", elapsed(start))?;
    out.raw(",\"original_error\":")?;
    if let Some(error) = error {
        out.string(error)?;
    } else {
        out.raw("null")?;
    }
    out.raw(",\"database_artifact\":")?;
    inventory(&mut out, db)?;
    out.raw("}")?;
    let outcome = out.finish()?;
    recorder.write_once("outcomes.json", &outcome)?;
    let mut manifest = Json::new();
    recorder.header(&mut manifest, "manifest", 4)?;
    manifest.raw(",\"driver_version\":\"e01-startup-original-receipts-v1\",\"e1_sample_status\":\"NOT_RUN\",\"e1_sample_count\":0,\"observation_consistency\":\"NOT_EVALUATED\",\"streams\":[")?;
    recorder.startup.descriptor(&mut manifest, "RECORDED")?;
    manifest.raw(",")?;
    recorder
        .jobs
        .descriptor(&mut manifest, "UNRUN-route_unavailable")?;
    manifest.raw(",")?;
    recorder.probes.descriptor(
        &mut manifest,
        if startup_status == "READY" {
            "RECORDED"
        } else {
            "UNRUN-startup_failed"
        },
    )?;
    manifest.raw(",")?;
    recorder.stdout.descriptor(&mut manifest, "COLLECTOR_LOG")?;
    manifest.raw(",")?;
    recorder.stderr.descriptor(&mut manifest, "COLLECTOR_LOG")?;
    manifest.raw("],\"identity_input\":{\"path\":\"identity.json\",")?;
    manifest.field("bytes", recorder.identity_bytes)?;
    manifest.raw(",")?;
    manifest.text("sha256", &recorder.identity_sha256)?;
    manifest.raw("},\"outcomes\":{\"path\":\"outcomes.json\",")?;
    manifest.field("bytes", outcome.len())?;
    manifest.raw(",")?;
    manifest.text(
        "sha256",
        &super::digest::hex(&super::digest::sha256(&outcome)),
    )?;
    manifest.raw("},\"unavailable\":[\"route-dependent-resources-pages-freelist-probes\",\"whole-operation-copies\",\"statement-fingerprint-bind-correlation\",\"indexed-visited-rows\",\"exact-eligible-debt\",\"queue-only-peak\",\"isolated-resource-runnable-wait\",\"host-linux-kernel-phase-residency\",\"physical-io\",\"cache-enforcement-calibration\"],\"forbidden_substitutions\":{\"phase_resident_bytes\":null,\"eligible_debt_peak_bytes\":null,\"queue_peak_jobs\":null,\"whole_operation_copy_bytes\":null},\"database_retained\":true}")?;
    recorder.write_once("manifest.json", &manifest.finish()?)
}
pub fn collect(db: &Path, out: &Path, identity: &Path) -> Result<(), Failure> {
    collect_with(
        db,
        out,
        identity,
        ProfileConfig::default(),
        OwnerConfig::default(),
    )
}
/// External proof cases select an original refusal; the executable always uses defaults.
pub fn collect_with(
    db: &Path,
    out: &Path,
    identity: &Path,
    profile: ProfileConfig,
    config: OwnerConfig,
) -> Result<(), Failure> {
    if !db.is_absolute() || !out.is_absolute() || db.starts_with(out) {
        return Err(Failure::before(io::Error::other(
            "separate absolute owned DB/output paths required",
        )));
    }
    let start = Instant::now();
    let mut recorder = Recorder::create(db, out, identity).map_err(Failure::before)?;
    let opened_ns = elapsed(start);
    let started = Owner::start_observed(db, profile, config);
    let closed_ns = elapsed(start);
    let recorded = (|| {
        let mut json = Json::new();
        recorder.header(&mut json, "startup", 0)?;
        json.raw(",\"clock\":\"process-local-Instant-elapsed\",")?;
        json.field("opened_ns", opened_ns)?;
        json.raw(",")?;
        json.field("closed_ns", closed_ns)?;
        json.raw(",\"configuration\":")?;
        records::configuration(&mut json, profile, config)?;
        records::startup(&mut json, &started)?;
        json.raw("}")?;
        recorder.startup.append(&json.finish()?)
    })();
    let OwnerStart {
        result, startup, ..
    } = started;
    let mut failure = Failure {
        cause: Box::new(io::Error::other("original E01 failure")),
        original_startup_error: None,
        original_stop_error: None,
        original_creation: Some(startup),
        retained_owner: None,
        recorder: None,
    };
    match result {
        Ok(owner) => failure.retained_owner = Some(owner),
        Err(error) => failure.original_startup_error = Some(error),
    }
    if let Err(error) = recorded {
        failure.cause = Box::new(error);
        failure.recorder = Some(recorder);
        return Err(failure);
    }
    if let Some(error) = &failure.original_startup_error {
        let message = format!("{error:?}");
        if let Err(error) = finish(
            &mut recorder,
            db,
            start,
            "FAILED",
            "UNRUN-no-ready-owner",
            Some(&message),
        ) {
            failure.cause = Box::new(error);
        }
        failure.recorder = Some(recorder);
        return Err(failure);
    }
    let probe = (|| {
        let owner = failure
            .retained_owner
            .as_ref()
            .expect("known original ready owner");
        diagnostics(&mut recorder, owner, "post-ready", 1, start)?;
        diagnostics(&mut recorder, owner, "pre-stop", 2, start)
    })();
    if let Err(error) = probe {
        failure.cause = Box::new(error);
        failure.recorder = Some(recorder);
        return Err(failure);
    }
    let stopped = failure
        .retained_owner
        .take()
        .expect("one original Stop owner")
        .stop();
    if let Err(error) = stopped {
        let message = format!("{error:?}");
        failure.original_stop_error = Some(error);
        if let Err(error) = finish(&mut recorder, db, start, "READY", "FAILED", Some(&message)) {
            failure.cause = Box::new(error);
        }
        failure.recorder = Some(recorder);
        return Err(failure);
    }
    finish(&mut recorder, db, start, "READY", "STOPPED", None).map_err(|error| {
        failure.cause = Box::new(error);
        failure.recorder = Some(recorder);
        failure
    })
}
