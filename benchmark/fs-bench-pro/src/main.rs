use layerfs_sdk::{
    BranchId, CandidateStats, Client, CommitId, ContainerId, CreateWorkspaceSession,
    EndWorkspaceMode, EntityName, ExecutionTransport, FuseWriteReceipt, LayerStackInitialization,
    LayerStackStore, LocalForkSource, NonEmpty, OperationFamily, OutputPage, Query, QueryItem,
    QueryKind, StorageReceipt, WorkspaceCommitResult, WorkspaceFileRangeEdit,
    WorkspaceFileReplacement, WorkspaceId, WorkspacePlacement, WorkspaceProjection,
};
use std::ffi::OsString;
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

type AnyResult<T> = Result<T, Box<dyn std::error::Error>>;

#[derive(Clone, Copy)]
struct ProcessResourceSnapshot {
    user_cpu_ns: u64,
    system_cpu_ns: u64,
    resident_bytes: u64,
    peak_resident_bytes: u64,
    physical_footprint_bytes: u64,
    disk_read_bytes: u64,
    disk_write_bytes: u64,
    context_switches: u64,
    swaps: u64,
    threads: u64,
}

#[derive(Clone, Copy)]
struct SqliteResourceSnapshot {
    memory_used_bytes: u64,
    memory_peak_bytes: u64,
    page_cache_overflow_bytes: u64,
    page_cache_overflow_peak_bytes: u64,
    allocation_count: u64,
    allocation_peak_count: u64,
    connection_cache_used_bytes: u64,
    connection_cache_target_bytes: u64,
}

#[derive(Clone, Copy)]
struct ContainerCgroupSnapshot {
    memory_current: u64,
    memory_peak: u64,
    swap_current: u64,
    pids_current: u64,
    oom: u64,
    oom_kill: u64,
}

fn container_cgroup_snapshot(container: &ContainerId) -> AnyResult<ContainerCgroupSnapshot> {
    let output = Command::new("docker")
        .args([
            "exec",
            container.0.as_str(),
            "sh",
            "-c",
            r#"printf "memory_current=%s\n" "$(cat /sys/fs/cgroup/memory.current)"
printf "memory_peak=%s\n" "$(cat /sys/fs/cgroup/memory.peak)"
printf "swap_current=%s\n" "$(cat /sys/fs/cgroup/memory.swap.current)"
printf "pids_current=%s\n" "$(cat /sys/fs/cgroup/pids.current)"
grep '^oom ' /sys/fs/cgroup/memory.events | tr ' ' '='
grep '^oom_kill ' /sys/fs/cgroup/memory.events | tr ' ' '='"#,
        ])
        .output()?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err("container cgroup snapshot failed".into());
    }
    let text = std::str::from_utf8(&output.stdout)?;
    let value = |name: &str| -> AnyResult<u64> {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .ok_or_else(|| format!("missing cgroup field: {name}").into())
            .and_then(|value| value.parse().map_err(Into::into))
    };
    Ok(ContainerCgroupSnapshot {
        memory_current: value("memory_current=")?,
        memory_peak: value("memory_peak=")?,
        swap_current: value("swap_current=")?,
        pids_current: value("pids_current=")?,
        oom: value("oom=")?,
        oom_kill: value("oom_kill=")?,
    })
}

fn sqlite_status(operation: i32, reset_peak: bool) -> AnyResult<(u64, u64)> {
    let mut current = 0_i64;
    let mut peak = 0_i64;
    // SAFETY: SQLite initializes both i64 outputs and the operation is process-global telemetry.
    let status = unsafe {
        rusqlite::ffi::sqlite3_status64(
            operation,
            std::ptr::from_mut(&mut current),
            std::ptr::from_mut(&mut peak),
            i32::from(reset_peak),
        )
    };
    if status != rusqlite::ffi::SQLITE_OK {
        return Err(format!("SQLite status failed: {operation} ({status})").into());
    }
    Ok((u64::try_from(current)?, u64::try_from(peak)?))
}

fn sqlite_resource_snapshot(
    store: &LayerStackStore,
    reset_peaks: bool,
) -> AnyResult<SqliteResourceSnapshot> {
    let (memory_used_bytes, memory_peak_bytes) =
        sqlite_status(rusqlite::ffi::SQLITE_STATUS_MEMORY_USED, reset_peaks)?;
    let (page_cache_overflow_bytes, page_cache_overflow_peak_bytes) =
        sqlite_status(rusqlite::ffi::SQLITE_STATUS_PAGECACHE_OVERFLOW, reset_peaks)?;
    let (allocation_count, allocation_peak_count) =
        sqlite_status(rusqlite::ffi::SQLITE_STATUS_MALLOC_COUNT, reset_peaks)?;
    let (connection_cache_used_bytes, connection_cache_target_bytes) =
        store.inspect_connection(|connection| -> AnyResult<(u64, u64)> {
            let mut current = 0_i32;
            let mut unused_peak = 0_i32;
            // SAFETY: the Store keeps the locked connection alive for this read-only status call.
            let status = unsafe {
                rusqlite::ffi::sqlite3_db_status(
                    connection.handle(),
                    rusqlite::ffi::SQLITE_DBSTATUS_CACHE_USED,
                    std::ptr::from_mut(&mut current),
                    std::ptr::from_mut(&mut unused_peak),
                    0,
                )
            };
            if status != rusqlite::ffi::SQLITE_OK {
                return Err(format!("SQLite connection cache status failed: {status}").into());
            }
            let cache_size: i64 =
                connection.pragma_query_value(None, "cache_size", |row| row.get(0))?;
            let page_size: i64 =
                connection.pragma_query_value(None, "page_size", |row| row.get(0))?;
            let target = if cache_size < 0 {
                cache_size
                    .checked_neg()
                    .and_then(|value| value.checked_mul(1024))
            } else {
                cache_size.checked_mul(page_size)
            }
            .ok_or("SQLite cache target overflow")?;
            Ok((u64::try_from(current)?, u64::try_from(target)?))
        })??;
    Ok(SqliteResourceSnapshot {
        memory_used_bytes,
        memory_peak_bytes,
        page_cache_overflow_bytes,
        page_cache_overflow_peak_bytes,
        allocation_count,
        allocation_peak_count,
        connection_cache_used_bytes,
        connection_cache_target_bytes,
    })
}

fn phase_peak_status(
    before: ProcessResourceSnapshot,
    after: ProcessResourceSnapshot,
) -> &'static str {
    if after.peak_resident_bytes > before.peak_resident_bytes {
        "exact-new-lifetime-high-water"
    } else {
        "unavailable-cumulative-high-water"
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[repr(C)]
#[derive(Default)]
struct NativeTimeval {
    seconds: i64,
    microseconds: i64,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
#[repr(C)]
#[derive(Default)]
struct NativeRusage {
    user_time: NativeTimeval,
    system_time: NativeTimeval,
    max_rss: i64,
    shared_memory: i64,
    unshared_data: i64,
    unshared_stack: i64,
    minor_faults: i64,
    major_faults: i64,
    swaps: i64,
    block_inputs: i64,
    block_outputs: i64,
    messages_sent: i64,
    messages_received: i64,
    signals: i64,
    voluntary_context_switches: i64,
    involuntary_context_switches: i64,
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
unsafe extern "C" {
    fn getrusage(who: i32, usage: *mut NativeRusage) -> i32;
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn native_peak_rss_and_swaps() -> AnyResult<(u64, u64)> {
    let mut usage = NativeRusage::default();
    // SAFETY: RUSAGE_SELF writes exactly one native rusage C-layout structure.
    if unsafe { getrusage(0, std::ptr::from_mut(&mut usage)) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let peak = u64::try_from(usage.max_rss)?;
    #[cfg(target_os = "linux")]
    let peak = peak.checked_mul(1024).ok_or("Linux peak RSS overflow")?;
    Ok((peak, u64::try_from(usage.swaps)?))
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Default)]
struct DarwinRusageInfoV2 {
    uuid: [u8; 16],
    user_time: u64,
    system_time: u64,
    package_idle_wakeups: u64,
    interrupt_wakeups: u64,
    pageins: u64,
    wired_size: u64,
    resident_size: u64,
    physical_footprint: u64,
    process_start_time: u64,
    process_exit_time: u64,
    child_user_time: u64,
    child_system_time: u64,
    child_package_idle_wakeups: u64,
    child_interrupt_wakeups: u64,
    child_pageins: u64,
    child_elapsed_time: u64,
    disk_read_bytes: u64,
    disk_write_bytes: u64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Default)]
struct DarwinTaskInfo {
    virtual_size: u64,
    resident_size: u64,
    total_user: u64,
    total_system: u64,
    threads_user: u64,
    threads_system: u64,
    policy: i32,
    faults: i32,
    pageins: i32,
    cow_faults: i32,
    messages_sent: i32,
    messages_received: i32,
    mach_syscalls: i32,
    unix_syscalls: i32,
    context_switches: i32,
    thread_count: i32,
    running_threads: i32,
    priority: i32,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Default)]
struct MachTimebaseInfo {
    numerator: u32,
    denominator: u32,
}

#[cfg(target_os = "macos")]
#[link(name = "proc")]
unsafe extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut std::ffi::c_void) -> i32;
    fn proc_pidinfo(
        pid: i32,
        flavor: i32,
        argument: u64,
        buffer: *mut std::ffi::c_void,
        size: i32,
    ) -> i32;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> i32;
}

#[cfg(target_os = "macos")]
fn process_resource_snapshot() -> AnyResult<ProcessResourceSnapshot> {
    const RUSAGE_INFO_V2: i32 = 2;
    const PROC_PIDTASKINFO: i32 = 4;
    let pid = i32::try_from(std::process::id())?;
    let mut usage = DarwinRusageInfoV2::default();
    let mut task = DarwinTaskInfo::default();
    let mut timebase = MachTimebaseInfo::default();
    // SAFETY: both calls target this process and receive correctly sized C-layout buffers.
    let usage_status =
        unsafe { proc_pid_rusage(pid, RUSAGE_INFO_V2, std::ptr::from_mut(&mut usage).cast()) };
    // SAFETY: PROC_PIDTASKINFO writes exactly one proc_taskinfo-compatible buffer.
    let task_bytes = unsafe {
        proc_pidinfo(
            pid,
            PROC_PIDTASKINFO,
            0,
            std::ptr::from_mut(&mut task).cast(),
            i32::try_from(std::mem::size_of::<DarwinTaskInfo>())?,
        )
    };
    // SAFETY: mach_timebase_info initializes the two-field C-layout structure.
    let timebase_status = unsafe { mach_timebase_info(std::ptr::from_mut(&mut timebase)) };
    if usage_status != 0
        || usize::try_from(task_bytes)? != std::mem::size_of::<DarwinTaskInfo>()
        || timebase_status != 0
        || timebase.denominator == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    let to_nanoseconds = |value: u64| -> AnyResult<u64> {
        u64::try_from(
            u128::from(value)
                .checked_mul(u128::from(timebase.numerator))
                .ok_or("Mach CPU time overflow")?
                / u128::from(timebase.denominator),
        )
        .map_err(Into::into)
    };
    let (peak_resident_bytes, swaps) = native_peak_rss_and_swaps()?;
    Ok(ProcessResourceSnapshot {
        user_cpu_ns: to_nanoseconds(usage.user_time)?,
        system_cpu_ns: to_nanoseconds(usage.system_time)?,
        resident_bytes: usage.resident_size,
        peak_resident_bytes,
        physical_footprint_bytes: usage.physical_footprint,
        disk_read_bytes: usage.disk_read_bytes,
        disk_write_bytes: usage.disk_write_bytes,
        context_switches: u64::try_from(task.context_switches)?,
        swaps,
        threads: u64::try_from(task.thread_count)?,
    })
}

#[cfg(target_os = "linux")]
fn process_resource_snapshot() -> AnyResult<ProcessResourceSnapshot> {
    static CLOCK_TICKS: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let clock_ticks = *CLOCK_TICKS.get_or_init(|| {
        Command::new("getconf")
            .arg("CLK_TCK")
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|output| output.trim().parse().ok())
            .unwrap_or(0)
    });
    if clock_ticks == 0 {
        return Err("Linux clock tick rate unavailable".into());
    }
    let status = std::fs::read_to_string("/proc/self/status")?;
    let status_value = |name: &str| -> AnyResult<u64> {
        status
            .lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|value| value.split_whitespace().next())
            .ok_or_else(|| format!("Linux process status field unavailable: {name}").into())
            .and_then(|value| value.parse().map_err(Into::into))
    };
    let io = std::fs::read_to_string("/proc/self/io")?;
    let io_value = |name: &str| -> AnyResult<u64> {
        io.lines()
            .find_map(|line| line.strip_prefix(name))
            .map(str::trim)
            .ok_or_else(|| format!("Linux process I/O field unavailable: {name}").into())
            .and_then(|value| value.parse().map_err(Into::into))
    };
    let stat = std::fs::read_to_string("/proc/self/stat")?;
    let fields = stat
        .get(stat.rfind(')').ok_or("Linux process stat shape")? + 2..)
        .ok_or("Linux process stat fields")?
        .split_whitespace()
        .collect::<Vec<_>>();
    let ticks_to_ns = |index: usize| -> AnyResult<u64> {
        let ticks = fields
            .get(index)
            .ok_or("Linux process CPU field")?
            .parse::<u64>()?;
        u64::try_from(u128::from(ticks) * 1_000_000_000 / u128::from(clock_ticks))
            .map_err(Into::into)
    };
    let resident_bytes = status_value("VmRSS:")?
        .checked_mul(1024)
        .ok_or("Linux resident bytes overflow")?;
    let (peak_resident_bytes, swaps) = native_peak_rss_and_swaps()?;
    Ok(ProcessResourceSnapshot {
        user_cpu_ns: ticks_to_ns(11)?,
        system_cpu_ns: ticks_to_ns(12)?,
        resident_bytes,
        peak_resident_bytes,
        physical_footprint_bytes: resident_bytes,
        disk_read_bytes: io_value("read_bytes:")?,
        disk_write_bytes: io_value("write_bytes:")?,
        context_switches: status_value("voluntary_ctxt_switches:")?
            .checked_add(status_value("nonvoluntary_ctxt_switches:")?)
            .ok_or("Linux context switches overflow")?,
        swaps,
        threads: status_value("Threads:")?,
    })
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process_resource_snapshot() -> AnyResult<ProcessResourceSnapshot> {
    Err("process resource snapshots are unsupported on this host".into())
}

fn resource_delta(after: u64, before: u64, name: &'static str) -> AnyResult<u64> {
    after.checked_sub(before).ok_or_else(|| name.into())
}

const MIB_32: u64 = 32 * 1024 * 1024;
const WORKSPACE_CREATE_HARD_NS: u64 = 20_000_000;
const SMALL_COMMIT_HARD_NS: u64 = 6_000_000;
const SMALL_COMPLETE_HARD_NS: u64 = 30_000_000;
const COLD_COMPLETE_HARD_NS: u64 = 150_000_000;
const EDIT16_HARD_NS: u64 = 200_000_000;
const PREPEND_HARD_NS: u64 = 250_000_000;
const NAMESPACE_100000_BINDING_INIT_NS: u64 = 3_235_294_118;
const NAMESPACE_100000_BINDING_BYTES_PER_SECOND: u64 = 153_000_000;
const NAMESPACE_100000_BINDING_FILES_PER_SECOND: u64 = 30_600;
const READ_HARD_NS: u64 = 150_000_000;
const REGISTERED_TOTAL_HARD_NS: u64 = 700_000_000;
const INNER_WRITE_MIN_BYTES_PER_SECOND: f64 = 300.0 * 1024.0 * 1024.0;
#[allow(dead_code)]
mod workload_source {
    include!("../workload.rs");
}

type NamespaceScenario = workload_source::NamespaceScenario;

