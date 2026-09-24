//! Temporary release-only resource diagnostic for one public SDK project Init.
use layerfs_sdk::{Error, Host};
use std::{fmt::Write as _, path::Path, time::Instant};

#[cfg(not(target_os = "macos"))]
compile_error!("this memory diagnostic uses the v0.1.6 Darwin resource probe");

#[repr(C)]
#[derive(Default)]
struct Timeval { seconds: i64, microseconds: i64 }
#[repr(C)]
#[derive(Default)]
struct Rusage {
    user: Timeval, system: Timeval, max_rss: i64,
    shared: i64, data: i64, stack: i64, minor: i64, major: i64,
    swaps: i64, input: i64, output: i64, sent: i64, received: i64,
    signals: i64, voluntary: i64, involuntary: i64,
}
#[repr(C)]
#[derive(Default)]
struct DarwinRusageInfoV2 {
    uuid: [u8; 16], user_time: u64, system_time: u64,
    package_idle_wakeups: u64, interrupt_wakeups: u64, pageins: u64,
    wired_size: u64, resident_size: u64, physical_footprint: u64,
    process_start_time: u64, process_exit_time: u64,
    child_user_time: u64, child_system_time: u64,
    child_package_idle_wakeups: u64, child_interrupt_wakeups: u64,
    child_pageins: u64, child_elapsed_time: u64,
    disk_read_bytes: u64, disk_write_bytes: u64,
}
unsafe extern "C" {
    fn getrusage(who: i32, usage: *mut Rusage) -> i32;
}
#[link(name = "proc")]
unsafe extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut std::ffi::c_void) -> i32;
}
#[derive(Clone, Copy)]
struct Memory {
    peak_rss: u64, current_rss: u64, footprint: u64,
    user_cpu_ns: u64, system_cpu_ns: u64, swaps: u64,
}
fn memory() -> Result<Memory, Box<dyn std::error::Error>> {
    let mut usage = Rusage::default();
    let mut current = DarwinRusageInfoV2::default();
    // SAFETY: both calls write into the same C layouts used by the v0.1.6 driver.
    if unsafe { getrusage(0, &mut usage) } != 0 ||
       unsafe { proc_pid_rusage(std::process::id() as i32, 2, (&mut current as *mut DarwinRusageInfoV2).cast()) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let cpu = |time: Timeval| -> Result<u64, Box<dyn std::error::Error>> {
        Ok(u64::try_from(time.seconds * 1_000_000_000 + time.microseconds * 1_000)?)
    };
    Ok(Memory {
        peak_rss: u64::try_from(usage.max_rss)?,
        current_rss: current.resident_size,
        footprint: current.physical_footprint,
        user_cpu_ns: cpu(usage.user)?, system_cpu_ns: cpu(usage.system)?,
        swaps: u64::try_from(usage.swaps)?,
    })
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("string write");
    }
    text
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 5 {
        return Err("source store history project-name required".into());
    }
    let host = Host::create(
        Path::new(&args[2]),
        Path::new(&args[3]),
        b"layerfs-bench-pro",
        &std::env::var("LAYERFS_PRIVATE_KEY")?,
        &std::env::var("LAYERFS_HISTORY_CURSOR_KEY")?,
    )?;
    let client = host.client();
    let before = memory()?;
    let start = Instant::now();
    let result = client.init_project(&args[4], Path::new(&args[1]));
    let operation_ns = start.elapsed().as_nanos();
    let after = memory()?;
    eprintln!("issue237-memory-v1 t0_peak_rss={} t1_peak_rss={} t0_rss={} t1_rss={} t0_footprint={} t1_footprint={} user_cpu_ns={} system_cpu_ns={} t0_swaps={} t1_swaps={} highwater_status={}", before.peak_rss, after.peak_rss, before.current_rss, after.current_rss, before.footprint, after.footprint, after.user_cpu_ns - before.user_cpu_ns, after.system_cpu_ns - before.system_cpu_ns, before.swaps, after.swaps, if after.peak_rss > before.peak_rss { "exact-new-lifetime-high-water" } else { "cumulative-high-water" });
    match result {
        Ok(project) => {
            println!("{{\"status\":\"COMPLETE\",\"operation_ns\":{operation_ns},\"root\":\"{}\",\"stack_body\":\"{}\",\"project_id\":\"{}\",\"genesis_layer\":\"{}\",\"root_serial\":{}}}",
                hex(&project.root), hex(&project.id[1..]), hex(&project.id),
                hex(&project.genesis_layer), project.root_serial);
            Ok(())
        }
        Err(Error::Backend(failure)) => {
            println!("{{\"status\":\"FAIL\",\"operation_ns\":{operation_ns},\"code\":\"{:?}\",\"unknown\":{},\"cleanup\":\"{:?}\"}}",
                failure.code, failure.unknown, failure.cleanup);
            Err("public SDK Init failed".into())
        }
        Err(Error::Unsupported) => Err("public SDK Init is unsupported".into()),
    }
}
