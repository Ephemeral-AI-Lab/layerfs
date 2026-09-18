//! The resource instruments must be able to see what they claim to measure.
//!
//! An instrument that silently reports zero is worse than no instrument: it turns
//! `resident_pages == 0` and `swaps == 0` into statements about the instrument
//! rather than about the row. So each check below makes the instrument observe a
//! condition this process created, and the fail-closed rules are tested in the
//! direction that matters — a missing or unusable reading must be refused.
//!
//! **Concurrency caveat, stated rather than implied.** The counting allocator is
//! process-global and the test harness runs tests on parallel threads, so a heap
//! window can observe an allocation made by another test. Every test in this file
//! therefore takes one file-scoped lock, and the heap assertions are *lower bounds*
//! plus the structural invariant `peak <= charged`, both of which concurrent noise
//! can only satisfy. An equality assertion on a global counter would be a flake,
//! not a stronger test.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use fs_bench_storage_content::support::instruments::{
    cpu_now, de_warm, heap_begin, heap_end, lifetime_peak_rss_bytes, page_size, residency, swaps,
    RssBundle, RssSampler, RSS_SAMPLING_INTERVAL_NS,
};
use fs_bench_storage_content::support::window::mono_raw_ns;

/// Serialises the tests in this file, because the allocator counters are global.
static INSTRUMENT_LOCK: Mutex<()> = Mutex::new(());

