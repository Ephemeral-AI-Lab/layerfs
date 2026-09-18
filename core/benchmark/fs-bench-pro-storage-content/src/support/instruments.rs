//! The resource instruments: heap, RSS, CPU, residency and device attestation.
//!
//! Three independent memory instruments with three different meanings, never
//! collapsed into one field called "memory":
//!
//! * **heap** — a counting `GlobalAlloc`. Exact requested bytes, deterministic for
//!   a fixed input, and the **only** valid gate for an O(1)-memory claim.
//! * **RSS** — a 10 ms sampler thread over `proc_pid_rusage` (macOS) or
//!   `/proc/self/status` (Linux). It cannot cover a phase under ~200 ms, so it is a
//!   G4 bound and an anomaly detector, never the gate.
//! * **residency** — `mincore` over an `mmap`, for `INELIGIBLE` decisions. It
//!   cannot distinguish a cache-served read from a device read, which is why
//!   `disk_read_bytes` exists beside it.
//!
//! `ps -o rss= -p <pid>` is **not** used: it forks a process per sample, capping the
//! rate near 100 Hz and perturbing what is being measured.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::window::mono_raw_ns;

// ---------------------------------------------------------------------------
// Heap: counting global allocator
// ---------------------------------------------------------------------------

/// Live bytes requested from the allocator.
static CURRENT: AtomicUsize = AtomicUsize::new(0);
/// High-water mark of `CURRENT` above the window base.
static PEAK: AtomicUsize = AtomicUsize::new(0);
/// Window base snapshot.
static BASE: AtomicUsize = AtomicUsize::new(0);
/// Allocation calls inside the window.
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
/// Deallocation calls inside the window.
static DEALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
/// Bytes requested by those calls.
static CHARGED: AtomicUsize = AtomicUsize::new(0);

/// Counting allocator over the system allocator.
///
/// Precision: *exact requested bytes*. It excludes allocator metadata, alignment
/// padding and any `mmap` the allocator serves outside `alloc`, so it is a lower
/// bound on true heap — stated rather than implied.
struct Counting;

impl Counting {
    fn record(size: usize) {
        let current = CURRENT.fetch_add(size, Ordering::Relaxed) + size;
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        CHARGED.fetch_add(size, Ordering::Relaxed);
        let base = BASE.load(Ordering::Relaxed);
        if current > base {
            PEAK.fetch_max(current - base, Ordering::Relaxed);
        }
    }
}

// SAFETY: every call forwards to the system allocator with the same layout it was
// given, and the counters are relaxed atomics that cannot fail. No allocation is
// served from this type itself, so the accounting cannot recurse.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            Counting::record(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
        DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let resized = unsafe { System.realloc(pointer, layout, new_size) };
        if !resized.is_null() {
            CURRENT.fetch_sub(layout.size(), Ordering::Relaxed);
            Counting::record(new_size);
        }
        resized
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// One heap window, every field named for what it measures.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HeapWindow {
    /// Live bytes when the window opened.
    pub base_bytes: u64,
    /// Live bytes when the window closed.
    pub end_bytes: u64,
    /// Largest `CURRENT - base` observed inside the window.
    pub peak_incremental_bytes: u64,
    /// Net change across the window; negative when the phase released more than it
    /// took, which is why it is signed.
    pub charged_bytes: i64,
    /// Allocation calls inside the window.
    pub allocations: u64,
    /// Deallocation calls inside the window.
    pub deallocations: u64,
}

/// Opens a heap window. One phase per process gives the cleanest figure, because
/// the allocator is process-global.
pub fn heap_begin() {
    let current = CURRENT.load(Ordering::Relaxed);
    BASE.store(current, Ordering::Relaxed);
    PEAK.store(0, Ordering::Relaxed);
    ALLOCATIONS.store(0, Ordering::Relaxed);
    DEALLOCATIONS.store(0, Ordering::Relaxed);
    CHARGED.store(0, Ordering::Relaxed);
}

