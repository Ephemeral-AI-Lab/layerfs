//! Audited SQLite process bootstrap and native observation boundary.
//!
//! Inventory: config MEMSTATUS with its exact c_int vararg; initialize once;
//! hard_heap_limit64 query/set/readback; status64 with checked output pointers
//! and resetFlag0; one malloc64/msize/write/free probe; static provider identity
//! and thread-capability reads. No raw pointer escapes this module. No native
//! shutdown, allocator replacement, limit restore, retry or counter reset exists.
//! Global configuration requires the public entry's exclusive caller authority.

#![allow(unsafe_code)]

use std::ffi::{c_int, c_void, CStr};
use std::ptr::NonNull;
use std::sync::OnceLock;

use rusqlite::ffi as native;

use crate::error::{StorageError, StorageResult};

use super::{
    EngineBootstrapCustody, EngineBootstrapFailure, EngineBootstrapObservation,
    EngineBootstrapStage as Stage, EngineGuard, EngineObservation, EngineProfile, NativeMemory,
    ENGINE_HEAP_LIMIT_BYTES, ENGINE_PROBE_REQUEST_BYTES,
};

static BOOTSTRAP: OnceLock<Result<EngineGuard, EngineBootstrapFailure>> = OnceLock::new();

/// Private successful-bootstrap witness; sibling safe code cannot construct it.
pub(super) struct Established {
    _private: (),
}

/// Establish the supported native heap owner once at an exclusive process startup.
/// The same original success or failure is returned to every later borrower.
/// The first native call configures MEMSTATUS1; no negative limit query can
/// initialize SQLite before that configuration. A prior initialized provider is
/// a defined eligibility refusal, preserved without shutdown/reset/retry.
///
/// # Safety
///
/// The caller must own exclusive authority over this linked SQLite library and
/// its global configuration throughout this call: no concurrent SQLite API,
/// initialization, configuration or shutdown call is allowed. A wrapper mutex
/// cannot exclude foreign callers. Prior initialization is permitted only as
/// the defined refusal case; eligible native startup establishes this boundary
/// before any SQL, telemetry/session workers or automatic initializer. Arbitrary
/// preconfigured allocator/page-cache pools are not qualified by this guard.
pub unsafe fn bootstrap_exclusive() -> Result<&'static EngineGuard, &'static EngineBootstrapFailure>
{
    let result = BOOTSTRAP.get_or_init(|| {
        // SAFETY: the public caller owns the complete exclusion boundary above.
        // OnceLock caches this one attempt; it is not the exclusion proof.
        unsafe { bootstrap_once() }
    });
    result.as_ref()
}

fn native_error(result: c_int) -> StorageError {
    StorageError::Engine(rusqlite::Error::SqliteFailure(
        native::Error::new(result),
        None,
    ))
}

fn acknowledged(result: c_int) -> StorageResult<()> {
    if result == native::SQLITE_OK {
        Ok(())
    } else {
        Err(native_error(result))
    }
}

struct NativeProbe {
    pointer: NonNull<c_void>,
}

impl Drop for NativeProbe {
    fn drop(&mut self) {
        // SAFETY: this nonnull pointer came from the one successful malloc64
        // below. It is never exposed, copied into another owner or freed earlier.
        unsafe { native::sqlite3_free(self.pointer.as_ptr()) };
    }
}

fn counters(code: c_int) -> StorageResult<(u64, u64)> {
    let mut current = 0i64;
    let mut highwater = 0i64;
    // SAFETY: both output pointers refer to distinct live i64 stack values with
    // the ABI widths required by status64. The closed selector is supported and
    // resetFlag0 preserves lifetime counters. All callers have an initialized
    // established engine or the exclusive acknowledged startup initializer.
    acknowledged(unsafe { native::sqlite3_status64(code, &mut current, &mut highwater, 0) })?;
    if current < 0 || highwater < current {
        return Err(StorageError::Integrity("SQLite native status counters"));
    }
    Ok((current as u64, highwater as u64))
}

