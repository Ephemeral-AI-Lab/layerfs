//! This process as a record names it (an `op`'s owner, a Store lease, a
//! forward): its id, kernel start time and boot, so a reused id does not
//! keep a record alive (moved from the Store leases, layerfs-project#103).
use layerfs_bridge::control::SdkValue;
use layerfs_isolation::local::LocalProcess;
use std::sync::OnceLock;

/// This process as leases name it, read once; `None` where the kernel does
/// not say (its leases are then matched by id alone).
pub fn me() -> Option<(u64, u128)> {
    static ME: OnceLock<Option<(u64, u128)>> = OnceLock::new();
    *ME.get_or_init(|| {
        let process = LocalProcess::observe(std::process::id()).ok()?;
        Some((process.start, process.boot?))
    })
}

/// The fields that name this process in a record (an `op`'s owner).
pub fn identity() -> Vec<(&'static str, SdkValue)> {
    let mut fields = vec![("pid", SdkValue::Int(i64::from(std::process::id())))];
    if let Some((start, boot)) = me() {
        fields.push(("pid_start", SdkValue::text(start.to_string())));
        fields.push(("pid_boot", SdkValue::text(format!("{boot:032x}"))));
    }
    fields
}

/// Whether the process a record's [`identity`] fields name is alive.
pub fn recorded_alive(value: &SdkValue) -> bool {
    let Some(pid) = value.get_int("pid").and_then(|p| u32::try_from(p).ok()) else {
        return false;
    };
    alive_as(
        pid,
        value.get_text("pid_start").and_then(|s| s.parse().ok()),
        value
            .get_text("pid_boot")
            .and_then(|b| u128::from_str_radix(b, 16).ok()),
    )
}

/// Whether process `pid` is alive on this host. Ids 0 and above `i32::MAX`
/// name no single process (`kill` would read them as a group): never alive.
pub fn alive(pid: u32) -> bool {
    use nix::errno::Errno;
    let id = match i32::try_from(pid) {
        Ok(id) if id > 0 => id,
        _ => return false,
    };
    match nix::sys::signal::kill(nix::unistd::Pid::from_raw(id), None) {
        Ok(()) => true,
        Err(Errno::EPERM) => true,
        Err(_) => false,
    }
}

/// Whether `pid` is alive and, when its start time and boot were recorded,
/// still the same process.
pub fn alive_as(pid: u32, start: Option<u64>, boot: Option<u128>) -> bool {
    if !alive(pid) {
        return false;
    }
    match (start, boot) {
        (Some(start), Some(boot)) => LocalProcess {
            pid,
            start,
            boot: Some(boot),
        }
        .alive(),
        _ => true,
    }
}
