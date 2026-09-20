#![cfg(feature = "native")]
//! Native heap observation, separate from process RSS, stacks and kernel buffers.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);
static TRACK: AtomicBool = AtomicBool::new(false);
struct Count;
fn change(n: isize) {
    let live = LIVE.fetch_add(n, Ordering::SeqCst) + n;
    if TRACK.load(Ordering::SeqCst) {
        PEAK.fetch_max(live, Ordering::SeqCst);
    }
}
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            change(layout.size() as isize);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        change(-(layout.size() as isize));
        unsafe { System.dealloc(p, layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, n: usize) -> *mut u8 {
        let next = unsafe { System.realloc(p, layout, n) };
        if !next.is_null() {
            change(n as isize - layout.size() as isize);
        }
        next
    }
}
#[global_allocator]
static ALLOCATOR: Count = Count;
#[test]
fn maximum_owned_envelope_and_closing_producers() {
    use std::{
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "maximum_envelope_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!(
                "envelope child timeout: {}",
                String::from_utf8_lossy(&output.stdout)
            );
        }
        std::thread::park_timeout(Duration::from_millis(5));
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    print!("{}", String::from_utf8_lossy(&output.stdout));
}

#[test]
#[ignore = "subprocess helper, invoked by maximum_owned_envelope_and_closing_producers"]
fn maximum_envelope_child() {
    use layerfs_telemetry::{
        operation::Diagnostic,
        output::{encode_operation, Collector, Identity, Output, OutputConfig},
        runtime::{Monitor, MonitorConfig},
    };
    use std::{sync::Barrier, time::Duration};
    // External OS pressure, before product workers start; no private hooks.
    let stderr = std::io::stderr();
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    let flags = OFlag::from_bits_retain(fcntl(&stderr, FcntlArg::F_GETFL).unwrap());
    fcntl(&stderr, FcntlArg::F_SETFL(flags | OFlag::O_NONBLOCK)).unwrap();
    let mut pipe_bytes = 0;
    loop {
        match nix::unistd::write(&stderr, &[b'x'; 4096]) {
            Ok(n) => {
                pipe_bytes += n;
                assert!(pipe_bytes <= 1024 * 1024);
            }
            Err(nix::errno::Errno::EAGAIN) => break,
            Err(e) => panic!("pipe fill: {e}"),
        }
    }
    let baseline = LIVE.load(Ordering::SeqCst);
    PEAK.store(baseline, Ordering::SeqCst);
    TRACK.store(true, Ordering::SeqCst);
    let identity = Identity {
        run: 192,
        pid: std::process::id(),
        role: 1,
        namespace: 1,
    };
    let monitor_config = MonitorConfig {
        cpu: true,
        memory: true,
        interval_ms: 100,
        history: 600,
        windows: 32,
    };
    let producer_output = Output::start(OutputConfig::forward()).unwrap();
    let monitor =
        Monitor::start_with_output(monitor_config, Some((producer_output.clone(), identity)))
            .unwrap()
            .unwrap();
    let windows = monitor.windows();
    let recorder = layerfs_telemetry::operation::OperationRecorder::new(
        layerfs_telemetry::timer::RecordingLimits::new(256, 32, 256 * 1024).unwrap(),
        8,
        4 * 1024 * 1024,
    )
    .unwrap()
    .with_windows(windows.clone());
    // Hosted components share one sampler and recording pool. Collector output
    // adds no sampler. Use the same public pieces assembled by Runtime.
    let component = recorder.clone();
    let (_, diagnostic) = recorder.run(0, "queue", |_| Ok::<_, ()>(()));
    let Diagnostic::Report(report) = diagnostic else {
        panic!("template report")
    };
    let template = encode_operation(identity, &report, 16384).unwrap();
    drop(report);
    let before_producer_queue = LIVE.load(Ordering::SeqCst);
    for capacity in [16384, 32768] {
        for _ in 0..256 {
            producer_output.submit(resident_envelope(&template, capacity));
        }
        assert!(LIVE.load(Ordering::SeqCst) - before_producer_queue <= 2 * 1024 * 1024 + 65536);
    }
    let mut reports = Vec::new();
    for key in 0..8 {
        let (result, report) = component.run(key, "full", |scope| {
            deep(scope, 31)?;
            for _ in 0..224 {
                scope.child("\n".repeat(128)).run(|_| Ok::<_, ()>(()))?;
            }
            Ok::<_, ()>(17)
        });
        assert_eq!(result, Ok(17));
        let Diagnostic::Report(report) = report else {
            panic!("reserved report")
        };
        reports.push(report);
    }
    let (result, excess) = recorder.run(9, "excess", |_| Ok::<_, ()>(17));
    assert_eq!(result, Ok(17));
    assert!(matches!(excess, Diagnostic::Omitted));
    let tokens: Vec<_> = (0..32).map(|_| windows.begin().unwrap()).collect();
    assert!(windows.begin().is_none());
    let mut config = OutputConfig::forward();
    config.queue_bytes = 8 * 1024 * 1024;
    config.queue_count = 256;
    config.rate = 1024 * 1024;
    config.burst = 1024 * 1024;
    let output = Output::start(config).unwrap();
    let collector = Collector::new(16, output.clone()).unwrap();
    assert!(Collector::new(17, output.clone()).is_none());
    let producers: Vec<_> = (0..16).map(|_| collector.connect().unwrap()).collect();
    assert!(collector.connect().is_none());
    let before_collector_queue = LIVE.load(Ordering::SeqCst);
    for capacity in [32768, 65536] {
        for n in 0..512 {
            assert!(producers[n % 16].submit(resident_envelope(&template, capacity)));
        }
        assert!(LIVE.load(Ordering::SeqCst) - before_collector_queue <= 8 * 1024 * 1024 + 131072);
    }
    drop(producers);
    let start = Barrier::new(9);
    let encoded_ready = Barrier::new(9);
    let release = Barrier::new(9);
    let before = native_process();
    let (encoded, observed, held) = std::thread::scope(|scope| {
        let mut jobs = Vec::new();
        for report in &reports {
            let start = &start;
            let ready = &encoded_ready;
            let release = &release;
            jobs.push(
                std::thread::Builder::new()
                    .stack_size(256 * 1024)
                    .spawn_scoped(scope, move || {
                        start.wait();
                        let values = vec![resident_envelope(
                            &encode_operation(identity, report, 16384).unwrap(),
                            16384,
                        )];
                        ready.wait();
                        release.wait();
                        values
                    })
                    .unwrap(),
            );
        }
        start.wait();
        encoded_ready.wait();
        let observed = native_process();
        let held = LIVE.load(Ordering::SeqCst) - baseline;
        println!("ENV05_SNAPSHOT heap_held={held} rss={} virtual={} threads={} before_encoder_threads={} closing_producers={}",observed.0,observed.1,observed.2,before.2,collector.occupied());
        // Always release the scoped children before making assertions.
        release.wait();
        let encoded: Vec<_> = jobs.into_iter().flat_map(|j| j.join().unwrap()).collect();
        (encoded, observed, held)
    });
    assert_eq!(encoded.len(), 8);
    assert!(encoded
        .iter()
        .all(|b| b.len() <= 16384 && b.capacity() == 16384));
    assert!(
        held >= 10 * 1024 * 1024,
        "maximum queues must really be retained: {held}"
    );
    assert!(observed.2 >= before.2 + 8);
    assert_eq!(collector.occupied(), 16);
    assert!(collector.connect().is_none());
    assert!(output.loss().dropped > 0);
    for token in tokens {
        windows.release(token);
    }
    drop(monitor);
    assert!(windows.begin().is_none());
    drop(windows);
    drop(encoded);
    drop(template);
    drop(reports);
    drop(component);
    drop(recorder);
    producer_output.shutdown(Duration::from_secs(2));
    drop(producer_output);
    output.shutdown(Duration::from_secs(2));
    assert_eq!(collector.occupied(), 0);
    assert_eq!(output.loss().failed, 1);
    let dropped = output.loss().dropped;
    output.submit(b"LFT1 {}\n".to_vec());
    assert_eq!(output.loss().dropped, dropped + 1);
    drop(collector);
    drop(output);
    let peak = PEAK.load(Ordering::SeqCst) - baseline;
    let residual = LIVE.load(Ordering::SeqCst) - baseline;
    TRACK.store(false, Ordering::SeqCst);
    assert!(
        peak <= 24 * 1024 * 1024,
        "8 MiB producer plus 16 MiB collector envelope: {peak}"
    );
    assert!(
        residual < 1024 * 1024,
        "closing ownership retained too much: {residual}"
    );
    println!("ENV05_RESULT heap_peak={peak} heap_residual={residual} producer_allowance=8388608 collector_allowance=16777216 collector_queue_allowance=8388608 recording_owners=8 encoded_owners=8 active_windows=32 collector_producers=16 telemetry_worker_stack_requests=786432 external_encoder_stack_requests=2097152 stderr_pipe_filled={pipe_bytes} closing_released=true");
}