fn memory() -> StorageResult<NativeMemory> {
    let (used_bytes, lifetime_highwater_bytes) = counters(native::SQLITE_STATUS_MEMORY_USED)?;
    let (allocations, lifetime_highwater_allocations) =
        counters(native::SQLITE_STATUS_MALLOC_COUNT)?;
    Ok(NativeMemory {
        used_bytes,
        lifetime_highwater_bytes,
        allocations,
        lifetime_highwater_allocations,
    })
}

fn hard_limit() -> StorageResult<u64> {
    // SAFETY: every caller is after acknowledged initialization, never before
    // MEMSTATUS configuration. A negative argument only queries the current
    // limit here and does not restore, increase or reset it.
    let limit = unsafe { native::sqlite3_hard_heap_limit64(-1) };
    u64::try_from(limit).map_err(|_| StorageError::Integrity("SQLite hard limit query failed"))
}

pub(super) fn observe(_owner: &EngineGuard) -> StorageResult<EngineObservation> {
    Ok(EngineObservation {
        hard_heap_limit_bytes: hard_limit()?,
        memory: memory()?,
    })
}

unsafe fn bootstrap_once() -> Result<EngineGuard, EngineBootstrapFailure> {
    let mut stage = Stage::MemStatus;
    let mut custody = EngineBootstrapCustody::default();
    let result = (|| {
        // SAFETY: caller supplies exclusive global configuration authority.
        // MEMSTATUS requires exactly one C int after its selector, not a bool.
        acknowledged(unsafe {
            native::sqlite3_config(native::SQLITE_CONFIG_MEMSTATUS, 1 as c_int)
        })?;
        custody.memstatus_acknowledged = true;
        stage = Stage::Initialize;
        custody.initialize_attempted = true;
        // SAFETY: configuration was acknowledged and caller still excludes all
        // competing initializers/global calls. No reset or second attempt occurs.
        acknowledged(unsafe { native::sqlite3_initialize() })?;
        custody.initialized_by_bootstrap = true;
        stage = Stage::PreviousLimit;
        // SAFETY: the explicit initializer above already returned SQLITE_OK.
        let previous = unsafe { native::sqlite3_hard_heap_limit64(-1) };
        custody.previous_hard_heap_limit = Some(previous);
        if previous != 0 && previous != ENGINE_HEAP_LIMIT_BYTES as i64 {
            return Err(StorageError::UnsupportedPolicy {
                field: "SQLite existing hard heap limit",
            });
        }
        stage = Stage::SetLimit;
        custody.hard_limit_set_attempted = true;
        // SAFETY: exclusive initialized caller sets exactly the selected class
        // once. A foreign different prior limit is refused, never increased.
        let setter_previous =
            unsafe { native::sqlite3_hard_heap_limit64(ENGINE_HEAP_LIMIT_BYTES as i64) };
        custody.setter_previous_hard_heap_limit = Some(setter_previous);
        if setter_previous != previous {
            return Err(StorageError::Integrity(
                "SQLite hard limit setter prior mismatch",
            ));
        }
        stage = Stage::Readback;
        // SAFETY: acknowledged initialized engine, query only after configuration.
        let actual_limit = unsafe { native::sqlite3_hard_heap_limit64(-1) };
        custody.observed_hard_heap_limit = Some(actual_limit);
        if actual_limit != ENGINE_HEAP_LIMIT_BYTES as i64 {
            return Err(StorageError::UnsupportedPolicy {
                field: "SQLite required hard heap limit",
            });
        }
        custody.hard_limit_installed = true;
        stage = Stage::Baseline;
        let before_probe = memory()?;
        if before_probe.used_bytes > ENGINE_HEAP_LIMIT_BYTES {
            return Err(StorageError::CapacityExceeded {
                what: "SQLite startup native bytes",
                limit: ENGINE_HEAP_LIMIT_BYTES,
                actual: before_probe.used_bytes,
            });
        }
        stage = Stage::ProbeAllocate;
        // SAFETY: fixed bounded native request under the established hard limit.
        // Null is a definite refusal; it creates no pointer/free ownership.
        let pointer = NonNull::new(unsafe { native::sqlite3_malloc64(ENGINE_PROBE_REQUEST_BYTES) })
            .ok_or_else(|| native_error(native::SQLITE_NOMEM))?;
        custody.probe_issued = true;
        let probe = NativeProbe { pointer };
        let probe_result = (|| {
            stage = Stage::ProbeSize;
            // SAFETY: this one live nonnull native malloc pointer is owned above.
            let allocated = unsafe { native::sqlite3_msize(probe.pointer.as_ptr()) };
            custody.probe_allocated_bytes = Some(allocated);
            if !(ENGINE_PROBE_REQUEST_BYTES..=ENGINE_HEAP_LIMIT_BYTES).contains(&allocated) {
                return Err(StorageError::Integrity(
                    "SQLite native probe allocation size",
                ));
            }
            // SAFETY: msize established at least the requested writable extent;
            // memset touches only these 8192 owned bytes before their one free.
            unsafe {
                std::ptr::write_bytes(
                    probe.pointer.as_ptr().cast::<u8>(),
                    0,
                    ENGINE_PROBE_REQUEST_BYTES as usize,
                )
            };
            stage = Stage::DuringProbe;
            let during_probe = memory()?;
            let expected_bytes = before_probe
                .used_bytes
                .checked_add(allocated)
                .ok_or(StorageError::Integrity("SQLite native probe byte overflow"))?;
            let expected_allocations =
                before_probe
                    .allocations
                    .checked_add(1)
                    .ok_or(StorageError::Integrity(
                        "SQLite native probe count overflow",
                    ))?;
            if during_probe.used_bytes != expected_bytes
                || during_probe.allocations != expected_allocations
                || during_probe.used_bytes > ENGINE_HEAP_LIMIT_BYTES
            {
                return Err(StorageError::Integrity(
                    "SQLite native accounting inactive/inexact",
                ));
            }
            Ok((allocated, during_probe))
        })();
        drop(probe); // Known native ownership ends once, even when probe checks fail.
        custody.probe_free_attempted = true;
        let (allocated, during_probe) = probe_result?;
        stage = Stage::AfterProbe;
        let after_probe = memory()?;
        if after_probe.used_bytes != before_probe.used_bytes
            || after_probe.allocations != before_probe.allocations
        {
            return Err(StorageError::Integrity(
                "SQLite native probe release counters",
            ));
        }
        stage = Stage::ProviderIdentity;
        // SAFETY: the linked provider returns immutable process-lifetime C strings.
        // It is initialized and cannot be unloaded while this linked crate lives.
        let version = unsafe { native::sqlite3_libversion() };
        let source = unsafe { native::sqlite3_sourceid() };
        if version.is_null() || source.is_null() {
            return Err(StorageError::Integrity(
                "SQLite provider identity unavailable",
            ));
        }
        // SAFETY: the checked nonnull pointers have the native static-string contract.
        let provider_version = unsafe { CStr::from_ptr(version) }
            .to_str()
            .map_err(|_| StorageError::Integrity("SQLite provider version encoding"))?;
        let provider_source_id = unsafe { CStr::from_ptr(source) }
            .to_str()
            .map_err(|_| StorageError::Integrity("SQLite provider source encoding"))?;
        // SAFETY: a static compile-time capability read on this linked provider.
        if unsafe { native::sqlite3_threadsafe() } == 0 {
            return Err(StorageError::UnsupportedPolicy {
                field: "SQLite threaded native engine",
            });
        }
        let profile = EngineProfile {
            hard_heap_limit_bytes: ENGINE_HEAP_LIMIT_BYTES,
            probe_request_bytes: ENGINE_PROBE_REQUEST_BYTES,
            provider_version,
            provider_source_id,
        };
        let observation = EngineBootstrapObservation {
            previous_hard_heap_limit_bytes: previous as u64,
            before_probe,
            during_probe,
            after_probe,
            probe_allocated_bytes: allocated,
        };
        Ok(EngineGuard::new(
            Established { _private: () },
            profile,
            observation,
        ))
    })();
    result.map_err(|cause| EngineBootstrapFailure::new(stage, cause, custody))
}
