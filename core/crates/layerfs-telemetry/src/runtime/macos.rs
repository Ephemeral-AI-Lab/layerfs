//! Safe POSIX CPU microseconds converted to nanoseconds, plus libproc RSS.
//! libproc's raw CPU fields are Mach absolute-time ticks on Apple silicon and
//! must not be relabelled as nanoseconds. No handwritten timebase FFI is needed.
use crate::observation::{Observation, Source};
use nix::sys::{
    resource::{getrusage, UsageWho},
    time::TimeValLike,
};
pub(crate) const SOURCE: Source = Source::Macos;
pub(crate) fn sample(at_ns: u64, cpu: bool, memory: bool) -> Option<Observation> {
    let s = libproc::pid_rusage::pidrusage::<libproc::pid_rusage::RUsageInfoV2>(
        std::process::id() as i32
    )
    .ok()?;
    let usage = if cpu {
        getrusage(UsageWho::RUSAGE_SELF).ok()
    } else {
        None
    };
    let nanos = |micros: i64| u64::try_from(micros).ok()?.checked_mul(1000);
    Some(Observation {
        selected: u8::from(cpu) | (u8::from(memory) << 1),
        source: SOURCE,
        probe_ns: None,
        at_ns,
        incarnation: s.ri_proc_start_abstime,
        user_ns: usage
            .as_ref()
            .and_then(|v| nanos(v.user_time().num_microseconds())),
        system_ns: usage
            .as_ref()
            .and_then(|v| nanos(v.system_time().num_microseconds())),
        rss: memory.then_some(s.ri_resident_size),
    })
}
