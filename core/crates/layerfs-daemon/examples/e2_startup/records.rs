//! Typed original observations. Unavailable values are never populated from defaults.
use super::json::Json;
use layerfs_daemon::{OwnerConfig, OwnerStart, OwnerWork};
use layerfs_overlay::{
    AllocationState, AllocationWork, DatabaseProfile, DatabaseWork, PayloadWork, ProfileConfig,
    StatementWork,
};
use std::io;

fn numbers(out: &mut Json, fields: &[(&str, u64)]) -> io::Result<()> {
    out.raw("{")?;
    for (index, (name, value)) in fields.iter().enumerate() {
        if index != 0 {
            out.raw(",")?;
        }
        out.field(name, value)?;
    }
    out.raw("}")
}
pub fn statement(out: &mut Json, v: StatementWork) -> io::Result<()> {
    numbers(
        out,
        &[
            ("attempts", v.attempts),
            ("executions", v.executions),
            ("rows_returned", v.rows_returned),
            ("rows_changed", v.rows_changed),
            ("direct_rows_changed", v.direct_rows_changed),
            ("returned_blob_bytes", v.returned_blob_bytes),
            ("returned_value_bytes", v.returned_value_bytes),
            ("vm_steps", v.vm_steps),
            ("fullscan_steps", v.fullscan_steps),
            ("sorts", v.sorts),
            ("autoindex_rows", v.autoindex_rows),
            ("reprepares", v.reprepares),
            ("bound_bytes", v.bound_bytes),
            ("sql_bytes", v.sql_bytes),
            ("statement_memory_samples", v.statement_memory_samples),
            (
                "statement_memory_sample_bytes",
                v.statement_memory_sample_bytes,
            ),
            ("elapsed_ns", v.elapsed_ns),
        ],
    )
}
pub fn database(out: &mut Json, v: &DatabaseWork) -> io::Result<()> {
    out.raw("{\"scope\":\"connection-statement-families\",\"families\":[")?;
    for (index, value) in v.statements.iter().enumerate() {
        if index != 0 {
            out.raw(",")?;
        }
        statement(out, *value)?;
    }
    out.raw("],\"total\":")?;
    statement(out, v.total())?;
    out.raw("}")
}
pub fn allocation(out: &mut Json, v: AllocationWork) -> io::Result<()> {
    numbers(
        out,
        &[
            ("attempts", v.attempts),
            ("requested_bytes", v.requested_bytes),
            ("admitted_jobs", v.admitted_jobs),
            ("refusals", v.refusals),
            ("observations", v.observations),
            ("freelist_queries", v.freelist_queries),
        ],
    )
}
pub fn allocation_state(out: &mut Json, v: AllocationState) -> io::Result<()> {
    out.raw("{\"status\":\"OBSERVED\",\"high_water_scope\":\"daemon-startup-lifetime-observed-allocation\",\"requested_volume_meaning\":\"range-call-volume-not-new-disk\",\"values\":")?;
    numbers(
        out,
        &[
            ("logical_bytes", v.logical_bytes),
            ("allocated_bytes", v.allocated_bytes),
            ("high_water_allocated_bytes", v.high_water_allocated_bytes),
            ("reserved_tail_bytes", v.reserved_tail_bytes),
            ("cleanup_headroom_bytes", v.cleanup_headroom_bytes),
        ],
    )?;
    out.raw(",\"work\":")?;
    allocation(out, v.work)?;
    out.raw("}")
}
pub fn payload(out: &mut Json, v: PayloadWork) -> io::Result<()> {
    numbers(
        out,
        &[
            ("write_input_bytes", v.write_input_bytes),
            ("write_cells", v.write_cells),
            ("partial_write_cells", v.partial_write_cells),
            ("cell_copy_bytes", v.cell_copy_bytes),
            ("cell_zeroed_bytes", v.cell_zeroed_bytes),
            ("read_window_zeroed_bytes", v.read_window_zeroed_bytes),
            ("read_local_copy_bytes", v.read_local_copy_bytes),
        ],
    )
}
fn array(out: &mut Json, values: &[u64]) -> io::Result<()> {
    out.raw("[")?;
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            out.raw(",")?;
        }
        out.raw(&value.to_string())?;
    }
    out.raw("]")
}
pub fn owner(out: &mut Json, v: OwnerWork) -> io::Result<()> {
    out.raw("{\"scope\":\"same-owner-cumulative\",\"credit_scope\":\"queued-executing-caller-held-results\",\"sql_foreground\":")?;
    database(out, &v.sql_foreground)?;
    out.raw(",\"sql_maintenance\":")?;
    database(out, &v.sql_maintenance)?;
    out.raw(",\"payload_foreground\":")?;
    payload(out, v.payload_foreground)?;
    out.raw(",\"payload_maintenance\":")?;
    payload(out, v.payload_maintenance)?;
    out.raw(",\"allocation_foreground\":")?;
    allocation(out, v.allocation_foreground)?;
    out.raw(",\"allocation_maintenance\":")?;
    allocation(out, v.allocation_maintenance)?;
    out.raw(",\"counters\":")?;
    numbers(
        out,
        &[
            ("admitted", v.admitted),
            ("credited_bytes", v.credited_bytes as u64),
            ("peak_credited_bytes", v.peak_credited_bytes as u64),
            ("outstanding", v.outstanding as u64),
            ("maintenance_jobs", v.maintenance_jobs),
            ("maintenance_rows", v.maintenance_rows),
            ("maintenance_data_bytes", v.maintenance_data_bytes),
            ("maintenance_ns", v.maintenance_ns),
        ],
    )?;
    out.raw(",\"completed\":")?;
    array(out, &v.completed)?;
    out.raw(",\"queue_wait_ns\":")?;
    array(out, &v.queue_wait_ns)?;
    out.raw(",\"service_ns\":")?;
    array(out, &v.service_ns)?;
    out.raw("}")
}
pub fn configuration(out: &mut Json, profile: ProfileConfig, owner: OwnerConfig) -> io::Result<()> {
    out.raw("{\"profile\":{")?;
    out.field("pager_kib", profile.pager_kib)?;
    out.raw(",\"max_pages\":")?;
    if let Some(value) = profile.max_pages {
        out.raw(&value.to_string())?;
    } else {
        out.raw("null")?;
    }
    out.raw("},\"owner\":")?;
    numbers(
        out,
        &[
            ("bytes", owner.bytes as u64),
            ("lifecycle_reserve", owner.lifecycle_reserve as u64),
            ("namespaces", owner.namespaces as u64),
            ("jobs_per_namespace", owner.jobs_per_namespace as u64),
            (
                "lifecycle_jobs_per_namespace",
                owner.lifecycle_jobs_per_namespace as u64,
            ),
        ],
    )?;
    out.raw("}")
}
pub fn profile(out: &mut Json, v: &DatabaseProfile) -> io::Result<()> {
    out.raw("{")?;
    out.text("sqlite_version", &v.sqlite_version)?;
    out.raw(",")?;
    out.text("journal_mode", &v.journal_mode)?;
    out.raw(",")?;
    out.text("locking_mode", &v.locking_mode)?;
    for (name, value) in [
        ("schema_version", v.schema_version),
        ("synchronous", v.synchronous),
        ("mmap_size", v.mmap_size),
        ("cache_size", v.cache_size),
        ("page_size", v.page_size),
        ("max_pages", v.max_pages),
        ("foreign_keys", v.foreign_keys),
        ("busy_timeout", v.busy_timeout),
        ("temp_store", v.temp_store),
        ("auto_vacuum", v.auto_vacuum),
    ] {
        out.raw(",")?;
        out.field(name, value)?;
    }
    out.raw(",\"explicit_page_quota\":")?;
    if let Some(value) = v.explicit_page_quota {
        out.raw(&value.to_string())?;
    } else {
        out.raw("null")?;
    }
    out.raw(",\"compile_options\":[")?;
    for (index, value) in v.compile_options.iter().enumerate() {
        if index != 0 {
            out.raw(",")?;
        }
        out.string(value)?;
    }
    out.raw("]}")
}
pub fn startup(out: &mut Json, started: &OwnerStart) -> io::Result<()> {
    out.raw(",\"scope\":\"original-startup-through-readiness-or-error\",\"attempt_count\":1,\"creation_reported\":")?;
    out.raw(if started.creation_reported {
        "true"
    } else {
        "false"
    })?;
    out.raw(",")?;
    out.field("owner_elapsed_ns", started.elapsed_ns)?;
    out.raw(",\"original_outcome\":{")?;
    match &started.result {
        Ok(owner) => {
            out.raw("\"status\":\"READY\",\"error\":null,\"profile\":")?;
            profile(out, owner.profile())?;
        }
        Err(error) => {
            out.raw("\"status\":\"FAILED\",\"profile\":null,")?;
            out.text("error", &format!("{error:?}"))?;
        }
    }
    out.raw("},\"creation\":")?;
    if !started.creation_reported {
        return out.raw("{\"status\":\"UNAVAILABLE\",\"work\":null,\"reason\":\"no-original-creation-receipt\"}");
    }
    let v = &started.startup;
    out.raw("{\"status\":\"OBSERVED\",\"work\":{\"calls\":")?;
    numbers(
        out,
        &[
            ("file_create_calls", v.file_create_calls),
            ("sqlite_open_calls", v.sqlite_open_calls),
            (
                "connection_configuration_calls",
                v.connection_configuration_calls,
            ),
            ("cache_configuration_calls", v.cache_configuration_calls),
        ],
    )?;
    out.raw(",\"sql\":")?;
    database(out, &v.sql)?;
    out.raw(",\"allocation\":")?;
    allocation(out, v.allocation)?;
    out.raw(",")?;
    out.field("elapsed_ns", v.elapsed_ns)?;
    out.raw(",\"payload\":null,\"payload_status\":\"UNAVAILABLE-not-reported-by-CreationWork\",\"allocation_state\":")?;
    match &v.allocation_state {
        Some(Ok(value)) => allocation_state(out,*value)?,
        Some(Err(error)) => {out.raw("{\"status\":\"FAILED\",\"values\":null,")?;out.text("error",&format!("{error:?}"))?;out.raw("}")?;}
        None => out.raw("{\"status\":\"UNAVAILABLE\",\"values\":null,\"reason\":\"allocation-owner-not-established\"}")?,
    }
    out.raw("}}")
}
