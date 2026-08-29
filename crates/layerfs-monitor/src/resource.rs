#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProcessSnapshot {
    pub process_id: u32,
    pub resident_bytes: Option<u64>,
    pub available_parallelism: usize,
}

pub(crate) fn process_snapshot() -> ProcessSnapshot {
    let process_id = std::process::id();
    let resident_bytes = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &process_id.to_string()])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .and_then(|kib| kib.checked_mul(1024));
    ProcessSnapshot {
        process_id,
        resident_bytes,
        available_parallelism: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
    }
}