/// Closes a heap window and reads it back.
pub fn heap_end() -> HeapWindow {
    let base = BASE.load(Ordering::Relaxed);
    let end = CURRENT.load(Ordering::Relaxed);
    let peak = PEAK.load(Ordering::Relaxed);
    let charged = CHARGED.load(Ordering::Relaxed);
    HeapWindow {
        base_bytes: base as u64,
        end_bytes: end as u64,
        peak_incremental_bytes: peak as u64,
        charged_bytes: charged as i64,
        allocations: ALLOCATIONS.load(Ordering::Relaxed) as u64,
        deallocations: DEALLOCATIONS.load(Ordering::Relaxed) as u64,
    }
}

// ---------------------------------------------------------------------------
// Native rusage: RSS, CPU, swaps and device attestation
// ---------------------------------------------------------------------------

/// `struct timeval`, C layout on Darwin and Linux.
///
/// The two platforms split the same sixteen bytes differently: Darwin's
/// `tv_usec` is a 32-bit `suseconds_t`, Linux's is a 64-bit `suseconds_t`. Reading
/// the low half is correct on both; the high half is only meaningful on Linux, so
/// [`Timeval::micros`] is `cfg`-gated rather than guessed.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Timeval {
    tv_sec: i64,
    tv_usec_low: i32,
    #[cfg(target_os = "linux")]
    tv_usec_high: i32,
    #[cfg(target_os = "macos")]
    tv_usec_padding: i32,
}

impl Timeval {
    #[cfg(target_os = "macos")]
    fn micros(self) -> i64 {
        i64::from(self.tv_usec_low)
    }

    #[cfg(target_os = "linux")]
    fn micros(self) -> i64 {
        (i64::from(self.tv_usec_high) << 32) | i64::from(self.tv_usec_low as u32)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn micros(self) -> i64 {
        i64::from(self.tv_usec_low)
    }

    fn nanos(self) -> i128 {
        i128::from(self.tv_sec) * 1_000_000_000 + i128::from(self.micros()) * 1_000
    }
}

/// `struct rusage`, C layout. Only the first two fields are read as time; the
/// remaining fields keep the buffer the size `getrusage` writes, which is the
/// buffer-overrun defect the earlier sketch in the resource document had.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct NativeRusage {
    utime: Timeval,
    stime: Timeval,
    max_rss: i64,
    integral_shared_memory: i64,
    integral_resident_set: i64,
    integral_resident_stack: i64,
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

#[cfg(unix)]
unsafe extern "C" {
    fn getrusage(who: i32, usage: *mut NativeRusage) -> i32;
}

/// `RUSAGE_SELF`.
const RUSAGE_SELF: i32 = 0;

/// CPU time consumed, split user and system.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CpuReading {
    /// User-mode nanoseconds.
    pub user_ns: u64,
    /// System-mode nanoseconds.
    pub system_ns: u64,
}

/// Reads `getrusage(RUSAGE_SELF)`.
pub fn cpu_now() -> Option<CpuReading> {
    #[cfg(unix)]
    {
        let mut usage = NativeRusage::default();
        // SAFETY: getrusage writes exactly one C-layout rusage structure.
        let status = unsafe { getrusage(RUSAGE_SELF, std::ptr::from_mut(&mut usage)) };
        if status != 0 {
            return None;
        }
        Some(CpuReading {
            user_ns: u64::try_from(usage.utime.nanos()).ok()?,
            system_ns: u64::try_from(usage.stime.nanos()).ok()?,
        })
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Swaps observed by this process. A non-zero value is a hard failure, so a
/// missing reading is reported as unavailable rather than as zero.
pub fn swaps() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut usage = NativeRusage::default();
        // SAFETY: getrusage writes exactly one C-layout rusage structure.
        let status = unsafe { getrusage(RUSAGE_SELF, std::ptr::from_mut(&mut usage)) };
        if status != 0 {
            return None;
        }
        u64::try_from(usage.swaps).ok()
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Peak RSS this process ever reached, in bytes (`ru_maxrss`).
///
/// This is a **lifetime** high-water mark, not a phase peak. It is reported as
/// such and never substituted for [`RssBundle::incremental_peak_bytes`].
pub fn lifetime_peak_rss_bytes() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut usage = NativeRusage::default();
        // SAFETY: getrusage writes exactly one C-layout rusage structure.
        let status = unsafe { getrusage(RUSAGE_SELF, std::ptr::from_mut(&mut usage)) };
        if status != 0 {
            return None;
        }
        let peak = u64::try_from(usage.max_rss).ok()?;
        // Linux reports kilobytes; Darwin reports bytes.
        #[cfg(target_os = "linux")]
        let peak = peak.checked_mul(1024)?;
        Some(peak)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Darwin's `rusage_info_v2`, the buffer `proc_pid_rusage` writes.
#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy, Default)]
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
    process_start_abstime: u64,
    process_exit_abstime: u64,
    child_user_time: u64,
    child_system_time: u64,
    child_package_idle_wakeups: u64,
    child_interrupt_wakeups: u64,
    child_pageins: u64,
    child_elapsed_abstime: u64,
    disk_read_bytes: u64,
    disk_write_bytes: u64,
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut std::ffi::c_void) -> i32;
}

