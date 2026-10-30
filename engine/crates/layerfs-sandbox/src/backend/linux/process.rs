//! Linux process facts for the native runtime: state, process group and start
//! time from `/proc/<pid>/stat`, this boot's identity from
//! `/proc/sys/kernel/random/boot_id`, and every process from `/proc`.
use super::super::native::process::Stat;
use std::io;
use std::sync::OnceLock;

/// This boot's id (`/proc/sys/kernel/random/boot_id`, a UUID) as a number,
/// read once per process.
pub(crate) fn boot() -> io::Result<u128> {
    static BOOT: OnceLock<u128> = OnceLock::new();
    if let Some(boot) = BOOT.get() {
        return Ok(*boot);
    }
    let text = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?;
    let hex: String = text.trim().chars().filter(|c| *c != '-').collect();
    let boot = (hex.len() == 32)
        .then(|| u128::from_str_radix(&hex, 16).ok())
        .flatten()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "boot id"))?;
    Ok(*BOOT.get_or_init(|| boot))
}

/// State, process group and start time of `pid`. The command name may hold
/// spaces and parentheses, so fields are counted after its last `)`.
pub(crate) fn stat(pid: u32) -> io::Result<Stat> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "process stat fields");
    let rest = text.rsplit_once(')').ok_or_else(invalid)?.1;
    // field 3 (the state) is the first after the name, so field n is at n - 3
    let fields: Vec<&str> = rest.split_ascii_whitespace().collect();
    let field = |n: usize| fields.get(n - 3).copied().ok_or_else(invalid);
    Ok(Stat {
        state: field(3)?.bytes().next().ok_or_else(invalid)?,
        group: field(5)?.parse().map_err(|_| invalid())?,
        start: field(22)?.parse().map_err(|_| invalid())?,
    })
}

/// Every process of this kernel now, with its stat. One that ends while
/// being read is left out.
pub(crate) fn processes() -> io::Result<Vec<(u32, Stat)>> {
    let mut table = Vec::new();
    for item in std::fs::read_dir("/proc")? {
        let Some(pid) = item?.file_name().to_str().and_then(|n| n.parse().ok()) else {
            continue;
        };
        if let Ok(stat) = stat(pid) {
            table.push((pid, stat));
        }
    }
    Ok(table)
}
