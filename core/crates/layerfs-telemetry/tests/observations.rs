use layerfs_telemetry::{
    observation::{Observation, Window, WindowSource},
    operation::{Diagnostic, OperationRecorder},
    timer::RecordingLimits,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
#[test]
fn long_shared_windows_preserve_first_and_sampled_max() {
    let mut window = Window::default();
    for n in 0..10000 {
        window.observe(
            Observation {
                selected: 3,
                source: Default::default(),
                probe_ns: None,
                at_ns: n * 100_000_000,
                incarnation: 7,
                user_ns: Some(n * 3),
                system_ns: Some(n * 2),
                rss: Some(if n == 5 { 999999 } else { 100 }),
            },
            2,
        );
    }
    assert_eq!(window.samples, 10000);
    assert_eq!(window.cpu_delta(), Some((9999 * 3, 9999 * 2)));
    assert_eq!(window.sampled_max_rss, Some(999999));
    assert_eq!(window.concurrency, 2);
    assert_eq!(window.first.unwrap().at_ns, 0);
    window.observe(
        Observation {
            selected: 3,
            source: Default::default(),
            probe_ns: None,
            at_ns: 1_000_000_000_000,
            incarnation: 8,
            user_ns: Some(1),
            system_ns: Some(1),
            rss: None,
        },
        1,
    );
    assert!(window.discontinuity);
    assert_eq!(window.cpu_delta(), None);
    let mut cpu_only = Window::default();
    for n in 1..=2 {
        cpu_only.observe(
            Observation {
                selected: 3,
                source: Default::default(),
                probe_ns: None,
                at_ns: n,
                incarnation: 1,
                user_ns: Some(n),
                system_ns: Some(n),
                rss: None,
            },
            1,
        );
    }
    assert_eq!(cpu_only.cpu_delta(), Some((1, 1)));
}
struct Windows(AtomicUsize);
impl WindowSource for Windows {
    fn begin(&self) -> Option<u64> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Some(1)
    }
    fn finish(&self, _: u64) -> Option<Window> {
        Some(Window::default())
    }
    fn release(&self, _: u64) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
#[test]
fn optional_windows_release_on_return_error_and_unwind() {
    let windows = Arc::new(Windows(AtomicUsize::new(0)));
    let recorder = OperationRecorder::new(RecordingLimits::new(4, 4, 4096).unwrap(), 1, 8192)
        .unwrap()
        .with_windows(windows.clone());
    let (result, report) = recorder.run(1, "error", |_| Err::<(), _>(42));
    assert_eq!(result, Err(42));
    assert_eq!(windows.0.load(Ordering::SeqCst), 0);
    drop(report);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        recorder.run::<(), (), _>(2, "panic", |_| panic!("original"))
    }));
    assert_eq!(windows.0.load(Ordering::SeqCst), 0);
    let disabled = OperationRecorder::disabled().with_windows(windows.clone());
    assert!(matches!(
        disabled.run(3, "off", |_| Ok::<_, ()>(17)).1,
        Diagnostic::Disabled
    ));
    assert_eq!(windows.0.load(Ordering::SeqCst), 0);
}
