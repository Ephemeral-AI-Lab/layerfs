#![cfg(feature = "native")]
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
static TRACK: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
struct Count;
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(l) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(p, l, n) }
    }
}
#[global_allocator]
static ALLOCATOR: Count = Count;
#[test]
fn master_off_allocates_nothing_after_configuration_is_supplied() {
    use layerfs_telemetry::{
        operation::Diagnostic,
        output::{Identity, OutputConfig},
        runtime::{Configuration, MonitorConfig, Runtime},
    };
    let config = Configuration {
        enabled: false,
        timing: true,
        monitor: MonitorConfig {
            cpu: true,
            memory: true,
            interval_ms: 0,
            history: usize::MAX,
            windows: usize::MAX,
        },
        output: OutputConfig::forward(),
        identity: Identity {
            run: 1,
            pid: 1,
            role: 1,
            namespace: 1,
        },
    };
    TRACK.store(true, Ordering::SeqCst);
    let runtime = Runtime::start(config).unwrap();
    let (result, diagnostic) = runtime.recorder().run(1, "off", |scope| {
        scope.child("nested").run(|_| Ok::<_, ()>(17))
    });
    runtime.publish(Diagnostic::Disabled);
    drop(runtime);
    TRACK.store(false, Ordering::SeqCst);
    assert_eq!(ALLOCATIONS.load(Ordering::SeqCst), 0);
    assert_eq!(result, Ok(17));
    assert!(matches!(diagnostic, Diagnostic::Disabled));
}
