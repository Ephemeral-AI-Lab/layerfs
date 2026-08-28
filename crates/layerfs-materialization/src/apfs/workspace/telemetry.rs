use super::*;

pub(super) fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

pub(super) fn finish_call(call: &mut ProjectionCallFacts, elapsed: u64, success: bool) {
    call.attempts = call.attempts.saturating_add(1);
    if success {
        call.successes = call.successes.saturating_add(1);
    } else {
        call.failures = call.failures.saturating_add(1);
    }
    call.wall.nanoseconds = call.wall.nanoseconds.saturating_add(elapsed);
}

pub(super) fn observed_call<T>(
    facts: &Recorder,
    select: fn(&mut ProjectionFacts) -> &mut ProjectionCallFacts,
    operation: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let start = Instant::now();
    let result = operation();
    let elapsed = elapsed_ns(start);
    facts.update(|facts| finish_call(select(facts), elapsed, result.is_ok()));
    result
}

pub(super) fn finish_write(write: &mut ProjectionWriteFacts, elapsed: u64, written: Option<usize>) {
    write.attempts = write.attempts.saturating_add(1);
    write.wall.nanoseconds = write.wall.nanoseconds.saturating_add(elapsed);
    match written {
        Some(bytes) => {
            write.successes = write.successes.saturating_add(1);
            write.bytes = write.bytes.saturating_add(bytes as u64);
        }
        None => write.failures = write.failures.saturating_add(1),
    }
}

pub(super) fn increment_class(counts: &mut DurabilityClassCounts, class: DurabilityClass) {
    match class {
        DurabilityClass::ProcessCrashReconciled => {
            counts.process_crash_reconciled = counts.process_crash_reconciled.saturating_add(1)
        }
        DurabilityClass::HostCrashOrdered => {
            counts.host_crash_ordered = counts.host_crash_ordered.saturating_add(1)
        }
        DurabilityClass::DeviceFlushRequested => {
            counts.device_flush_requested = counts.device_flush_requested.saturating_add(1)
        }
        DurabilityClass::PowerLossQualified => {
            counts.power_loss_qualified = counts.power_loss_qualified.saturating_add(1)
        }
    }
}

pub(super) fn finish_sync(sync: &mut ProjectionSyncFacts, elapsed: u64, success: bool) {
    sync.attempts = sync.attempts.saturating_add(1);
    increment_class(&mut sync.requested, DurabilityClass::ProcessCrashReconciled);
    if success {
        sync.successes = sync.successes.saturating_add(1);
        increment_class(&mut sync.achieved, DurabilityClass::ProcessCrashReconciled);
    } else {
        sync.failures = sync.failures.saturating_add(1);
    }
    sync.wall.nanoseconds = sync.wall.nanoseconds.saturating_add(elapsed);
}

pub(super) fn sync_file(file: &File, facts: &Recorder, owner: FileSyncOwner) -> Result<()> {
    let start = Instant::now();
    let result = file.sync_all();
    let elapsed = elapsed_ns(start);
    facts.update(|facts| {
        finish_sync(&mut facts.regular_file_sync, elapsed, result.is_ok());
        let owner = match owner {
            FileSyncOwner::RecoveryMarker => &mut facts.recovery_marker_file_sync,
            FileSyncOwner::ContentTemp => &mut facts.content_temp_file_sync,
            FileSyncOwner::PostHardLink => &mut facts.post_hardlink_file_sync,
        };
        finish_sync(owner, elapsed, result.is_ok());
    });
    result.map_err(Into::into)
}

pub(super) fn sync_directory_file_io(
    file: &File,
    facts: &Recorder,
    owner: DirectorySyncOwner,
) -> std::io::Result<()> {
    let start = Instant::now();
    let result = file.sync_all();
    let elapsed = elapsed_ns(start);
    facts.update(|facts| {
        finish_sync(&mut facts.directory_sync, elapsed, result.is_ok());
        let owner = match owner {
            DirectorySyncOwner::Staging => &mut facts.staging_directory_sync,
            DirectorySyncOwner::RootParent => &mut facts.root_parent_directory_sync,
            DirectorySyncOwner::InstallParent => &mut facts.install_parent_directory_sync,
            DirectorySyncOwner::DirtyTree => &mut facts.dirty_tree_directory_sync,
            DirectorySyncOwner::FinalRoot => &mut facts.final_root_directory_sync,
        };
        finish_sync(owner, elapsed, result.is_ok());
    });
    result
}