#[derive(Clone, Debug, Eq, PartialEq)]
struct NamespaceManifest {
    regular_files: u64,
    data_directories: u64,
    logical_bytes: u64,
    empty_files: u64,
    tiny_files: u64,
    small_files: u64,
    medium_files: u64,
    anchor_files: u64,
    anchor_bytes: u64,
    file_mode: u32,
    directory_mode: u32,
    mtime_seconds: i64,
    mtime_nanoseconds: u32,
    digest: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NamespaceVerification {
    manifest: NamespaceManifest,
    maximum_verifier_buffer_bytes: u64,
    verifier_worker_count: u64,
    verifier_plan_bytes: u64,
    verifier_path_state_peak_bytes: u64,
    verifier_digest_state_peak_bytes: u64,
}

#[derive(Clone)]
struct FragmentCheckpoint {
    cohort: &'static str,
    operations: u64,
    piece_count: u64,
    piece_height: u64,
    piece_charge: u64,
    tree_visits: u64,
    digest: String,
    root: String,
}

#[derive(Clone, Copy)]
struct NamespaceReadMetrics {
    maximum_product_read_ahead_bytes: u64,
    read_ahead_hits: u64,
    read_ahead_misses: u64,
    read_ahead_fetches: u64,
    read_ahead_requested_bytes: u64,
    read_ahead_fetched_bytes: u64,
    read_ahead_served_bytes: u64,
    read_ahead_unused_bytes: u64,
    local_calls: u64,
    local_ids: u64,
    local_rows: u64,
    local_bytes: u64,
}

struct GeneratedNamespaceFixture {
    manifest: NamespaceManifest,
    edited_digest: String,
    edit_path: String,
    edit_size: u64,
    fixture_plan_ns: u64,
    fixture_generate_ns: u64,
    fixture_manifest_ns: u64,
    maximum_fixture_write_buffer_bytes: u64,
    fixture_write_calls: u64,
    fixture_open_calls: u64,
    fixture_content_bytes_generated: u64,
    fixture_content_bytes_written: u64,
    fixture_content_hash_input_bytes: u64,
    fixture_plan_bytes: u64,
    fixture_path_state_bytes: u64,
    fixture_digest_record_bytes: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct NamespaceSample {
    layerstack_init_ns: u64,
    branch_fork_ns: u64,
    workspace_create_ns: u64,
    edit_ns: u64,
    commit_ns: u64,
    workspace_end_ns: u64,
    reconnect_ns: u64,
    reopen_workspace_create_ns: u64,
    reopen_content_verify_ns: u64,
    reopen_verify_ns: u64,
    reopen_workspace_end_ns: u64,
    complete_product_ns: u64,
    product_lifecycle_ns: u64,
}

impl NamespaceSample {
    fn validate(&self) -> AnyResult<()> {
        let reopen = [
            self.reconnect_ns,
            self.reopen_workspace_create_ns,
            self.reopen_content_verify_ns,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
        .ok_or("namespace reopen phase overflow")?;
        let phases = [
            self.layerstack_init_ns,
            self.branch_fork_ns,
            self.workspace_create_ns,
            self.edit_ns,
            self.commit_ns,
            self.workspace_end_ns,
            self.reopen_verify_ns,
        ]
        .into_iter()
        .try_fold(0_u64, u64::checked_add)
        .ok_or("namespace phase overflow")?;
        if reopen != self.reopen_verify_ns
            || phases != self.complete_product_ns
            || phases != self.product_lifecycle_ns
            || self.reconnect_ns == 0
            || self.reopen_workspace_create_ns == 0
            || self.reopen_verify_ns == 0
            || self.reopen_workspace_end_ns == 0
        {
            return Err("namespace phase equation".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
struct LifecycleSample {
    workspace_create_ns: u64,
    execution_ns: u64,
    commit_api_ns: u64,
    layerstack_visible_ns: u64,
    workspace_end_ns: u64,
    complete_lifecycle_ns: u64,
    inner_write_ns: Option<u64>,
}

struct LifecycleRun {
    sample: LifecycleSample,
    output: OutputPage,
    head: Option<CommitId>,
}

struct ProofCase {
    store: PathBuf,
    branch: BranchId,
    head: Option<CommitId>,
    placement: WorkspacePlacement,
    expected: ProofExpected,
}

#[derive(Clone, Copy)]
enum ProofExpected {
    Fixture,
    Prepend,
}

impl LifecycleSample {
    fn validate(&self) -> AnyResult<()> {
        if self.layerstack_visible_ns
            != self
                .workspace_create_ns
                .saturating_add(self.execution_ns)
                .saturating_add(self.commit_api_ns)
            || self.complete_lifecycle_ns
                != self
                    .layerstack_visible_ns
                    .saturating_add(self.workspace_end_ns)
        {
            return Err("lifecycle phase equation".into());
        }
        Ok(())
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("fs-benchmark-pro: {error}");
        std::process::exit(1);
    }
}

fn run() -> AnyResult<()> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [command] if command == "self-check" => self_check(),
        [command, fixture, scenario] if command == "namespace-fixture" => {
            let scenario = namespace_scenario(&scenario.to_string_lossy())?;
            let manifest = create_namespace_fixture(Path::new(fixture), scenario)?;
            emit_namespace_manifest(scenario, &manifest);
            Ok(())
        }
        [command, fixture] if command == "same-count-fixture" => {
            if Path::new(fixture).exists() {
                return Err("same-count fixture already exists".into());
            }
            std::fs::create_dir(fixture)?;
            std::fs::write(
                Path::new(fixture).join("payload.bin"),
                workload_source::edit_same_count::fixture_bytes(),
            )?;
            println!(
                "fixture_bytes={}",
                workload_source::edit_same_count::FIXTURE_BYTES
            );
            Ok(())
        }
        [command, fixture] if command == "count-changing-fixture" => {
            if Path::new(fixture).exists() {
                return Err("count-changing fixture already exists".into());
            }
            std::fs::create_dir(fixture)?;
            std::fs::write(
                Path::new(fixture).join("payload.bin"),
                workload_source::edit_count_changing::fixture_bytes(),
            )?;
            println!(
                "fixture_bytes={}",
                workload_source::edit_count_changing::FIXTURE_BYTES
            );
            Ok(())
        }
        [
            command,
            root,
            fixture,
            container,
            scenario,
            iteration,
            fixture_digest,
            edited_digest,
            edit_path,
            edit_size,
            fixture_cache_profile,
        ] if command == "namespace" => {
            namespace_case(
                Path::new(root),
                Path::new(fixture),
                ContainerId(container.to_string_lossy().into_owned()),
                namespace_scenario(&scenario.to_string_lossy())?,
                iteration.to_string_lossy().parse()?,
                &fixture_digest.to_string_lossy(),
                &edited_digest.to_string_lossy(),
                &edit_path.to_string_lossy(),
                edit_size.to_string_lossy().parse()?,
                &fixture_cache_profile.to_string_lossy(),
            )
        }
        [
            command,
            root,
            fixture,
            container,
            scenario,
            seed,
            source,
            fixture_digest,
            edited_digest,
            edit_path,
            edit_size,
            fixture_cache_profile,
        ] if command == "namespace-performance" => namespace_performance_case(
            Path::new(root),
            Path::new(fixture),
            ContainerId(container.to_string_lossy().into_owned()),
            namespace_scenario(&scenario.to_string_lossy())?,
            seed.to_string_lossy().parse()?,
            &source.to_string_lossy(),
            &fixture_digest.to_string_lossy(),
            &edited_digest.to_string_lossy(),
            &edit_path.to_string_lossy(),
            edit_size.to_string_lossy().parse()?,
            &fixture_cache_profile.to_string_lossy(),
        ),
        [
            command,
            root,
            fixture,
            container,
            scenario,
            seed,
            source,
            fixture_digest,
            edited_digest,
            edit_path,
            edit_size,
            fixture_cache_profile,
        ] if command == "namespace-verify-case" => namespace_verify_case(
            Path::new(root),
            Path::new(fixture),
            ContainerId(container.to_string_lossy().into_owned()),
            namespace_scenario(&scenario.to_string_lossy())?,
            seed.to_string_lossy().parse()?,
            &source.to_string_lossy(),
            &fixture_digest.to_string_lossy(),
            &edited_digest.to_string_lossy(),
            &edit_path.to_string_lossy(),
            edit_size.to_string_lossy().parse()?,
            &fixture_cache_profile.to_string_lossy(),
        ),
        [
            command,
            root,
            fixture,
            scenario,
            iteration,
            fixture_digest,
            fixture_cache_profile,
        ] if command == "namespace-init-diagnostic" => namespace_init_diagnostic(
            Path::new(root),
            Path::new(fixture),
            namespace_scenario(&scenario.to_string_lossy())?,
            iteration.to_string_lossy().parse()?,
            &fixture_digest.to_string_lossy(),
            &fixture_cache_profile.to_string_lossy(),
        ),
        [command, root, fixture, container, scenario, seed, source, fixture_cache_profile]
            if command == "same-count-performance" =>
        {
            same_count_performance_case(
                Path::new(root),
                Path::new(fixture),
                ContainerId(container.to_string_lossy().into_owned()),
                &scenario.to_string_lossy(),
                seed.to_string_lossy().parse()?,
                &source.to_string_lossy(),
                &fixture_cache_profile.to_string_lossy(),
            )
        }
        [command, root, fixture, container, scenario, seed, source, fixture_cache_profile]
            if command == "count-changing-performance" =>
        {
            count_changing_performance_case(
                Path::new(root),
                Path::new(fixture),
                ContainerId(container.to_string_lossy().into_owned()),
                &scenario.to_string_lossy(),
                seed.to_string_lossy().parse()?,
                &source.to_string_lossy(),
                &fixture_cache_profile.to_string_lossy(),
            )
        }
        [
            command,
            root,
            fixture,
            container,
            scenario,
            seed,
            source,
            expected_digest,
            fixture_cache_profile,
        ] if command == "same-count-verify" => same_count_verify_case(
            Path::new(root),
            Path::new(fixture),
            ContainerId(container.to_string_lossy().into_owned()),
            &scenario.to_string_lossy(),
            seed.to_string_lossy().parse()?,
            &source.to_string_lossy(),
            &expected_digest.to_string_lossy(),
            &fixture_cache_profile.to_string_lossy(),
        ),
        [
            command,
            root,
            fixture,
            container,
            scenario,
            seed,
            source,
            expected_digest,
            expected_size,
            fixture_cache_profile,
        ] if command == "count-changing-verify" => count_changing_verify_case(
            Path::new(root),
            Path::new(fixture),
            ContainerId(container.to_string_lossy().into_owned()),
            &scenario.to_string_lossy(),
            seed.to_string_lossy().parse()?,
            &source.to_string_lossy(),
            &expected_digest.to_string_lossy(),
            expected_size.to_string_lossy().parse()?,
            &fixture_cache_profile.to_string_lossy(),
            false,
        ),
        [command, root, fixture, container, verifier, source, expected_digest, expected_size]
            if command == "count-changing-structural-verify" =>
        {
            count_changing_verify_case(
                Path::new(root),
                Path::new(fixture),
                ContainerId(container.to_string_lossy().into_owned()),
                &verifier.to_string_lossy(),
                1,
                &source.to_string_lossy(),
                &expected_digest.to_string_lossy(),
                expected_size.to_string_lossy().parse()?,
                "reused-verifier-uncontrolled",
                true,
            )
        }
        [
            command,
            root,
            fixture,
            container,
            control,
            seed,
            source,
            expected_files,
            expected_logical_bytes,
            edit_path,
            edit_size,
            fixture_digest,
            edited_digest,
        ] if command == "store-footprint-performance" || command == "store-footprint-verify" => {
            store_footprint_case(
                Path::new(root),
                Path::new(fixture),
                ContainerId(container.to_string_lossy().into_owned()),
                &control.to_string_lossy(),
                seed.to_string_lossy().parse()?,
                &source.to_string_lossy(),
                expected_files.to_string_lossy().parse()?,
                expected_logical_bytes.to_string_lossy().parse()?,
                &edit_path.to_string_lossy(),
                edit_size.to_string_lossy().parse()?,
                &fixture_digest.to_string_lossy(),
                &edited_digest.to_string_lossy(),
                command == "store-footprint-verify",
            )
        }
        [command, root, fixture, oracle, container, source, seed]
            if command == "same-count-fragmentation-verify" =>
        {
            same_count_fragmentation_verify(
                Path::new(root),
                Path::new(fixture),
                Path::new(oracle),
                ContainerId(container.to_string_lossy().into_owned()),
                &source.to_string_lossy(),
                seed.to_string_lossy().parse()?,
            )
        }
        [command, root, fixture] if command == "run" => {
            campaign(Path::new(root), Path::new(fixture), None, 3)
        }
        [command, root, fixture, container] if command == "run" => campaign(
            Path::new(root),
            Path::new(fixture),
            Some(ContainerId(container.to_string_lossy().into_owned())),
            3,
        ),
        [command, root, fixture, container, iterations] if command == "run" => campaign(
            Path::new(root),
            Path::new(fixture),
            Some(ContainerId(container.to_string_lossy().into_owned())),
            iterations.to_string_lossy().parse()?,
        ),
        [command, root, fixture, container, case] if command == "workspace-range" => {
            workspace_range_acceptance(
                Path::new(root),
                Path::new(fixture),
                ContainerId(container.to_string_lossy().into_owned()),
                &case.to_string_lossy(),
            )
        }
        _ => Err("usage: fs-benchmark-pro self-check | namespace-fixture FIXTURE SCENARIO | namespace ROOT FIXTURE CONTAINER SCENARIO ITERATION FIXTURE_DIGEST EDITED_DIGEST EDIT_PATH EDIT_SIZE FIXTURE_CACHE_PROFILE | namespace-performance|namespace-verify-case ROOT FIXTURE CONTAINER SCENARIO SEED SOURCE FIXTURE_DIGEST EDITED_DIGEST EDIT_PATH EDIT_SIZE FIXTURE_CACHE_PROFILE | namespace-init-diagnostic ROOT FIXTURE SCENARIO ITERATION FIXTURE_DIGEST FIXTURE_CACHE_PROFILE | run ROOT FIXTURE [CONTAINER ITERATIONS]".into()),
    }
}

fn self_check() -> AnyResult<()> {
    LifecycleSample {
        workspace_create_ns: 1,
        execution_ns: 2,
        commit_api_ns: 3,
        layerstack_visible_ns: 6,
        workspace_end_ns: 4,
        complete_lifecycle_ns: 10,
        inner_write_ns: Some(1),
    }
    .validate()?;
    namespace_self_check()?;
    workload_source::edit_same_count::self_check()?;
    workload_source::edit_count_changing::self_check()?;
    workload_source::store_footprint::self_check()?;
    println!("PASS fs-bench-pro one-Store lifecycle equations");
    Ok(())
}

fn namespace_scenario(id: &str) -> AnyResult<NamespaceScenario> {
    workload_source::namespace_scenario(id)
}

fn create_namespace_fixture(
    root: &Path,
    scenario: NamespaceScenario,
) -> AnyResult<GeneratedNamespaceFixture> {
    if root.exists() {
        return Err("namespace fixture already exists".into());
    }
    let plan_started = Instant::now();
    let plan = workload_source::namespace_plan(scenario.id)?;
    let fixture_plan_ns = elapsed_ns(plan_started);
    let fixture_plan_bytes = workload_source::namespace_plan_owned_bytes(&plan)?;
    let fixture_path_state_bytes = u64::try_from(
        plan.files
            .iter()
            .try_fold(plan.edit_path.capacity(), |total, file| {
                total.checked_add(file.relative_path.capacity())
            })
            .ok_or("namespace fixture path ownership")?,
    )?;
    let parent = root.parent().ok_or("namespace fixture parent")?;
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("namespace fixture name")?;
    let partial = parent.join(format!(
        ".{name}.partial-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let generated = (|| -> AnyResult<GeneratedNamespaceFixture> {
        std::fs::create_dir(&partial)?;
        for directory in 0..scenario.data_directories {
            std::fs::create_dir(partial.join(format!("d{directory:04}")))?;
        }
        let generated_started = Instant::now();
        let mut buffer = vec![0_u8; workload_source::NAMESPACE_SCRATCH_BYTES];
        let maximum_fixture_write_buffer_bytes = u64::try_from(buffer.capacity())?;
        let mut content_digests = vec![None; plan.files.len()];
        let fixture_digest_record_bytes = u64::try_from(
            content_digests
                .capacity()
                .checked_mul(std::mem::size_of::<Option<[u8; 32]>>())
                .ok_or("namespace fixture digest ownership")?,
        )?;
        let edit_index = plan
            .files
            .iter()
            .position(|file| file.relative_path == plan.edit_path)
            .ok_or("namespace fixture edit index")?;
        let edit_offset = workload_source::namespace_edit_offset(plan.edit_size)?;
        let mut edited_content_digest = None;
        let mut fixture_write_calls = 0_u64;
        let mut fixture_open_calls = 0_u64;
        let mut fixture_content_bytes_generated = 0_u64;
        let mut fixture_content_bytes_written = 0_u64;
        let mut fixture_content_hash_input_bytes = 0_u64;
        for (index, file) in plan.files.iter().enumerate() {
            let path = partial.join(&file.relative_path);
            let mut output = std::fs::File::create(&path)?;
            fixture_open_calls = fixture_open_calls
                .checked_add(1)
                .ok_or("namespace fixture open calls")?;
            let mut stream = workload_source::NamespaceContentStream::new(scenario, file);
            let mut content_hash = workload_source::Sha256::new();
            let mut edited_hash = (index == edit_index).then(workload_source::Sha256::new);
            let mut offset = 0_u64;
            while offset < file.size {
                let count =
                    usize::try_from((file.size - offset).min(u64::try_from(buffer.len())?))?;
                stream.fill(&mut buffer[..count]);
                fixture_content_bytes_generated = fixture_content_bytes_generated
                    .checked_add(u64::try_from(count)?)
                    .ok_or("namespace fixture generated bytes")?;
                output.write_all(&buffer[..count])?;
                fixture_write_calls = fixture_write_calls
                    .checked_add(1)
                    .ok_or("namespace fixture write calls")?;
                fixture_content_bytes_written = fixture_content_bytes_written
                    .checked_add(u64::try_from(count)?)
                    .ok_or("namespace fixture written bytes")?;
                content_hash.update(&buffer[..count]);
                fixture_content_hash_input_bytes = fixture_content_hash_input_bytes
                    .checked_add(u64::try_from(count)?)
                    .ok_or("namespace fixture hash input bytes")?;
                if let Some(hash) = edited_hash.as_mut() {
                    update_edited_hash(hash, &buffer[..count], offset, edit_offset)?;
                    fixture_content_hash_input_bytes = fixture_content_hash_input_bytes
                        .checked_add(u64::try_from(count)?)
                        .ok_or("namespace fixture edited hash input bytes")?;
                }
                offset = offset
                    .checked_add(u64::try_from(count)?)
                    .ok_or("namespace fixture write offset")?;
            }
            if output.metadata()?.len() != file.size {
                return Err("namespace fixture generated size".into());
            }
            drop(output);
            workload_source::set_namespace_metadata(&path, false)?;
            content_digests[index] = Some(content_hash.finish());
            if let Some(hash) = edited_hash {
                edited_content_digest = Some(hash.finish());
            }
        }
        for directory in 0..scenario.data_directories {
            workload_source::set_namespace_metadata(
                &partial.join(format!("d{directory:04}")),
                true,
            )?;
        }
        workload_source::set_namespace_metadata(&partial, true)?;
        let fixture_generate_ns = elapsed_ns(generated_started);
        let manifest_started = Instant::now();
        let fixture_digest = workload_source::namespace_tree_digest(&plan, &content_digests)?;
        let original_edit_digest = content_digests[edit_index]
            .replace(edited_content_digest.ok_or("namespace edited content digest")?)
            .ok_or("namespace original edit digest")?;
        let edited_digest = workload_source::namespace_tree_digest(&plan, &content_digests)?;
        content_digests[edit_index] = Some(original_edit_digest);
        let manifest = manifest_from_plan(&plan, fixture_digest);
        std::fs::rename(&partial, root)?;
        let fixture_manifest_ns = elapsed_ns(manifest_started);
        Ok(GeneratedNamespaceFixture {
            manifest,
            edited_digest,
            edit_path: plan.edit_path.clone(),
            edit_size: plan.edit_size,
            fixture_plan_ns,
            fixture_generate_ns,
            fixture_manifest_ns,
            maximum_fixture_write_buffer_bytes,
            fixture_write_calls,
            fixture_open_calls,
            fixture_content_bytes_generated,
            fixture_content_bytes_written,
            fixture_content_hash_input_bytes,
            fixture_plan_bytes,
            fixture_path_state_bytes,
            fixture_digest_record_bytes,
        })
    })();
    if generated.is_err() {
        let _ = std::fs::remove_dir_all(&partial);
    }
    generated
}

fn update_edited_hash(
    hash: &mut workload_source::Sha256,
    bytes: &[u8],
    chunk_offset: u64,
    edit_offset: u64,
) -> AnyResult<()> {
    let chunk_end = chunk_offset
        .checked_add(u64::try_from(bytes.len())?)
        .ok_or("namespace edited chunk end")?;
    let edit_end = edit_offset
        .checked_add(u64::try_from(workload_source::NAMESPACE_EDIT_MARKER.len())?)
        .ok_or("namespace edit end")?;
    if chunk_end <= edit_offset || chunk_offset >= edit_end {
        hash.update(bytes);
        return Ok(());
    }
    let overlap_start = chunk_offset.max(edit_offset);
    let overlap_end = chunk_end.min(edit_end);
    let before = usize::try_from(overlap_start - chunk_offset)?;
    let after = usize::try_from(overlap_end - chunk_offset)?;
    let marker_start = usize::try_from(overlap_start - edit_offset)?;
    let marker_end = usize::try_from(overlap_end - edit_offset)?;
    hash.update(&bytes[..before]);
    hash.update(&workload_source::NAMESPACE_EDIT_MARKER[marker_start..marker_end]);
    hash.update(&bytes[after..]);
    Ok(())
}

fn manifest_from_plan(plan: &workload_source::NamespacePlan, digest: String) -> NamespaceManifest {
    NamespaceManifest {
        regular_files: plan.scenario.regular_files,
        data_directories: plan.scenario.data_directories,
        logical_bytes: plan.scenario.logical_bytes,
        empty_files: plan.empty_files,
        tiny_files: plan.tiny_files,
        small_files: plan.small_files,
        medium_files: plan.medium_files,
        anchor_files: plan.anchor_files,
        anchor_bytes: plan.anchor_bytes,
        file_mode: workload_source::NAMESPACE_FILE_MODE,
        directory_mode: workload_source::NAMESPACE_DIRECTORY_MODE,
        mtime_seconds: workload_source::NAMESPACE_MTIME_SECONDS,
        mtime_nanoseconds: workload_source::NAMESPACE_MTIME_NANOSECONDS,
        digest,
    }
}

fn emit_namespace_manifest(scenario: NamespaceScenario, fixture: &GeneratedNamespaceFixture) {
    let manifest = &fixture.manifest;
    let files_per_second = rate(manifest.regular_files, fixture.fixture_generate_ns);
    let bytes_per_second = rate(manifest.logical_bytes, fixture.fixture_generate_ns);
    println!(
        "{{\"schema\":\"{}\",\"scenario\":\"{}\",\"fixture_profile\":\"{}\",\"fixture_digest_profile\":\"{}\",\"edit_contract\":\"{}\",\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"empty_files\":{},\"tiny_files\":{},\"small_files\":{},\"medium_files\":{},\"anchor_files\":{},\"anchor_bytes\":{},\"file_mode\":{},\"directory_mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},\"fixture_digest\":\"{}\",\"edited_fixture_digest\":\"{}\",\"edit_path\":\"{}\",\"edit_size\":{},\"fixture_plan_ns\":{},\"fixture_generate_ns\":{},\"fixture_manifest_ns\":{},\"fixture_files_per_second\":{},\"fixture_bytes_per_second\":{},\"fixture_worker_count\":1,\"fixture_cache_profile\":\"generated-warm-uncontrolled\",\"maximum_fixture_write_buffer_bytes\":{},\"fixture_plan_bytes\":{},\"fixture_path_state_bytes\":{},\"fixture_digest_record_bytes\":{},\"fixture_open_calls\":{},\"fixture_write_calls\":{},\"fixture_content_bytes_generated\":{},\"fixture_content_bytes_written\":{},\"fixture_content_hash_input_bytes\":{},\"post_generation_content_rereads\":0,\"complete_file_vec_allocations\":0,\"per_file_fsyncs\":0,\"atomic_publish\":true}}",
        workload_source::NAMESPACE_FIXTURE_SCHEMA,
        scenario.id,
        workload_source::NAMESPACE_FIXTURE_PROFILE,
        workload_source::NAMESPACE_DIGEST_PROFILE,
        workload_source::NAMESPACE_EDIT_CONTRACT,
        manifest.regular_files,
        manifest.data_directories,
        manifest.logical_bytes,
        manifest.empty_files,
        manifest.tiny_files,
        manifest.small_files,
        manifest.medium_files,
        manifest.anchor_files,
        manifest.anchor_bytes,
        manifest.file_mode,
        manifest.directory_mode,
        manifest.mtime_seconds,
        manifest.mtime_nanoseconds,
        manifest.digest,
        fixture.edited_digest,
        fixture.edit_path,
        fixture.edit_size,
        fixture.fixture_plan_ns,
        fixture.fixture_generate_ns,
        fixture.fixture_manifest_ns,
        files_per_second,
        bytes_per_second,
        fixture.maximum_fixture_write_buffer_bytes,
        fixture.fixture_plan_bytes,
        fixture.fixture_path_state_bytes,
        fixture.fixture_digest_record_bytes,
        fixture.fixture_open_calls,
        fixture.fixture_write_calls,
        fixture.fixture_content_bytes_generated,
        fixture.fixture_content_bytes_written,
        fixture.fixture_content_hash_input_bytes,
    );
}

fn rate(units: u64, elapsed_ns: u64) -> u64 {
    u128::from(units)
        .checked_mul(1_000_000_000)
        .and_then(|value| value.checked_div(u128::from(elapsed_ns.max(1))))
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or(u64::MAX)
}

fn namespace_self_check() -> AnyResult<()> {
    workload_source::init_namespace::self_check()?;
    if NAMESPACE_100000_BINDING_INIT_NS != 3_235_294_118
        || NAMESPACE_100000_BINDING_BYTES_PER_SECOND != 153_000_000
        || NAMESPACE_100000_BINDING_FILES_PER_SECOND != 30_600
        || !(3_019_172_334 <= NAMESPACE_100000_BINDING_INIT_NS
            && 165_608_300 >= NAMESPACE_100000_BINDING_BYTES_PER_SECOND
            && 33_121 >= NAMESPACE_100000_BINDING_FILES_PER_SECOND)
        || 3_235_294_119 <= NAMESPACE_100000_BINDING_INIT_NS
        || 152_999_999 >= NAMESPACE_100000_BINDING_BYTES_PER_SECOND
        || 30_599 >= NAMESPACE_100000_BINDING_FILES_PER_SECOND
    {
        return Err("namespace-100000 binding threshold self-check".into());
    }
    let expected = [
        ("namespace-100", [1, 78, 15, 5, 1], 125_000_000),
        ("namespace-1000", [10, 789, 150, 50, 1], 200_000_000),
        ("namespace-10000", [100, 7_899, 1_500, 500, 1], 300_000_000),
        (
            "namespace-100000",
            [1_000, 78_998, 15_000, 5_000, 2],
            500_000_000,
        ),
    ];
    for (id, counts, logical_bytes) in expected {
        let first = workload_source::namespace_plan(id)?;
        if [
            first.empty_files,
            first.tiny_files,
            first.small_files,
            first.medium_files,
            first.anchor_files,
        ] != counts
            || first.scenario.logical_bytes != logical_bytes
            || (id == "namespace-100" && first != workload_source::namespace_plan(id)?)
        {
            return Err("namespace-v2 planner self-check".into());
        }
    }
    NamespaceSample {
        layerstack_init_ns: 1,
        branch_fork_ns: 2,
        workspace_create_ns: 3,
        edit_ns: 4,
        commit_ns: 5,
        workspace_end_ns: 6,
        reconnect_ns: 7,
        reopen_workspace_create_ns: 8,
        reopen_content_verify_ns: 9,
        reopen_workspace_end_ns: 10,
        reopen_verify_ns: 24,
        complete_product_ns: 45,
        product_lifecycle_ns: 45,
    }
    .validate()?;
    Ok(())
}

fn namespace_placement(
    container_id: &ContainerId,
    scenario: NamespaceScenario,
    iteration: usize,
    phase: &str,
) -> WorkspacePlacement {
    WorkspacePlacement::Container {
        container_id: container_id.clone(),
        root: PathBuf::from(format!(
            "/workspace/layerfs-{}-{iteration}-{phase}-{}",
            scenario.id,
            std::process::id()
        )),
    }
}

fn operation_candidate(
    snapshot: &layerfs_sdk::MonitorSnapshot,
    family: OperationFamily,
) -> AnyResult<CandidateStats> {
    let mut candidates = snapshot.operations.iter().filter_map(|operation| {
        (operation.operation.family == family)
            .then_some(operation.candidate)
            .flatten()
    });
    let candidate = candidates.next().ok_or("namespace candidate receipt")?;
    if candidates.next().is_some() || !candidate.validate_for(family) {
        return Err("namespace candidate receipt cardinality or equation".into());
    }
    Ok(candidate)
}

fn operation_workspace_read(
    snapshot: &layerfs_sdk::MonitorSnapshot,
) -> AnyResult<NamespaceReadMetrics> {
    let mut reads = snapshot.operations.iter().flat_map(|operation| {
        operation
            .storage
            .iter()
            .filter_map(|receipt| match receipt {
                StorageReceipt::WorkspaceRead(receipt) => Some(*receipt),
                _ => None,
            })
    });
    let read = reads.next().ok_or("namespace Workspace read receipt")?;
    if reads.next().is_some()
        || read.read_ahead_fetches == 0
        || read.read_ahead_fetches != read.read_ahead_misses
        || read.max_readahead_bytes == 0
    {
        return Err("namespace product read-ahead equation".into());
    }
    Ok(NamespaceReadMetrics {
        maximum_product_read_ahead_bytes: read.max_readahead_bytes,
        read_ahead_hits: read.read_ahead_hits,
        read_ahead_misses: read.read_ahead_misses,
        read_ahead_fetches: read.read_ahead_fetches,
        read_ahead_requested_bytes: read.read_ahead_requested_bytes,
        read_ahead_fetched_bytes: read.read_ahead_fetched_bytes,
        read_ahead_served_bytes: read.read_ahead_served_bytes,
        read_ahead_unused_bytes: read.read_ahead_unused_bytes,
        local_calls: read.local_calls,
        local_ids: read.local_ids,
        local_rows: read.local_rows,
        local_bytes: read.local_bytes,
    })
}

fn operation_workspace_create(
    snapshot: &layerfs_sdk::MonitorSnapshot,
) -> AnyResult<layerfs_sdk::WorkspaceLifecycleReceipt> {
    let mut creates = snapshot.operations.iter().flat_map(|operation| {
        operation
            .storage
            .iter()
            .filter_map(|receipt| match receipt {
                StorageReceipt::WorkspaceLifecycle(receipt)
                    if receipt.kind == layerfs_sdk::WorkspaceLifecycleKind::Attach =>
                {
                    Some(*receipt)
                }
                _ => None,
            })
    });
    let create = creates.next().ok_or("namespace Workspace Create receipt")?;
    if creates.next().is_some() {
        return Err("namespace Workspace Create receipt cardinality".into());
    }
    Ok(create)
}

fn operation_workspace_commit(
    snapshot: &layerfs_sdk::MonitorSnapshot,
) -> AnyResult<layerfs_sdk::WorkspaceCommitReceipt> {
    let mut commits = snapshot.operations.iter().flat_map(|operation| {
        operation
            .storage
            .iter()
            .filter_map(|receipt| match receipt {
                StorageReceipt::WorkspaceCommit(receipt) => Some(*receipt),
                _ => None,
            })
    });
    let commit = commits.next().ok_or("namespace Workspace Commit receipt")?;
    if commits.next().is_some() {
        return Err("namespace Workspace Commit receipt cardinality".into());
    }
    Ok(commit)
}

fn sum_metric(left: u64, right: u64, name: &'static str) -> AnyResult<u64> {
    left.checked_add(right).ok_or_else(|| name.into())
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        && value.bytes().all(|byte| !byte.is_ascii_uppercase())
}

fn parse_namespace_verification(output: &OutputPage) -> AnyResult<NamespaceVerification> {
    let bytes = output
        .chunks
        .iter()
        .flat_map(|chunk| chunk.bytes.iter().copied())
        .collect::<Vec<_>>();
    parse_namespace_verification_text(std::str::from_utf8(&bytes)?)
}

fn parse_namespace_verification_text(output: &str) -> AnyResult<NamespaceVerification> {
    let mut fields = std::collections::BTreeMap::new();
    for line in output.lines() {
        let (name, value) = line
            .split_once('=')
            .ok_or("malformed namespace verification field")?;
        if fields.insert(name, value).is_some() {
            return Err("duplicate namespace verification field".into());
        }
    }
    if fields.len() != 15 {
        return Err("namespace verification field count".into());
    }
    let digest = fields
        .remove("namespace_digest")
        .ok_or("namespace verification digest")?;
    if !valid_digest(digest) {
        return Err("namespace verification digest shape".into());
    }
    let manifest = NamespaceManifest {
        regular_files: fields
            .remove("regular_files")
            .ok_or("namespace verification files")?
            .parse()?,
        data_directories: fields
            .remove("data_directories")
            .ok_or("namespace verification directories")?
            .parse()?,
        logical_bytes: fields
            .remove("logical_bytes")
            .ok_or("namespace verification bytes")?
            .parse()?,
        empty_files: fields
            .remove("empty_files")
            .ok_or("namespace verification empty files")?
            .parse()?,
        tiny_files: fields
            .remove("tiny_files")
            .ok_or("namespace verification tiny files")?
            .parse()?,
        small_files: fields
            .remove("small_files")
            .ok_or("namespace verification small files")?
            .parse()?,
        medium_files: fields
            .remove("medium_files")
            .ok_or("namespace verification medium files")?
            .parse()?,
        anchor_files: fields
            .remove("anchor_files")
            .ok_or("namespace verification anchor files")?
            .parse()?,
        anchor_bytes: fields
            .remove("anchor_bytes")
            .ok_or("namespace verification anchor bytes")?
            .parse()?,
        file_mode: workload_source::NAMESPACE_FILE_MODE,
        directory_mode: workload_source::NAMESPACE_DIRECTORY_MODE,
        mtime_seconds: workload_source::NAMESPACE_MTIME_SECONDS,
        mtime_nanoseconds: workload_source::NAMESPACE_MTIME_NANOSECONDS,
        digest: digest.to_owned(),
    };
    let verification = NamespaceVerification {
        manifest,
        maximum_verifier_buffer_bytes: fields
            .remove("maximum_verifier_buffer_bytes")
            .ok_or("namespace maximum verifier buffer")?
            .parse()?,
        verifier_worker_count: fields
            .remove("verifier_worker_count")
            .ok_or("namespace verifier worker count")?
            .parse()?,
        verifier_plan_bytes: fields
            .remove("verifier_plan_bytes")
            .ok_or("namespace verifier plan bytes")?
            .parse()?,
        verifier_path_state_peak_bytes: fields
            .remove("verifier_path_state_peak_bytes")
            .ok_or("namespace verifier path bytes")?
            .parse()?,
        verifier_digest_state_peak_bytes: fields
            .remove("verifier_digest_state_peak_bytes")
            .ok_or("namespace verifier digest bytes")?
            .parse()?,
    };
    if !fields.is_empty()
        || verification.maximum_verifier_buffer_bytes
            > u64::try_from(workload_source::NAMESPACE_SCRATCH_BYTES)?
        || verification.verifier_worker_count == 0
    {
        return Err("namespace verifier resource bound".into());
    }
    Ok(verification)
}

fn namespace_manifest(scenario: NamespaceScenario, digest: &str) -> AnyResult<NamespaceManifest> {
    Ok(NamespaceManifest {
        regular_files: scenario.regular_files,
        data_directories: scenario.data_directories,
        logical_bytes: scenario.logical_bytes,
        empty_files: scenario.empty_files,
        tiny_files: scenario.tiny_files,
        small_files: scenario.small_files,
        medium_files: scenario.medium_files,
        anchor_files: scenario.anchor_files,
        anchor_bytes: scenario
            .anchor_files
            .checked_mul(workload_source::NAMESPACE_ANCHOR_BYTES)
            .ok_or("namespace anchor bytes")?,
        file_mode: workload_source::NAMESPACE_FILE_MODE,
        directory_mode: workload_source::NAMESPACE_DIRECTORY_MODE,
        mtime_seconds: workload_source::NAMESPACE_MTIME_SECONDS,
        mtime_nanoseconds: workload_source::NAMESPACE_MTIME_NANOSECONDS,
        digest: digest.to_owned(),
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_namespace_family_arguments(
    fixture: &Path,
    seed: u8,
    source: &str,
    fixture_digest: &str,
    edited_digest: &str,
    edit_path: &str,
    edit_size: u64,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    if !fixture.is_dir()
        || !workload_source::SEEDS.contains(&seed)
        || !matches!(source, "baseline" | "candidate")
        || !valid_digest(fixture_digest)
        || !valid_digest(edited_digest)
        || fixture_digest == edited_digest
        || !matches!(
            fixture_cache_profile,
            "generated-first-sample-uncontrolled"
                | "generated-subsequent-sample-uncontrolled"
                | "reused-first-sample-uncontrolled"
                | "reused-subsequent-sample-uncontrolled"
        )
        || edit_path.starts_with('/')
        || edit_path.contains("..")
        || edit_size <= u64::try_from(workload_source::NAMESPACE_EDIT_MARKER.len())?
    {
        return Err("namespace family arguments".into());
    }
    Ok(())
}

fn output_u64(output: &OutputPage, name: &str) -> AnyResult<u64> {
    let prefix = format!("{name}=");
    output
        .chunks
        .iter()
        .flat_map(|chunk| {
            String::from_utf8_lossy(&chunk.bytes)
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .find_map(|line| line.strip_prefix(&prefix)?.parse().ok())
        .ok_or_else(|| format!("missing workload field: {name}").into())
}

fn output_string(output: &OutputPage, name: &str) -> AnyResult<String> {
    let prefix = format!("{name}=");
    output
        .chunks
        .iter()
        .flat_map(|chunk| {
            String::from_utf8_lossy(&chunk.bytes)
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .find_map(|line| line.strip_prefix(&prefix).map(str::to_owned))
        .ok_or_else(|| format!("missing workload field: {name}").into())
}

fn operation_fuse_write(snapshot: &layerfs_sdk::MonitorSnapshot) -> AnyResult<FuseWriteReceipt> {
    let mut writes = snapshot.operations.iter().flat_map(|operation| {
        operation
            .storage
            .iter()
            .filter_map(|receipt| match receipt {
                StorageReceipt::FuseWrite(receipt) => Some(*receipt),
                _ => None,
            })
    });
    let write = writes.next().ok_or("same-count FUSE write receipt")?;
    if writes.next().is_some() || write.kernel_write_requests == 0 {
        return Err("same-count FUSE write receipt cardinality".into());
    }
    Ok(write)
}

fn same_count_placement(
    container_id: &ContainerId,
    scenario: workload_source::edit_same_count::Scenario,
    seed: u8,
    phase: &str,
) -> WorkspacePlacement {
    WorkspacePlacement::Container {
        container_id: container_id.clone(),
        root: PathBuf::from(format!(
            "/workspace/layerfs-{}-{seed}-{phase}-{}",
            scenario.id,
            std::process::id()
        )),
    }
}

fn same_count_performance_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    scenario_id: &str,
    seed: u8,
    source: &str,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    let (scenario, supplemental_control) =
        match workload_source::edit_same_count::scenario(scenario_id) {
            Ok(scenario) => (scenario, false),
            Err(_) => {
                let control = workload_source::edit_same_count::pair_control(scenario_id)?;
                (
                    workload_source::edit_same_count::Scenario {
                        id: control.id,
                        display_name: control.id,
                        operations: control.operations,
                        position: control.position,
                        frozen: false,
                    },
                    true,
                )
            }
        };
    if scenario.frozen {
        return same_count_anchor_performance_case(
            root,
            fixture,
            container_id,
            scenario,
            seed,
            source,
            fixture_cache_profile,
        );
    }
    if !workload_source::edit_same_count::SEEDS.contains(&seed)
        || !matches!(source, "baseline" | "candidate" | "repeat-a" | "repeat-b")
        || !matches!(
            fixture_cache_profile,
            "generated-first-sample-uncontrolled"
                | "generated-subsequent-sample-uncontrolled"
                | "reused-first-sample-uncontrolled"
                | "reused-subsequent-sample-uncontrolled"
        )
        || root.exists()
        || !fixture.is_dir()
        || std::fs::metadata(fixture.join("payload.bin"))?.len()
            != workload_source::edit_same_count::FIXTURE_BYTES
    {
        return Err("same-count performance arguments".into());
    }
    std::fs::create_dir(root)?;
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect(store.clone())?;
    let store_baseline_bytes = std::fs::metadata(&store_path)?.len();
    let process_before = process_resource_snapshot()?;
    let container_before = container_cgroup_snapshot(&container_id)?;

    let init_started = Instant::now();
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("{}-{source}-{seed}", scenario.id))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let layerstack_init_ns = elapsed_ns(init_started);
    let scan_receipts = store.take_layerstack_initialization_receipts();
    let [scan] = scan_receipts.as_slice() else {
        return Err("same-count initialization receipt cardinality".into());
    };
    if scan.scanned_files != 1
        || scan.scanned_bytes != workload_source::edit_same_count::FIXTURE_BYTES
    {
        return Err("same-count initialization receipt".into());
    }
    let branch_started = Instant::now();
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let branch_fork_ns = elapsed_ns(branch_started);

    let t0 = Instant::now();
    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: same_count_placement(&container_id, scenario, seed, "performance"),
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let t1 = Instant::now();
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let output = execute_workload(
        &client,
        workspace.id,
        vec![
            workload,
            OsString::from(if supplemental_control {
                "same-count-control-edit"
            } else {
                "same-count-edit"
            }),
            OsString::from("payload.bin"),
            OsString::from(scenario.id),
            OsString::from(seed.to_string()),
        ],
    )?;
    let t2 = Instant::now();
    let attempted = output_u64(&output, "attempted_operations")?;
    let completed = output_u64(&output, "completed_operations")?;
    let final_bytes = output_u64(&output, "final_file_bytes")?;
    let supplied = output_u64(&output, "supplied_bytes")?;
    let unique = output_u64(&output, "unique_bytes")?;
    let overlapping = output_u64(&output, "overlapping_bytes")?;
    let identical = output_u64(&output, "identical_bytes")?;
    let superseded = output_u64(&output, "superseded_bytes")?;
    let inner_edit_ns = output_u64(&output, "inner_edit_ns")?;
    if attempted != scenario.operations as u64
        || completed != attempted
        || final_bytes != workload_source::edit_same_count::FIXTURE_BYTES
        || supplied != unique + overlapping
        || superseded != overlapping
    {
        return Err("same-count workload validity".into());
    }
    let head = match client.commit_workspace_session(workspace.id)? {
        WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
        result => return Err(format!("same-count Commit failed: {result:?}").into()),
    };
    let commit_return = Instant::now();
    visible_head(&client, branch, head)?;
    let t3 = Instant::now();
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    let t4 = Instant::now();
    if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
        return Err("same-count cleanup".into());
    }
    let process_after = process_resource_snapshot()?;
    let container_after = container_cgroup_snapshot(&container_id)?;
    if process_before.swaps != 0
        || process_after.swaps != 0
        || container_before.swap_current != 0
        || container_after.swap_current != 0
        || container_before.oom != container_after.oom
        || container_before.oom_kill != container_after.oom_kill
    {
        return Err("same-count swap or OOM".into());
    }
    let snapshot = client.monitor_snapshot()?;
    let initialize_candidate =
        operation_candidate(&snapshot, OperationFamily::LayerStackInitialize)?;
    let candidate = operation_candidate(&snapshot, OperationFamily::WorkspaceCommit)?;
    let commit = operation_workspace_commit(&snapshot)?;
    let fuse = operation_fuse_write(&snapshot)?;
    if commit.edit_count != completed
        || commit.edit_piece_count == 0
        || commit.edit_piece_height == 0
        || commit.edit_piece_logical_charge == 0
        || commit.edit_spool_allocated_bytes != fuse.spool_write_bytes
        || commit.edit_spool_live_bytes + commit.edit_spool_superseded_bytes
            != commit.edit_spool_allocated_bytes
        || commit.edit_tree_visits == 0
        || commit.edit_metric_nodes_scanned != 1
    {
        return Err("same-count edit receipt".into());
    }
    let execution_ns = nanos(t1, t2);
    println!(
        "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"scenario_id\":\"{}\",\"display_name\":\"{}\",\"mode\":\"performance\",\"source_arm\":\"{}\",\"seed\":{},\"execution_profile\":\"macbook-docker-desktop-linux-fuse-v1\",\"fixture_profile\":\"{}\",\"fixture_cache_profile\":\"{}\",\"operation\":\"overwrite\",\"position\":\"{}\",\"operation_count\":{},\"attempted_operations\":{},\"completed_operations\":{},\"initial_file_bytes\":{},\"final_file_bytes\":{},\"supplied_bytes\":{},\"unique_bytes\":{},\"overlapping_bytes\":{},\"identical_bytes\":{},\"superseded_bytes\":{},\"layerstack_init_ns\":{},\"branch_fork_ns\":{},\"workspace_create_ns\":{},\"execution_ns\":{},\"inner_edit_ns\":{},\"commit_call_ns\":{},\"visibility_ack_ns\":{},\"commit_api_ns\":{},\"layerstack_visible_ns\":{},\"workspace_end_ns\":{},\"complete_lifecycle_ns\":{},\"operations_per_second\":{},\"supplied_bytes_per_second\":{},\"fuse_max_write_bytes\":{},\"fuse_kernel_write_requests\":{},\"fuse_kernel_write_bytes\":{},\"fuse_client_request_copy_bytes\":{},\"fuse_frame_payload_copy_bytes\":{},\"fuse_client_frame_bytes\":{},\"fuse_host_frame_bytes\":{},\"fuse_host_decode_copy_bytes\":{},\"spool_write_bytes\":{},\"spool_write_open_count\":{},\"spool_allocated_bytes\":{},\"physical_spool_high_water_bytes\":{},\"spool_live_bytes\":{},\"spool_superseded_bytes\":{},\"piece_count\":{},\"piece_height\":{},\"piece_logical_charge_bytes\":{},\"tree_visits\":{},\"metric_nodes_scanned\":{},\"forbidden_path_runtime_counters\":\"not-applicable-no-entry-points\",\"forbidden_path_static_proof\":\"implicit-offset-persistent-piece-tree\",\"commit_total_ns\":{},\"commit_pause_fence_ns\":{},\"commit_quiesce_ns\":{},\"commit_capture_ns\":{},\"commit_candidate_plan_ns\":{},\"commit_dirty_compare_ns\":{},\"commit_content_ns\":{},\"commit_namespace_ns\":{},\"commit_candidate_finish_ns\":{},\"commit_local_admission_ns\":{},\"commit_object_admission_ns\":{},\"commit_publication_ns\":{},\"commit_rebase_ns\":{},\"commit_resume_ns\":{},\"commit_unattributed_ns\":{},\"candidate_objects\":{},\"candidate_bytes\":{},\"inserted_objects\":{},\"inserted_bytes_total\":{},\"reused_objects\":{},\"reused_bytes\":{},\"admission_transactions\":{},\"max_transaction_objects\":{},\"max_transaction_bytes\":{},\"initialization_candidate_objects\":{},\"initialization_candidate_bytes\":{},\"scanned_files\":{},\"scanned_bytes\":{},\"commit_payload_bytes_read\":{},\"commit_cdc_bytes_scanned\":{},\"process_user_cpu_ns\":{},\"process_system_cpu_ns\":{},\"process_disk_read_bytes\":{},\"process_disk_write_bytes\":{},\"process_context_switches\":{},\"process_peak_rss_bytes\":{},\"process_physical_footprint_bytes\":{},\"container_memory_current_bytes\":{},\"container_memory_peak_bytes\":{},\"container_pids_current\":{},\"store_baseline_bytes\":{},\"store_database_bytes\":{},\"swap_bytes\":0,\"oom\":false,\"timeout\":false,\"verification_status\":\"not-run-performance-mode\",\"cleanup_status\":\"pass\"}}",
        workload_source::edit_same_count::PERFORMANCE_SCHEMA,
        workload_source::edit_same_count::FAMILY_ID,
        scenario.id,
        scenario.display_name,
        source,
        seed,
        workload_source::edit_same_count::FIXTURE_PROFILE,
        fixture_cache_profile,
        scenario.position.name(),
        scenario.operations,
        attempted,
        completed,
        workload_source::edit_same_count::FIXTURE_BYTES,
        final_bytes,
        supplied,
        unique,
        overlapping,
        identical,
        superseded,
        layerstack_init_ns,
        branch_fork_ns,
        nanos(t0, t1),
        execution_ns,
        inner_edit_ns,
        nanos(t2, commit_return),
        nanos(commit_return, t3),
        nanos(t2, t3),
        nanos(t0, t3),
        nanos(t3, t4),
        nanos(t0, t4),
        rate(completed, inner_edit_ns),
        rate(supplied, inner_edit_ns),
        fuse.max_write_bytes,
        fuse.kernel_write_requests,
        fuse.kernel_write_bytes,
        fuse.client_request_copy_bytes,
        fuse.frame_payload_copy_bytes,
        fuse.client_frame_bytes,
        fuse.host_frame_bytes,
        fuse.host_decode_copy_bytes,
        fuse.spool_write_bytes,
        fuse.spool_write_open_count,
        commit.edit_spool_allocated_bytes,
        commit.edit_spool_peak_bytes,
        commit.edit_spool_live_bytes,
        commit.edit_spool_superseded_bytes,
        commit.edit_piece_count,
        commit.edit_piece_height,
        commit.edit_piece_logical_charge,
        commit.edit_tree_visits,
        commit.edit_metric_nodes_scanned,
        commit.total_ns,
        commit.pause_fence_ns,
        commit.quiesce_ns,
        commit.capture_ns,
        commit.candidate_plan_ns,
        commit.dirty_compare_ns,
        commit.content_ns,
        commit.namespace_ns,
        commit.candidate_finish_ns,
        commit.local_admission_ns,
        commit.object_admission_ns,
        commit.publication_ns,
        commit.in_place_rebase_ns,
        commit.resume_ns,
        commit.unattributed_ns,
        candidate.candidate_objects,
        candidate.candidate_bytes,
        candidate.inserted_objects,
        candidate.inserted_bytes,
        candidate.reused_objects,
        candidate.reused_bytes,
        candidate.admission_transactions,
        candidate.max_transaction_objects,
        candidate.max_transaction_bytes,
        initialize_candidate.candidate_objects,
        initialize_candidate.candidate_bytes,
        scan.scanned_files,
        scan.scanned_bytes,
        commit.payload_bytes_read,
        commit.cdc_bytes_scanned,
        resource_delta(process_after.user_cpu_ns, process_before.user_cpu_ns, "same-count user CPU")?,
        resource_delta(process_after.system_cpu_ns, process_before.system_cpu_ns, "same-count system CPU")?,
        resource_delta(process_after.disk_read_bytes, process_before.disk_read_bytes, "same-count disk read")?,
        resource_delta(process_after.disk_write_bytes, process_before.disk_write_bytes, "same-count disk write")?,
        resource_delta(process_after.context_switches, process_before.context_switches, "same-count context switches")?,
        process_after.peak_resident_bytes,
        process_after.physical_footprint_bytes,
        container_after.memory_current,
        container_after.memory_peak,
        container_after.pids_current,
        store_baseline_bytes,
        std::fs::metadata(&store_path)?.len(),
    );
    Ok(())
}

#[derive(Default)]
struct EditFuseMetrics {
    max_write_bytes: u64,
    kernel_read_requests: u64,
    kernel_read_bytes: u64,
    read_ahead_hits: u64,
    read_ahead_misses: u64,
    read_ahead_fetches: u64,
    read_ahead_requested_bytes: u64,
    read_ahead_fetched_bytes: u64,
    read_ahead_served_bytes: u64,
    read_ahead_unused_bytes: u64,
    kernel_write_requests: u64,
    kernel_write_bytes: u64,
    client_request_copy_bytes: u64,
    frame_payload_copy_bytes: u64,
    client_frame_bytes: u64,
    host_frame_bytes: u64,
    host_decode_copy_bytes: u64,
    spool_write_bytes: u64,
    spool_write_open_count: u64,
}

fn edit_fuse_metrics(snapshot: &layerfs_sdk::MonitorSnapshot) -> EditFuseMetrics {
    let mut total = EditFuseMetrics::default();
    for receipt in snapshot.operations.iter().flat_map(|operation| {
        operation
            .storage
            .iter()
            .filter_map(|receipt| match receipt {
                StorageReceipt::WorkspaceRead(receipt) => Some(receipt),
                _ => None,
            })
    }) {
        total.kernel_read_requests = total
            .kernel_read_requests
            .saturating_add(receipt.kernel_read_requests);
        total.kernel_read_bytes = total
            .kernel_read_bytes
            .saturating_add(receipt.kernel_read_bytes);
        total.read_ahead_hits = total
            .read_ahead_hits
            .saturating_add(receipt.read_ahead_hits);
        total.read_ahead_misses = total
            .read_ahead_misses
            .saturating_add(receipt.read_ahead_misses);
        total.read_ahead_fetches = total
            .read_ahead_fetches
            .saturating_add(receipt.read_ahead_fetches);
        total.read_ahead_requested_bytes = total
            .read_ahead_requested_bytes
            .saturating_add(receipt.read_ahead_requested_bytes);
        total.read_ahead_fetched_bytes = total
            .read_ahead_fetched_bytes
            .saturating_add(receipt.read_ahead_fetched_bytes);
        total.read_ahead_served_bytes = total
            .read_ahead_served_bytes
            .saturating_add(receipt.read_ahead_served_bytes);
        total.read_ahead_unused_bytes = total
            .read_ahead_unused_bytes
            .saturating_add(receipt.read_ahead_unused_bytes);
    }
    for receipt in snapshot.operations.iter().flat_map(|operation| {
        operation
            .storage
            .iter()
            .filter_map(|receipt| match receipt {
                StorageReceipt::FuseWrite(receipt) => Some(receipt),
                _ => None,
            })
    }) {
        total.max_write_bytes = total.max_write_bytes.max(receipt.max_write_bytes);
        total.kernel_write_requests = total
            .kernel_write_requests
            .saturating_add(receipt.kernel_write_requests);
        total.kernel_write_bytes = total
            .kernel_write_bytes
            .saturating_add(receipt.kernel_write_bytes);
        total.client_request_copy_bytes = total
            .client_request_copy_bytes
            .saturating_add(receipt.client_request_copy_bytes);
        total.frame_payload_copy_bytes = total
            .frame_payload_copy_bytes
            .saturating_add(receipt.frame_payload_copy_bytes);
        total.client_frame_bytes = total
            .client_frame_bytes
            .saturating_add(receipt.client_frame_bytes);
        total.host_frame_bytes = total
            .host_frame_bytes
            .saturating_add(receipt.host_frame_bytes);
        total.host_decode_copy_bytes = total
            .host_decode_copy_bytes
            .saturating_add(receipt.host_decode_copy_bytes);
        total.spool_write_bytes = total
            .spool_write_bytes
            .saturating_add(receipt.spool_write_bytes);
        total.spool_write_open_count = total
            .spool_write_open_count
            .saturating_add(receipt.spool_write_open_count);
    }
    total
}

fn count_changing_placement(
    container_id: &ContainerId,
    scenario: workload_source::edit_count_changing::Scenario,
    seed: u8,
    phase: &str,
) -> WorkspacePlacement {
    WorkspacePlacement::Container {
        container_id: container_id.clone(),
        root: PathBuf::from(format!(
            "/workspace/layerfs-{}-{seed}-{phase}-{}",
            scenario.id,
            std::process::id()
        )),
    }
}

fn count_changing_performance_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    scenario_id: &str,
    seed: u8,
    source: &str,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    let scenario = workload_source::edit_count_changing::scenario(scenario_id)?;
    if scenario.frozen {
        return count_changing_anchor_performance_case(
            root,
            fixture,
            container_id,
            scenario,
            seed,
            source,
            fixture_cache_profile,
        );
    }
    if !workload_source::edit_count_changing::SEEDS.contains(&seed)
        || !matches!(source, "baseline" | "candidate" | "repeat-a" | "repeat-b")
        || root.exists()
        || !fixture.is_dir()
        || std::fs::metadata(fixture.join("payload.bin"))?.len()
            != workload_source::edit_count_changing::FIXTURE_BYTES
    {
        return Err("count-changing performance arguments".into());
    }
    let schedule = workload_source::edit_count_changing::schedule(scenario, seed)?;
    let expected_final = schedule
        .last()
        .ok_or("count-changing empty schedule")?
        .final_len;
    std::fs::create_dir(root)?;
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect(store.clone())?;
    let store_baseline_bytes = std::fs::metadata(&store_path)?.len();
    let process_before = process_resource_snapshot()?;
    let container_before = container_cgroup_snapshot(&container_id)?;
    let init_started = Instant::now();
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("{}-{source}-{seed}", scenario.id))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let layerstack_init_ns = elapsed_ns(init_started);
    let scan_receipts = store.take_layerstack_initialization_receipts();
    let [scan] = scan_receipts.as_slice() else {
        return Err("count-changing initialization receipt cardinality".into());
    };
    let branch_started = Instant::now();
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let branch_fork_ns = elapsed_ns(branch_started);
    let t0 = Instant::now();
    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: count_changing_placement(&container_id, scenario, seed, "performance"),
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let t1 = Instant::now();
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let output = execute_workload(
        &client,
        workspace.id,
        vec![
            workload,
            OsString::from("count-changing-edit"),
            OsString::from("payload.bin"),
            OsString::from(scenario.id),
            OsString::from(seed.to_string()),
        ],
    )?;
    let t2 = Instant::now();
    let attempted = output_u64(&output, "attempted_operations")?;
    let completed = output_u64(&output, "completed_operations")?;
    let final_bytes = output_u64(&output, "final_file_bytes")?;
    let initial_inode = output_u64(&output, "initial_inode")?;
    let final_inode = output_u64(&output, "final_inode")?;
    let supplied = output_u64(&output, "supplied_bytes")?;
    let inserted = output_u64(&output, "inserted_bytes")?;
    let deleted = output_u64(&output, "deleted_bytes")?;
    let overlapping = output_u64(&output, "overlapping_bytes")?;
    let superseded = output_u64(&output, "superseded_bytes")?;
    let logical_zero = output_u64(&output, "logical_zero_bytes")?;
    let copied_payload = output_u64(&output, "copied_payload_bytes")?;
    let read_payload = output_u64(&output, "read_payload_bytes")?;
    let inner_edit_ns = output_u64(&output, "inner_edit_ns")?;
    if attempted != scenario.operations as u64
        || completed != attempted
        || final_bytes != expected_final
        || final_bytes == workload_source::edit_count_changing::FIXTURE_BYTES
        || supplied != inserted
        || superseded != deleted
        || scenario.kind.temp_copy() == (initial_inode == final_inode)
    {
        return Err("count-changing workload validity".into());
    }
    let head = match client.commit_workspace_session(workspace.id)? {
        WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
        result => return Err(format!("count-changing Commit failed: {result:?}").into()),
    };
    let commit_return = Instant::now();
    visible_head(&client, branch, head)?;
    let t3 = Instant::now();
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    let t4 = Instant::now();
    if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
        return Err("count-changing cleanup".into());
    }
    let process_after = process_resource_snapshot()?;
    let container_after = container_cgroup_snapshot(&container_id)?;
    if process_before.swaps != 0
        || process_after.swaps != 0
        || container_before.swap_current != 0
        || container_after.swap_current != 0
        || container_before.oom != container_after.oom
        || container_before.oom_kill != container_after.oom_kill
    {
        return Err("count-changing swap or OOM".into());
    }
    let snapshot = client.monitor_snapshot()?;
    let initialize_candidate =
        operation_candidate(&snapshot, OperationFamily::LayerStackInitialize)?;
    let candidate = operation_candidate(&snapshot, OperationFamily::WorkspaceCommit)?;
    let commit = operation_workspace_commit(&snapshot)?;
    let fuse = edit_fuse_metrics(&snapshot);
    if (!scenario.kind.temp_copy()
        && (commit.edit_piece_count == 0
            || commit.edit_piece_height == 0
            || commit.edit_piece_logical_charge == 0))
        || if scenario.kind.temp_copy() {
            fuse.spool_write_bytes < commit.edit_spool_allocated_bytes
                || commit.edit_spool_allocated_bytes != commit.edit_spool_live_bytes
                || commit.edit_spool_superseded_bytes != 0
        } else {
            commit.edit_spool_allocated_bytes != fuse.spool_write_bytes
                || commit.edit_spool_live_bytes + commit.edit_spool_superseded_bytes
                    != commit.edit_spool_allocated_bytes
        }
        || commit.edit_metric_nodes_scanned == 0
        || commit.edit_spool_peak_bytes < commit.edit_spool_allocated_bytes
        || commit.edit_spool_peak_bytes > 128 * 1024 * 1024
        || (scenario.kind == workload_source::edit_count_changing::Kind::Sparse
            && (fuse.spool_write_bytes > supplied
                || commit.edit_spool_live_bytes > supplied
                || commit.edit_spool_peak_bytes > supplied
                || commit.edit_piece_logical_charge > 64 * 1024))
    {
        return Err(format!(
            "count-changing edit receipt: temp_copy={} pieces={} height={} charge={} allocated={} fuse_spool={} live={} superseded={} metric_nodes={}",
            scenario.kind.temp_copy(),
            commit.edit_piece_count,
            commit.edit_piece_height,
            commit.edit_piece_logical_charge,
            commit.edit_spool_allocated_bytes,
            fuse.spool_write_bytes,
            commit.edit_spool_live_bytes,
            commit.edit_spool_superseded_bytes,
            commit.edit_metric_nodes_scanned,
        )
        .into());
    }
    let execution_ns = nanos(t1, t2);
    println!(
        "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"scenario_id\":\"{}\",\"display_name\":\"{}\",\"paired_same_count_control_id\":\"{}\",\"pair_fixture_bytes\":{},\"pair_byte_quantity_match\":null,\"pair_byte_quantity_basis\":\"{}\",\"mode\":\"performance\",\"source_arm\":\"{}\",\"seed\":{},\"execution_profile\":\"macbook-docker-desktop-linux-fuse-v1\",\"fixture_profile\":\"{}\",\"fixture_cache_profile\":\"{}\",\"operation\":\"{}\",\"position\":\"{}\",\"implementation\":\"{}\",\"operation_count\":{},\"attempted_operations\":{},\"completed_operations\":{},\"initial_file_bytes\":{},\"final_file_bytes\":{},\"initial_inode\":{},\"final_inode\":{},\"inode_behavior\":\"{}\",\"supplied_bytes\":{},\"inserted_bytes\":{},\"deleted_bytes\":{},\"overlapping_bytes\":{},\"superseded_bytes\":{},\"logical_zero_bytes\":{},\"copied_payload_bytes\":{},\"read_payload_bytes\":{},\"fuse_kernel_read_requests\":{},\"fuse_kernel_read_bytes\":{},\"read_ahead_hits\":{},\"read_ahead_misses\":{},\"read_ahead_fetches\":{},\"read_ahead_requested_bytes\":{},\"read_ahead_fetched_bytes\":{},\"read_ahead_served_bytes\":{},\"read_ahead_unused_bytes\":{},\"layerstack_init_ns\":{},\"branch_fork_ns\":{},\"workspace_create_ns\":{},\"execution_ns\":{},\"inner_edit_ns\":{},\"commit_call_ns\":{},\"visibility_ack_ns\":{},\"commit_api_ns\":{},\"layerstack_visible_ns\":{},\"workspace_end_ns\":{},\"complete_lifecycle_ns\":{},\"operations_per_second\":{},\"supplied_bytes_per_second\":{},\"copied_payload_bytes_per_second\":{},\"fuse_max_write_bytes\":{},\"fuse_kernel_write_requests\":{},\"fuse_kernel_write_bytes\":{},\"fuse_client_request_copy_bytes\":{},\"fuse_frame_payload_copy_bytes\":{},\"fuse_client_frame_bytes\":{},\"fuse_host_frame_bytes\":{},\"fuse_host_decode_copy_bytes\":{},\"spool_write_bytes\":{},\"spool_write_open_count\":{},\"spool_allocated_bytes\":{},\"physical_spool_high_water_bytes\":{},\"spool_live_bytes\":{},\"spool_superseded_bytes\":{},\"piece_count\":{},\"piece_height\":{},\"piece_logical_charge_bytes\":{},\"tree_visits\":{},\"metric_nodes_scanned\":{},\"forbidden_path_runtime_counters\":\"not-applicable-no-entry-points\",\"forbidden_path_static_proof\":\"implicit-offset-persistent-piece-tree\",\"commit_total_ns\":{},\"commit_pause_fence_ns\":{},\"commit_quiesce_ns\":{},\"commit_capture_ns\":{},\"commit_candidate_plan_ns\":{},\"commit_dirty_compare_ns\":{},\"commit_content_ns\":{},\"commit_namespace_ns\":{},\"commit_candidate_finish_ns\":{},\"commit_local_admission_ns\":{},\"commit_object_admission_ns\":{},\"commit_publication_ns\":{},\"commit_rebase_ns\":{},\"commit_resume_ns\":{},\"commit_unattributed_ns\":{},\"candidate_objects\":{},\"candidate_bytes\":{},\"inserted_objects\":{},\"inserted_bytes_total\":{},\"reused_objects\":{},\"reused_bytes\":{},\"admission_transactions\":{},\"max_transaction_objects\":{},\"max_transaction_bytes\":{},\"initialization_candidate_objects\":{},\"initialization_candidate_bytes\":{},\"scanned_files\":{},\"scanned_bytes\":{},\"commit_payload_bytes_read\":{},\"commit_cdc_bytes_scanned\":{},\"process_user_cpu_ns\":{},\"process_system_cpu_ns\":{},\"process_disk_read_bytes\":{},\"process_disk_write_bytes\":{},\"process_context_switches\":{},\"process_peak_rss_bytes\":{},\"process_physical_footprint_bytes\":{},\"container_memory_current_bytes\":{},\"container_memory_peak_bytes\":{},\"container_pids_current\":{},\"store_baseline_bytes\":{},\"store_database_bytes\":{},\"swap_bytes\":0,\"oom\":false,\"timeout\":false,\"verification_status\":\"not-run-performance-mode\",\"cleanup_status\":\"pass\"}}",
        workload_source::edit_count_changing::PERFORMANCE_SCHEMA,
        workload_source::edit_count_changing::FAMILY_ID,
        scenario.id,
        scenario.display_name,
        scenario.paired_same_count_control_id,
        workload_source::edit_count_changing::FIXTURE_BYTES,
        if matches!(
            scenario.kind,
            workload_source::edit_count_changing::Kind::Delete
                | workload_source::edit_count_changing::Kind::Truncate
        ) {
            "deleted_bytes"
        } else {
            "supplied_bytes"
        },
        source,
        seed,
        workload_source::edit_count_changing::FIXTURE_PROFILE,
        fixture_cache_profile,
        scenario.kind.operation(),
        scenario.kind.position(),
        if scenario.kind.temp_copy() { "temp-copy-fsync-rename" } else { "direct-posix" },
        scenario.operations,
        attempted,
        completed,
        workload_source::edit_count_changing::FIXTURE_BYTES,
        final_bytes,
        initial_inode,
        final_inode,
        if scenario.kind.temp_copy() { "replaced" } else { "preserved" },
        supplied,
        inserted,
        deleted,
        overlapping,
        superseded,
        logical_zero,
        copied_payload,
        read_payload,
        fuse.kernel_read_requests,
        fuse.kernel_read_bytes,
        fuse.read_ahead_hits,
        fuse.read_ahead_misses,
        fuse.read_ahead_fetches,
        fuse.read_ahead_requested_bytes,
        fuse.read_ahead_fetched_bytes,
        fuse.read_ahead_served_bytes,
        fuse.read_ahead_unused_bytes,
        layerstack_init_ns,
        branch_fork_ns,
        nanos(t0, t1),
        execution_ns,
        inner_edit_ns,
        nanos(t2, commit_return),
        nanos(commit_return, t3),
        nanos(t2, t3),
        nanos(t0, t3),
        nanos(t3, t4),
        nanos(t0, t4),
        rate(completed, inner_edit_ns),
        rate(supplied, inner_edit_ns),
        rate(copied_payload, inner_edit_ns),
        fuse.max_write_bytes,
        fuse.kernel_write_requests,
        fuse.kernel_write_bytes,
        fuse.client_request_copy_bytes,
        fuse.frame_payload_copy_bytes,
        fuse.client_frame_bytes,
        fuse.host_frame_bytes,
        fuse.host_decode_copy_bytes,
        fuse.spool_write_bytes,
        fuse.spool_write_open_count,
        commit.edit_spool_allocated_bytes,
        commit.edit_spool_peak_bytes,
        commit.edit_spool_live_bytes,
        commit.edit_spool_superseded_bytes,
        commit.edit_piece_count,
        commit.edit_piece_height,
        commit.edit_piece_logical_charge,
        commit.edit_tree_visits,
        commit.edit_metric_nodes_scanned,
        commit.total_ns,
        commit.pause_fence_ns,
        commit.quiesce_ns,
        commit.capture_ns,
        commit.candidate_plan_ns,
        commit.dirty_compare_ns,
        commit.content_ns,
        commit.namespace_ns,
        commit.candidate_finish_ns,
        commit.local_admission_ns,
        commit.object_admission_ns,
        commit.publication_ns,
        commit.in_place_rebase_ns,
        commit.resume_ns,
        commit.unattributed_ns,
        candidate.candidate_objects,
        candidate.candidate_bytes,
        candidate.inserted_objects,
        candidate.inserted_bytes,
        candidate.reused_objects,
        candidate.reused_bytes,
        candidate.admission_transactions,
        candidate.max_transaction_objects,
        candidate.max_transaction_bytes,
        initialize_candidate.candidate_objects,
        initialize_candidate.candidate_bytes,
        scan.scanned_files,
        scan.scanned_bytes,
        commit.payload_bytes_read,
        commit.cdc_bytes_scanned,
        resource_delta(process_after.user_cpu_ns, process_before.user_cpu_ns, "count-changing user CPU")?,
        resource_delta(process_after.system_cpu_ns, process_before.system_cpu_ns, "count-changing system CPU")?,
        resource_delta(process_after.disk_read_bytes, process_before.disk_read_bytes, "count-changing disk read")?,
        resource_delta(process_after.disk_write_bytes, process_before.disk_write_bytes, "count-changing disk write")?,
        resource_delta(process_after.context_switches, process_before.context_switches, "count-changing context switches")?,
        process_after.peak_resident_bytes,
        process_after.physical_footprint_bytes,
        container_after.memory_current,
        container_after.memory_peak,
        container_after.pids_current,
        store_baseline_bytes,
        std::fs::metadata(&store_path)?.len(),
    );
    Ok(())
}

fn count_changing_anchor_performance_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    scenario: workload_source::edit_count_changing::Scenario,
    seed: u8,
    source: &str,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    if scenario.id != "prepend-temp-copy-rename"
        || !workload_source::edit_count_changing::SEEDS.contains(&seed)
        || !matches!(source, "baseline" | "candidate" | "repeat-a" | "repeat-b")
        || root.exists()
        || !fixture.is_dir()
        || std::fs::metadata(fixture.join("payload.bin"))?.len() != MIB_32
    {
        return Err("count-changing anchor arguments".into());
    }
    std::fs::create_dir(root)?;
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect(store.clone())?;
    let process_before = process_resource_snapshot()?;
    let container_before = container_cgroup_snapshot(&container_id)?;
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("{}-{source}-{seed}", scenario.id))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let scan_receipts = store.take_layerstack_initialization_receipts();
    let [scan] = scan_receipts.as_slice() else {
        return Err("count-changing anchor initialization receipt".into());
    };
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let t0 = Instant::now();
    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: count_changing_placement(&container_id, scenario, seed, "performance"),
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let t1 = Instant::now();
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let output = execute_workload(
        &client,
        workspace.id,
        vec![
            workload,
            OsString::from("prepend"),
            OsString::from("payload.bin"),
        ],
    )?;
    let t2 = Instant::now();
    let attempted = output_u64(&output, "attempted_operations")?;
    let completed = output_u64(&output, "completed_operations")?;
    let final_bytes = output_u64(&output, "final_file_bytes")?;
    let initial_inode = output_u64(&output, "initial_inode")?;
    let final_inode = output_u64(&output, "final_inode")?;
    if attempted != 1
        || completed != 1
        || final_bytes != MIB_32 + 10
        || initial_inode == final_inode
    {
        return Err("count-changing anchor observed workload".into());
    }
    let head = match client.commit_workspace_session(workspace.id)? {
        WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
        result => return Err(format!("count-changing anchor Commit failed: {result:?}").into()),
    };
    let commit_return = Instant::now();
    visible_head(&client, branch, head)?;
    let t3 = Instant::now();
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    let t4 = Instant::now();
    let process_after = process_resource_snapshot()?;
    let container_after = container_cgroup_snapshot(&container_id)?;
    if process_before.swaps != 0
        || process_after.swaps != 0
        || container_before.swap_current != 0
        || container_after.swap_current != 0
        || container_before.oom != container_after.oom
        || container_before.oom_kill != container_after.oom_kill
        || process_after.peak_resident_bytes > 128 * 1024 * 1024
        || container_after.memory_peak > 128 * 1024 * 1024
        || client.active_workspace_count()? != 0
        || client.active_execution_count()? != 0
    {
        return Err("count-changing anchor resource or cleanup".into());
    }
    let snapshot = client.monitor_snapshot()?;
    let candidate = operation_candidate(&snapshot, OperationFamily::WorkspaceCommit)?;
    let commit = operation_workspace_commit(&snapshot)?;
    let fuse = edit_fuse_metrics(&snapshot);
    if fuse.kernel_write_bytes != MIB_32 + 10
        || fuse.spool_write_bytes != MIB_32 + 10
        || commit.edit_spool_allocated_bytes != MIB_32 + 10
        || commit.edit_spool_peak_bytes != MIB_32 + 10
        || commit.edit_spool_live_bytes != MIB_32 + 10
        || commit.edit_spool_superseded_bytes != 0
    {
        return Err("count-changing anchor FUSE/spool receipt".into());
    }
    let execution_ns = nanos(t1, t2);
    let complete = nanos(t0, t4);
    emit_sample(
        "prepend-temp-copy-rename",
        usize::from(seed),
        &LifecycleSample {
            workspace_create_ns: nanos(t0, t1),
            execution_ns,
            commit_api_ns: nanos(t2, t3),
            layerstack_visible_ns: nanos(t0, t3),
            workspace_end_ns: nanos(t3, t4),
            complete_lifecycle_ns: complete,
            inner_write_ns: None,
        },
    );
    println!(
        "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"scenario_id\":\"{}\",\"display_name\":\"{}\",\"paired_same_count_control_id\":\"{}\",\"pair_fixture_bytes\":{},\"pair_byte_quantity_match\":null,\"pair_byte_quantity_basis\":\"not-applicable-frozen-anchor\",\"mode\":\"performance\",\"source_arm\":\"{}\",\"seed\":{},\"legacy_schema_emitted\":true,\"fixture_profile\":\"registered-32m-v0.1.0\",\"fixture_cache_profile\":\"{}\",\"operation\":\"prepend\",\"position\":\"head\",\"implementation\":\"temp-copy-fsync-rename\",\"operation_count\":1,\"attempted_operations\":1,\"completed_operations\":1,\"initial_file_bytes\":{},\"final_file_bytes\":{},\"initial_inode\":{},\"final_inode\":{},\"inode_behavior\":\"replaced\",\"supplied_bytes\":10,\"inserted_bytes\":10,\"deleted_bytes\":0,\"overlapping_bytes\":0,\"superseded_bytes\":0,\"logical_zero_bytes\":0,\"copied_payload_bytes\":{},\"read_payload_bytes\":{},\"fuse_kernel_read_requests\":{},\"fuse_kernel_read_bytes\":{},\"read_ahead_hits\":{},\"read_ahead_misses\":{},\"read_ahead_fetches\":{},\"read_ahead_requested_bytes\":{},\"read_ahead_fetched_bytes\":{},\"read_ahead_served_bytes\":{},\"read_ahead_unused_bytes\":{},\"workspace_create_ns\":{},\"execution_ns\":{},\"inner_edit_ns\":{},\"commit_call_ns\":{},\"visibility_ack_ns\":{},\"commit_api_ns\":{},\"layerstack_visible_ns\":{},\"workspace_end_ns\":{},\"complete_lifecycle_ns\":{},\"operations_per_second\":{},\"supplied_bytes_per_second\":{},\"copied_payload_bytes_per_second\":{},\"fuse_kernel_write_requests\":{},\"fuse_kernel_write_bytes\":{},\"spool_write_bytes\":{},\"spool_allocated_bytes\":{},\"physical_spool_high_water_bytes\":{},\"spool_live_bytes\":{},\"spool_superseded_bytes\":{},\"piece_count\":{},\"piece_height\":{},\"piece_logical_charge_bytes\":{},\"tree_visits\":{},\"metric_nodes_scanned\":{},\"forbidden_path_runtime_counters\":\"not-applicable-no-entry-points\",\"forbidden_path_static_proof\":\"implicit-offset-persistent-piece-tree\",\"commit_total_ns\":{},\"candidate_objects\":{},\"candidate_bytes\":{},\"inserted_objects\":{},\"inserted_bytes_total\":{},\"reused_objects\":{},\"reused_bytes\":{},\"max_transaction_objects\":{},\"max_transaction_bytes\":{},\"scanned_files\":{},\"scanned_bytes\":{},\"commit_payload_bytes_read\":{},\"commit_cdc_bytes_scanned\":{},\"process_peak_rss_bytes\":{},\"process_physical_footprint_bytes\":{},\"container_memory_peak_bytes\":{},\"swap_bytes\":0,\"oom\":false,\"timeout\":false,\"verification_status\":\"not-run-performance-mode\",\"cleanup_status\":\"pass\"}}",
        workload_source::edit_count_changing::PERFORMANCE_SCHEMA,
        workload_source::edit_count_changing::FAMILY_ID,
        scenario.id,
        scenario.display_name,
        scenario.paired_same_count_control_id,
        MIB_32,
        source,
        seed,
        fixture_cache_profile,
        MIB_32,
        final_bytes,
        initial_inode,
        final_inode,
        MIB_32,
        MIB_32,
        fuse.kernel_read_requests,
        fuse.kernel_read_bytes,
        fuse.read_ahead_hits,
        fuse.read_ahead_misses,
        fuse.read_ahead_fetches,
        fuse.read_ahead_requested_bytes,
        fuse.read_ahead_fetched_bytes,
        fuse.read_ahead_served_bytes,
        fuse.read_ahead_unused_bytes,
        nanos(t0, t1),
        execution_ns,
        execution_ns,
        nanos(t2, commit_return),
        nanos(commit_return, t3),
        nanos(t2, t3),
        nanos(t0, t3),
        nanos(t3, t4),
        complete,
        rate(1, execution_ns),
        rate(10, execution_ns),
        rate(MIB_32, execution_ns),
        fuse.kernel_write_requests,
        fuse.kernel_write_bytes,
        fuse.spool_write_bytes,
        commit.edit_spool_allocated_bytes,
        commit.edit_spool_peak_bytes,
        commit.edit_spool_live_bytes,
        commit.edit_spool_superseded_bytes,
        commit.edit_piece_count,
        commit.edit_piece_height,
        commit.edit_piece_logical_charge,
        commit.edit_tree_visits,
        commit.edit_metric_nodes_scanned,
        commit.total_ns,
        candidate.candidate_objects,
        candidate.candidate_bytes,
        candidate.inserted_objects,
        candidate.inserted_bytes,
        candidate.reused_objects,
        candidate.reused_bytes,
        candidate.max_transaction_objects,
        candidate.max_transaction_bytes,
        scan.scanned_files,
        scan.scanned_bytes,
        commit.payload_bytes_read,
        commit.cdc_bytes_scanned,
        process_after.peak_resident_bytes,
        process_after.physical_footprint_bytes,
        container_after.memory_peak,
    );
    if output.receipt.is_none() {
        return Err("count-changing anchor execution receipt".into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn count_changing_verify_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    case_id: &str,
    seed: u8,
    source: &str,
    expected_digest: &str,
    expected_size: u64,
    fixture_cache_profile: &str,
    structural: bool,
) -> AnyResult<()> {
    let scenario = if structural {
        if !workload_source::edit_count_changing::VERIFIERS.contains(&case_id) {
            return Err("unknown count-changing verifier".into());
        }
        None
    } else {
        let scenario = workload_source::edit_count_changing::scenario(case_id)?;
        Some(scenario)
    };
    let initial_size = if scenario.is_some_and(|scenario| scenario.frozen) {
        MIB_32
    } else if structural {
        8 * 1024 * 1024
    } else {
        workload_source::edit_count_changing::FIXTURE_BYTES
    };
    if !workload_source::edit_count_changing::SEEDS.contains(&seed)
        || !matches!(source, "baseline" | "candidate" | "repeat-a" | "repeat-b")
        || root.exists()
        || !fixture.is_dir()
        || std::fs::metadata(fixture.join("payload.bin"))?.len() != initial_size
    {
        return Err("count-changing verification arguments".into());
    }
    std::fs::create_dir(root)?;
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite"))?);
    let client = Client::connect(store.clone())?;
    let process_before = process_resource_snapshot()?;
    let container_before = container_cgroup_snapshot(&container_id)?;
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("count-changing-verify-{seed}"))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let base = store.pin_branch(branch)?;
    let old_payload_ids = payload_ids(&base.reader, base.root, "payload.bin")?;
    let base_core = layerfs_layerstack_store::CoreReader(&base.reader);
    let base_path = layerfs_content::CanonicalPath::new("payload.bin")?;
    let (base_stat, _) = layerfs_content::filesystem::stat(&base_core, base.root, &base_path)?;
    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: WorkspacePlacement::Container {
            container_id: container_id.clone(),
            root: PathBuf::from(format!(
                "/workspace/layerfs-verify-{case_id}-{seed}-{}",
                std::process::id()
            )),
        },
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let mut argv = vec![workload.clone()];
    if scenario.is_some_and(|scenario| scenario.frozen) {
        argv.extend([OsString::from("prepend"), OsString::from("payload.bin")]);
    } else if structural {
        argv.extend([
            OsString::from("count-changing-proof"),
            OsString::from("payload.bin"),
            OsString::from(case_id),
        ]);
    } else {
        argv.extend([
            OsString::from("count-changing-edit"),
            OsString::from("payload.bin"),
            OsString::from(case_id),
            OsString::from(seed.to_string()),
        ]);
    }
    let output = execute_workload(&client, workspace.id, argv)?;
    let initial_inode = output_u64(&output, "initial_inode")?;
    let final_inode = output_u64(&output, "final_inode")?;
    if output_u64(&output, "attempted_operations")? == 0
        || output_u64(&output, "attempted_operations")?
            != output_u64(&output, "completed_operations")?
        || output_u64(&output, "final_file_bytes")? != expected_size
        || scenario
            .is_some_and(|scenario| scenario.kind.temp_copy() == (initial_inode == final_inode))
        || (structural && initial_inode == final_inode)
    {
        return Err("count-changing verifier workload".into());
    }
    let head = match client.commit_workspace_session(workspace.id)? {
        WorkspaceCommitResult::Created { commit_id, .. } => commit_id,
        result => return Err(format!("count-changing verifier Commit failed: {result:?}").into()),
    };
    let snapshot = client.monitor_snapshot()?;
    let receipt = operation_workspace_commit(&snapshot)?;
    let candidate = operation_candidate(&snapshot, OperationFamily::WorkspaceCommit)?;
    if structural && case_id.starts_with("rewrite-full") && receipt.payload_bytes_read != 0 {
        return Err("count-changing full replacement read old payload".into());
    }
    if receipt.edit_spool_peak_bytes > 128 * 1024 * 1024 {
        return Err("count-changing verifier spool peak".into());
    }
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    let committed = store
        .commit(head)?
        .ok_or("count-changing verifier commit")?;
    let pinned = store.pin_branch(branch)?;
    let new_payload_ids = payload_ids(&pinned.reader, committed.root_id, "payload.bin")?;
    let core = layerfs_layerstack_store::CoreReader(&pinned.reader);
    let path = layerfs_content::CanonicalPath::new("payload.bin")?;
    let (stat, _) = layerfs_content::filesystem::stat(&core, committed.root_id, &path)?;
    let oracle_file = std::env::var_os("LAYERFS_BENCH_ORACLE_FILE")
        .ok_or("count-changing independent oracle file")?;
    let direct = scenario.filter(|scenario| !scenario.kind.temp_copy());
    let (oracle_base, edit_start, delete_len, oracle_offset) = match direct.map(|row| row.kind) {
        Some(workload_source::edit_count_changing::Kind::Truncate) => (
            Some(layerfs_content::file::rope::FileStateRoot(
                base_stat.content_root,
            )),
            expected_size,
            initial_size - expected_size,
            expected_size,
        ),
        Some(
            workload_source::edit_count_changing::Kind::Append
            | workload_source::edit_count_changing::Kind::Sparse,
        ) => (
            Some(layerfs_content::file::rope::FileStateRoot(
                base_stat.content_root,
            )),
            initial_size,
            0,
            initial_size,
        ),
        _ => (None, 0, 0, 0),
    };
    let expected_file_root = independent_file_edit_root(
        &pinned.reader,
        oracle_base,
        Path::new(&oracle_file),
        edit_start,
        delete_len,
        oracle_offset,
    )?;
    if stat.content_root != expected_file_root.0 {
        return Err("count-changing independent canonical root".into());
    }
    let reopened = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: WorkspacePlacement::Container {
            container_id: container_id.clone(),
            root: PathBuf::from(format!(
                "/workspace/layerfs-reopen-{case_id}-{seed}-{}",
                std::process::id()
            )),
        },
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let digest_output = execute_workload(
        &client,
        reopened.id,
        vec![
            workload,
            OsString::from("digest"),
            OsString::from("payload.bin"),
        ],
    )?;
    let (reopened_size, reopened_digest) = parse_digest(&digest_output)?;
    if reopened_size != expected_size
        || reopened_digest != expected_digest
        || store.pin_branch(branch)?.root != committed.root_id
    {
        return Err("count-changing verifier reopen".into());
    }
    client.end_workspace_session(reopened.id, EndWorkspaceMode::Clean)?;
    let process_after = process_resource_snapshot()?;
    let container_after = container_cgroup_snapshot(&container_id)?;
    if process_before.swaps != 0
        || process_after.swaps != 0
        || container_before.swap_current != 0
        || container_after.swap_current != 0
        || container_before.oom != container_after.oom
        || container_before.oom_kill != container_after.oom_kill
        || process_after.peak_resident_bytes > 128 * 1024 * 1024
        || container_after.memory_peak > 128 * 1024 * 1024
        || client.active_workspace_count()? != 0
        || client.active_execution_count()? != 0
    {
        return Err("count-changing verifier resource or cleanup".into());
    }
    let logical_zero_bytes_verified = scenario
        .filter(|scenario| scenario.kind == workload_source::edit_count_changing::Kind::Sparse)
        .map(|scenario| {
            workload_source::edit_count_changing::schedule(scenario, seed)
                .map(|edits| edits.into_iter().map(|edit| edit.logical_zero as u64).sum())
        })
        .transpose()?
        .unwrap_or(0);
    println!(
        "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"verifier_id\":\"{}\",\"source_arm\":\"{}\",\"seed\":{},\"fixture_cache_profile\":\"{}\",\"initial_file_bytes\":{},\"final_file_bytes\":{},\"logical_zero_bytes_verified\":{},\"sha256\":\"{}\",\"initial_inode\":{},\"final_inode\":{},\"inode_behavior\":\"{}\",\"canonical_root\":\"{}\",\"canonical_file_root\":\"{}\",\"independent_canonical_file_root\":\"{}\",\"old_payload_object_ids\":{},\"old_payload_object_ids_retained\":{},\"commit_payload_bytes_read\":{},\"commit_cdc_bytes_scanned\":{},\"piece_count\":{},\"piece_height\":{},\"piece_logical_charge_bytes\":{},\"spool_allocated_bytes\":{},\"physical_spool_high_water_bytes\":{},\"spool_live_bytes\":{},\"spool_superseded_bytes\":{},\"candidate_objects\":{},\"candidate_bytes\":{},\"inserted_objects\":{},\"reused_objects\":{},\"max_transaction_objects\":{},\"max_transaction_bytes\":{},\"process_peak_rss_bytes\":{},\"container_memory_peak_bytes\":{},\"swap_bytes\":0,\"oom\":false,\"independent_oracle\":true,\"fresh_fuse_reopen\":true,\"performance_distribution\":false,\"cleanup_status\":\"pass\",\"status\":\"pass\"}}",
        workload_source::edit_count_changing::VERIFICATION_SCHEMA,
        workload_source::edit_count_changing::FAMILY_ID,
        case_id,
        source,
        seed,
        fixture_cache_profile,
        initial_size,
        expected_size,
        logical_zero_bytes_verified,
        expected_digest,
        initial_inode,
        final_inode,
        if structural || scenario.is_some_and(|scenario| scenario.kind.temp_copy()) {
            "replaced"
        } else {
            "preserved"
        },
        committed.root_id,
        stat.content_root,
        expected_file_root.0,
        old_payload_ids.len(),
        old_payload_ids.intersection(&new_payload_ids).count(),
        receipt.payload_bytes_read,
        receipt.cdc_bytes_scanned,
        receipt.edit_piece_count,
        receipt.edit_piece_height,
        receipt.edit_piece_logical_charge,
        receipt.edit_spool_allocated_bytes,
        receipt.edit_spool_peak_bytes,
        receipt.edit_spool_live_bytes,
        receipt.edit_spool_superseded_bytes,
        candidate.candidate_objects,
        candidate.candidate_bytes,
        candidate.inserted_objects,
        candidate.reused_objects,
        candidate.max_transaction_objects,
        candidate.max_transaction_bytes,
        process_after.peak_resident_bytes,
        container_after.memory_peak,
    );
    Ok(())
}

fn independent_file_edit_root(
    reader: &layerfs_layerstack_store::SnapshotReader,
    base: Option<layerfs_content::file::rope::FileStateRoot>,
    oracle: &Path,
    start: u64,
    delete_len: u64,
    oracle_offset: u64,
) -> AnyResult<layerfs_content::file::rope::FileStateRoot> {
    let mut objects = layerfs_layerstack_store::ObjectBuffer::new(reader)?;
    let mut batch = layerfs_content::file::rope::FileMutationBatch::new(&mut objects, base)?;
    let mut replacement = std::fs::File::open(oracle)?;
    replacement.seek(SeekFrom::Start(oracle_offset))?;
    batch.replace(start, delete_len, replacement)?;
    let (root, _) = batch.finish()?;
    Ok(root)
}

#[allow(clippy::too_many_arguments)]
fn same_count_anchor_performance_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    scenario: workload_source::edit_same_count::Scenario,
    seed: u8,
    source: &str,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    if !scenario.frozen
        || !workload_source::edit_same_count::SEEDS.contains(&seed)
        || !matches!(source, "baseline" | "candidate" | "repeat-a" | "repeat-b")
        || fixture_cache_profile.is_empty()
        || root.exists()
        || std::fs::metadata(fixture.join("payload.bin"))?.len() != MIB_32
    {
        return Err("same-count anchor arguments".into());
    }
    let (client, branch) = case_client(
        root,
        &format!("{}-{source}-{seed}", scenario.id),
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let mut oracle = std::fs::read(fixture.join("payload.bin"))?;
    let mut coverage = vec![false; MIB_32 as usize];
    let mut identical = 0_u64;
    for operation in 0..scenario.operations {
        let index = if scenario.id == "small-edit" {
            0_u64
        } else {
            operation as u64 + 1
        };
        let marker = format!("E{:09}", index + 1);
        let offset = (index + 1) * 2_654_435_761 % (MIB_32 - 10);
        let range = offset as usize..offset as usize + 10;
        identical += oracle[range.clone()]
            .iter()
            .zip(marker.as_bytes())
            .filter(|(left, right)| left == right)
            .count() as u64;
        oracle[range.clone()].copy_from_slice(marker.as_bytes());
        coverage[range].fill(true);
    }
    let unique = coverage.into_iter().filter(|covered| *covered).count() as u64;
    drop(oracle);
    let process_before = process_resource_snapshot()?;
    let container_before = container_cgroup_snapshot(&container_id)?;
    let t0 = Instant::now();
    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: same_count_placement(&container_id, scenario, seed, "performance"),
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let t1 = Instant::now();
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let mut execution_ns = 0_u64;
    let mut commit_call_ns = 0_u64;
    let mut visibility_ack_ns = 0_u64;
    for operation in 0..scenario.operations {
        let started = Instant::now();
        let output = execute_workload(
            &client,
            workspace.id,
            vec![
                workload.clone(),
                OsString::from("edit"),
                OsString::from("payload.bin"),
                OsString::from(if scenario.id == "small-edit" {
                    "0".to_owned()
                } else {
                    (operation + 1).to_string()
                }),
                OsString::from(MIB_32.to_string()),
            ],
        )?;
        execution_ns = execution_ns.saturating_add(elapsed_ns(started));
        if output.receipt.is_none() {
            return Err("same-count anchor execution receipt".into());
        }
        let started = Instant::now();
        let head = match client.commit_workspace_session(workspace.id)? {
            WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
            result => return Err(format!("same-count anchor Commit failed: {result:?}").into()),
        };
        commit_call_ns = commit_call_ns.saturating_add(elapsed_ns(started));
        let started = Instant::now();
        visible_head(&client, branch, head)?;
        visibility_ack_ns = visibility_ack_ns.saturating_add(elapsed_ns(started));
    }
    let t3 = Instant::now();
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    let t4 = Instant::now();
    let process_after = process_resource_snapshot()?;
    let container_after = container_cgroup_snapshot(&container_id)?;
    if process_before.swaps != 0
        || process_after.swaps != 0
        || container_before.swap_current != 0
        || container_after.swap_current != 0
        || container_before.oom != container_after.oom
        || container_before.oom_kill != container_after.oom_kill
        || client.active_workspace_count()? != 0
        || client.active_execution_count()? != 0
    {
        return Err("same-count anchor resource or cleanup".into());
    }
    let snapshot = client.monitor_snapshot()?;
    let commits = snapshot
        .operations
        .iter()
        .filter(|operation| operation.operation.family == OperationFamily::WorkspaceCommit)
        .flat_map(|operation| operation.storage.iter())
        .filter_map(|receipt| match receipt {
            StorageReceipt::WorkspaceCommit(receipt) => Some(*receipt),
            _ => None,
        })
        .collect::<Vec<_>>();
    let fuses = snapshot
        .operations
        .iter()
        .flat_map(|operation| operation.storage.iter())
        .filter_map(|receipt| match receipt {
            StorageReceipt::FuseWrite(receipt) => Some(*receipt),
            _ => None,
        })
        .collect::<Vec<_>>();
    let candidates = snapshot
        .operations
        .iter()
        .filter(|operation| operation.operation.family == OperationFamily::WorkspaceCommit)
        .filter_map(|operation| operation.candidate)
        .collect::<Vec<_>>();
    if commits.len() != scenario.operations
        || fuses.len() != scenario.operations
        || candidates.len() != scenario.operations
    {
        return Err("same-count anchor receipt cardinality".into());
    }
    let complete = nanos(t0, t4);
    let sample = LifecycleSample {
        workspace_create_ns: nanos(t0, t1),
        execution_ns,
        commit_api_ns: commit_call_ns.saturating_add(visibility_ack_ns),
        layerstack_visible_ns: nanos(t0, t3),
        workspace_end_ns: nanos(t3, t4),
        complete_lifecycle_ns: complete,
        inner_write_ns: None,
    };
    if scenario.id == "small-edit" {
        emit_sample("small-edit", usize::from(seed), &sample);
    } else {
        println!(
            "{{\"schema\":\"fs-bench-pro-v4\",\"case\":\"edit16\",\"iteration\":{},\"complete_lifecycle_ns\":{complete}}}",
            seed
        );
    }
    let sum = |values: Vec<u64>| values.into_iter().fold(0_u64, u64::saturating_add);
    let supplied = scenario.operations as u64 * 10;
    println!(
        "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"scenario_id\":\"{}\",\"display_name\":\"{}\",\"mode\":\"performance\",\"source_arm\":\"{}\",\"seed\":{},\"legacy_schema_emitted\":true,\"fixture_profile\":\"registered-32m-v0.1.0\",\"fixture_cache_profile\":\"{}\",\"operation\":\"overwrite\",\"position\":\"distributed\",\"operation_count\":{},\"attempted_operations\":{},\"completed_operations\":{},\"initial_file_bytes\":{},\"final_file_bytes\":{},\"supplied_bytes\":{},\"unique_bytes\":{},\"overlapping_bytes\":{},\"identical_bytes\":{},\"superseded_bytes\":{},\"workspace_create_ns\":{},\"execution_ns\":{},\"commit_call_ns\":{},\"visibility_ack_ns\":{},\"commit_api_ns\":{},\"layerstack_visible_ns\":{},\"workspace_end_ns\":{},\"complete_lifecycle_ns\":{},\"operations_per_second\":{},\"supplied_bytes_per_second\":{},\"fuse_kernel_write_requests\":{},\"fuse_kernel_write_bytes\":{},\"spool_write_bytes\":{},\"spool_allocated_bytes\":{},\"spool_live_bytes_peak\":{},\"spool_superseded_bytes\":{},\"piece_count_peak\":{},\"piece_height_peak\":{},\"piece_logical_charge_bytes_peak\":{},\"tree_visits\":{},\"metric_nodes_scanned\":{},\"forbidden_path_runtime_counters\":\"not-applicable-no-entry-points\",\"forbidden_path_static_proof\":\"implicit-offset-persistent-piece-tree\",\"commit_total_ns\":{},\"candidate_objects\":{},\"candidate_bytes\":{},\"inserted_objects\":{},\"inserted_bytes_total\":{},\"reused_objects\":{},\"reused_bytes\":{},\"max_transaction_objects\":{},\"max_transaction_bytes\":{},\"commit_payload_bytes_read\":{},\"commit_cdc_bytes_scanned\":{},\"process_peak_rss_bytes\":{},\"process_physical_footprint_bytes\":{},\"container_memory_peak_bytes\":{},\"swap_bytes\":0,\"oom\":false,\"timeout\":false,\"verification_status\":\"not-run-performance-mode\",\"cleanup_status\":\"pass\"}}",
        workload_source::edit_same_count::PERFORMANCE_SCHEMA,
        workload_source::edit_same_count::FAMILY_ID,
        scenario.id,
        scenario.display_name,
        source,
        seed,
        fixture_cache_profile,
        scenario.operations,
        scenario.operations,
        scenario.operations,
        MIB_32,
        MIB_32,
        supplied,
        unique,
        supplied - unique,
        identical,
        supplied - unique,
        sample.workspace_create_ns,
        execution_ns,
        commit_call_ns,
        visibility_ack_ns,
        sample.commit_api_ns,
        sample.layerstack_visible_ns,
        sample.workspace_end_ns,
        complete,
        rate(scenario.operations as u64, execution_ns),
        rate(supplied, execution_ns),
        sum(fuses.iter().map(|receipt| receipt.kernel_write_requests).collect()),
        sum(fuses.iter().map(|receipt| receipt.kernel_write_bytes).collect()),
        sum(fuses.iter().map(|receipt| receipt.spool_write_bytes).collect()),
        sum(commits.iter().map(|receipt| receipt.edit_spool_allocated_bytes).collect()),
        commits.iter().map(|receipt| receipt.edit_spool_live_bytes).max().unwrap_or(0),
        sum(commits.iter().map(|receipt| receipt.edit_spool_superseded_bytes).collect()),
        commits.iter().map(|receipt| receipt.edit_piece_count).max().unwrap_or(0),
        commits.iter().map(|receipt| receipt.edit_piece_height).max().unwrap_or(0),
        commits.iter().map(|receipt| receipt.edit_piece_logical_charge).max().unwrap_or(0),
        sum(commits.iter().map(|receipt| receipt.edit_tree_visits).collect()),
        sum(commits
            .iter()
            .map(|receipt| receipt.edit_metric_nodes_scanned)
            .collect()),
        sum(commits.iter().map(|receipt| receipt.total_ns).collect()),
        sum(candidates.iter().map(|candidate| candidate.candidate_objects).collect()),
        sum(candidates.iter().map(|candidate| candidate.candidate_bytes).collect()),
        sum(candidates.iter().map(|candidate| candidate.inserted_objects).collect()),
        sum(candidates.iter().map(|candidate| candidate.inserted_bytes).collect()),
        sum(candidates.iter().map(|candidate| candidate.reused_objects).collect()),
        sum(candidates.iter().map(|candidate| candidate.reused_bytes).collect()),
        candidates.iter().map(|candidate| candidate.max_transaction_objects).max().unwrap_or(0),
        candidates.iter().map(|candidate| candidate.max_transaction_bytes).max().unwrap_or(0),
        sum(commits.iter().map(|receipt| receipt.payload_bytes_read).collect()),
        sum(commits.iter().map(|receipt| receipt.cdc_bytes_scanned).collect()),
        process_after.peak_resident_bytes,
        process_after.physical_footprint_bytes,
        container_after.memory_peak,
    );
    if scenario.id == "small-edit" && commits[0].total_ns > SMALL_COMMIT_HARD_NS {
        return Err("small-edit anchor hard gate".into());
    }
    if scenario.id == "edit16" && complete > EDIT16_HARD_NS {
        return Err("edit16 anchor hard gate".into());
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn same_count_verify_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    scenario_id: &str,
    seed: u8,
    source: &str,
    expected_digest: &str,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    let scenario = workload_source::edit_same_count::scenario(scenario_id)?;
    let expected_size = if scenario.frozen {
        MIB_32
    } else {
        workload_source::edit_same_count::FIXTURE_BYTES
    };
    if !workload_source::edit_same_count::SEEDS.contains(&seed)
        || !matches!(source, "baseline" | "candidate" | "repeat-a" | "repeat-b")
        || !valid_digest(expected_digest)
        || fixture_cache_profile.is_empty()
        || root.exists()
        || std::fs::metadata(fixture.join("payload.bin"))?.len() != expected_size
    {
        return Err("same-count verification arguments".into());
    }
    let (client, branch) = case_client(
        root,
        &format!("verify-{}-{source}-{seed}", scenario.id),
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: same_count_placement(&container_id, scenario, seed, "verify-prepare"),
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let commit_id = if scenario.frozen {
        let mut head = None;
        for operation in 0..scenario.operations {
            execute_workload(
                &client,
                workspace.id,
                vec![
                    workload.clone(),
                    OsString::from("edit"),
                    OsString::from("payload.bin"),
                    OsString::from(if scenario.id == "small-edit" {
                        "0".to_owned()
                    } else {
                        (operation + 1).to_string()
                    }),
                    OsString::from(MIB_32.to_string()),
                ],
            )?;
            head = match client.commit_workspace_session(workspace.id)? {
                WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
                result => {
                    return Err(format!("same-count verifier Commit failed: {result:?}").into())
                }
            };
        }
        head.ok_or("same-count verifier anchor head")?
    } else {
        let output = execute_workload(
            &client,
            workspace.id,
            vec![
                workload.clone(),
                OsString::from("same-count-edit"),
                OsString::from("payload.bin"),
                OsString::from(scenario.id),
                OsString::from(seed.to_string()),
            ],
        )?;
        if output_u64(&output, "completed_operations")? != scenario.operations as u64
            || output_u64(&output, "final_file_bytes")? != expected_size
        {
            return Err("same-count verifier preparation".into());
        }
        match client.commit_workspace_session(workspace.id)? {
            WorkspaceCommitResult::Created { commit_id, .. } => commit_id,
            result => return Err(format!("same-count verifier Commit failed: {result:?}").into()),
        }
    };
    visible_head(&client, branch, Some(commit_id))?;
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    let committed_root = client
        .query(Query::new(QueryKind::Commits).limit(512))?
        .items
        .into_iter()
        .find_map(|item| match item {
            QueryItem::Commit(commit) if commit.id == commit_id => Some(commit.root_id),
            _ => None,
        })
        .ok_or("same-count committed root")?;
    drop(client);

    let store = Arc::new(LayerStackStore::connect(root.join("store.sqlite"))?);
    let reopened = Client::connect(store.clone())?;
    let pinned = store.pin_branch(branch)?;
    if pinned.root != committed_root || pinned.branch.head_commit_id != Some(commit_id) {
        return Err("same-count reopened root".into());
    }
    let workspace = reopened.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: same_count_placement(&container_id, scenario, seed, "verify-reopen"),
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let output = execute_workload(
        &reopened,
        workspace.id,
        vec![
            workload,
            OsString::from("verify"),
            OsString::from("payload.bin"),
            OsString::from(expected_size.to_string()),
            OsString::from(expected_digest),
        ],
    )?;
    if parse_digest(&output)? != (expected_size, expected_digest.to_owned()) {
        return Err("same-count verifier digest".into());
    }
    reopened.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    if reopened.active_workspace_count()? != 0 || reopened.active_execution_count()? != 0 {
        return Err("same-count verifier cleanup".into());
    }
    println!(
        "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"scenario_id\":\"{}\",\"mode\":\"verify\",\"source_arm\":\"{}\",\"seed\":{},\"final_file_bytes\":{},\"sha256\":\"{}\",\"canonical_root\":\"{}\",\"fresh_reconnect\":true,\"fresh_fuse_reopen\":true,\"performance_distribution\":false,\"cleanup_status\":\"pass\",\"status\":\"pass\"}}",
        workload_source::edit_same_count::VERIFICATION_SCHEMA,
        workload_source::edit_same_count::FAMILY_ID,
        scenario.id,
        source,
        seed,
        expected_size,
        expected_digest,
        committed_root,
    );
    Ok(())
}

fn same_count_fragmentation_verify(
    root: &Path,
    fixture: &Path,
    oracle: &Path,
    container_id: ContainerId,
    source: &str,
    seed: u8,
) -> AnyResult<()> {
    if root.exists()
        || !matches!(source, "baseline" | "candidate" | "repeat-a" | "repeat-b")
        || !oracle.is_dir()
        || !workload_source::edit_same_count::SEEDS.contains(&seed)
        || std::fs::metadata(fixture.join("payload.bin"))?.len()
            != workload_source::edit_same_count::FIXTURE_BYTES
    {
        return Err("same-count fragmentation verifier arguments".into());
    }
    std::fs::create_dir(root)?;
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite"))?);
    let client = Client::connect(store.clone())?;
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("fragmentation-{source}-{seed}"))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let base = store.pin_branch(client.fork_branch(
        EntityName::new("oracle-base")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?)?;
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let oracle_workload =
        std::env::var_os("LAYERFS_BENCH_ORACLE_WORKLOAD").ok_or("same-count oracle workload")?;
    let mut checkpoints = Vec::with_capacity(6);
    for cohort in ["increasing", "descending", "hotspot"] {
        for operations in [100_u64, 1_000] {
            let oracle_case = oracle.join(format!("{cohort}-{operations}"));
            let oracle_file = oracle_case.join("payload.bin");
            let (oracle_size, oracle_digest) =
                process_digest(&oracle_workload, oracle_file.as_os_str())?;
            if oracle_size != workload_source::edit_same_count::FIXTURE_BYTES {
                return Err("same-count independent oracle size".into());
            }
            let expected_file_root = independent_file_root(
                &base.reader,
                base.root,
                &oracle_file,
                &oracle_case.join("ranges.txt"),
            )?;
            let branch = client.fork_branch(
                EntityName::new(format!("{cohort}-{operations}"))?,
                LocalForkSource::Layer {
                    layer_id: initialized.genesis_layer_id,
                },
            )?;
            let workspace = client.create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: WorkspacePlacement::Container {
                    container_id: container_id.clone(),
                    root: PathBuf::from(format!(
                        "/workspace/layerfs-fragment-{cohort}-{operations}-{seed}-{}",
                        std::process::id()
                    )),
                },
                projection: Some(WorkspaceProjection::Fuse),
            })?;
            let output = execute_workload(
                &client,
                workspace.id,
                vec![
                    workload.clone(),
                    OsString::from("same-count-fragmented"),
                    OsString::from("payload.bin"),
                    OsString::from(cohort),
                    OsString::from(operations.to_string()),
                    OsString::from(seed.to_string()),
                ],
            )?;
            if output_u64(&output, "completed_operations")? != operations
                || output_u64(&output, "final_file_bytes")?
                    != workload_source::edit_same_count::FIXTURE_BYTES
            {
                return Err("same-count fragmentation workload".into());
            }
            let before = execute_workload(
                &client,
                workspace.id,
                vec![
                    workload.clone(),
                    OsString::from("digest"),
                    OsString::from("payload.bin"),
                ],
            )?;
            let (size, digest) = parse_digest(&before)?;
            if size != oracle_size || digest != oracle_digest {
                return Err("same-count independent oracle digest".into());
            }
            let commit_id = match client.commit_workspace_session(workspace.id)? {
                WorkspaceCommitResult::Created { commit_id, .. } => commit_id,
                result => return Err(format!("same-count fragmentation Commit: {result:?}").into()),
            };
            visible_head(&client, branch, Some(commit_id))?;
            client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
            let root_id = store.commit(commit_id)?.ok_or("fragment commit")?.root_id;
            let (committed_file, _) = layerfs_content::filesystem::stat(
                &layerfs_layerstack_store::CoreReader(&store.snapshot_reader(root_id)),
                root_id,
                &layerfs_content::CanonicalPath::new("payload.bin")?,
            )?;
            if committed_file.content_root != expected_file_root.0 {
                return Err("same-count independent canonical file root".into());
            }
            let snapshot = client.monitor_snapshot()?;
            let operation = snapshot
                .operations
                .iter()
                .find(|operation| {
                    operation.operation.family == OperationFamily::WorkspaceCommit
                        && operation.operation.workspace_id == Some(workspace.id)
                })
                .ok_or("same-count fragmentation operation")?;
            let receipt = operation
                .storage
                .iter()
                .find_map(|receipt| match receipt {
                    StorageReceipt::WorkspaceCommit(receipt) => Some(*receipt),
                    _ => None,
                })
                .ok_or("same-count fragmentation receipt")?;
            let reopened = client.create_workspace_session(CreateWorkspaceSession {
                branch_id: branch,
                placement: WorkspacePlacement::Container {
                    container_id: container_id.clone(),
                    root: PathBuf::from(format!(
                        "/workspace/layerfs-fragment-reopen-{cohort}-{operations}-{seed}-{}",
                        std::process::id()
                    )),
                },
                projection: Some(WorkspaceProjection::Fuse),
            })?;
            let output = execute_workload(
                &client,
                reopened.id,
                vec![
                    workload.clone(),
                    OsString::from("digest"),
                    OsString::from("payload.bin"),
                ],
            )?;
            if parse_digest(&output)? != (size, digest.clone())
                || store.pin_branch(branch)?.root != root_id
            {
                return Err("same-count fragmentation reopen".into());
            }
            client.end_workspace_session(reopened.id, EndWorkspaceMode::Clean)?;
            let checkpoint = FragmentCheckpoint {
                cohort,
                operations,
                piece_count: receipt.edit_piece_count,
                piece_height: receipt.edit_piece_height,
                piece_charge: receipt.edit_piece_logical_charge,
                tree_visits: receipt.edit_tree_visits,
                digest,
                root: root_id.to_string(),
            };
            println!(
                "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"verifier_id\":\"{}\",\"source_arm\":\"{}\",\"seed\":{},\"cohort\":\"{}\",\"operations\":{},\"piece_count\":{},\"piece_height\":{},\"piece_logical_charge_bytes\":{},\"tree_visits\":{},\"metric_nodes_scanned\":{},\"forbidden_path_runtime_counters\":\"not-applicable-no-entry-points\",\"forbidden_path_static_proof\":\"implicit-offset-persistent-piece-tree\",\"final_file_bytes\":{},\"sha256\":\"{}\",\"canonical_root\":\"{}\",\"canonical_file_root\":\"{}\",\"independent_oracle\":true,\"fresh_fuse_reopen\":true,\"performance_distribution\":false,\"status\":\"pass\"}}",
                workload_source::edit_same_count::VERIFICATION_SCHEMA,
                workload_source::edit_same_count::FAMILY_ID,
                workload_source::edit_same_count::VERIFIER_ID,
                source,
                seed,
                cohort,
                operations,
                checkpoint.piece_count,
                checkpoint.piece_height,
                checkpoint.piece_charge,
                checkpoint.tree_visits,
                receipt.edit_metric_nodes_scanned,
                size,
                checkpoint.digest,
                checkpoint.root,
                expected_file_root.0,
            );
            checkpoints.push(checkpoint);
        }
    }
    for cohort in ["increasing", "descending", "hotspot"] {
        let hundred = checkpoints
            .iter()
            .find(|row| row.cohort == cohort && row.operations == 100)
            .ok_or("fragment checkpoint 100")?;
        let thousand = checkpoints
            .iter()
            .find(|row| row.cohort == cohort && row.operations == 1_000)
            .ok_or("fragment checkpoint 1000")?;
        if thousand.piece_count > 12 * hundred.piece_count.max(1)
            || thousand.piece_height > 12 * hundred.piece_height.max(1)
            || thousand.piece_charge > 12 * hundred.piece_charge.max(1)
            || thousand.tree_visits > 18 * hundred.tree_visits.max(1)
        {
            return Err(format!("same-count fragmentation structural gate: {cohort}").into());
        }
    }
    if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
        return Err("same-count fragmentation cleanup".into());
    }
    println!(
        "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"verifier_id\":\"{}\",\"source_arm\":\"{}\",\"seed\":{},\"live_metadata_ratio_limit\":12,\"tree_visit_ratio_limit\":18,\"forbidden_path_runtime_counters\":\"not-applicable-no-entry-points\",\"forbidden_path_static_proof\":\"implicit-offset-persistent-piece-tree\",\"cleanup_status\":\"pass\",\"performance_distribution\":false,\"status\":\"pass\"}}",
        workload_source::edit_same_count::VERIFICATION_SCHEMA,
        workload_source::edit_same_count::FAMILY_ID,
        workload_source::edit_same_count::VERIFIER_ID,
        source,
        seed,
    );
    Ok(())
}

fn independent_file_root(
    reader: &layerfs_layerstack_store::SnapshotReader,
    base_root: layerfs_content::ObjectId,
    oracle_file: &Path,
    ranges_file: &Path,
) -> AnyResult<layerfs_content::file::rope::FileStateRoot> {
    let core = layerfs_layerstack_store::CoreReader(reader);
    let path = layerfs_content::CanonicalPath::new("payload.bin")?;
    let (base_file, _) = layerfs_content::filesystem::stat(&core, base_root, &path)?;
    let oracle = std::fs::read(oracle_file)?;
    let mut objects = layerfs_layerstack_store::ObjectBuffer::new(reader)?;
    let mut batch = layerfs_content::file::rope::FileMutationBatch::new(
        &mut objects,
        Some(layerfs_content::file::rope::FileStateRoot(
            base_file.content_root,
        )),
    )?;
    let mut prior_end = 0_usize;
    for line in std::fs::read_to_string(ranges_file)?.lines() {
        let (start, end) = line
            .split_once(' ')
            .ok_or("same-count independent oracle range")?;
        let start: usize = start.parse()?;
        let end: usize = end.parse()?;
        if start < prior_end || start >= end || end > oracle.len() {
            return Err("same-count independent oracle range order".into());
        }
        batch.replace(
            start as u64,
            (end - start) as u64,
            std::io::Cursor::new(&oracle[start..end]),
        )?;
        prior_end = end;
    }
    let (root, _) = batch.finish()?;
    Ok(root)
}

fn store_owned_bytes(root: &Path) -> AnyResult<(u64, u64)> {
    fn walk(path: &Path, files: &mut u64, bytes: &mut u64) -> AnyResult<()> {
        for entry in std::fs::read_dir(path)? {
            let path = entry?.path();
            if path.is_dir() {
                walk(&path, files, bytes)?;
            } else if path.is_file() {
                *files = files.checked_add(1).ok_or("Store file count")?;
                *bytes = bytes
                    .checked_add(std::fs::metadata(path)?.len())
                    .ok_or("Store durable bytes")?;
            }
        }
        Ok(())
    }
    let (mut files, mut bytes) = (0, 0);
    walk(root, &mut files, &mut bytes)?;
    Ok((files, bytes))
}

#[allow(clippy::too_many_arguments)]
fn store_footprint_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    control_id: &str,
    seed: u8,
    source: &str,
    expected_files: u64,
    expected_logical_bytes: u64,
    edit_path: &str,
    edit_size: u64,
    fixture_digest: &str,
    edited_digest: &str,
    verify: bool,
) -> AnyResult<()> {
    let control = workload_source::store_footprint::control(control_id)?;
    if root.exists()
        || !fixture.is_dir()
        || !workload_source::store_footprint::SEEDS.contains(&seed)
        || !matches!(source, "baseline" | "candidate")
        || expected_files == 0
        || expected_logical_bytes == 0
        || edit_size <= workload_source::NAMESPACE_EDIT_MARKER.len() as u64
        || edit_path.starts_with('/')
        || edit_path.contains("..")
        || !valid_digest(fixture_digest)
        || !valid_digest(edited_digest)
        || fixture_digest == edited_digest
    {
        return Err("Store-footprint arguments".into());
    }
    std::fs::create_dir(root)?;
    let store_path = root.join("store.sqlite");
    let process_before = process_resource_snapshot()?;
    let container_before = container_cgroup_snapshot(&container_id)?;
    let total_started = Instant::now();
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect(store.clone())?;
    let store_baseline_bytes = std::fs::metadata(&store_path)?.len();
    let initialization_started = Instant::now();
    let initialized = client.initialize_layerstack(
        EntityName::new("store-footprint")?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let initialization_ns = elapsed_ns(initialization_started);
    let scans = store.take_layerstack_initialization_receipts();
    let [scan] = scans.as_slice() else {
        return Err("Store-footprint initialization receipt cardinality".into());
    };
    if scan.scanned_files != expected_files || scan.scanned_bytes != expected_logical_bytes {
        return Err("Store-footprint fixture census".into());
    }
    let initialize_candidate = operation_candidate(
        &client.monitor_snapshot()?,
        OperationFamily::LayerStackInitialize,
    )?;
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: WorkspacePlacement::Container {
            container_id: container_id.clone(),
            root: PathBuf::from(format!(
                "/workspace/layerfs-store-footprint-{seed}-{}",
                std::process::id()
            )),
        },
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let output = execute_workload(
        &client,
        workspace.id,
        vec![
            workload.clone(),
            OsString::from("namespace-edit"),
            OsString::from(edit_path),
        ],
    )?;
    let attempted_operations = output_u64(&output, "attempted_operations")?;
    let completed_operations = output_u64(&output, "completed_operations")?;
    let final_file_bytes = output_u64(&output, "final_file_bytes")?;
    if attempted_operations != 1
        || completed_operations != attempted_operations
        || final_file_bytes != edit_size
    {
        return Err("Store-footprint workload validity".into());
    }
    let commit_started = Instant::now();
    let head = match client.commit_workspace_session(workspace.id)? {
        WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
        result => return Err(format!("Store-footprint Commit failed: {result:?}").into()),
    };
    visible_head(&client, branch, head)?;
    let commit_ns = elapsed_ns(commit_started);
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
        return Err("Store-footprint cleanup".into());
    }
    let snapshot = client.monitor_snapshot()?;
    let commit_candidate = operation_candidate(&snapshot, OperationFamily::WorkspaceCommit)?;
    let commit_receipt = operation_workspace_commit(&snapshot)?;
    let fuse = operation_fuse_write(&snapshot)?;
    if commit_receipt.edit_spool_allocated_bytes != fuse.spool_write_bytes
        || commit_receipt.edit_spool_live_bytes + commit_receipt.edit_spool_superseded_bytes
            != commit_receipt.edit_spool_allocated_bytes
        || commit_receipt.edit_spool_peak_bytes < commit_receipt.edit_spool_allocated_bytes
    {
        return Err("Store-footprint Workspace temporary-byte accounting".into());
    }
    drop(client);
    drop(store);

    let reopen_started = Instant::now();
    let reopened_store = Arc::new(LayerStackStore::connect(&store_path)?);
    let reopened = Client::connect(reopened_store.clone())?;
    visible_head(&reopened, branch, head)?;
    let pinned = reopened_store.pin_branch(branch)?;
    let commit_id = head.ok_or("Store-footprint missing Commit")?;
    if reopened_store
        .commit(commit_id)?
        .ok_or("Store-footprint Commit record")?
        .root_id
        != pinned.root
    {
        return Err("Store-footprint reopened root".into());
    }
    let canonical = reopened_store.canonical_storage()?;
    let storage = reopened_store.storage_snapshot()?;
    let reopen_ns = elapsed_ns(reopen_started);
    let mut verification_ns = 0;
    if verify {
        let verification_started = Instant::now();
        let reopened_workspace = reopened.create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: WorkspacePlacement::Container {
                container_id: container_id.clone(),
                root: PathBuf::from(format!(
                    "/workspace/layerfs-store-footprint-verify-{seed}-{}",
                    std::process::id()
                )),
            },
            projection: Some(WorkspaceProjection::Fuse),
        })?;
        eprintln!(
            "layerfs-store-verifier-phase-v1 event=workspace-created elapsed_ns={}",
            elapsed_ns(verification_started)
        );
        let output = execute_workload(
            &reopened,
            reopened_workspace.id,
            vec![
                workload,
                OsString::from("store-footprint-digest"),
                OsString::from("."),
            ],
        )?;
        eprintln!(
            "layerfs-store-verifier-phase-v1 event=tree-digest-complete elapsed_ns={}",
            elapsed_ns(verification_started)
        );
        if output_u64(&output, "regular_files")? != expected_files
            || output_u64(&output, "logical_bytes")? != expected_logical_bytes
            || output_string(&output, "tree_digest")? != edited_digest
        {
            return Err("Store-footprint exact reopen digest".into());
        }
        reopened.end_workspace_session(reopened_workspace.id, EndWorkspaceMode::Clean)?;
        eprintln!(
            "layerfs-store-verifier-phase-v1 event=workspace-ended elapsed_ns={}",
            elapsed_ns(verification_started)
        );
        verification_ns = elapsed_ns(verification_started);
    }
    if reopened.active_workspace_count()? != 0 || reopened.active_execution_count()? != 0 {
        return Err("Store-footprint reopened cleanup".into());
    }
    drop(reopened);
    drop(reopened_store);
    let (durable_files, total_durable_store_bytes) = store_owned_bytes(root)?;
    let sqlite_database_bytes = storage.database_bytes;
    let other_durable_store_bytes = total_durable_store_bytes
        .checked_sub(sqlite_database_bytes)
        .ok_or("Store-footprint durable equation")?;
    let process_after = process_resource_snapshot()?;
    let container_after = container_cgroup_snapshot(&container_id)?;
    if process_before.swaps != 0
        || process_after.swaps != 0
        || container_before.swap_current != 0
        || container_after.swap_current != 0
        || container_before.oom != container_after.oom
        || container_before.oom_kill != container_after.oom_kill
    {
        return Err("Store-footprint swap or OOM".into());
    }
    let schema = if verify {
        workload_source::store_footprint::VERIFICATION_SCHEMA
    } else {
        workload_source::store_footprint::PERFORMANCE_SCHEMA
    };
    println!(
        "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"control_id\":\"{}\",\"display_name\":\"{}\",\"mode\":\"{}\",\"source_arm\":\"{}\",\"seed\":{},\"fixture_digest\":\"{}\",\"edited_fixture_digest\":\"{}\",\"canonical_root\":\"{}\",\"regular_files\":{},\"logical_bytes\":{},\"attempted_operations\":{},\"completed_operations\":{},\"final_file_bytes\":{},\"initialization_ns\":{},\"commit_ns\":{},\"reopen_ns\":{},\"verification_ns\":{},\"complete_ns\":{},\"store_baseline_bytes\":{},\"sqlite_database_bytes\":{},\"other_durable_store_bytes\":{},\"total_durable_store_bytes\":{},\"durable_store_files\":{},\"canonical_objects\":{},\"canonical_bytes\":{},\"initialization_candidate_objects\":{},\"initialization_candidate_bytes\":{},\"commit_candidate_objects\":{},\"commit_candidate_bytes\":{},\"commit_payload_bytes_read\":{},\"fuse_kernel_write_bytes\":{},\"workspace_spool_write_bytes\":{},\"workspace_spool_allocated_bytes\":{},\"workspace_spool_peak_bytes\":{},\"workspace_spool_live_bytes\":{},\"workspace_spool_superseded_bytes\":{},\"process_user_cpu_ns\":{},\"process_system_cpu_ns\":{},\"process_disk_read_bytes\":{},\"process_disk_write_bytes\":{},\"process_peak_rss_bytes\":{},\"process_physical_footprint_bytes\":{},\"container_memory_peak_bytes\":{},\"swap_bytes\":0,\"oom\":false,\"timeout\":false,\"cleanup_status\":\"pass\",\"status\":\"pass\"}}",
        schema,
        workload_source::store_footprint::FAMILY_ID,
        control.id,
        control.display_name,
        if verify { "verify" } else { "performance" },
        source,
        seed,
        fixture_digest,
        edited_digest,
        pinned.root,
        expected_files,
        expected_logical_bytes,
        attempted_operations,
        completed_operations,
        final_file_bytes,
        initialization_ns,
        commit_ns,
        reopen_ns,
        verification_ns,
        elapsed_ns(total_started),
        store_baseline_bytes,
        sqlite_database_bytes,
        other_durable_store_bytes,
        total_durable_store_bytes,
        durable_files,
        canonical.objects,
        canonical.encoded_bytes,
        initialize_candidate.candidate_objects,
        initialize_candidate.candidate_bytes,
        commit_candidate.candidate_objects,
        commit_candidate.candidate_bytes,
        commit_receipt.payload_bytes_read,
        fuse.kernel_write_bytes,
        fuse.spool_write_bytes,
        commit_receipt.edit_spool_allocated_bytes,
        commit_receipt.edit_spool_peak_bytes,
        commit_receipt.edit_spool_live_bytes,
        commit_receipt.edit_spool_superseded_bytes,
        resource_delta(process_after.user_cpu_ns, process_before.user_cpu_ns, "Store user CPU")?,
        resource_delta(
            process_after.system_cpu_ns,
            process_before.system_cpu_ns,
            "Store system CPU",
        )?,
        resource_delta(process_after.disk_read_bytes, process_before.disk_read_bytes, "Store reads")?,
        resource_delta(
            process_after.disk_write_bytes,
            process_before.disk_write_bytes,
            "Store writes",
        )?,
        process_after.peak_resident_bytes,
        process_after.physical_footprint_bytes,
        container_after.memory_peak,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn namespace_performance_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    scenario: NamespaceScenario,
    seed: u8,
    source: &str,
    fixture_digest: &str,
    edited_digest: &str,
    edit_path: &str,
    edit_size: u64,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    validate_namespace_family_arguments(
        fixture,
        seed,
        source,
        fixture_digest,
        edited_digest,
        edit_path,
        edit_size,
        fixture_cache_profile,
    )?;
    let fixture_manifest = namespace_manifest(scenario, fixture_digest)?;
    std::fs::create_dir(root)?;
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect(store.clone())?;
    let store_baseline_bytes = std::fs::metadata(&store_path)?.len();
    let process_before = process_resource_snapshot()?;
    let container_before = container_cgroup_snapshot(&container_id)?;

    let init_started = Instant::now();
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("{}-{source}-{seed}", scenario.id))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let layerstack_init_ns = elapsed_ns(init_started);
    let scan_receipts = store.take_layerstack_initialization_receipts();
    let [scan] = scan_receipts.as_slice() else {
        return Err("LayerStack initialization receipt cardinality".into());
    };
    if scan.layer_stack_id != initialized.layer_stack_id
        || scan.scanned_files != fixture_manifest.regular_files
        || scan.scanned_bytes != fixture_manifest.logical_bytes
    {
        return Err("LayerStack initialization scan receipt mismatch".into());
    }
    let branch_started = Instant::now();
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let branch_fork_ns = elapsed_ns(branch_started);

