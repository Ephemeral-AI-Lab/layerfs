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
fn two_hosted_roles_retain_full_recording_pools_under_combined_heap_budget() {
    use layerfs_telemetry::{
        operation::Diagnostic,
        output::{Identity, OutputConfig, OutputMode},
        runtime::{Configuration, MonitorConfig, Runtime},
    };
    let root = std::env::temp_dir().join(format!("layerfs-owned-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let baseline = LIVE.load(Ordering::SeqCst);
    PEAK.store(baseline, Ordering::SeqCst);
    TRACK.store(true, Ordering::SeqCst);
    let mut roles = Vec::new();
    let mut reports = Vec::new();
    for role in 1..=2 {
        let mut output = OutputConfig::forward();
        output.mode = OutputMode::Local;
        output.directory = Some(root.join(role.to_string()));
        let runtime = Runtime::start(Configuration {
            enabled: true,
            timing: true,
            monitor: MonitorConfig {
                cpu: true,
                memory: true,
                interval_ms: 10,
                history: 600,
                windows: 32,
            },
            output,
            identity: Identity {
                run: 192,
                pid: std::process::id(),
                role,
                namespace: 1,
            },
        })
        .unwrap();
        let recorder = runtime.clone().recorder();
        for key in 0..8 {
            let (result, report) = recorder.run(key, "full", |scope| {
                for _ in 0..255 {
                    scope
                        .child("a bounded label with escaped data: \"\n\t")
                        .run(|_| Ok::<_, ()>(()))?;
                }
                Ok::<_, ()>(17)
            });
            assert_eq!(result, Ok(17));
            assert!(matches!(report, Diagnostic::Report(_)));
            reports.push(report);
        }
        assert!(matches!(
            recorder.run(9, "excess", |_| Ok::<_, ()>(())).1,
            Diagnostic::Omitted
        ));
        roles.push(runtime);
    }
    for (index, report) in reports.into_iter().enumerate() {
        roles[index / 8].publish(report);
    }
    drop(roles);
    TRACK.store(false, Ordering::SeqCst);
    let peak = PEAK.load(Ordering::SeqCst) - baseline;
    assert!(
        peak > 0 && peak <= 16 * 1024 * 1024,
        "combined observed heap peak {peak}"
    );
    println!("two hosted producer roles, 16 simultaneous 256-node report owners: observed additional heap high-water={peak} bytes; combined producer ownership allowance=16777216 bytes; excludes OS stacks, kernel buffers and allocator metadata");
    std::fs::remove_dir_all(root).unwrap();
}
