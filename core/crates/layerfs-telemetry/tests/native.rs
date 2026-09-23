#![cfg(feature = "native")]
use layerfs_telemetry::{
    operation::{Diagnostic, OperationRecorder},
    output::{encode_operation, Collector, Identity, Output, OutputConfig, OutputMode},
    runtime::{Configuration, Monitor, MonitorConfig, Runtime},
    timer::RecordingLimits,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
fn directory() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "layerfs-telemetry-{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}
#[test]
fn native_monitor_retains_active_window_across_ring_wrap() {
    let monitor = Monitor::start(MonitorConfig {
        cpu: true,
        memory: true,
        interval_ms: 10,
        history: 1,
        windows: 1,
    })
    .unwrap()
    .unwrap();
    let windows = monitor.windows();
    let token = windows.begin().unwrap();
    assert!(windows.begin().is_none());
    let end = Instant::now() + Duration::from_secs(2);
    while windows.finish(token).unwrap().samples < 5 && Instant::now() < end {
        std::thread::park_timeout(Duration::from_millis(10));
    }
    let report = windows.finish(token).unwrap();
    assert!(report.samples >= 5);
    assert!(report.sampled_max_rss.unwrap() > 0);
    assert!(report.first.unwrap().incarnation > 0);
    assert!(report.cpu_delta().is_some());
    windows.release(token);
    let next = windows.begin().unwrap();
    windows.release(token);
    assert!(windows.begin().is_none());
    windows.release(next);
    assert!(windows.begin().is_some());
}
#[test]
fn owned_retention_queue_and_producer_churn() {
    let root = directory();
    std::fs::create_dir(&root).unwrap();
    let evidence = root.join("benchmark-evidence.json");
    std::fs::write(&evidence, b"append-only").unwrap();
    let namespace = root.join("operational");
    let config = || {
        let mut c = OutputConfig::forward();
        c.mode = OutputMode::Local;
        c.directory = Some(namespace.clone());
        c.record_bytes = 1024;
        c.segment_bytes = 1024;
        c.segments = 2;
        c.queue_bytes = 32768;
        c.queue_count = 32;
        c.rate = 1024 * 1024;
        c.burst = 1024 * 1024;
        c
    };
    let output = Output::start(config()).unwrap();
    assert!(Output::start(config()).is_err());
    let collector = Collector::new(2, output.clone()).unwrap();
    let a = collector.connect().unwrap();
    let b = collector.connect().unwrap();
    assert!(collector.connect().is_none());
    drop(b);
    for _ in 0..100 {
        let producer = collector.connect().unwrap();
        drop(producer);
    }
    for n in 0..100 {
        assert!(a.submit(
            format!("LFT1 {{\"n\":{n},\"padding\":\"{}\"}}\n", "x".repeat(800)).into_bytes()
        ));
    }
    output.submit(vec![0; 2000]);
    drop(a);
    output.shutdown(Duration::from_secs(2));
    assert_eq!(collector.occupied(), 0);
    let entries: Vec<_> = std::fs::read_dir(&namespace)
        .unwrap()
        .map(Result::unwrap)
        .filter(|e| e.file_name().to_string_lossy().starts_with("segment-"))
        .collect();
    assert!(entries.len() <= 2);
    assert!(!entries.is_empty());
    for entry in entries {
        assert!(entry.metadata().unwrap().len() <= 1024);
    }
    assert_eq!(std::fs::read(&evidence).unwrap(), b"append-only");
    assert!(output.loss().dropped > 0);
    let reopened = Output::start(config()).unwrap();
    reopened.submit(b"LFT1 {\"reopened\":true}\n".to_vec());
    reopened.shutdown(Duration::from_secs(2));
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn disabled_runtime_ignores_invalid_native_configuration() {
    let path = directory();
    let mut output = OutputConfig::forward();
    output.directory = Some(path.clone());
    output.mode = OutputMode::Local;
    output.queue_bytes = usize::MAX;
    let runtime = Runtime::start(Configuration {
        enabled: false,
        timing: true,
        monitor: MonitorConfig {
            cpu: true,
            memory: true,
            interval_ms: 0,
            history: usize::MAX,
            windows: usize::MAX,
        },
        output,
        identity: Identity {
            run: 1,
            pid: 1,
            role: 1,
            namespace: 1,
        },
    })
    .unwrap();
    assert!(matches!(
        runtime.recorder().run(1, "off", |_| Ok::<_, ()>(())).1,
        Diagnostic::Disabled
    ));
    drop(runtime);
    assert!(!path.exists());
}
#[test]
fn output_failure_is_independent_and_encoding_is_valid() {
    let path = directory();
    let mut c = OutputConfig::forward();
    c.mode = OutputMode::Local;
    c.directory = Some(path.clone());
    let output = Output::start(c).unwrap();
    std::fs::remove_dir_all(&path).unwrap();
    output.submit(b"LFT1 {\"value\":1}\n".to_vec());
    output.shutdown(Duration::from_secs(2));
    assert_eq!(output.loss().failed, 1);
    assert!(!path.exists());
    let recorder = OperationRecorder::new(
        RecordingLimits::new(256, 32, 256 * 1024).unwrap(),
        1,
        512 * 1024,
    )
    .unwrap();
    let (result, diagnostic) = recorder.run(7, "original", |s| {
        for _ in 0..255 {
            s.child("a\n\"label").run(|_| Ok::<_, ()>(()))?;
        }
        Err::<(), _>(())
    });
    assert_eq!(result, Err(()));
    let Diagnostic::Report(report) = diagnostic else {
        panic!("report")
    };
    let encoded = encode_operation(
        Identity {
            run: 192,
            pid: std::process::id(),
            role: 1,
            namespace: 1,
        },
        &report,
        1024,
    )
    .unwrap();
    assert!(encoded.len() <= 1024);
    assert_eq!(encoded.iter().filter(|b| **b == b'\n').count(), 1);
    use std::io::Write;
    let mut child=std::process::Command::new("python3").args(["-c","import json,sys; v=json.load(sys.stdin); assert v['success'] is False and v['timing']['incomplete'] is True"]).stdin(std::process::Stdio::piped()).spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&encoded[5..])
        .unwrap();
    assert!(child.wait().unwrap().success());
}

#[cfg(target_os = "macos")]
#[test]
fn native_cpu_units_match_posix_process_counters() {
    use nix::sys::{
        resource::{getrusage, UsageWho},
        time::TimeValLike,
    };
    let mut value = 7u64;
    for _ in 0..100000 {
        value = value.wrapping_mul(6364136223846793005).wrapping_add(1);
        std::hint::black_box(value);
    }
    let before = getrusage(UsageWho::RUSAGE_SELF).unwrap();
    let monitor = Monitor::start(MonitorConfig {
        cpu: true,
        memory: true,
        interval_ms: 10,
        history: 1,
        windows: 1,
    })
    .unwrap()
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    let sample = loop {
        if let Some(sample) = monitor.latest() {
            break sample;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    let after = getrusage(UsageWho::RUSAGE_SELF).unwrap();
    let lower = before.user_time().num_microseconds() as u64 * 1000;
    let upper = after.user_time().num_microseconds() as u64 * 1000;
    assert!(
        sample.user_ns.unwrap() >= lower && sample.user_ns.unwrap() <= upper,
        "native user_ns={:?}, POSIX bracket={lower}..={upper}",
        sample.user_ns
    );
}

#[test]
fn retention_expiry_open_reader_and_deletion_failure() {
    use std::{
        fs::{File, FileTimes},
        io::Read,
    };
    let root = directory();
    let config = || {
        let mut c = OutputConfig::forward();
        c.mode = OutputMode::Local;
        c.directory = Some(root.clone());
        c.record_bytes = 1024;
        c.segment_bytes = 1024;
        c.segments = 1;
        // The old segment is backdated to UNIX_EPOCH below; keep the new one
        // alive through shutdown so the test does not race its own expiry.
        c.expiry = Duration::from_secs(60);
        c
    };
    let output = Output::start(config()).unwrap();
    output.submit(vec![b'a'; 800]);
    output.shutdown(Duration::from_secs(1));
    let path = root.join("segment-00.jsonl");
    let mut reader = File::open(&path).unwrap();
    reader
        .set_times(FileTimes::new().set_modified(UNIX_EPOCH))
        .unwrap();
    let next = Output::start(config()).unwrap();
    assert!(!path.exists());
    next.submit(vec![b'b'; 800]);
    next.shutdown(Duration::from_secs(1));
    let mut retained = Vec::new();
    reader.read_to_end(&mut retained).unwrap();
    assert_eq!(retained, vec![b'a'; 800]);
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 800);
    drop(reader);
    let failing = Output::start(config()).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    failing.submit(vec![b'c'; 800]);
    failing.shutdown(Duration::from_secs(1));
    assert_eq!(failing.loss().failed, 1);
    assert!(path.is_dir());
    std::fs::remove_dir_all(&root).unwrap();
    std::fs::write(&root, b"unowned path").unwrap();
    assert!(Output::start(config()).is_err());
    assert_eq!(std::fs::read(&root).unwrap(), b"unowned path");
    std::fs::remove_file(root).unwrap();
}

#[test]
fn independent_measurement_selection_and_aggregate_configuration() {
    assert!(Monitor::start(MonitorConfig {
        cpu: false,
        memory: false,
        interval_ms: 0,
        history: 0,
        windows: 0
    })
    .unwrap()
    .is_none());
    for (cpu, memory) in [(true, false), (false, true)] {
        let monitor = Monitor::start(MonitorConfig {
            cpu,
            memory,
            interval_ms: 10,
            history: 1,
            windows: 1,
        })
        .unwrap()
        .unwrap();
        let end = Instant::now() + Duration::from_secs(1);
        let sample = loop {
            if let Some(s) = monitor.latest() {
                break s;
            }
            assert!(Instant::now() < end);
            std::thread::yield_now();
        };
        assert_eq!(sample.selected, u8::from(cpu) | (u8::from(memory) << 1));
        assert_eq!(sample.user_ns.is_some(), cpu);
        assert_eq!(sample.rss.is_some(), memory);
    }
    let recorder = OperationRecorder::new(RecordingLimits::new(4, 4, 4096).unwrap(), 1, 8192)
        .unwrap()
        .with_timing(false);
    let (result, diagnostic) = recorder.run(1, "untimed", |_| Err::<(), _>(42));
    assert_eq!(result, Err(42));
    let Diagnostic::Report(report) = diagnostic else {
        panic!("report")
    };
    assert!(!report.success());
    assert_eq!(report.resource_status(), "disabled");
    assert!(report.timing().root().is_none());
    let path = directory();
    let mut output = OutputConfig::forward();
    output.mode = OutputMode::Local;
    output.directory = Some(path.clone());
    output.queue_bytes = 8 * 1024 * 1024;
    assert!(Runtime::start(Configuration {
        enabled: true,
        timing: true,
        monitor: MonitorConfig {
            cpu: true,
            memory: true,
            interval_ms: 10,
            history: 600,
            windows: 32
        },
        output,
        identity: Identity {
            run: 1,
            pid: 1,
            role: 1,
            namespace: 1
        }
    })
    .is_err());
    assert!(!path.exists());
}

#[test]
fn concurrent_final_handles_release_the_owned_writer() {
    use std::sync::{Arc, Barrier};
    let path = directory();
    let config = || {
        let mut c = OutputConfig::forward();
        c.mode = OutputMode::Local;
        c.directory = Some(path.clone());
        c
    };
    let output = Output::start(config()).unwrap();
    let gate = Arc::new(Barrier::new(9));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let handle = output.clone();
            let gate = gate.clone();
            scope.spawn(move || {
                gate.wait();
                drop(handle);
            });
        }
        drop(output);
        gate.wait();
    });
    // Observe asynchronous writer teardown through its real exclusive namespace,
    // with no private state or product-only test controls.
    let end = Instant::now() + Duration::from_secs(1);
    let reopened = loop {
        match Output::start(config()) {
            Ok(output) => break output,
            Err(error) => {
                assert!(Instant::now() < end, "last-owner teardown: {error}");
                std::thread::park_timeout(Duration::from_millis(5));
            }
        }
    };
    reopened.shutdown(Duration::from_secs(1));
    std::fs::remove_dir_all(path).unwrap();
}
