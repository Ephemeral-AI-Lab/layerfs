use crate::stage1_fixture::{workspace_root, EvalResult};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::Command;
pub(crate) fn durable_write(path: &Path, contents: &str) -> EvalResult<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(contents.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}
pub(crate) fn durable_replace(path: &Path, contents: &str) -> EvalResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| "durable replacement has no parent".to_owned())?;
    let temporary = parent.join(format!(".summary-json-final-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(io_error)?;
    file.write_all(contents.as_bytes()).map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&temporary, path).map_err(io_error)?;
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(io_error)
}
pub(crate) fn sync_rows(run: &Path) -> EvalResult<()> {
    match OpenOptions::new().read(true).open(run.join("rows.jsonl")) {
        Ok(rows) => rows.sync_all().map_err(io_error),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}
pub(crate) fn fd_count() -> EvalResult<u64> {
    let path = if Path::new("/dev/fd").is_dir() {
        Path::new("/dev/fd")
    } else {
        Path::new("/proc/self/fd")
    };
    Ok(fs::read_dir(path).map_err(io_error)?.count() as u64)
}
pub(crate) fn attempt_residue_count() -> EvalResult<u64> {
    let path = workspace_root().join("target/layerfs-stage1-attempts");
    match fs::read_dir(path) {
        Ok(entries) => Ok(entries.count() as u64),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(io_error(error)),
    }
}
pub(crate) fn open_store_connection_count() -> EvalResult<u64> {
    let pid = std::process::id().to_string();
    let output = command_output("/usr/sbin/lsof", &["-Fn", "-p", &pid])?;
    Ok(output
        .lines()
        .filter(|line| {
            line.starts_with('n')
                && line.contains("generation-")
                && line.contains(".sqlite")
                && line.contains("layerfs-stage1")
        })
        .count() as u64)
}
pub(crate) fn current_rss_bytes() -> EvalResult<u64> {
    let pid = std::process::id().to_string();
    let output = command_output("/bin/ps", &["-o", "rss=", "-p", &pid])?;
    let kib = output.trim().parse::<u64>().map_err(display_error)?;
    kib.checked_mul(1_024)
        .ok_or_else(|| "RSS byte conversion overflow".to_owned())
}
#[cfg(target_os = "macos")]
pub(crate) fn maximum_rss_bytes() -> EvalResult<u64> {
    use std::ffi::c_int;
    #[repr(C)]
    #[derive(Default)]
    struct TimeVal {
        seconds: i64,
        microseconds: i64,
    }
    #[repr(C)]
    #[derive(Default)]
    struct RUsage {
        user: TimeVal,
        system: TimeVal,
        maximum_resident_set_bytes: i64,
        remaining: [i64; 13],
    }
    unsafe extern "C" {
        fn getrusage(who: c_int, usage: *mut RUsage) -> c_int;
    }
    let mut usage = RUsage::default();
    // SAFETY: `usage` is a live Darwin-compatible rusage buffer for the call.
    if unsafe { getrusage(0, &mut usage) } != 0 || usage.maximum_resident_set_bytes < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(usage.maximum_resident_set_bytes as u64)
}
#[cfg(not(target_os = "macos"))]
pub(crate) fn maximum_rss_bytes() -> EvalResult<u64> {
    current_rss_bytes()
}
pub(crate) fn command_output(program: &str, arguments: &[&str]) -> EvalResult<String> {
    String::from_utf8(command_bytes(program, arguments)?).map_err(display_error)
}
pub(crate) fn command_bytes(program: &str, arguments: &[&str]) -> EvalResult<Vec<u8>> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(workspace_root())
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(format!(
            "{program} {} exited {}",
            arguments.join(" "),
            output.status
        ));
    }
    Ok(output.stdout)
}
pub(crate) fn json_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            '\n' => "\\n".chars().collect(),
            '\r' => "\\r".chars().collect(),
            '\t' => "\\t".chars().collect(),
            character if character.is_control() => "?".chars().collect(),
            character => vec![character],
        })
        .collect()
}
pub(crate) fn json_string(json: &str, key: &str) -> EvalResult<String> {
    let needle = format!("\"{key}\":\"");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON string {key}"))?
        + needle.len();
    let end = json[start..]
        .find('"')
        .ok_or_else(|| format!("unterminated JSON string {key}"))?
        + start;
    Ok(json[start..end].to_owned())
}
pub(crate) fn json_u128(json: &str, key: &str) -> EvalResult<u128> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON integer {key}"))?
        + needle.len();
    let end = json[start..]
        .find(|character: char| !character.is_ascii_digit())
        .map_or(json.len(), |offset| start + offset);
    json[start..end].parse::<u128>().map_err(display_error)
}
pub(crate) fn json_bool(json: &str, key: &str) -> EvalResult<bool> {
    let needle = format!("\"{key}\":");
    let start = json
        .find(&needle)
        .ok_or_else(|| format!("missing JSON boolean {key}"))?
        + needle.len();
    if json[start..].starts_with("true") {
        Ok(true)
    } else if json[start..].starts_with("false") {
        Ok(false)
    } else {
        Err(format!("invalid JSON boolean {key}"))
    }
}
pub(crate) fn json_u128_array(values: &[u128]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(u128::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
}
pub(crate) fn io_error(error: std::io::Error) -> String {
    error.to_string()
}
pub(crate) fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