    let t0 = Instant::now();
    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: namespace_placement(&container_id, scenario, usize::from(seed), "performance"),
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let t1 = Instant::now();
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let output = execute_workload(
        &client,
        workspace.id,
        vec![
            workload,
            OsString::from("namespace-edit"),
            OsString::from(edit_path),
        ],
    )?;
    let t2 = Instant::now();
    let attempted_operations = output_u64(&output, "attempted_operations")?;
    let completed_operations = output_u64(&output, "completed_operations")?;
    let final_file_bytes = output_u64(&output, "final_file_bytes")?;
    if attempted_operations != 1 || completed_operations != 1 || final_file_bytes != edit_size {
        return Err("namespace performance workload validity".into());
    }
    let head = match client.commit_workspace_session(workspace.id)? {
        WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
        result => return Err(format!("namespace performance Commit failed: {result:?}").into()),
    };
    visible_head(&client, branch, head)?;
    let t3 = Instant::now();
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    let t4 = Instant::now();
    if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
        return Err("namespace performance cleanup".into());
    }
    let process_after = process_resource_snapshot()?;
    let container_after = container_cgroup_snapshot(&container_id)?;
    if process_before.swaps != 0
        || process_after.swaps != 0
        || container_before.swap_current != 0
        || container_after.swap_current != 0
        || container_after.oom != container_before.oom
        || container_after.oom_kill != container_before.oom_kill
    {
        return Err("namespace performance swap or OOM".into());
    }
    let snapshot = client.monitor_snapshot()?;
    let initialize_candidate =
        operation_candidate(&snapshot, OperationFamily::LayerStackInitialize)?;
    let commit_candidate = operation_candidate(&snapshot, OperationFamily::WorkspaceCommit)?;
    let commit_receipt = operation_workspace_commit(&snapshot)?;
    let store_database_bytes = std::fs::metadata(&store_path)?.len();
    let execution_ns = nanos(t1, t2);
    let supplied_bytes = u64::try_from(workload_source::NAMESPACE_EDIT_MARKER.len())?;
    let operations_per_second = rate(completed_operations, execution_ns);
    println!(
        "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"scenario_id\":\"{}\",\"display_alias\":\"{}\",\"display_name\":\"{}\",\"mode\":\"performance\",\"source_arm\":\"{}\",\"seed\":{},\"seed_label\":\"layerfs-v0.1.2-seed-{}\",\"execution_profile\":\"macbook-docker-desktop-linux-fuse-v1\",\"fixture_profile\":\"{}\",\"fixture_digest\":\"{}\",\"fixture_cache_profile\":\"{}\",\"operation\":\"overwrite\",\"position\":\"deterministic-non-anchor\",\"operation_count\":1,\"attempted_operations\":{},\"completed_operations\":{},\"initial_file_bytes\":{},\"final_file_bytes\":{},\"supplied_bytes\":{},\"inserted_bytes\":0,\"deleted_bytes\":0,\"logical_zero_bytes\":0,\"layerstack_init_ns\":{},\"branch_fork_ns\":{},\"workspace_create_ns\":{},\"execution_ns\":{},\"commit_api_ns\":{},\"layerstack_visible_ns\":{},\"workspace_end_ns\":{},\"complete_lifecycle_ns\":{},\"operations_per_second\":{},\"supplied_bytes_per_second\":{},\"scanned_files\":{},\"scanned_bytes\":{},\"candidate_objects\":{},\"candidate_bytes\":{},\"inserted_objects\":{},\"inserted_bytes_total\":{},\"reused_objects\":{},\"reused_bytes\":{},\"commit_payload_bytes_read\":{},\"store_baseline_bytes\":{},\"store_database_bytes\":{},\"process_peak_rss_bytes\":{},\"cgroup_memory_peak_bytes\":{},\"swap_bytes\":0,\"oom\":false,\"timeout\":false,\"verification_status\":\"not-run-performance-mode\",\"cleanup_status\":\"pass\"}}",
        workload_source::PERFORMANCE_SCHEMA,
        workload_source::FAMILY_ID,
        scenario.id,
        scenario.alias,
        scenario.display_name,
        source,
        seed,
        seed,
        workload_source::NAMESPACE_FIXTURE_PROFILE,
        fixture_digest,
        fixture_cache_profile,
        attempted_operations,
        completed_operations,
        edit_size,
        final_file_bytes,
        supplied_bytes,
        layerstack_init_ns,
        branch_fork_ns,
        nanos(t0, t1),
        execution_ns,
        nanos(t2, t3),
        nanos(t0, t3),
        nanos(t3, t4),
        nanos(t0, t4),
        operations_per_second,
        rate(supplied_bytes, execution_ns),
        scan.scanned_files,
        scan.scanned_bytes,
        initialize_candidate.candidate_objects + commit_candidate.candidate_objects,
        initialize_candidate.candidate_bytes + commit_candidate.candidate_bytes,
        initialize_candidate.inserted_objects + commit_candidate.inserted_objects,
        initialize_candidate.inserted_bytes + commit_candidate.inserted_bytes,
        initialize_candidate.reused_objects + commit_candidate.reused_objects,
        initialize_candidate.reused_bytes + commit_candidate.reused_bytes,
        commit_receipt.payload_bytes_read,
        store_baseline_bytes,
        store_database_bytes,
        process_after.peak_resident_bytes,
        container_after.memory_peak,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn namespace_verify_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    scenario: NamespaceScenario,
    seed: u8,
    source: &str,
    fixture_digest: &str,
    edited_digest: &str,
    edit_path: &str,
    edit_size: u64,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    validate_namespace_family_arguments(
        fixture,
        seed,
        source,
        fixture_digest,
        edited_digest,
        edit_path,
        edit_size,
        fixture_cache_profile,
    )?;
    std::fs::create_dir(root)?;
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect(store.clone())?;
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("{}-{source}-{seed}-verify", scenario.id))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: namespace_placement(
            &container_id,
            scenario,
            usize::from(seed),
            "verify-prepare",
        ),
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    execute_workload(
        &client,
        workspace.id,
        vec![
            workload.clone(),
            OsString::from("namespace-edit"),
            OsString::from(edit_path),
        ],
    )?;
    let head = match client.commit_workspace_session(workspace.id)? {
        WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
        result => {
            return Err(format!("namespace verifier preparation Commit failed: {result:?}").into())
        }
    };
    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    drop(client);
    drop(store);