fn deep(
    scope: &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>,
    left: u8,
) -> Result<(), ()> {
    if left > 0 {
        scope
            .child("\n".repeat(128))
            .run(|next| deep(next, left - 1))?;
    }
    Ok(())
}

// Materialize spare capacity to exercise worst resident ownership, then restore
// the original valid envelope length. No product result or fixture is modified.
fn resident_envelope(template: &[u8], capacity: usize) -> Vec<u8> {
    let mut bytes = vec![0xa5; capacity];
    bytes.truncate(0);
    bytes.extend_from_slice(template);
    bytes
}

#[cfg(target_os = "macos")]
fn native_process() -> (u64, u64, u64) {
    let info =
        libproc::proc_pid::pidinfo::<libproc::task_info::TaskInfo>(std::process::id() as i32, 0)
            .unwrap();
    (
        info.pti_resident_size,
        info.pti_virtual_size,
        info.pti_threadnum as u64,
    )
}
#[cfg(target_os = "linux")]
fn native_process() -> (u64, u64, u64) {
    use std::io::Read;
    let mut text = String::new();
    std::fs::File::open("/proc/self/status")
        .unwrap()
        .take(16384)
        .read_to_string(&mut text)
        .unwrap();
    let number = |key: &str| {
        text.lines()
            .find(|line| line.starts_with(key))
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u64>()
            .unwrap()
    };
    (
        number("VmRSS:") * 1024,
        number("VmSize:") * 1024,
        number("Threads:"),
    )
}