/// A private scratch directory. No `tempfile` dependency: the harness lockfile is
/// compared entry by entry with the product seal, so a new crate is not an option.
fn scratch_dir(tag: &str) -> PathBuf {
    let nanos = mono_raw_ns().unwrap_or(0);
    let directory = std::env::temp_dir().join(format!(
        "fs-bench-instruments-{}-{tag}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

fn cleanup(directory: &Path) {
    let _ = std::fs::remove_dir_all(directory);
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    INSTRUMENT_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Marks the re-executed child of the heap-window test.
const HEAP_CHILD_ENV: &str = "FS_BENCH_HEAP_WINDOW_CHILD";

/// The heap window, measured in a process that is running nothing else.
///
/// `heap_begin` documents that "one phase per process gives the cleanest figure,
/// because the allocator is process-global", and that is not a stylistic remark:
/// the test harness's own scheduler thread allocates and frees while a test runs,
/// and a concurrent free between `heap_begin` and the pattern lowers `CURRENT`
/// below the window base, so `peak_incremental_bytes` understates the pattern by
/// exactly the bytes someone else released. That was observed as a peak of
/// 4,194,156 bytes for a 4,194,304-byte buffer.
///
/// Locking cannot fix it, because the interfering thread is the harness itself.
/// So the measured half of this test re-executes the test binary with only this
/// one test selected, which is the same discipline the harness applies to a real
/// row: a measured phase gets a process to itself.
#[test]
fn the_heap_window_attributes_a_known_allocation_pattern() {
    if std::env::var(HEAP_CHILD_ENV).is_err() {
        let binary = std::env::current_exe().expect("the test binary path");
        let output = std::process::Command::new(binary)
            .args([
                "--exact",
                "the_heap_window_attributes_a_known_allocation_pattern",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(HEAP_CHILD_ENV, "1")
            .output()
            .expect("re-executing the test binary");
        assert!(
            output.status.success(),
            "the dedicated-process heap window failed:\n--- stdout ---\n{}\n--- stderr ---\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    // From here down this is the child, and it is the only test running.
    const PATTERN: usize = 4 << 20;

    heap_begin();
    let mut buffer = vec![0_u8; PATTERN];
    // Touch every page so the allocation is really backed, not just reserved.
    for index in (0..PATTERN).step_by(4096) {
        buffer[index] = (index % 251) as u8;
    }
    let window = heap_end();

    assert!(
        window.peak_incremental_bytes >= PATTERN as u64,
        "a live {PATTERN}-byte buffer produced a peak of {} bytes",
        window.peak_incremental_bytes
    );
    assert!(
        window.charged_bytes >= PATTERN as i64,
        "a {PATTERN}-byte allocation charged {} bytes",
        window.charged_bytes
    );
    assert!(
        window.allocations >= 1,
        "an allocating window counted no allocation calls"
    );
    assert!(
        window.peak_incremental_bytes <= window.charged_bytes as u64,
        "the high-water mark exceeded every byte ever charged, which is impossible"
    );
    assert!(
        window.end_bytes >= window.base_bytes,
        "the window ends with fewer live bytes than it started with while a buffer is held"
    );

    drop(buffer);
}

#[test]
fn an_allocating_window_charges_more_than_a_quiet_one() {
    let _guard = lock();
    const PATTERN: usize = 2 << 20;

    heap_begin();
    let quiet = heap_end();

    heap_begin();
    let held = vec![0_u8; PATTERN];
    std::hint::black_box(&held);
    let busy = heap_end();

    assert!(
        busy.charged_bytes >= PATTERN as i64,
        "the busy window charged {} bytes for a {PATTERN}-byte allocation",
        busy.charged_bytes
    );
    assert!(
        busy.charged_bytes > quiet.charged_bytes,
        "a window holding {PATTERN} bytes charged no more than an empty one \
         ({} vs {}): the instrument is not attributing",
        busy.charged_bytes,
        quiet.charged_bytes
    );

    drop(held);
}

#[test]
fn residency_sees_a_file_this_process_just_wrote_and_de_warm_clears_it() {
    let _guard = lock();
    let page = page_size().expect("the page size must be readable on a unix host");
    let directory = scratch_dir("residency");
    let path = directory.join("probe.bin");
    let payload: Vec<u8> = (0..=255_u8).cycle().take(page as usize * 4).collect();
    std::fs::write(&path, &payload).expect("writing the probe");
    // Touch it: this is the *check*, not a de-warm strategy, and it is never
    // applied to a measured artifact.
    let _ = std::fs::read(&path).expect("reading the probe");

    let before = residency(&path).expect("a residency reading");
    assert_eq!(before.length_bytes, payload.len() as u64);
    assert_eq!(before.page_size_bytes, page);
    assert_eq!(before.total_pages, 4, "four pages of file were written");
    assert!(
        before.resident_pages > 0,
        "a freshly written and read file reports zero resident pages: the instrument \
         cannot see what it is measuring"
    );
    assert!(!before.is_dewarmed());

    let report = de_warm(&path).expect("a de-warm");
    assert!(
        report.resident_first > 0,
        "de-warm saw nothing resident, so it proves nothing"
    );
    assert!(
        report.invalidated,
        "de-warm did not issue msync(MS_INVALIDATE)"
    );
    assert_eq!(
        report.resident_after, 0,
        "de-warm left {} of {} pages resident",
        report.resident_after, report.total_pages
    );
    let after = residency(&path).expect("a second residency reading");
    assert!(after.is_dewarmed(), "the de-warmed file is still resident");

    cleanup(&directory);
}

#[test]
fn a_zero_length_file_is_resident_by_definition_not_by_reading() {
    let _guard = lock();
    let directory = scratch_dir("empty");
    let path = directory.join("empty.bin");
    std::fs::write(&path, b"").expect("writing an empty file");
    let reading = residency(&path).expect("a residency reading of an empty file");
    assert_eq!(reading.length_bytes, 0);
    assert_eq!(reading.total_pages, 0);
    assert_eq!(reading.resident_pages, 0);
    assert!(reading.is_dewarmed());
    // Nothing to invalidate, so the de-warm must not claim it did.
    let report = de_warm(&path).expect("a de-warm of an empty file");
    assert!(!report.invalidated);
    assert_eq!(report.resident_after, 0);
    cleanup(&directory);
}

#[test]
fn the_page_size_is_read_rather_than_assumed() {
    let page = page_size().expect("the page size must be readable on a unix host");
    assert!(
        page.is_power_of_two(),
        "a page size of {page} is not a power of two"
    );
    assert!(
        page >= 4096,
        "a page size of {page} is below the platform minimum"
    );
    assert!(page <= 65_536, "a page size of {page} is implausible");
}

#[test]
fn the_rss_bundle_refuses_a_peak_it_cannot_stand_behind() {
    let usable = RssBundle {
        sampling_interval_ns: RSS_SAMPLING_INTERVAL_NS,
        sample_count: 10,
        first_sample_ns: 1,
        last_sample_ns: 100_000_000,
        maximum_sample_gap_ns: RSS_SAMPLING_INTERVAL_NS,
        baseline_bytes: 1 << 20,
        phase_peak_bytes: 2 << 20,
        incremental_peak_bytes: 1 << 20,
        final_bytes: 2 << 20,
        unavailable_samples: 0,
    };
    assert!(usable.peak_is_usable(), "a complete bundle must be usable");

    // A missed sample means the platform could not produce the reading, so the
    // peak is unavailable rather than merely uncertain.
    assert!(
        !RssBundle {
            unavailable_samples: 1,
            ..usable
        }
        .peak_is_usable(),
        "a missed sample must make the peak unusable"
    );
    // One sample cannot show a peak at all.
    assert!(!RssBundle {
        sample_count: 1,
        ..usable
    }
    .peak_is_usable());
    assert!(!RssBundle {
        sample_count: 0,
        ..usable
    }
    .peak_is_usable());
    // A gap over twice the declared interval is a hole in the trace.
    assert!(
        !RssBundle {
            maximum_sample_gap_ns: 2 * RSS_SAMPLING_INTERVAL_NS + 1,
            ..usable
        }
        .peak_is_usable(),
        "a gap beyond twice the declared interval must make the peak unusable"
    );
    // The bound is inclusive: exactly twice the interval is still usable.
    assert!(RssBundle {
        maximum_sample_gap_ns: 2 * RSS_SAMPLING_INTERVAL_NS,
        ..usable
    }
    .peak_is_usable());
    // The all-zero default is the fail-closed case: zero samples is not a peak of
    // zero, it is no reading at all.
    assert!(
        !RssBundle::default().peak_is_usable(),
        "the default bundle must not be reported as a usable zero peak"
    );
    assert_eq!(
        RSS_SAMPLING_INTERVAL_NS, 10_000_000,
        "the declared rate is 10 ms"
    );
}

#[test]
fn the_rss_sampler_takes_a_series_and_keeps_its_own_interval() {
    let _guard = lock();
    let sampler = RssSampler::start();
    std::thread::sleep(Duration::from_millis(120));
    let bundle = sampler.stop();

    assert_eq!(bundle.sampling_interval_ns, RSS_SAMPLING_INTERVAL_NS);
    assert!(
        bundle.sample_count > 1,
        "a 120 ms window at 10 ms produced {} samples",
        bundle.sample_count
    );
    assert!(
        bundle.baseline_bytes > 0,
        "this process must have resident memory to sample"
    );
    assert!(
        bundle.phase_peak_bytes >= bundle.baseline_bytes,
        "the peak is below the baseline, which the running maximum cannot produce"
    );
    assert!(bundle.incremental_peak_bytes <= bundle.phase_peak_bytes);
    assert!(
        bundle.first_sample_ns > 0,
        "the first sample carries no clock reading"
    );
    assert!(bundle.last_sample_ns >= bundle.first_sample_ns);
    assert_eq!(
        bundle.unavailable_samples, 0,
        "this host failed to produce an RSS reading"
    );
}

#[test]
fn the_rusage_readings_are_available_on_this_host() {
    let _guard = lock();
    let cpu = cpu_now().expect("getrusage(RUSAGE_SELF) must be readable on a unix host");
    assert!(
        cpu.user_ns > 0 || cpu.system_ns > 0,
        "a running process has consumed no CPU"
    );
    // The swap gate is a hard failure on a non-zero count, so a reading that cannot
    // be taken must be `None` rather than a fabricated zero. This asserts the
    // instrument answers, not what it answers.
    assert!(
        swaps().is_some(),
        "the swap counter must be readable on this host"
    );
    let lifetime = lifetime_peak_rss_bytes().expect("ru_maxrss must be readable on a unix host");
    assert!(
        lifetime > 0,
        "a process that has already allocated reports a lifetime peak of zero"
    );
}