    let started = Instant::now();
    let reopened_store = Arc::new(LayerStackStore::connect(&store_path)?);
    let reopened = Client::connect(reopened_store)?;
    visible_head(&reopened, branch, head)?;
    let reopened_workspace = reopened.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: namespace_placement(&container_id, scenario, usize::from(seed), "verify"),
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let output = execute_workload(
        &reopened,
        reopened_workspace.id,
        vec![
            workload,
            OsString::from("namespace-verify"),
            OsString::from("."),
            OsString::from(scenario.id),
        ],
    )?;
    let verified = parse_namespace_verification(&output)?;
    let expected = namespace_manifest(scenario, edited_digest)?;
    if verified.manifest != expected {
        return Err("namespace exact verifier mismatch".into());
    }
    reopened.end_workspace_session(reopened_workspace.id, EndWorkspaceMode::Clean)?;
    if reopened.active_workspace_count()? != 0 || reopened.active_execution_count()? != 0 {
        return Err("namespace verifier cleanup".into());
    }
    let verification_ns = elapsed_ns(started);
    println!(
        "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"scenario_id\":\"{}\",\"display_alias\":\"{}\",\"display_name\":\"{}\",\"verification_id\":\"exact-result\",\"mode\":\"verify\",\"source_arm\":\"{}\",\"seed\":{},\"status\":\"pass\",\"expected_file_bytes\":{},\"observed_file_bytes\":{},\"expected_sha256\":\"{}\",\"observed_sha256\":\"{}\",\"root_status\":\"pass\",\"fresh_reopen_status\":\"pass\",\"resource_status\":\"pass\",\"cleanup_status\":\"pass\",\"verification_ns\":{},\"maximum_verifier_buffer_bytes\":{},\"verifier_worker_count\":{}}}",
        workload_source::VERIFICATION_SCHEMA,
        workload_source::FAMILY_ID,
        scenario.id,
        scenario.alias,
        scenario.display_name,
        source,
        seed,
        expected.logical_bytes,
        verified.manifest.logical_bytes,
        expected.digest,
        verified.manifest.digest,
        verification_ns,
        verified.maximum_verifier_buffer_bytes,
        verified.verifier_worker_count,
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn namespace_case(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    scenario: NamespaceScenario,
    iteration: usize,
    fixture_digest: &str,
    edited_digest: &str,
    edit_path: &str,
    edit_size: u64,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    if iteration == 0 {
        return Err("namespace iteration must be positive".into());
    }
    if !fixture.is_dir()
        || !valid_digest(fixture_digest)
        || !valid_digest(edited_digest)
        || fixture_digest == edited_digest
        || !matches!(
            fixture_cache_profile,
            "generated-first-sample-uncontrolled"
                | "generated-subsequent-sample-uncontrolled"
                | "reused-first-sample-uncontrolled"
                | "reused-subsequent-sample-uncontrolled"
        )
        || edit_path.starts_with('/')
        || edit_path.contains("..")
        || edit_size <= u64::try_from(workload_source::NAMESPACE_EDIT_MARKER.len())?
    {
        return Err("namespace fixture manifest arguments".into());
    }
    let setup_started = Instant::now();
    let fixture_manifest = namespace_manifest(scenario, fixture_digest)?;
    std::fs::create_dir(root)?;
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect(store.clone())?;
    let store_baseline_bytes = std::fs::metadata(&store_path)?.len();
    let setup_ns = elapsed_ns(setup_started);

    let stale = store.take_layerstack_initialization_receipts();
    if !stale.is_empty() {
        return Err("stale LayerStack initialization receipt".into());
    }
    let sqlite_resources_before = sqlite_resource_snapshot(&store, true)?;
    let initialization_resources_before = process_resource_snapshot()?;
    let t0 = Instant::now();
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("{}-{iteration}", scenario.id))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let t1 = Instant::now();
    let initialization_resources_after = process_resource_snapshot()?;
    let sqlite_resources_after = sqlite_resource_snapshot(&store, false)?;
    let receipts = store.take_layerstack_initialization_receipts();
    let [scan] = receipts.as_slice() else {
        return Err("LayerStack initialization receipt cardinality".into());
    };
    if scan.layer_stack_id != initialized.layer_stack_id
        || scan.scanned_files != fixture_manifest.regular_files
        || scan.scanned_bytes != fixture_manifest.logical_bytes
    {
        return Err(format!("LayerStack initialization scan receipt mismatch: {scan:?}").into());
    }
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let t2 = Instant::now();

    let workspace = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: namespace_placement(&container_id, scenario, iteration, "edit"),
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let t3 = Instant::now();

    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    execute_workload(
        &client,
        workspace.id,
        vec![
            workload.clone(),
            OsString::from("namespace-edit"),
            OsString::from(edit_path),
        ],
    )?;
    let t4 = Instant::now();

    let commit = client.commit_workspace_session(workspace.id);
    let t5 = Instant::now();
    let commit_failure = |error: String| -> AnyResult<()> {
        let ended = client.end_workspace_session(workspace.id, EndWorkspaceMode::Discard);
        let t6 = Instant::now();
        println!(
            "{{\"schema\":\"{}\",\"scenario\":\"{}\",\"iteration\":{iteration},\"fixture_profile\":\"{}\",\"fixture_digest_profile\":\"{}\",\"edit_contract\":\"{}\",\"result_profile\":\"{}\",\"measurement_mode\":\"product-lifecycle\",\"fixture_cache_profile\":\"{}\",\"failed_phase\":\"commit\",\"error\":{:?},\"layerstack_init_ns\":{},\"branch_fork_ns\":{},\"workspace_create_ns\":{},\"edit_ns\":{},\"commit_ns\":{},\"workspace_end_ns\":{},\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"empty_files\":{},\"tiny_files\":{},\"small_files\":{},\"medium_files\":{},\"anchor_files\":{},\"anchor_bytes\":{},\"file_mode\":{},\"directory_mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},\"fixture_digest\":\"{}\",\"scanned_files\":{},\"scanned_bytes\":{}}}",
            workload_source::NAMESPACE_FAILURE_SCHEMA,
            scenario.id,
            workload_source::NAMESPACE_FIXTURE_PROFILE,
            workload_source::NAMESPACE_DIGEST_PROFILE,
            workload_source::NAMESPACE_EDIT_CONTRACT,
            workload_source::NAMESPACE_LIFECYCLE_PROFILE,
            fixture_cache_profile,
            error,
            nanos(t0, t1),
            nanos(t1, t2),
            nanos(t2, t3),
            nanos(t3, t4),
            nanos(t4, t5),
            nanos(t5, t6),
            fixture_manifest.regular_files,
            fixture_manifest.data_directories,
            fixture_manifest.logical_bytes,
            fixture_manifest.empty_files,
            fixture_manifest.tiny_files,
            fixture_manifest.small_files,
            fixture_manifest.medium_files,
            fixture_manifest.anchor_files,
            fixture_manifest.anchor_bytes,
            fixture_manifest.file_mode,
            fixture_manifest.directory_mode,
            fixture_manifest.mtime_seconds,
            fixture_manifest.mtime_nanoseconds,
            fixture_manifest.digest,
            scan.scanned_files,
            scan.scanned_bytes,
        );
        eprintln!(
            "NAMESPACE_DIAGNOSTIC operations={:?}",
            client.monitor_snapshot()
        );
        ended?;
        Err(error.into())
    };
    let head = match commit {
        Ok(WorkspaceCommitResult::Created { commit_id, .. }) => Some(commit_id),
        Ok(result) => {
            return commit_failure(format!(
                "namespace Commit did not create a Commit: {result:?}"
            ));
        }
        Err(error) => {
            return commit_failure(format!("namespace Commit failed: {error}"));
        }
    };

    client.end_workspace_session(workspace.id, EndWorkspaceMode::Clean)?;
    let t6 = Instant::now();
    let snapshot = client.monitor_snapshot()?;
    let initialize_candidate =
        operation_candidate(&snapshot, OperationFamily::LayerStackInitialize)?;
    let commit_candidate = operation_candidate(&snapshot, OperationFamily::WorkspaceCommit)?;
    let workspace_commit = operation_workspace_commit(&snapshot)?;
    let workspace_create = operation_workspace_create(&snapshot)?;
    let workspace_create_non_attach_ns = nanos(t2, t3)
        .checked_sub(workspace_create.total_ns)
        .ok_or("namespace Workspace Create timing equation")?;
    let reconnect_started = Instant::now();
    drop(client);
    drop(store);

    let reopened_store = Arc::new(LayerStackStore::connect(&store_path)?);
    let reopened = Client::connect(reopened_store.clone())?;
    visible_head(&reopened, branch, head)?;
    let t7 = Instant::now();
    let reopened_workspace = reopened
        .create_workspace_session(CreateWorkspaceSession {
            branch_id: branch,
            placement: namespace_placement(&container_id, scenario, iteration, "reopen"),
            projection: Some(WorkspaceProjection::Fuse),
        })
        .map_err(|error| format!("namespace reopened Workspace create failed: {error}"))?;
    let t8 = Instant::now();
    let verified = (|| -> AnyResult<_> {
        let output = execute_workload(
            &reopened,
            reopened_workspace.id,
            vec![
                workload.clone(),
                OsString::from("namespace-verify"),
                OsString::from("."),
                OsString::from(scenario.id),
            ],
        )
        .map_err(|error| format!("namespace reopen verification execution failed: {error}"))?;
        let verified = parse_namespace_verification(&output)?;
        if verified.manifest != namespace_manifest(scenario, edited_digest)? {
            return Err("namespace reopened verification mismatch".into());
        }
        Ok(verified)
    })();
    let t9 = Instant::now();
    let product_resources_after = process_resource_snapshot();
    let normal_overwrite_started = Instant::now();
    let normal_overwrite = if verified.is_ok() {
        execute_workload(
            &reopened,
            reopened_workspace.id,
            vec![
                workload,
                OsString::from("namespace-edit-normal"),
                OsString::from(edit_path),
            ],
        )
        .and_then(|output| parse_normal_overwrite_mtime(&output))
    } else {
        Err("normal-overwrite diagnostic skipped after verification failure".into())
    };
    let normal_overwrite_diagnostic_ns = elapsed_ns(normal_overwrite_started);
    let reopen_end_started = Instant::now();
    let ended = reopened
        .end_workspace_session(
            reopened_workspace.id,
            if verified.is_ok() {
                EndWorkspaceMode::Discard
            } else {
                EndWorkspaceMode::Clean
            },
        )
        .map_err(|error| format!("namespace reopened Workspace End failed: {error}"));
    let t10 = Instant::now();
    ended?;
    let container_resources_after = container_cgroup_snapshot(&container_id)?;
    let product_resources_after = product_resources_after?;
    let verified = verified?;
    let (normal_overwrite_mtime_seconds, normal_overwrite_mtime_nanoseconds) = normal_overwrite?;
    let normal_overwrite_changed_mtime = u8::from(
        (
            normal_overwrite_mtime_seconds,
            normal_overwrite_mtime_nanoseconds,
        ) != (
            workload_source::NAMESPACE_MTIME_SECONDS,
            i64::from(workload_source::NAMESPACE_MTIME_NANOSECONDS),
        ),
    );
    if reopened.active_workspace_count()? != 0 || reopened.active_execution_count()? != 0 {
        return Err("namespace reopened Workspace leaked runtime state".into());
    }
    let reopen_snapshot = reopened.monitor_snapshot()?;
    let read = operation_workspace_read(&reopen_snapshot)?;
    let store_storage = reopened_store.storage_snapshot()?;
    let canonical_storage = reopened_store.canonical_storage()?;
    let store_database_bytes = store_storage.database_bytes;
    let store_growth_bytes = store_database_bytes.saturating_sub(store_baseline_bytes);
    eprintln!("NAMESPACE_DIAGNOSTIC scan={scan:?}");
    eprintln!("NAMESPACE_DIAGNOSTIC operations={snapshot:?}");
    eprintln!("NAMESPACE_DIAGNOSTIC reopen_operations={reopen_snapshot:?}");

    let sample = NamespaceSample {
        layerstack_init_ns: nanos(t0, t1),
        branch_fork_ns: nanos(t1, t2),
        workspace_create_ns: nanos(t2, t3),
        edit_ns: nanos(t3, t4),
        commit_ns: nanos(t4, t5),
        workspace_end_ns: nanos(t5, t6),
        reconnect_ns: nanos(reconnect_started, t7),
        reopen_workspace_create_ns: nanos(t7, t8),
        reopen_content_verify_ns: nanos(t8, t9),
        reopen_verify_ns: 0,
        reopen_workspace_end_ns: nanos(reopen_end_started, t10),
        complete_product_ns: 0,
        product_lifecycle_ns: 0,
    };
    let reopen_verify_ns = [
        sample.reconnect_ns,
        sample.reopen_workspace_create_ns,
        sample.reopen_content_verify_ns,
    ]
    .into_iter()
    .try_fold(0_u64, u64::checked_add)
    .ok_or("namespace reopen phase overflow")?;
    let complete_product_ns = [
        sample.layerstack_init_ns,
        sample.branch_fork_ns,
        sample.workspace_create_ns,
        sample.edit_ns,
        sample.commit_ns,
        sample.workspace_end_ns,
        reopen_verify_ns,
    ]
    .into_iter()
    .try_fold(0_u64, u64::checked_add)
    .ok_or("namespace product lifecycle overflow")?;
    let sample = NamespaceSample {
        reopen_verify_ns,
        complete_product_ns,
        product_lifecycle_ns: complete_product_ns,
        ..sample
    };
    sample.validate()?;

    let candidate_objects = sum_metric(
        initialize_candidate.candidate_objects,
        commit_candidate.candidate_objects,
        "namespace candidate object overflow",
    )?;
    let candidate_bytes = sum_metric(
        initialize_candidate.candidate_bytes,
        commit_candidate.candidate_bytes,
        "namespace candidate byte overflow",
    )?;
    let inserted_objects = sum_metric(
        initialize_candidate.inserted_objects,
        commit_candidate.inserted_objects,
        "namespace inserted object overflow",
    )?;
    let inserted_bytes = sum_metric(
        initialize_candidate.inserted_bytes,
        commit_candidate.inserted_bytes,
        "namespace inserted byte overflow",
    )?;
    let reused_objects = sum_metric(
        initialize_candidate.reused_objects,
        commit_candidate.reused_objects,
        "namespace reused object overflow",
    )?;
    let reused_bytes = sum_metric(
        initialize_candidate.reused_bytes,
        commit_candidate.reused_bytes,
        "namespace reused byte overflow",
    )?;
    if candidate_objects
        != sum_metric(
            inserted_objects,
            reused_objects,
            "namespace combined object equation overflow",
        )?
        || candidate_bytes
            != sum_metric(
                inserted_bytes,
                reused_bytes,
                "namespace combined byte equation overflow",
            )?
    {
        return Err("namespace combined candidate equation".into());
    }

    let init_bytes_per_second = rate(fixture_manifest.logical_bytes, sample.layerstack_init_ns);
    let init_files_per_second = rate(fixture_manifest.regular_files, sample.layerstack_init_ns);
    let initialization_peak_status = phase_peak_status(
        initialization_resources_before,
        initialization_resources_after,
    );
    let product_peak_status =
        phase_peak_status(initialization_resources_before, product_resources_after);
    let SqliteResourceSnapshot {
        memory_used_bytes: sqlite_t0_memory_used_bytes,
        memory_peak_bytes: sqlite_t0_memory_peak_bytes,
        page_cache_overflow_bytes: sqlite_t0_page_cache_overflow_bytes,
        page_cache_overflow_peak_bytes: sqlite_t0_page_cache_overflow_peak_bytes,
        allocation_count: sqlite_t0_allocation_count,
        allocation_peak_count: sqlite_t0_allocation_peak_count,
        connection_cache_used_bytes: sqlite_t0_connection_cache_used_bytes,
        connection_cache_target_bytes: sqlite_t0_connection_cache_target_bytes,
    } = sqlite_resources_before;
    let SqliteResourceSnapshot {
        memory_used_bytes: sqlite_t1_memory_used_bytes,
        memory_peak_bytes: sqlite_t1_memory_peak_bytes,
        page_cache_overflow_bytes: sqlite_t1_page_cache_overflow_bytes,
        page_cache_overflow_peak_bytes: sqlite_t1_page_cache_overflow_peak_bytes,
        allocation_count: sqlite_t1_allocation_count,
        allocation_peak_count: sqlite_t1_allocation_peak_count,
        connection_cache_used_bytes: sqlite_t1_connection_cache_used_bytes,
        connection_cache_target_bytes: sqlite_t1_connection_cache_target_bytes,
    } = sqlite_resources_after;
    eprintln!(
        "layerfs-normal-overwrite-v1 nonce={} elapsed_ns={normal_overwrite_diagnostic_ns} mtime_seconds={normal_overwrite_mtime_seconds} mtime_nanoseconds={normal_overwrite_mtime_nanoseconds} changed={normal_overwrite_changed_mtime}",
        std::env::var("LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE")?,
    );
    eprintln!(
        "layerfs-container-cgroup-after-v1 nonce={} memory_current={} memory_peak={} swap_current={} pids_current={} oom={} oom_kill={}",
        std::env::var("LAYERFS_INITIALIZATION_DIAGNOSTIC_NONCE")?,
        container_resources_after.memory_current,
        container_resources_after.memory_peak,
        container_resources_after.swap_current,
        container_resources_after.pids_current,
        container_resources_after.oom,
        container_resources_after.oom_kill,
    );
    println!(
        "{{\"schema\":\"{}\",\"scenario\":\"{}\",\"iteration\":{iteration},\"fixture_profile\":\"{}\",\"fixture_digest_profile\":\"{}\",\"edit_contract\":\"{}\",\"result_profile\":\"{}\",\"measurement_mode\":\"product-lifecycle\",\"fixture_cache_profile\":\"{}\",\"setup_ns\":{setup_ns},\"layerstack_init_ns\":{},\"branch_fork_ns\":{},\"workspace_create_ns\":{},\"edit_ns\":{},\"commit_ns\":{},\"workspace_end_ns\":{},\"reconnect_ns\":{},\"reopen_workspace_create_ns\":{},\"reopen_content_verify_ns\":{},\"reopen_workspace_end_ns\":{},\"reopen_verify_ns\":{},\"complete_product_ns\":{},\"product_lifecycle_ns\":{},\"init_bytes_per_second\":{init_bytes_per_second},\"init_files_per_second\":{init_files_per_second},\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"empty_files\":{},\"tiny_files\":{},\"small_files\":{},\"medium_files\":{},\"anchor_files\":{},\"anchor_bytes\":{},\"file_mode\":{},\"directory_mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},\"edit_path\":\"{}\",\"edit_size\":{},\"fixture_digest\":\"{}\",\"verified_digest\":\"{}\",\"scanned_files\":{},\"scanned_bytes\":{},\"candidate_objects\":{candidate_objects},\"candidate_bytes\":{candidate_bytes},\"inserted_objects\":{inserted_objects},\"inserted_bytes\":{inserted_bytes},\"reused_objects\":{reused_objects},\"reused_bytes\":{reused_bytes},\"max_transaction_objects\":{},\"max_transaction_bytes\":{},\"initialize_candidate_objects\":{},\"initialize_candidate_bytes\":{},\"initialize_inserted_objects\":{},\"initialize_inserted_bytes\":{},\"initialize_reused_objects\":{},\"initialize_reused_bytes\":{},\"initialize_batch_inserted_objects\":{},\"initialize_batch_inserted_bytes\":{},\"initialize_final_inserted_objects\":{},\"initialize_final_inserted_bytes\":{},\"initialize_preexisting_reused_objects\":{},\"initialize_preexisting_reused_bytes\":{},\"initialize_admission_transactions\":{},\"initialize_max_transaction_objects\":{},\"initialize_max_transaction_bytes\":{},\"commit_candidate_objects\":{},\"commit_candidate_bytes\":{},\"commit_inserted_objects\":{},\"commit_inserted_bytes\":{},\"commit_reused_objects\":{},\"commit_reused_bytes\":{},\"commit_admission_transactions\":{},\"commit_max_transaction_objects\":{},\"commit_max_transaction_bytes\":{},\"store_baseline_bytes\":{store_baseline_bytes},\"store_database_bytes\":{store_database_bytes},\"store_growth_bytes\":{store_growth_bytes},\"store_canonical_objects\":{},\"store_canonical_bytes\":{},\"maximum_verifier_buffer_bytes\":{},\"verifier_worker_count\":{},\"verifier_plan_bytes\":{},\"verifier_path_state_peak_bytes\":{},\"verifier_digest_state_peak_bytes\":{},\"maximum_product_read_ahead_bytes\":{},\"read_ahead_hits\":{},\"read_ahead_misses\":{},\"read_ahead_fetches\":{},\"read_ahead_requested_bytes\":{},\"read_ahead_fetched_bytes\":{},\"read_ahead_served_bytes\":{},\"read_ahead_unused_bytes\":{},\"workspace_read_local_calls\":{},\"workspace_read_local_ids\":{},\"workspace_read_local_rows\":{},\"workspace_read_local_bytes\":{},\"workspace_create_attach_ns\":{},\"workspace_create_non_attach_ns\":{},\"snapshot_database_calls\":{},\"snapshot_database_rows\":{},\"snapshot_database_bytes\":{},\"snapshot_cache_rows_at_create\":{},\"snapshot_cache_bytes_at_create\":{},\"snapshot_store_wide_scans\":{},\"small_file_prefetch_eligible\":{},\"small_file_prefetch_bytes\":{},\"anchor_prefetch_count\":{},\"commit_snapshot_database_calls\":{},\"commit_snapshot_database_rows\":{},\"commit_snapshot_database_bytes\":{},\"commit_payload_bytes_read\":{},\"commit_anchor_payload_reads\":{},\"process_t0_rss_bytes\":{},\"process_t1_rss_bytes\":{},\"process_t1_rss_growth_bytes\":{},\"process_t0_peak_rss_bytes\":{},\"process_t1_peak_rss_bytes\":{},\"process_initialization_incremental_peak_rss_bytes\":{},\"process_initialization_peak_status\":\"{initialization_peak_status}\",\"process_t0_swaps\":{},\"process_t1_swaps\":{},\"process_t0_physical_footprint_bytes\":{},\"process_t1_physical_footprint_bytes\":{},\"initialization_user_cpu_ns\":{},\"initialization_system_cpu_ns\":{},\"initialization_disk_read_bytes\":{},\"initialization_disk_write_bytes\":{},\"initialization_context_switches\":{},\"process_threads_before\":{},\"process_threads_after\":{},\"sqlite_t0_memory_used_bytes\":{sqlite_t0_memory_used_bytes},\"sqlite_t0_memory_peak_bytes\":{sqlite_t0_memory_peak_bytes},\"sqlite_t0_page_cache_overflow_bytes\":{sqlite_t0_page_cache_overflow_bytes},\"sqlite_t0_page_cache_overflow_peak_bytes\":{sqlite_t0_page_cache_overflow_peak_bytes},\"sqlite_t0_allocation_count\":{sqlite_t0_allocation_count},\"sqlite_t0_allocation_peak_count\":{sqlite_t0_allocation_peak_count},\"sqlite_t0_connection_cache_used_bytes\":{sqlite_t0_connection_cache_used_bytes},\"sqlite_connection_cache_target_bytes\":{sqlite_t0_connection_cache_target_bytes},\"sqlite_t1_memory_used_bytes\":{sqlite_t1_memory_used_bytes},\"sqlite_t1_memory_peak_bytes\":{sqlite_t1_memory_peak_bytes},\"sqlite_t1_page_cache_overflow_bytes\":{sqlite_t1_page_cache_overflow_bytes},\"sqlite_t1_page_cache_overflow_peak_bytes\":{sqlite_t1_page_cache_overflow_peak_bytes},\"sqlite_t1_allocation_count\":{sqlite_t1_allocation_count},\"sqlite_t1_allocation_peak_count\":{sqlite_t1_allocation_peak_count},\"sqlite_t1_connection_cache_used_bytes\":{sqlite_t1_connection_cache_used_bytes},\"sqlite_t1_connection_cache_target_bytes\":{sqlite_t1_connection_cache_target_bytes},\"process_t7_rss_bytes\":{},\"process_t7_peak_rss_bytes\":{},\"process_product_incremental_peak_rss_bytes\":{},\"process_product_peak_status\":\"{product_peak_status}\",\"process_t7_swaps\":{},\"process_t7_physical_footprint_bytes\":{},\"product_user_cpu_ns\":{},\"product_system_cpu_ns\":{},\"product_disk_read_bytes\":{},\"product_disk_write_bytes\":{},\"product_context_switches\":{},\"process_threads_at_t7\":{}}}",
        workload_source::NAMESPACE_SCHEMA,
        scenario.id,
        workload_source::NAMESPACE_FIXTURE_PROFILE,
        workload_source::NAMESPACE_DIGEST_PROFILE,
        workload_source::NAMESPACE_EDIT_CONTRACT,
        workload_source::NAMESPACE_LIFECYCLE_PROFILE,
        fixture_cache_profile,
        sample.layerstack_init_ns,
        sample.branch_fork_ns,
        sample.workspace_create_ns,
        sample.edit_ns,
        sample.commit_ns,
        sample.workspace_end_ns,
        sample.reconnect_ns,
        sample.reopen_workspace_create_ns,
        sample.reopen_content_verify_ns,
        sample.reopen_workspace_end_ns,
        sample.reopen_verify_ns,
        sample.complete_product_ns,
        sample.product_lifecycle_ns,
        fixture_manifest.regular_files,
        fixture_manifest.data_directories,
        fixture_manifest.logical_bytes,
        fixture_manifest.empty_files,
        fixture_manifest.tiny_files,
        fixture_manifest.small_files,
        fixture_manifest.medium_files,
        fixture_manifest.anchor_files,
        fixture_manifest.anchor_bytes,
        fixture_manifest.file_mode,
        fixture_manifest.directory_mode,
        fixture_manifest.mtime_seconds,
        fixture_manifest.mtime_nanoseconds,
        edit_path,
        edit_size,
        fixture_manifest.digest,
        verified.manifest.digest,
        scan.scanned_files,
        scan.scanned_bytes,
        initialize_candidate
            .max_transaction_objects
            .max(commit_candidate.max_transaction_objects),
        initialize_candidate
            .max_transaction_bytes
            .max(commit_candidate.max_transaction_bytes),
        initialize_candidate.candidate_objects,
        initialize_candidate.candidate_bytes,
        initialize_candidate.inserted_objects,
        initialize_candidate.inserted_bytes,
        initialize_candidate.reused_objects,
        initialize_candidate.reused_bytes,
        initialize_candidate.batch_inserted_objects,
        initialize_candidate.batch_inserted_bytes,
        initialize_candidate.final_inserted_objects,
        initialize_candidate.final_inserted_bytes,
        initialize_candidate.preexisting_reused_objects,
        initialize_candidate.preexisting_reused_bytes,
        initialize_candidate.admission_transactions,
        initialize_candidate.max_transaction_objects,
        initialize_candidate.max_transaction_bytes,
        commit_candidate.candidate_objects,
        commit_candidate.candidate_bytes,
        commit_candidate.inserted_objects,
        commit_candidate.inserted_bytes,
        commit_candidate.reused_objects,
        commit_candidate.reused_bytes,
        commit_candidate.admission_transactions,
        commit_candidate.max_transaction_objects,
        commit_candidate.max_transaction_bytes,
        canonical_storage.objects,
        canonical_storage.encoded_bytes,
        verified.maximum_verifier_buffer_bytes,
        verified.verifier_worker_count,
        verified.verifier_plan_bytes,
        verified.verifier_path_state_peak_bytes,
        verified.verifier_digest_state_peak_bytes,
        read.maximum_product_read_ahead_bytes,
        read.read_ahead_hits,
        read.read_ahead_misses,
        read.read_ahead_fetches,
        read.read_ahead_requested_bytes,
        read.read_ahead_fetched_bytes,
        read.read_ahead_served_bytes,
        read.read_ahead_unused_bytes,
        read.local_calls,
        read.local_ids,
        read.local_rows,
        read.local_bytes,
        workspace_create.total_ns,
        workspace_create_non_attach_ns,
        workspace_create.snapshot_database_calls,
        workspace_create.snapshot_database_rows,
        workspace_create.snapshot_database_bytes,
        workspace_create.snapshot_cache_rows_at_create,
        workspace_create.snapshot_cache_bytes_at_create,
        workspace_create.snapshot_store_wide_scans,
        workspace_create.small_file_prefetch_eligible,
        workspace_create.small_file_prefetch_bytes,
        workspace_create.anchor_prefetch_count,
        workspace_commit.snapshot_database_calls,
        workspace_commit.snapshot_database_rows,
        workspace_commit.snapshot_database_bytes,
        workspace_commit.payload_bytes_read,
        u64::from(workspace_commit.payload_bytes_read > edit_size),
        initialization_resources_before.resident_bytes,
        initialization_resources_after.resident_bytes,
        initialization_resources_after
            .resident_bytes
            .saturating_sub(initialization_resources_before.resident_bytes),
        initialization_resources_before.peak_resident_bytes,
        initialization_resources_after.peak_resident_bytes,
        initialization_resources_after
            .peak_resident_bytes
            .saturating_sub(initialization_resources_before.resident_bytes),
        initialization_resources_before.swaps,
        initialization_resources_after.swaps,
        initialization_resources_before.physical_footprint_bytes,
        initialization_resources_after.physical_footprint_bytes,
        resource_delta(
            initialization_resources_after.user_cpu_ns,
            initialization_resources_before.user_cpu_ns,
            "initialization user CPU",
        )?,
        resource_delta(
            initialization_resources_after.system_cpu_ns,
            initialization_resources_before.system_cpu_ns,
            "initialization system CPU",
        )?,
        resource_delta(
            initialization_resources_after.disk_read_bytes,
            initialization_resources_before.disk_read_bytes,
            "initialization disk reads",
        )?,
        resource_delta(
            initialization_resources_after.disk_write_bytes,
            initialization_resources_before.disk_write_bytes,
            "initialization disk writes",
        )?,
        resource_delta(
            initialization_resources_after.context_switches,
            initialization_resources_before.context_switches,
            "initialization context switches",
        )?,
        initialization_resources_before.threads,
        initialization_resources_after.threads,
        product_resources_after.resident_bytes,
        product_resources_after.peak_resident_bytes,
        product_resources_after
            .peak_resident_bytes
            .saturating_sub(initialization_resources_before.resident_bytes),
        product_resources_after.swaps,
        product_resources_after.physical_footprint_bytes,
        resource_delta(
            product_resources_after.user_cpu_ns,
            initialization_resources_before.user_cpu_ns,
            "product user CPU",
        )?,
        resource_delta(
            product_resources_after.system_cpu_ns,
            initialization_resources_before.system_cpu_ns,
            "product system CPU",
        )?,
        resource_delta(
            product_resources_after.disk_read_bytes,
            initialization_resources_before.disk_read_bytes,
            "product disk reads",
        )?,
        resource_delta(
            product_resources_after.disk_write_bytes,
            initialization_resources_before.disk_write_bytes,
            "product disk writes",
        )?,
        resource_delta(
            product_resources_after.context_switches,
            initialization_resources_before.context_switches,
            "product context switches",
        )?,
        product_resources_after.threads,
    );
    Ok(())
}

fn namespace_init_diagnostic(
    root: &Path,
    fixture: &Path,
    scenario: NamespaceScenario,
    iteration: usize,
    fixture_digest: &str,
    fixture_cache_profile: &str,
) -> AnyResult<()> {
    if iteration == 0
        || !fixture.is_dir()
        || !valid_digest(fixture_digest)
        || !matches!(
            fixture_cache_profile,
            "generated-first-sample-uncontrolled"
                | "generated-subsequent-sample-uncontrolled"
                | "reused-first-sample-uncontrolled"
                | "reused-subsequent-sample-uncontrolled"
        )
    {
        return Err("namespace init-only diagnostic arguments".into());
    }
    let fixture_manifest = namespace_manifest(scenario, fixture_digest)?;
    let setup_started = Instant::now();
    std::fs::create_dir(root)?;
    let store_path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&store_path)?);
    let client = Client::connect(store.clone())?;
    let store_baseline_bytes = std::fs::metadata(&store_path)?.len();
    let setup_ns = elapsed_ns(setup_started);
    if !store.take_layerstack_initialization_receipts().is_empty() {
        return Err("stale LayerStack initialization receipt".into());
    }

    let sqlite_resources_before = sqlite_resource_snapshot(&store, true)?;
    let initialization_resources_before = process_resource_snapshot()?;
    let t0 = Instant::now();
    let initialized = client.initialize_layerstack(
        EntityName::new(format!("{}-{iteration}-init-diagnostic", scenario.id))?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let t1 = Instant::now();
    let initialization_resources_after = process_resource_snapshot()?;
    let sqlite_resources_after = sqlite_resource_snapshot(&store, false)?;
    let receipts = store.take_layerstack_initialization_receipts();
    let [scan] = receipts.as_slice() else {
        return Err("LayerStack initialization receipt cardinality".into());
    };
    if scan.layer_stack_id != initialized.layer_stack_id
        || scan.scanned_files != fixture_manifest.regular_files
        || scan.scanned_bytes != fixture_manifest.logical_bytes
    {
        return Err("LayerStack initialization scan receipt mismatch".into());
    }
    let snapshot = client.monitor_snapshot()?;
    let candidate = operation_candidate(&snapshot, OperationFamily::LayerStackInitialize)?;
    let storage = store.storage_snapshot()?;
    let canonical = store.canonical_storage()?;
    if client.active_workspace_count()? != 0 || client.active_execution_count()? != 0 {
        return Err("init-only diagnostic created runtime state".into());
    }
    let store_database_bytes = storage.database_bytes;
    let store_growth_bytes = store_database_bytes.saturating_sub(store_baseline_bytes);
    let teardown_started = Instant::now();
    drop(client);
    drop(store);
    let teardown_ns = elapsed_ns(teardown_started);
    let layerstack_init_ns = nanos(t0, t1);
    let init_bytes_per_second = rate(fixture_manifest.logical_bytes, layerstack_init_ns);
    let init_files_per_second = rate(fixture_manifest.regular_files, layerstack_init_ns);
    let initialization_peak_status = phase_peak_status(
        initialization_resources_before,
        initialization_resources_after,
    );
    let SqliteResourceSnapshot {
        memory_used_bytes: sqlite_t0_memory_used_bytes,
        memory_peak_bytes: sqlite_t0_memory_peak_bytes,
        page_cache_overflow_bytes: sqlite_t0_page_cache_overflow_bytes,
        page_cache_overflow_peak_bytes: sqlite_t0_page_cache_overflow_peak_bytes,
        allocation_count: sqlite_t0_allocation_count,
        allocation_peak_count: sqlite_t0_allocation_peak_count,
        connection_cache_used_bytes: sqlite_t0_connection_cache_used_bytes,
        connection_cache_target_bytes: sqlite_t0_connection_cache_target_bytes,
    } = sqlite_resources_before;
    let SqliteResourceSnapshot {
        memory_used_bytes: sqlite_t1_memory_used_bytes,
        memory_peak_bytes: sqlite_t1_memory_peak_bytes,
        page_cache_overflow_bytes: sqlite_t1_page_cache_overflow_bytes,
        page_cache_overflow_peak_bytes: sqlite_t1_page_cache_overflow_peak_bytes,
        allocation_count: sqlite_t1_allocation_count,
        allocation_peak_count: sqlite_t1_allocation_peak_count,
        connection_cache_used_bytes: sqlite_t1_connection_cache_used_bytes,
        connection_cache_target_bytes: sqlite_t1_connection_cache_target_bytes,
    } = sqlite_resources_after;
    println!(
        "{{\"schema\":\"{}\",\"scenario\":\"{}\",\"iteration\":{iteration},\"fixture_profile\":\"{}\",\"fixture_digest_profile\":\"{}\",\"edit_contract\":\"{}\",\"result_profile\":\"{}\",\"measurement_mode\":\"init-only-diagnostic\",\"nonterminal\":true,\"fixture_cache_profile\":\"{}\",\"setup_ns\":{setup_ns},\"layerstack_init_ns\":{layerstack_init_ns},\"teardown_ns\":{teardown_ns},\"init_bytes_per_second\":{init_bytes_per_second},\"init_files_per_second\":{init_files_per_second},\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"empty_files\":{},\"tiny_files\":{},\"small_files\":{},\"medium_files\":{},\"anchor_files\":{},\"anchor_bytes\":{},\"file_mode\":{},\"directory_mode\":{},\"mtime_seconds\":{},\"mtime_nanoseconds\":{},\"fixture_digest\":\"{}\",\"scanned_files\":{},\"scanned_bytes\":{},\"candidate_objects\":{},\"candidate_bytes\":{},\"inserted_objects\":{},\"inserted_bytes\":{},\"reused_objects\":{},\"reused_bytes\":{},\"initialize_batch_inserted_objects\":{},\"initialize_batch_inserted_bytes\":{},\"initialize_final_inserted_objects\":{},\"initialize_final_inserted_bytes\":{},\"initialize_preexisting_reused_objects\":{},\"initialize_preexisting_reused_bytes\":{},\"initialize_admission_transactions\":{},\"initialize_max_transaction_objects\":{},\"initialize_max_transaction_bytes\":{},\"store_baseline_bytes\":{store_baseline_bytes},\"store_database_bytes\":{store_database_bytes},\"store_growth_bytes\":{store_growth_bytes},\"store_canonical_objects\":{},\"store_canonical_bytes\":{},\"process_t0_rss_bytes\":{},\"process_t1_rss_bytes\":{},\"process_t1_rss_growth_bytes\":{},\"process_t0_peak_rss_bytes\":{},\"process_t1_peak_rss_bytes\":{},\"process_initialization_incremental_peak_rss_bytes\":{},\"process_initialization_peak_status\":\"{initialization_peak_status}\",\"process_t0_swaps\":{},\"process_t1_swaps\":{},\"process_t0_physical_footprint_bytes\":{},\"process_t1_physical_footprint_bytes\":{},\"initialization_user_cpu_ns\":{},\"initialization_system_cpu_ns\":{},\"initialization_disk_read_bytes\":{},\"initialization_disk_write_bytes\":{},\"initialization_context_switches\":{},\"process_threads_before\":{},\"process_threads_after\":{},\"sqlite_t0_memory_used_bytes\":{sqlite_t0_memory_used_bytes},\"sqlite_t0_memory_peak_bytes\":{sqlite_t0_memory_peak_bytes},\"sqlite_t0_page_cache_overflow_bytes\":{sqlite_t0_page_cache_overflow_bytes},\"sqlite_t0_page_cache_overflow_peak_bytes\":{sqlite_t0_page_cache_overflow_peak_bytes},\"sqlite_t0_allocation_count\":{sqlite_t0_allocation_count},\"sqlite_t0_allocation_peak_count\":{sqlite_t0_allocation_peak_count},\"sqlite_t0_connection_cache_used_bytes\":{sqlite_t0_connection_cache_used_bytes},\"sqlite_connection_cache_target_bytes\":{sqlite_t0_connection_cache_target_bytes},\"sqlite_t1_memory_used_bytes\":{sqlite_t1_memory_used_bytes},\"sqlite_t1_memory_peak_bytes\":{sqlite_t1_memory_peak_bytes},\"sqlite_t1_page_cache_overflow_bytes\":{sqlite_t1_page_cache_overflow_bytes},\"sqlite_t1_page_cache_overflow_peak_bytes\":{sqlite_t1_page_cache_overflow_peak_bytes},\"sqlite_t1_allocation_count\":{sqlite_t1_allocation_count},\"sqlite_t1_allocation_peak_count\":{sqlite_t1_allocation_peak_count},\"sqlite_t1_connection_cache_used_bytes\":{sqlite_t1_connection_cache_used_bytes},\"sqlite_t1_connection_cache_target_bytes\":{sqlite_t1_connection_cache_target_bytes}}}",
        workload_source::NAMESPACE_SCHEMA,
        scenario.id,
        workload_source::NAMESPACE_FIXTURE_PROFILE,
        workload_source::NAMESPACE_DIGEST_PROFILE,
        workload_source::NAMESPACE_EDIT_CONTRACT,
        workload_source::NAMESPACE_INIT_DIAGNOSTIC_PROFILE,
        fixture_cache_profile,
        fixture_manifest.regular_files,
        fixture_manifest.data_directories,
        fixture_manifest.logical_bytes,
        fixture_manifest.empty_files,
        fixture_manifest.tiny_files,
        fixture_manifest.small_files,
        fixture_manifest.medium_files,
        fixture_manifest.anchor_files,
        fixture_manifest.anchor_bytes,
        fixture_manifest.file_mode,
        fixture_manifest.directory_mode,
        fixture_manifest.mtime_seconds,
        fixture_manifest.mtime_nanoseconds,
        fixture_manifest.digest,
        scan.scanned_files,
        scan.scanned_bytes,
        candidate.candidate_objects,
        candidate.candidate_bytes,
        candidate.inserted_objects,
        candidate.inserted_bytes,
        candidate.reused_objects,
        candidate.reused_bytes,
        candidate.batch_inserted_objects,
        candidate.batch_inserted_bytes,
        candidate.final_inserted_objects,
        candidate.final_inserted_bytes,
        candidate.preexisting_reused_objects,
        candidate.preexisting_reused_bytes,
        candidate.admission_transactions,
        candidate.max_transaction_objects,
        candidate.max_transaction_bytes,
        canonical.objects,
        canonical.encoded_bytes,
        initialization_resources_before.resident_bytes,
        initialization_resources_after.resident_bytes,
        initialization_resources_after
            .resident_bytes
            .saturating_sub(initialization_resources_before.resident_bytes),
        initialization_resources_before.peak_resident_bytes,
        initialization_resources_after.peak_resident_bytes,
        initialization_resources_after
            .peak_resident_bytes
            .saturating_sub(initialization_resources_before.resident_bytes),
        initialization_resources_before.swaps,
        initialization_resources_after.swaps,
        initialization_resources_before.physical_footprint_bytes,
        initialization_resources_after.physical_footprint_bytes,
        resource_delta(
            initialization_resources_after.user_cpu_ns,
            initialization_resources_before.user_cpu_ns,
            "initialization user CPU",
        )?,
        resource_delta(
            initialization_resources_after.system_cpu_ns,
            initialization_resources_before.system_cpu_ns,
            "initialization system CPU",
        )?,
        resource_delta(
            initialization_resources_after.disk_read_bytes,
            initialization_resources_before.disk_read_bytes,
            "initialization disk reads",
        )?,
        resource_delta(
            initialization_resources_after.disk_write_bytes,
            initialization_resources_before.disk_write_bytes,
            "initialization disk writes",
        )?,
        resource_delta(
            initialization_resources_after.context_switches,
            initialization_resources_before.context_switches,
            "initialization context switches",
        )?,
        initialization_resources_before.threads,
        initialization_resources_after.threads,
    );
    Ok(())
}