pub(super) fn sync_directory_file(
    file: &File,
    facts: &Recorder,
    owner: DirectorySyncOwner,
) -> Result<()> {
    sync_directory_file_io(file, facts, owner).map_err(Into::into)
}

pub(super) fn metadata_value_bytes(metadata: &NativeMetadata) -> u64 {
    metadata.xattrs.payload_bytes() as u64 + metadata.acl.as_ref().map_or(0, |acl| acl.len() as u64)
}

pub(super) fn write_metadata_values(
    file: &File,
    metadata: &NativeMetadata,
    facts: &Recorder,
) -> Result<()> {
    let start = Instant::now();
    let result = super::metadata::write(file, metadata);
    let elapsed = elapsed_ns(start);
    let bytes = metadata_value_bytes(metadata);
    facts.update(|facts| {
        let written = result
            .is_ok()
            .then_some(usize::try_from(bytes).unwrap_or(usize::MAX));
        finish_write(&mut facts.metadata_value_write, elapsed, written);
        finish_write(&mut facts.aggregate_native_write, elapsed, written);
    });
    result
}

pub(super) fn metadata_apply_step(
    elapsed: &mut u64,
    operation: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let start = Instant::now();
    let result = operation();
    *elapsed = elapsed.saturating_add(elapsed_ns(start));
    result
}

pub(super) fn finish_cleanup(facts: &Recorder, start: Instant, success: bool) {
    let elapsed = elapsed_ns(start);
    facts.update(|facts| {
        facts.cleanup.attempts = facts.cleanup.attempts.saturating_add(1);
        if success {
            facts.cleanup.successes = facts.cleanup.successes.saturating_add(1);
        } else {
            facts.cleanup.failures = facts.cleanup.failures.saturating_add(1);
            facts.cleanup.residue = facts.cleanup.residue.saturating_add(1);
        }
        facts.cleanup.wall.nanoseconds = facts.cleanup.wall.nanoseconds.saturating_add(elapsed);
    });
}

pub(super) fn finish_replace(
    facts: &Recorder,
    start: Instant,
    prior_existed: bool,
    result: &Result<()>,
) {
    let elapsed = elapsed_ns(start);
    facts.update(|facts| {
        facts.replace.attempts = facts.replace.attempts.saturating_add(1);
        facts.replace.wall.nanoseconds = facts.replace.wall.nanoseconds.saturating_add(elapsed);
        if prior_existed {
            facts.replace.prior_visible = facts.replace.prior_visible.saturating_add(1);
        }
        match result {
            Ok(()) => {
                facts.replace.successes = facts.replace.successes.saturating_add(1);
                facts.replace.requested_visible = facts.replace.requested_visible.saturating_add(1);
            }
            Err(DriverError::DurabilityAmbiguous) => {
                facts.replace.failures = facts.replace.failures.saturating_add(1);
                facts.replace.requested_visible = facts.replace.requested_visible.saturating_add(1);
                facts.replace.durability_ambiguous =
                    facts.replace.durability_ambiguous.saturating_add(1);
            }
            Err(DriverError::VisibilityAmbiguous) => {
                facts.replace.failures = facts.replace.failures.saturating_add(1);
                facts.replace.visibility_ambiguous =
                    facts.replace.visibility_ambiguous.saturating_add(1);
            }
            Err(_) => facts.replace.failures = facts.replace.failures.saturating_add(1),
        }
    });
}

pub(super) fn record_replace_durability_ambiguity<T>(facts: &Recorder, result: &Result<T>) {
    if matches!(result, Err(DriverError::DurabilityAmbiguous)) {
        facts.update(|facts| {
            facts.replace.durability_ambiguous =
                facts.replace.durability_ambiguous.saturating_add(1)
        });
    }
}
