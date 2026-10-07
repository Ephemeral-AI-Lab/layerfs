//! Final source-scoped prefix inventory; no numerical or resource qualification.
use super::{
    digest::{hex, sha256},
    json::Json,
    streams::Recorder,
};
use std::io;
pub fn finish(r: &mut Recorder, status: &str, error: Option<&str>) -> io::Result<()> {
    let line = if status == "RECORDED" {
        "E04_DIAGNOSTIC_RECORDED qualification=NOT_EVALUATED samples=0\n".to_owned()
    } else {
        format!(
            "E04_DIAGNOSTIC_FAILED {}\n",
            error.unwrap_or("original failure")
        )
    };
    r.append(
        if status == "RECORDED" {
            "stdout"
        } else {
            "stderr"
        },
        line.into_bytes(),
    )?;
    let mut outcome = Json::new();
    r.header(&mut outcome, "outcomes", 0)?;
    outcome.raw(",")?;
    outcome.text("status", status)?;
    outcome.raw(",")?;
    outcome.text("startup_status", r.startup_status)?;
    outcome.raw(",")?;
    outcome.text("stop_status", r.stop_status)?;
    outcome.raw(",\"fixture\":")?;
    match &r.fixture_context {
        Some(value) => outcome.raw(value.trim())?,
        None => outcome.raw("null")?,
    };
    outcome.raw(",\"oracle\":")?;
    match &r.oracle_summary {
        Some(value) => outcome.raw(value.trim())?,
        None => outcome.raw("null")?,
    };
    outcome.raw(",\"database_artifact\":")?;
    inventory(&mut outcome, std::path::Path::new(&r.invocation[5]))?;
    outcome.raw(",")?;
    outcome.field("write_records", r.trace.records)?;
    outcome.raw(",")?;
    outcome.field("write_attempts", r.write_attempts)?;
    outcome.raw(",\"complete_command_ns\":null,\"complete_command_status\":\"UNAVAILABLE-external-command-wall-required\",")?;
    outcome.field("collector_span_ns", r.at())?;
    outcome.raw(",\"original_error\":")?;
    match error {
        Some(value) => outcome.string(value)?,
        None => outcome.raw("null")?,
    };
    outcome.raw(",\"database_retained\":true,\"E05_status\":\"NOT_RUN\"}")?;
    let outcome = outcome.finish()?;
    r.write_once("outcomes.json", &outcome)?;
    let mut manifest = Json::new();
    r.header(&mut manifest, "manifest", 0)?;
    manifest.raw(",\"driver_version\":\"e04-original-write-receipts-v2\",\"e1_sample_status\":\"NOT_RUN\",\"e1_sample_count\":0,\"E05_status\":\"NOT_RUN\",\"observation_consistency\":\"INCOMPLETE\",\"write_window_consistency\":\"NOT_EVALUATED\",\"streams\":[")?;
    for (index, stream) in [
        &r.startup, &r.jobs, &r.probes, &r.trace, &r.stdout, &r.stderr,
    ]
    .into_iter()
    .enumerate()
    {
        if index != 0 {
            manifest.raw(",")?;
        }
        stream.descriptor(
            &mut manifest,
            if stream.records == 0 {
                "NOT_RUN-or-empty"
            } else {
                "RECORDED"
            },
        )?;
    }
    manifest.raw("],\"fixture_inputs\":{")?;
    descriptor(
        &mut manifest,
        "assignment",
        &r.invocation[2],
        r.assignment_bytes,
        &r.assignment_sha256,
    )?;
    manifest.raw(",")?;
    descriptor(
        &mut manifest,
        "acquisition",
        &std::path::Path::new(&r.invocation[2])
            .with_extension("fixture")
            .to_string_lossy(),
        r.acquisition_bytes,
        &r.acquisition_sha256,
    )?;
    manifest.raw(",")?;
    descriptor(
        &mut manifest,
        "base",
        &r.invocation[3],
        16777216,
        &r.base_sha256,
    )?;
    manifest.raw(",")?;
    descriptor(
        &mut manifest,
        "replacements",
        &r.invocation[4],
        4096000,
        &r.replacements_sha256,
    )?;
    manifest.raw(",\"seed\":1,\"base_identity\":0,\"writes\":1000},\"oracle_source\":{\"path\":\"oracle-source.rs\",")?;
    let oracle = include_str!("oracle.rs");
    manifest.field("bytes", oracle.len())?;
    manifest.raw(",")?;
    manifest.text("sha256", &hex(&sha256(oracle.as_bytes())))?;
    manifest.raw(",\"source_path\":\"core/crates/layerfs-daemon/examples/e2_writes/oracle.rs\"},\"outcomes\":{")?;
    manifest.text("path", "outcomes.json")?;
    manifest.raw(",")?;
    manifest.field("bytes", outcome.len())?;
    manifest.raw(",")?;
    manifest.text("sha256", &hex(&sha256(&outcome)))?;
    manifest.raw("},\"unavailable\":[\"private-base-facts-provider-id-trace\",\"per-original-job-statement-families\",\"whole-operation-copies\",\"statement-fingerprint-bind-correlation\",\"indexed-visited-rows\",\"exact-eligible-debt\",\"queue-only-peak\",\"isolated-resource-runnable-wait\",\"host-linux-kernel-phase-residency\",\"physical-io\",\"cache-enforcement-calibration\"],\"forbidden_substitutions\":{\"phase_resident_bytes\":null,\"eligible_debt_peak_bytes\":null,\"queue_peak_jobs\":null,\"whole_operation_copy_bytes\":null},\"unrun_writes\":{")?;
    manifest.field("from_index", r.write_attempts)?;
    manifest.raw(",\"to_exclusive\":1000},\"database_retained\":true}")?;
    r.write_once("manifest.json", &manifest.finish()?)
}

fn descriptor(out: &mut Json, name: &str, path: &str, bytes: u64, hash: &str) -> io::Result<()> {
    out.string(name)?;
    out.raw(":{")?;
    out.text("path", path)?;
    out.raw(",")?;
    out.field("bytes", bytes)?;
    out.raw(",")?;
    out.text("sha256", hash)?;
    out.raw("}")
}
fn inventory(out: &mut Json, path: &std::path::Path) -> io::Result<()> {
    out.raw("{\"scope\":\"separate-post-stop-or-original-failure-artifact-stat\",")?;
    out.text("path", &path.to_string_lossy())?;
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            out.raw(",\"status\":\"OBSERVED\",")?;
            out.field("logical_bytes", meta.len())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                out.raw(",")?;
                out.field(
                    "allocated_bytes",
                    meta.blocks()
                        .checked_mul(512)
                        .ok_or_else(|| io::Error::other("allocation overflow"))?,
                )?;
                out.raw(",")?;
                out.field("device", meta.dev())?;
                out.raw(",")?;
                out.field("inode", meta.ino())?;
                out.raw(",")?;
                out.field("links", meta.nlink())?;
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