fn workspace_range_acceptance(
    root: &Path,
    fixture: &Path,
    container_id: ContainerId,
    case: &str,
) -> AnyResult<()> {
    let (operations, replacement_len, delete_len, position) = match case {
        "workspace-range-prepend-head-10b-on-32m" => (1_u64, 10_usize, 0_u64, "head"),
        "workspace-range-overwrite-middle-4k-on-256k-100" => {
            (100, 4 * 1024, 4 * 1024, "distributed")
        }
        "workspace-range-insert-middle-4k-on-256k-100" => (100, 4 * 1024, 0, "middle"),
        _ => return Err("unknown Workspace range acceptance case".into()),
    };
    let payload = fixture.join("payload.bin");
    let initial_len = std::fs::metadata(&payload)?.len();
    if !fixture.is_dir()
        || (operations == 1 && initial_len != MIB_32)
        || (operations == 100 && initial_len != 256 * 1024)
        || root.exists()
    {
        return Err("Workspace range acceptance arguments".into());
    }
    std::fs::create_dir(root)?;
    let store = Arc::new(LayerStackStore::create(root.join("store.sqlite"))?);
    let client = Client::connect(store.clone())?;
    let initialized = client.initialize_layerstack(
        EntityName::new(case)?,
        LayerStackInitialization::Directory(fixture.to_owned()),
    )?;
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    let base = store.pin_branch(branch)?;
    let old_payload_ids = payload_ids(&base.reader, base.root, "payload.bin")?;
    let resources_before = process_resource_snapshot()?;
    let cgroup_before = container_cgroup_snapshot(&container_id)?;
    let t0 = Instant::now();
    let session = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: branch,
        placement: WorkspacePlacement::Container {
            container_id: container_id.clone(),
            root: PathBuf::from(format!("/workspace/{case}-{}", std::process::id())),
        },
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let t1 = Instant::now();
    let edit_started = Instant::now();
    let mut current_len = initial_len;
    let mut edits = Vec::with_capacity(operations as usize);
    for operation in 0..operations {
        let start = match position {
            "head" => 0,
            "middle" => current_len / 2,
            _ => (operation + 1).saturating_mul(2_654_435_761) % (current_len - 4096),
        };
        let bytes = (0..replacement_len)
            .map(|index| (operation as usize + index * 29) as u8)
            .collect();
        edits.push(WorkspaceFileRangeEdit {
            workspace_id: session.id,
            path: "payload.bin".to_owned(),
            start,
            delete_len,
            replacement: WorkspaceFileReplacement::Inline(bytes),
        });
        current_len = current_len - delete_len + replacement_len as u64;
    }
    client.edit_workspace_file_ranges(edits)?;
    let edit_ns = elapsed_ns(edit_started);
    let commit_started = Instant::now();
    let head = match client.commit_workspace_session(session.id)? {
        WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
        result => return Err(format!("Workspace range Commit failed: {result:?}").into()),
    };
    visible_head(&client, branch, head)?;
    let committed_root = store
        .commit(head.ok_or("Workspace range Commit ID")?)?
        .ok_or("Workspace range Commit")?
        .root_id;
    let committed = store.pin_branch(branch)?;
    let new_payload_ids = payload_ids(&committed.reader, committed_root, "payload.bin")?;
    let old_payload_ids_lost = old_payload_ids.difference(&new_payload_ids).count() as u64;
    let edit_commit_ns = elapsed_ns(edit_started);
    let commit_ns = elapsed_ns(commit_started);
    client.end_workspace_session(session.id, EndWorkspaceMode::Clean)?;
    let t4 = Instant::now();
    let resources_after = process_resource_snapshot()?;
    let cgroup_after = container_cgroup_snapshot(&container_id)?;
    let commit = operation_workspace_commit(&client.monitor_snapshot()?)?;
    let snapshot = client.monitor_snapshot()?;
    let fuse_writes = snapshot
        .operations
        .iter()
        .flat_map(|operation| operation.storage.iter())
        .filter_map(|receipt| match receipt {
            StorageReceipt::FuseWrite(receipt) => Some(*receipt),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [fuse] = fuse_writes.as_slice() else {
        return Err("Workspace range FUSE receipt cardinality".into());
    };
    let unchanged_fuse_transfer_bytes = fuse
        .kernel_write_bytes
        .saturating_add(fuse.client_request_copy_bytes)
        .saturating_add(fuse.frame_payload_copy_bytes)
        .saturating_add(fuse.client_frame_bytes)
        .saturating_add(fuse.host_frame_bytes)
        .saturating_add(fuse.host_decode_copy_bytes);
    if fuse.spool_write_bytes != 0
        || unchanged_fuse_transfer_bytes != 0
        || (case == "workspace-range-prepend-head-10b-on-32m" && old_payload_ids_lost != 0)
    {
        return Err("Workspace range transfer or payload retention".into());
    }
    if resources_before.swaps != 0
        || resources_after.swaps != 0
        || cgroup_before.swap_current != 0
        || cgroup_after.swap_current != 0
        || cgroup_before.oom != cgroup_after.oom
        || cgroup_before.oom_kill != cgroup_after.oom_kill
        || client.active_workspace_count()? != 0
        || client.active_execution_count()? != 0
    {
        return Err("Workspace range resource or cleanup failure".into());
    }
    println!(
        "{{\"schema\":\"fs-bench-pro-edit-engine-acceptance-v1\",\"case\":\"{case}\",\"operations\":{operations},\"initial_file_bytes\":{initial_len},\"final_file_bytes\":{current_len},\"supplied_bytes\":{},\"workspace_create_ns\":{},\"edit_ns\":{edit_ns},\"commit_ns\":{commit_ns},\"edit_commit_ns\":{edit_commit_ns},\"complete_lifecycle_ns\":{},\"operations_per_second\":{},\"replacement_cdc_bytes\":{},\"old_payload_bytes_read\":{},\"unchanged_fuse_transfer_bytes\":{},\"spool_write_bytes\":{},\"old_payload_object_ids\":{},\"old_payload_object_ids_lost\":{},\"piece_count\":{},\"piece_height\":{},\"piece_logical_charge_bytes\":{},\"candidate_plan_ns\":{},\"content_ns\":{},\"object_admission_ns\":{},\"publication_ns\":{},\"process_peak_rss_bytes\":{},\"cgroup_peak_bytes\":{},\"swap_bytes\":0,\"oom\":false,\"cleanup_status\":\"pass\"}}",
        operations * replacement_len as u64,
        nanos(t0, t1),
        nanos(t0, t4),
        rate(operations, edit_ns),
        commit.cdc_bytes_scanned,
        commit.payload_bytes_read,
        unchanged_fuse_transfer_bytes,
        fuse.spool_write_bytes,
        old_payload_ids.len(),
        old_payload_ids_lost,
        commit.edit_piece_count,
        commit.edit_piece_height,
        commit.edit_piece_logical_charge,
        commit.candidate_plan_ns,
        commit.content_ns,
        commit.object_admission_ns,
        commit.publication_ns,
        resources_after.peak_resident_bytes,
        cgroup_after.memory_peak,
    );
    Ok(())
}

fn payload_ids(
    store: &dyn layerfs_layerstack_store::ObjectSource,
    root: layerfs_content::ObjectId,
    path: &str,
) -> AnyResult<std::collections::BTreeSet<layerfs_content::ObjectId>> {
    let store = layerfs_layerstack_store::CoreReader(store);
    let path = layerfs_content::CanonicalPath::new(path)?;
    let (stat, _) = layerfs_content::filesystem::stat(&store, root, &path)?;
    let mut ids = std::collections::BTreeSet::new();
    layerfs_content::file::rope::visit_extents(
        &store,
        layerfs_content::file::rope::FileStateRoot(stat.content_root),
        |extents| {
            ids.extend(extents.iter().map(|extent| extent.payload_object_id));
            Ok(())
        },
    )?;
    Ok(ids)
}

fn campaign(
    root: &Path,
    fixture: &Path,
    container: Option<ContainerId>,
    iterations: usize,
) -> AnyResult<()> {
    if iterations == 0 || !fixture.is_file() || std::fs::metadata(fixture)?.len() != MIB_32 {
        return Err("campaign arguments".into());
    }
    std::fs::create_dir_all(root)?;
    let workload = std::env::var_os("LAYERFS_BENCH_WORKLOAD")
        .unwrap_or_else(|| OsString::from("fs-benchmark-workload"));
    let oracle_workload =
        std::env::var_os("LAYERFS_BENCH_ORACLE_WORKLOAD").unwrap_or_else(|| workload.clone());
    let fixture_exec =
        std::env::var_os("LAYERFS_BENCH_FIXTURE").unwrap_or_else(|| fixture.as_os_str().to_owned());
    let mut cold = Vec::new();
    let mut small = Vec::new();
    let mut edit16 = Vec::new();
    let mut prepend = Vec::new();
    let mut read = Vec::new();
    let mut proofs = Vec::new();

    for iteration in 0..iterations {
        let iteration_root = root.join(format!("iteration-{iteration:03}"));
        std::fs::create_dir_all(&iteration_root)?;
        let seed = iteration_root.join("seed");
        std::fs::create_dir(&seed)?;
        std::fs::copy(fixture, seed.join("payload.bin"))?;

        let cold_root = iteration_root.join("cold");
        let (cold_client, cold_branch) = case_client(
            &cold_root,
            &format!("cold-{iteration}"),
            LayerStackInitialization::Empty,
        )?;

        let cold_run = lifecycle(
            &cold_client,
            cold_branch,
            case_placement(&container, &cold_root, iteration, "cold"),
            vec![
                workload.clone(),
                OsString::from("create"),
                fixture_exec.clone(),
                OsString::from("payload.bin"),
            ],
        )?;
        emit_sample("cold-create-32m", iteration, &cold_run.sample);
        emit_execution_receipt("cold-create-32m", iteration, &cold_run.output);
        emit_diagnostics(&cold_client, "cold-create-32m", iteration)?;
        proofs.push(ProofCase {
            store: cold_root.join("store.sqlite"),
            branch: cold_branch,
            head: cold_run.head,
            placement: case_placement(&container, &cold_root, iteration, "cold-proof"),
            expected: ProofExpected::Fixture,
        });
        cold.push(cold_run.sample);
        drop(cold_client);
        reopen_visible(&cold_root.join("store.sqlite"), cold_branch, cold_run.head)?;
        emit_store_census("cold-create-32m", iteration, &cold_root)?;

        let small_root = iteration_root.join("small");
        let (small_client, small_branch) = case_client(
            &small_root,
            &format!("small-{iteration}"),
            LayerStackInitialization::Directory(seed.clone()),
        )?;

        let small_run = lifecycle(
            &small_client,
            small_branch,
            case_placement(&container, &small_root, iteration, "small"),
            vec![
                workload.clone(),
                OsString::from("edit"),
                OsString::from("payload.bin"),
                OsString::from("0"),
                OsString::from(MIB_32.to_string()),
            ],
        )?;
        emit_sample("small-edit", iteration, &small_run.sample);
        emit_execution_receipt("small-edit", iteration, &small_run.output);
        emit_diagnostics(&small_client, "small-edit", iteration)?;
        small.push(small_run.sample);
        drop(small_client);
        reopen_visible(
            &small_root.join("store.sqlite"),
            small_branch,
            small_run.head,
        )?;
        emit_store_census("small-edit", iteration, &small_root)?;

        let edit_root = iteration_root.join("edit16");
        let (edit_client, edit_branch) = case_client(
            &edit_root,
            &format!("edit-{iteration}"),
            LayerStackInitialization::Directory(seed.clone()),
        )?;

        let edit_started = Instant::now();
        let edit_workspace = edit_client.create_workspace_session(CreateWorkspaceSession {
            branch_id: edit_branch,
            placement: case_placement(&container, &edit_root, iteration, "edit16"),
            projection: Some(WorkspaceProjection::Fuse),
        })?;
        let mut edit_receipts = Vec::with_capacity(16);
        let mut edit_head = None;
        for edit in 0..16 {
            let output = execute_workload(
                &edit_client,
                edit_workspace.id,
                vec![
                    workload.clone(),
                    OsString::from("edit"),
                    OsString::from("payload.bin"),
                    OsString::from((edit + 1).to_string()),
                    OsString::from(MIB_32.to_string()),
                ],
            )?;
            edit_receipts.push(output.receipt.ok_or("EDIT16 execution receipt")?);
            let result = edit_client.commit_workspace_session(edit_workspace.id)?;
            match result {
                WorkspaceCommitResult::Created { commit_id, .. } => {
                    edit_head = Some(commit_id);
                }
                result => {
                    emit_diagnostics(&edit_client, "edit16-failed", iteration)?;
                    return Err(format!(
                        "EDIT16 edit {} did not create a Commit: {result:?}",
                        edit + 1
                    )
                    .into());
                }
            }
        }
        edit_client.end_workspace_session(edit_workspace.id, EndWorkspaceMode::Clean)?;
        let edit_ns = elapsed_ns(edit_started);
        let snapshot = edit_client.monitor_snapshot()?;
        let commit_receipts = snapshot
            .operations
            .iter()
            .filter(|receipt| {
                receipt.operation.family == OperationFamily::WorkspaceCommit
                    && receipt.operation.workspace_id == Some(edit_workspace.id)
            })
            .collect::<Vec<_>>();
        if commit_receipts.len() != 16 {
            return Err("EDIT16 Monitor receipt cardinality".into());
        }
        println!(
            "{{\"schema\":\"fs-bench-pro-v4\",\"case\":\"edit16\",\"iteration\":{iteration},\"complete_lifecycle_ns\":{edit_ns}}}"
        );
        for receipt in edit_receipts {
            println!("DIAGNOSTIC case=edit16 execution={receipt:?}");
        }
        for receipt in commit_receipts {
            println!("DIAGNOSTIC case=edit16 commit={receipt:?}");
        }
        edit16.push(edit_ns);
        drop(edit_client);
        reopen_visible(&edit_root.join("store.sqlite"), edit_branch, edit_head)?;
        emit_store_census("edit16", iteration, &edit_root)?;

        let prepend_root = iteration_root.join("prepend");
        let (prepend_client, prepend_branch) = case_client(
            &prepend_root,
            &format!("prepend-{iteration}"),
            LayerStackInitialization::Directory(seed.clone()),
        )?;
        let prepend_run = lifecycle(
            &prepend_client,
            prepend_branch,
            case_placement(&container, &prepend_root, iteration, "prepend"),
            vec![
                workload.clone(),
                OsString::from("prepend"),
                OsString::from("payload.bin"),
            ],
        )?;
        emit_sample("prepend-temp-copy-rename", iteration, &prepend_run.sample);
        emit_execution_receipt("prepend-temp-copy-rename", iteration, &prepend_run.output);
        emit_diagnostics(&prepend_client, "prepend-temp-copy-rename", iteration)?;
        proofs.push(ProofCase {
            store: prepend_root.join("store.sqlite"),
            branch: prepend_branch,
            head: prepend_run.head,
            placement: case_placement(&container, &prepend_root, iteration, "prepend-proof"),
            expected: ProofExpected::Prepend,
        });
        prepend.push(prepend_run.sample);
        drop(prepend_client);
        reopen_visible(
            &prepend_root.join("store.sqlite"),
            prepend_branch,
            prepend_run.head,
        )?;
        emit_store_census("prepend-temp-copy-rename", iteration, &prepend_root)?;

        let read_root = iteration_root.join("read");
        let (read_client, read_branch) = case_client(
            &read_root,
            &format!("read-{iteration}"),
            LayerStackInitialization::Directory(seed),
        )?;
        let read_run = lifecycle(
            &read_client,
            read_branch,
            case_placement(&container, &read_root, iteration, "read"),
            vec![
                workload.clone(),
                OsString::from("read"),
                OsString::from("payload.bin"),
            ],
        )?;
        if parse_read_bytes(&read_run.output)? != MIB_32 {
            return Err("read output size".into());
        }
        emit_sample("read-32m", iteration, &read_run.sample);
        emit_execution_receipt("read-32m", iteration, &read_run.output);
        emit_diagnostics(&read_client, "read-32m", iteration)?;
        proofs.push(ProofCase {
            store: read_root.join("store.sqlite"),
            branch: read_branch,
            head: read_run.head,
            placement: case_placement(&container, &read_root, iteration, "read-proof"),
            expected: ProofExpected::Fixture,
        });
        read.push(read_run.sample);
        drop(read_client);
        reopen_visible(&read_root.join("store.sqlite"), read_branch, read_run.head)?;
        emit_store_census("read-32m", iteration, &read_root)?;
    }

    let (fixture_size, fixture_digest) = process_digest(&oracle_workload, fixture.as_os_str())?;
    if fixture_size != MIB_32 {
        return Err("fixture digest size".into());
    }
    let prepend_oracle = root.join("prepend-oracle.bin");
    build_prepend_oracle(fixture, &prepend_oracle)?;
    let (prepend_size, prepend_digest) =
        process_digest(&oracle_workload, prepend_oracle.as_os_str())?;
    if prepend_size != MIB_32 + 10 {
        return Err("prepend digest size".into());
    }
    for proof in proofs {
        let (size, digest) = match proof.expected {
            ProofExpected::Fixture => (fixture_size, fixture_digest.as_str()),
            ProofExpected::Prepend => (prepend_size, prepend_digest.as_str()),
        };
        prove_case(&workload, proof, size, digest)?;
    }
    std::fs::remove_file(prepend_oracle)?;

    let cold_commit = median(cold.iter().map(|sample| sample.commit_api_ns).collect());
    let cold_complete = median(
        cold.iter()
            .map(|sample| sample.complete_lifecycle_ns)
            .collect(),
    );
    let create = median(
        cold.iter()
            .chain(&small)
            .chain(&prepend)
            .chain(&read)
            .map(|sample| sample.workspace_create_ns)
            .collect(),
    );
    let small_commit = median(small.iter().map(|sample| sample.commit_api_ns).collect());
    let small_complete = median(
        small
            .iter()
            .map(|sample| sample.complete_lifecycle_ns)
            .collect(),
    );
    let edit16 = median(edit16);
    let prepend_complete = median(
        prepend
            .iter()
            .map(|sample| sample.complete_lifecycle_ns)
            .collect(),
    );
    let read_complete = median(
        read.iter()
            .map(|sample| sample.complete_lifecycle_ns)
            .collect(),
    );
    let registered_total = cold_complete
        .saturating_add(edit16)
        .saturating_add(prepend_complete)
        .saturating_add(read_complete);
    let inner_write_ns = median(
        cold.iter()
            .filter_map(|sample| sample.inner_write_ns)
            .collect(),
    );
    let throughput = if inner_write_ns == 0 {
        0.0
    } else {
        MIB_32 as f64 * 1_000_000_000.0 / inner_write_ns as f64
    };
    let process_peak_rss_bytes = linux_process_peak_rss_bytes();
    let cgroup_peak_bytes = read_u64("/sys/fs/cgroup/memory.peak");
    let cgroup_swap_bytes = read_u64("/sys/fs/cgroup/memory.swap.current");
    println!(
        "{{\"schema\":\"fs-bench-pro-v4-summary\",\"execution_profile\":\"fresh-direct-argv\",\"acknowledgement_profile\":\"memory-off-live-process\",\"workspace_create_ns\":{create},\"small_commit_ns\":{small_commit},\"small_complete_ns\":{small_complete},\"cold_commit_ns\":{cold_commit},\"cold_complete_ns\":{cold_complete},\"edit16_ns\":{edit16},\"prepend_complete_ns\":{prepend_complete},\"read_complete_ns\":{read_complete},\"registered_total_ns\":{registered_total},\"inner_write_bytes_per_second\":{throughput:.3},\"process_peak_rss_bytes\":{process_peak_rss_bytes},\"cgroup_peak_bytes\":{cgroup_peak_bytes},\"cgroup_swap_bytes\":{cgroup_swap_bytes}}}"
    );
    let failed = create > WORKSPACE_CREATE_HARD_NS
        || small_commit > SMALL_COMMIT_HARD_NS
        || small_complete > SMALL_COMPLETE_HARD_NS
        || cold_complete > COLD_COMPLETE_HARD_NS
        || edit16 > EDIT16_HARD_NS
        || prepend_complete > PREPEND_HARD_NS
        || read_complete > READ_HARD_NS
        || registered_total > REGISTERED_TOTAL_HARD_NS
        || throughput < INNER_WRITE_MIN_BYTES_PER_SECOND;
    if failed {
        return Err("one or more hard performance gates failed".into());
    }
    Ok(())
}

fn case_client(
    root: &Path,
    name: &str,
    source: LayerStackInitialization,
) -> AnyResult<(Client, BranchId)> {
    std::fs::create_dir(root)?;
    let path = root.join("store.sqlite");
    let store = Arc::new(LayerStackStore::create(&path)?);
    let client = Client::connect(store.clone())?;
    let initialized = client.initialize_layerstack(EntityName::new(name)?, source)?;
    let branch = client.fork_branch(
        EntityName::new("main")?,
        LocalForkSource::Layer {
            layer_id: initialized.genesis_layer_id,
        },
    )?;
    drop(client);
    drop(store);
    let store = Arc::new(LayerStackStore::connect(path)?);
    Ok((Client::connect(store)?, branch))
}

fn case_placement(
    container: &Option<ContainerId>,
    root: &Path,
    iteration: usize,
    case: &str,
) -> WorkspacePlacement {
    match container {
        Some(container_id) => WorkspacePlacement::Container {
            container_id: container_id.clone(),
            root: PathBuf::from(format!("/workspace/layerfs-bench-{iteration}-{case}")),
        },
        None => WorkspacePlacement::Host {
            root: root.join("mount"),
        },
    }
}

fn reopen_visible(path: &Path, branch: BranchId, expected: Option<CommitId>) -> AnyResult<()> {
    let store = Arc::new(LayerStackStore::connect(path)?);
    let client = Client::connect(store)?;
    visible_head(&client, branch, expected)
}

fn lifecycle(
    client: &Client,
    branch_id: BranchId,
    placement: WorkspacePlacement,
    argv: Vec<OsString>,
) -> AnyResult<LifecycleRun> {
    let t0 = Instant::now();
    let session = client.create_workspace_session(CreateWorkspaceSession {
        branch_id,
        placement,
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let t1 = Instant::now();
    let output = execute_workload(client, session.id, argv)?;
    let t2 = Instant::now();
    let commit_id = match client.commit_workspace_session(session.id)? {
        WorkspaceCommitResult::Created { commit_id, .. } => Some(commit_id),
        WorkspaceCommitResult::UpToDate { head } => head,
        result => return Err(format!("Commit failed: {result:?}").into()),
    };
    let t3 = Instant::now();
    client.end_workspace_session(session.id, EndWorkspaceMode::Clean)?;
    let t4 = Instant::now();
    visible_head(client, branch_id, commit_id)?;
    let sample = LifecycleSample {
        workspace_create_ns: nanos(t0, t1),
        execution_ns: nanos(t1, t2),
        commit_api_ns: nanos(t2, t3),
        layerstack_visible_ns: nanos(t0, t3),
        workspace_end_ns: nanos(t3, t4),
        complete_lifecycle_ns: nanos(t0, t4),
        inner_write_ns: parse_inner_write_ns(&output),
    };
    sample.validate()?;
    Ok(LifecycleRun {
        sample,
        output,
        head: commit_id,
    })
}

fn execute(
    client: &Client,
    workspace_id: WorkspaceId,
    argv: Vec<OsString>,
) -> AnyResult<layerfs_sdk::OutputPage> {
    let execution = client.exec_workspace_session(workspace_id, NonEmpty::new(argv)?)?;
    let reader = client.workspace_output(execution.id)?;
    let mut output = reader.read(0, true)?;
    while !output.exited {
        let next = reader.read(output.next_sequence, true)?;
        output.chunks.extend(next.chunks);
        output.next_sequence = next.next_sequence;
        output.truncated |= next.truncated;
        output.exited = next.exited;
        output.receipt = next.receipt;
    }
    if !output.exited
        || output.truncated
        || output
            .receipt
            .as_ref()
            .and_then(|receipt| receipt.exit_code)
            != Some(0)
    {
        return Err(format!("fresh-process execution failed: {output:?}").into());
    }
    if std::env::var("LAYERFS_EXEC_TRANSPORT").as_deref() == Ok("daemon") {
        let receipt = output.receipt.as_ref().ok_or("daemon execution receipt")?;
        if receipt.transport != ExecutionTransport::Daemon
            || receipt.daemon_timing.is_none()
            || receipt.docker_engine_calls != 0
            || !receipt.timing_balanced()
        {
            return Err(format!("invalid daemon execution receipt: {receipt:?}").into());
        }
    }
    Ok(output)
}

fn execute_workload(
    client: &Client,
    workspace_id: WorkspaceId,
    argv: Vec<OsString>,
) -> AnyResult<layerfs_sdk::OutputPage> {
    if std::env::var("LAYERFS_BENCH_SHELL").as_deref() != Ok("1") {
        return execute(client, workspace_id, argv);
    }
    let command = argv
        .iter()
        .map(|value| format!("'{}'", value.to_string_lossy().replace('\'', "'\"'\"'")))
        .collect::<Vec<_>>()
        .join(" ");
    execute(
        client,
        workspace_id,
        vec![
            OsString::from("/bin/sh"),
            OsString::from("-c"),
            OsString::from(command),
        ],
    )
}

fn visible_head(client: &Client, branch_id: BranchId, expected: Option<CommitId>) -> AnyResult<()> {
    let mut query = Query::new(QueryKind::Branches).limit(512);
    loop {
        let page = client.query(query.clone())?;
        if page.items.iter().any(|item| {
            matches!(item, QueryItem::Branch(branch) if branch.id == branch_id && branch.head_commit_id == expected)
        }) {
            return Ok(());
        }
        let Some(next) = page.into_next_query(&query) else {
            return Err("Commit not visible from public SDK query".into());
        };
        query = next;
    }
}

fn parse_inner_write_ns(output: &layerfs_sdk::OutputPage) -> Option<u64> {
    output
        .chunks
        .iter()
        .flat_map(|chunk| {
            String::from_utf8_lossy(&chunk.bytes)
                .into_owned()
                .lines()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .find_map(|line| line.strip_prefix("inner_write_ns=")?.parse().ok())
}

fn parse_digest(output: &OutputPage) -> AnyResult<(u64, String)> {
    let bytes = output
        .chunks
        .iter()
        .flat_map(|chunk| chunk.bytes.iter().copied())
        .collect::<Vec<_>>();
    parse_digest_text(std::str::from_utf8(&bytes)?)
}

fn parse_read_bytes(output: &OutputPage) -> AnyResult<u64> {
    let bytes = output
        .chunks
        .iter()
        .flat_map(|chunk| chunk.bytes.iter().copied())
        .collect::<Vec<_>>();
    std::str::from_utf8(&bytes)?
        .lines()
        .find_map(|line| line.strip_prefix("read_bytes=")?.parse().ok())
        .ok_or_else(|| "read output".into())
}

fn parse_normal_overwrite_mtime(output: &OutputPage) -> AnyResult<(i64, i64)> {
    let bytes = output
        .chunks
        .iter()
        .flat_map(|chunk| chunk.bytes.iter().copied())
        .collect::<Vec<_>>();
    let text = std::str::from_utf8(&bytes)?;
    let value = |name: &str| -> AnyResult<i64> {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .ok_or_else(|| format!("missing normal-overwrite field: {name}").into())
            .and_then(|value| value.parse().map_err(Into::into))
    };
    Ok((
        value("normal_overwrite_mtime_seconds=")?,
        value("normal_overwrite_mtime_nanoseconds=")?,
    ))
}

fn parse_digest_text(output: &str) -> AnyResult<(u64, String)> {
    for line in output.lines() {
        let Some((size, digest)) = line.split_once('\t') else {
            continue;
        };
        if digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Ok((size.parse()?, digest.to_ascii_lowercase()));
        }
    }
    Err("digest output".into())
}

fn process_digest(workload: &OsString, path: &std::ffi::OsStr) -> AnyResult<(u64, String)> {
    let output = Command::new(workload).arg("digest").arg(path).output()?;
    if !output.status.success() || !output.stderr.is_empty() {
        return Err("oracle digest process".into());
    }
    parse_digest_text(std::str::from_utf8(&output.stdout)?)
}

fn build_prepend_oracle(fixture: &Path, output: &Path) -> AnyResult<()> {
    let mut target = std::fs::File::create(output)?;
    target.write_all(b"PREPEND010")?;
    std::io::copy(&mut std::fs::File::open(fixture)?, &mut target)?;
    target.sync_all()?;
    Ok(())
}

fn prove_case(
    workload: &OsString,
    proof: ProofCase,
    expected_size: u64,
    expected_digest: &str,
) -> AnyResult<()> {
    let store = Arc::new(LayerStackStore::connect(&proof.store)?);
    let client = Client::connect(store)?;
    visible_head(&client, proof.branch, proof.head)?;
    let session = client.create_workspace_session(CreateWorkspaceSession {
        branch_id: proof.branch,
        placement: proof.placement,
        projection: Some(WorkspaceProjection::Fuse),
    })?;
    let output = execute_workload(
        &client,
        session.id,
        vec![
            workload.clone(),
            OsString::from("verify"),
            OsString::from("payload.bin"),
            OsString::from(expected_size.to_string()),
            OsString::from(expected_digest),
        ],
    )?;
    if parse_digest(&output)? != (expected_size, expected_digest.to_owned()) {
        return Err("proof digest output".into());
    }
    client.end_workspace_session(session.id, EndWorkspaceMode::Clean)?;
    visible_head(&client, proof.branch, proof.head)
}

fn emit_sample(case: &str, iteration: usize, sample: &LifecycleSample) {
    println!(
        "{{\"schema\":\"fs-bench-pro-v4\",\"case\":\"{case}\",\"iteration\":{iteration},\"workspace_create_ns\":{},\"execution_ns\":{},\"commit_api_ns\":{},\"layerstack_visible_ns\":{},\"workspace_end_ns\":{},\"complete_lifecycle_ns\":{},\"inner_write_ns\":{}}}",
        sample.workspace_create_ns,
        sample.execution_ns,
        sample.commit_api_ns,
        sample.layerstack_visible_ns,
        sample.workspace_end_ns,
        sample.complete_lifecycle_ns,
        sample.inner_write_ns.map_or("null".to_owned(), |value| value.to_string()),
    );
}

fn emit_execution_receipt(case: &str, iteration: usize, output: &OutputPage) {
    println!(
        "DIAGNOSTIC case={case} iteration={iteration} execution={:?}",
        output.receipt
    );
}

fn emit_diagnostics(client: &Client, case: &str, iteration: usize) -> AnyResult<()> {
    let snapshot = client.monitor_snapshot()?;
    for operation in snapshot.operations.iter().rev().take(6).rev() {
        println!("DIAGNOSTIC case={case} iteration={iteration} operation={operation:?}");
    }
    Ok(())
}

fn emit_store_census(case: &str, iteration: usize, root: &Path) -> AnyResult<()> {
    let mut files = std::fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    files.sort();
    if files != [OsString::from("store.sqlite")] {
        return Err(format!("Store file census: {files:?}").into());
    }
    let metadata = std::fs::metadata(root.join("store.sqlite"))?;
    let connection = rusqlite::Connection::open_with_flags(
        root.join("store.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    let (canonical_objects, canonical_bytes): (i64, i64) = connection.query_row(
        "SELECT count(*), coalesce(sum(length(bytes)), 0) FROM objects",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let commits: i64 =
        connection.query_row("SELECT count(*) FROM commits", [], |row| row.get(0))?;
    let canonical_objects = u64::try_from(canonical_objects)?;
    let canonical_bytes = u64::try_from(canonical_bytes)?;
    let commits = u64::try_from(commits)?;
    #[cfg(unix)]
    let allocated_bytes = {
        use std::os::unix::fs::MetadataExt;
        metadata.blocks().saturating_mul(512)
    };
    #[cfg(not(unix))]
    let allocated_bytes = metadata.len();
    println!(
        "{{\"schema\":\"fs-bench-pro-v4-store\",\"case\":\"{case}\",\"iteration\":{iteration},\"database_bytes\":{},\"allocated_bytes\":{allocated_bytes},\"page_count\":{},\"canonical_objects\":{canonical_objects},\"canonical_bytes\":{canonical_bytes},\"commits\":{commits}}}",
        metadata.len(),
        metadata.len() / (64 * 1024)
    );
    Ok(())
}

fn median(mut values: Vec<u64>) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    values[values.len() / 2]
}

fn read_u64(path: &str) -> u64 {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn linux_process_peak_rss_bytes() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                let kib = line.strip_prefix("VmHWM:")?.split_whitespace().next()?;
                kib.parse::<u64>().ok()?.checked_mul(1024)
            })
        })
        .unwrap_or(0)
}

#[cfg(not(target_os = "linux"))]
fn linux_process_peak_rss_bytes() -> u64 {
    0
}

fn nanos(start: Instant, end: Instant) -> u64 {
    end.duration_since(start)
        .as_nanos()
        .min(u128::from(u64::MAX)) as u64
}

fn elapsed_ns(start: Instant) -> u64 {
    start.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn namespace_family_registry_and_dispatch_are_exact() {
        workload_source::init_namespace::self_check().unwrap();
    }

    #[test]
    fn same_count_family_registry_and_schedules_are_exact() {
        use workload_source::edit_same_count::{self, Position};
        edit_same_count::self_check().unwrap();
        assert_eq!(edit_same_count::SCENARIOS.len(), 14);
        assert_eq!(edit_same_count::SCENARIOS[0].id, "small-edit");
        assert_eq!(edit_same_count::SCENARIOS[1].id, "edit16");
        assert_eq!(
            edit_same_count::VERIFIER_ID,
            "overwrite-fragmented-10b-ops-1000-proof"
        );
        for seed in edit_same_count::SEEDS {
            for position in [
                Position::Head,
                Position::Middle,
                Position::Tail,
                Position::Distributed,
            ] {
                let scenarios = edit_same_count::SCENARIOS
                    .iter()
                    .filter(|scenario| !scenario.frozen && scenario.position == position)
                    .copied()
                    .collect::<Vec<_>>();
                let hundred = edit_same_count::schedule(scenarios[2], seed).unwrap();
                assert_eq!(
                    edit_same_count::schedule(scenarios[0], seed).unwrap(),
                    hundred[..1]
                );
                assert_eq!(
                    edit_same_count::schedule(scenarios[1], seed).unwrap(),
                    hundred[..10]
                );
            }
        }
    }

    #[test]
    fn count_changing_family_registry_and_schedules_are_exact() {
        use workload_source::edit_count_changing;
        edit_count_changing::self_check().unwrap();
        assert_eq!(edit_count_changing::SCENARIOS.len(), 25);
        assert_eq!(
            edit_count_changing::SCENARIOS[0].id,
            "prepend-temp-copy-rename"
        );
        assert_eq!(edit_count_changing::VERIFIERS.len(), 4);
        for seed in edit_count_changing::SEEDS {
            for start in (1..edit_count_changing::SCENARIOS.len()).step_by(3) {
                let hundred =
                    edit_count_changing::schedule(edit_count_changing::SCENARIOS[start + 2], seed)
                        .unwrap();
                assert_eq!(
                    edit_count_changing::schedule(edit_count_changing::SCENARIOS[start], seed)
                        .unwrap(),
                    hundred[..1]
                );
                assert_eq!(
                    edit_count_changing::schedule(edit_count_changing::SCENARIOS[start + 1], seed)
                        .unwrap(),
                    hundred[..10]
                );
                assert!(hundred.iter().all(|edit| edit.prior_len != edit.final_len));
            }
        }
    }

    #[test]
    fn lifecycle_equations_and_median_are_exact() {
        LifecycleSample {
            workspace_create_ns: 1,
            execution_ns: 2,
            commit_api_ns: 3,
            layerstack_visible_ns: 6,
            workspace_end_ns: 4,
            complete_lifecycle_ns: 10,
            inner_write_ns: Some(1),
        }
        .validate()
        .unwrap();
        assert_eq!(median(vec![5, 1, 3]), 3);
    }
}
