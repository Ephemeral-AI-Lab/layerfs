//! Bounded procfs self observation; units obtained from safe sysconf.
use crate::observation::{Observation, Source};
pub(crate) const SOURCE: Source = Source::Linux;
use std::io::Read;
pub(crate) fn sample(at_ns: u64, cpu: bool, memory: bool) -> Option<Observation> {
    let mut bytes = [0u8; 4096];
    let mut file = std::fs::File::open("/proc/self/stat").ok()?;
    let mut count = 0;
    loop {
        if count == bytes.len() {
            return None;
        }
        let n = file.read(&mut bytes[count..]).ok()?;
        if n == 0 {
            break;
        }
        count += n;
    }
    let text = std::str::from_utf8(&bytes[..count]).ok()?;
    let tail = text.rsplit_once(')')?.1;
    let mut user = None;
    let mut system = None;
    let mut start = None;
    let mut rss = None;
    for (i, v) in tail.split_whitespace().enumerate() {
        match i {
            11 => user = v.parse::<u64>().ok(),
            12 => system = v.parse::<u64>().ok(),
            19 => start = v.parse::<u64>().ok(),
            21 => rss = v.parse::<u64>().ok(),
            _ => {}
        }
    }
    let ticks = nix::unistd::sysconf(nix::unistd::SysconfVar::CLK_TCK).ok()?? as u64;
    let page = nix::unistd::sysconf(nix::unistd::SysconfVar::PAGE_SIZE).ok()?? as u64;
    let ns = |value: Option<u64>| value?.checked_mul(1_000_000_000)?.checked_div(ticks);
    Some(Observation {
        selected: u8::from(cpu) | (u8::from(memory) << 1),
        source: SOURCE,
        probe_ns: None,
        at_ns,
        incarnation: start?,
        user_ns: if cpu { ns(user) } else { None },
        system_ns: if cpu { ns(system) } else { None },
        rss: if memory { rss?.checked_mul(page) } else { None },
    })
}