/// `RUSAGE_INFO_V2`.
#[cfg(target_os = "macos")]
const RUSAGE_INFO_V2: i32 = 2;

/// Darwin's per-process readings this harness needs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessUsage {
    /// Resident bytes right now.
    pub resident_bytes: u64,
    /// Bytes this process has read from the device.
    pub disk_read_bytes: u64,
}

/// Reads the process's own residency and device reads.
///
/// `disk_read_bytes` is the **device attestation** instrument: a row claiming a
/// cold or de-warmed read must show `disk_read_bytes >= 0.9 x requested`, because
/// `mincore` alone cannot tell a cache-served read from a device read.
pub fn process_usage() -> Option<ProcessUsage> {
    #[cfg(target_os = "macos")]
    {
        let mut usage = DarwinRusageInfoV2::default();
        let pid = i32::try_from(std::process::id()).ok()?;
        // SAFETY: proc_pid_rusage receives a correctly sized C-layout V2 buffer.
        let status = unsafe { proc_pid_rusage(pid, RUSAGE_INFO_V2, std::ptr::from_mut(&mut usage).cast()) };
        if status != 0 {
            return None;
        }
        Some(ProcessUsage {
            resident_bytes: usage.resident_size,
            disk_read_bytes: usage.disk_read_bytes,
        })
    }
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        let resident = status
            .lines()
            .find_map(|line| line.strip_prefix("VmRSS:")?.split_whitespace().next())?
            .parse::<u64>()
            .ok()?
            .checked_mul(1024)?;
        let disk = std::fs::read_to_string("/proc/self/io")
            .ok()
            .and_then(|text| {
                text.lines()
                    .find_map(|line| line.strip_prefix("read_bytes:")?.split_whitespace().next())
                    .and_then(|value| value.parse::<u64>().ok())
            })
            .unwrap_or(0);
        Some(ProcessUsage {
            resident_bytes: resident,
            disk_read_bytes: disk,
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Nominal sampler interval: 10 ms, the rate the resource document declares.
pub const RSS_SAMPLING_INTERVAL_NS: u64 = 10_000_000;

/// The receipt bundle a 10 ms RSS sampler must publish.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RssBundle {
    /// Declared sampling interval in nanoseconds.
    pub sampling_interval_ns: u64,
    /// Samples taken.
    pub sample_count: u64,
    /// Clock reading of the first sample.
    pub first_sample_ns: u64,
    /// Clock reading of the last sample.
    pub last_sample_ns: u64,
    /// Largest gap between consecutive samples.
    pub maximum_sample_gap_ns: u64,
    /// The first sample: where the phase started from.
    pub baseline_bytes: u64,
    /// Largest sample seen.
    pub phase_peak_bytes: u64,
    /// `phase_peak_bytes` above `baseline_bytes`, saturating at zero.
    pub incremental_peak_bytes: u64,
    /// The last sample.
    pub final_bytes: u64,
    /// Samples the platform could not produce.
    pub unavailable_samples: u64,
}

impl RssBundle {
    /// A phase whose peak is unusable must be `INELIGIBLE`, never quietly fast.
    ///
    /// The rule is fail-closed in both directions: a missed sample or a gap over
    /// twice the declared interval makes the peak unavailable.
    pub fn peak_is_usable(&self) -> bool {
        self.unavailable_samples == 0
            && self.sample_count > 1
            && self.maximum_sample_gap_ns <= 2 * self.sampling_interval_ns
    }
}

struct RssShared {
    stop: AtomicBool,
    samples: AtomicU64,
    first_ns: AtomicU64,
    last_ns: AtomicU64,
    max_gap_ns: AtomicU64,
    baseline: AtomicU64,
    peak: AtomicU64,
    final_bytes: AtomicU64,
    unavailable: AtomicU64,
}

/// A 10 ms sampler thread over this process's own RSS.
pub struct RssSampler {
    shared: Arc<RssShared>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl RssSampler {
    /// Starts sampling. The first sample is taken inline, so `baseline_bytes` is
    /// the phase's starting point rather than whatever the thread happens to see.
    pub fn start() -> Self {
        let shared = Arc::new(RssShared {
            stop: AtomicBool::new(false),
            samples: AtomicU64::new(0),
            first_ns: AtomicU64::new(0),
            last_ns: AtomicU64::new(0),
            max_gap_ns: AtomicU64::new(0),
            baseline: AtomicU64::new(0),
            peak: AtomicU64::new(0),
            final_bytes: AtomicU64::new(0),
            unavailable: AtomicU64::new(0),
        });
        let worker = Arc::clone(&shared);
        let handle = std::thread::spawn(move || {
            let mut previous = 0_u64;
            while !worker.stop.load(Ordering::Relaxed) {
                let stamp = mono_raw_ns().unwrap_or(0);
                match process_usage() {
                    Some(usage) => {
                        let count = worker.samples.fetch_add(1, Ordering::Relaxed);
                        if count == 0 {
                            worker.baseline.store(usage.resident_bytes, Ordering::Relaxed);
                            worker.first_ns.store(stamp, Ordering::Relaxed);
                        }
                        worker.peak.fetch_max(usage.resident_bytes, Ordering::Relaxed);
                        worker.final_bytes.store(usage.resident_bytes, Ordering::Relaxed);
                        worker.last_ns.store(stamp, Ordering::Relaxed);
                        if previous != 0 {
                            worker
                                .max_gap_ns
                                .fetch_max(stamp.saturating_sub(previous), Ordering::Relaxed);
                        }
                        previous = stamp;
                    }
                    None => {
                        worker.unavailable.fetch_add(1, Ordering::Relaxed);
                    }
                }
                std::thread::sleep(Duration::from_nanos(RSS_SAMPLING_INTERVAL_NS));
            }
        });
        Self {
            shared,
            handle: Some(handle),
        }
    }

    /// Reads the bundle without stopping the thread.
    pub fn snapshot(&self) -> RssBundle {
        RssBundle {
            sampling_interval_ns: RSS_SAMPLING_INTERVAL_NS,
            sample_count: self.shared.samples.load(Ordering::Relaxed),
            first_sample_ns: self.shared.first_ns.load(Ordering::Relaxed),
            last_sample_ns: self.shared.last_ns.load(Ordering::Relaxed),
            maximum_sample_gap_ns: self.shared.max_gap_ns.load(Ordering::Relaxed),
            baseline_bytes: self.shared.baseline.load(Ordering::Relaxed),
            phase_peak_bytes: self.shared.peak.load(Ordering::Relaxed),
            incremental_peak_bytes: self
                .shared
                .peak
                .load(Ordering::Relaxed)
                .saturating_sub(self.shared.baseline.load(Ordering::Relaxed)),
            final_bytes: self.shared.final_bytes.load(Ordering::Relaxed),
            unavailable_samples: self.shared.unavailable.load(Ordering::Relaxed),
        }
    }

    /// Stops the thread and returns the final bundle.
    pub fn stop(mut self) -> RssBundle {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        self.snapshot()
    }
}

// ---------------------------------------------------------------------------
// Residency: mmap, mincore, msync(MS_INVALIDATE)
// ---------------------------------------------------------------------------

/// `MS_INVALIDATE`, identical on Darwin and Linux.
pub const MS_INVALIDATE: i32 = 0x0002;

#[cfg(target_os = "macos")]
/// `_SC_PAGESIZE` on Darwin.
const SC_PAGESIZE: i32 = 29;
#[cfg(target_os = "linux")]
/// `_SC_PAGESIZE` on Linux.
const SC_PAGESIZE: i32 = 30;

#[cfg(unix)]
unsafe extern "C" {
    fn sysconf(name: i32) -> i64;
    fn mmap(
        addr: *mut std::ffi::c_void,
        length: usize,
        prot: i32,
        flags: i32,
        fd: i32,
        offset: i64,
    ) -> *mut std::ffi::c_void;
    fn munmap(addr: *mut std::ffi::c_void, length: usize) -> i32;
    fn mincore(addr: *mut std::ffi::c_void, length: usize, vec: *mut u8) -> i32;
    fn msync(addr: *mut std::ffi::c_void, length: usize, flags: i32) -> i32;
}

#[cfg(unix)]
const PROT_READ: i32 = 0x1;
#[cfg(unix)]
const MAP_SHARED: i32 = 0x0001;
/// `MAP_FAILED` is `(void *)-1` on Darwin and Linux alike.
#[cfg(unix)]
const MAP_FAILED_SENTINEL: isize = -1;

/// Page size of this host.
///
/// Read from `sysconf` rather than assumed: an Apple Silicon host uses 16 KiB
/// pages, and a hardcoded 4 KiB would size the `mincore` vector wrongly and
/// misreport residency.
pub fn page_size() -> Option<u64> {
    #[cfg(unix)]
    {
        // SAFETY: sysconf is a pure query.
        let value = unsafe { sysconf(SC_PAGESIZE) };
        u64::try_from(value).ok().filter(|size| *size > 0)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// What a residency reading measured.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Residency {
    /// Bytes in the mapped file.
    pub length_bytes: u64,
    /// Page size the count is expressed in.
    pub page_size_bytes: u64,
    /// Pages the kernel reports resident.
    pub resident_pages: u64,
    /// Pages examined.
    pub total_pages: u64,
}

impl Residency {
    /// A row claiming a de-warmed or cold read needs exactly this.
    pub fn is_dewarmed(&self) -> bool {
        self.resident_pages == 0
    }
}

/// Result of the ordered de-warm sequence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeWarmReport {
    /// Residency before any invalidation.
    pub resident_first: u64,
    /// Residency after `msync(MS_INVALIDATE)`.
    pub resident_after: u64,
    /// Total pages.
    pub total_pages: u64,
    /// Whether `msync(MS_INVALIDATE)` was issued at all.
    pub invalidated: bool,
}

/// Reads residency of `path` through an `mmap` and `mincore`.
pub fn residency(path: &std::path::Path) -> std::io::Result<Residency> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;

        let page = page_size()
            .ok_or_else(|| std::io::Error::other("page size unavailable on this host"))?;
        let file = std::fs::File::open(path)?;
        let length = file.metadata()?.len();
        if length == 0 {
            return Ok(Residency {
                length_bytes: 0,
                page_size_bytes: page,
                resident_pages: 0,
                total_pages: 0,
            });
        }
        let mapped = usize::try_from(length)
            .map_err(|_| std::io::Error::other("mapping larger than this address space"))?;
        // SAFETY: mmap of a file this process owns; the returned pointer is checked
        // against MAP_FAILED before any use, and munmap is called on all paths.
        let address = unsafe {
            mmap(
                std::ptr::null_mut(),
                mapped,
                PROT_READ,
                MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if address as isize == MAP_FAILED_SENTINEL {
            return Err(std::io::Error::last_os_error());
        }
        let total_pages = mapped.div_ceil(page as usize);
        let mut vector = vec![0_u8; total_pages];
        // SAFETY: the mapping is live for `mapped` bytes and the vector holds at
        // least ceil(mapped / page) entries, which is what mincore writes.
        let status = unsafe { mincore(address, mapped, vector.as_mut_ptr()) };
        if status != 0 {
            let error = std::io::Error::last_os_error();
            // SAFETY: unmapping the region mmap just created.
            unsafe { munmap(address, mapped) };
            return Err(error);
        }
        let resident_pages = vector.iter().filter(|byte| **byte & 1 == 1).count() as u64;
        // SAFETY: unmapping the region mmap just created.
        unsafe { munmap(address, mapped) };
        Ok(Residency {
            length_bytes: length,
            page_size_bytes: page,
            resident_pages,
            total_pages: total_pages as u64,
        })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(std::io::Error::other("residency is unsupported on this host"))
    }
}

/// The ordered de-warm sequence: `mincore` -> `msync(MS_INVALIDATE)` only when
/// `resident_first > 0` -> `mincore`.
///
/// **Never touch-every-page.** Reading the whole file to "flush" it is why v0.1.6
/// paid a measured 18.57 s for a 100k-file fixture, and it warms the very pages it
/// claims to evict. The `msync` is skipped when nothing is resident, so the common
/// case costs one `mincore`.
pub fn de_warm(path: &std::path::Path) -> std::io::Result<DeWarmReport> {
    #[cfg(unix)]
    {
        let first = residency(path)?;
        if first.resident_pages == 0 || first.length_bytes == 0 {
            return Ok(DeWarmReport {
                resident_first: 0,
                resident_after: 0,
                total_pages: first.total_pages,
                invalidated: false,
            });
        }
        use std::os::unix::io::AsRawFd;

        let file = std::fs::OpenOptions::new().read(true).write(true).open(path)?;
        let mapped = usize::try_from(first.length_bytes)
            .map_err(|_| std::io::Error::other("mapping larger than this address space"))?;
        // SAFETY: as `residency`: a mapping this process owns, checked before use.
        let address = unsafe {
            mmap(
                std::ptr::null_mut(),
                mapped,
                PROT_READ,
                MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if address as isize == MAP_FAILED_SENTINEL {
            return Err(std::io::Error::last_os_error());
        }
        // SAFETY: invalidating a live MAP_SHARED mapping of the same length.
        let status = unsafe { msync(address, mapped, MS_INVALIDATE) };
        let error = std::io::Error::last_os_error();
        // SAFETY: unmapping the region mmap just created.
        unsafe { munmap(address, mapped) };
        if status != 0 {
            return Err(error);
        }
        let after = residency(path)?;
        Ok(DeWarmReport {
            resident_first: first.resident_pages,
            resident_after: after.resident_pages,
            total_pages: first.total_pages,
            invalidated: true,
        })
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err(std::io::Error::other("de-warm is unsupported on this host"))
    }
}

/// Space: allocated bytes from `st_blocks`, and apparent bytes from `st_size`.
///
/// The two are **never pooled**: they answer different questions, and a COW clone's
/// `st_blocks` double-counts blocks shared with its master, which is exactly why
/// the reflink rung is forbidden for any row that gates allocated bytes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Space {
    /// `st_size`: apparent bytes.
    pub apparent_bytes: u64,
    /// `st_blocks * 512`: allocated bytes, as the filesystem reports them.
    pub allocated_bytes: u64,
}

/// Reads allocated and apparent bytes for one path.
pub fn space(path: &std::path::Path) -> std::io::Result<Space> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = std::fs::symlink_metadata(path)?;
        Ok(Space {
            apparent_bytes: metadata.size(),
            allocated_bytes: metadata.blocks().saturating_mul(512),
        })
    }
    #[cfg(not(unix))]
    {
        let metadata = std::fs::symlink_metadata(path)?;
        Ok(Space {
            apparent_bytes: metadata.len(),
            allocated_bytes: metadata.len(),
        })
    }
}
