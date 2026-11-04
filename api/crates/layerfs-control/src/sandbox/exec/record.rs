//! The foreground record a running command leaves in the Sandbox's `/tmp`:
//! the wrapper that creates and removes it, the id it carries, and the scan
//! that reads it back. Only a backend with `/proc` writes one.
use super::runtime;
use crate::common::outcome::refused;
use crate::sandbox::registry::{random, SandboxState};
use layerfs_bridge::control::SdkRefusal;
use layerfs_sandbox::Backend;

/// Hex digits `process_token` writes: 12 random bytes, two digits each. One
/// width only: a narrower token is not an id this binding generates.
const PROCESS_TOKEN_HEX: usize = 24;
/// Runs one foreground command in a new session and leaves a record in
/// `/tmp` while it runs. argv: `bash -c FOREGROUND_WRAPPER layerfs-exec <id>
/// <command>`; `$0` is the marker, `$1` the id and `$2` the command, passed
/// as its own argv and never concatenated into a script. The inner shell
/// writes the Workspace, its own pid and that pid's start time before it
/// `exec`s the command; the outer shell removes the record when the new
/// session ends. `setsid --wait` is util-linux (Debian 12 has it).
const FOREGROUND_WRAPPER: &str = r#"id=$1
cmd=$2
setsid --wait bash -c 'id=$1
cmd=$2
p=$$
s=$(cat /proc/$$/stat 2>/dev/null) || exit 125
set -- ${s##*) }
shift 19
start=$1
umask 077
r=/tmp/layerfs-exec-$id.record
t=$r.$$
printf "%s %s %s %s\n" "$id" "$PWD" "$p" "$start" > "$t" || { rm -f "$t"; exit 125; }
mv -f "$t" "$r" || { rm -f "$t"; exit 125; }
exec bash -c "$cmd"' _ "$id" "$cmd"
s=$?
rm -f "/tmp/layerfs-exec-$id.record"
exit $s
"#;

/// Prints one line per live foreground record: id, Workspace, pid, start.
/// A record is live while its pid exists, is not a zombie, and still has the
/// start time the record names. Every record in `/tmp` is scanned; the caller
/// keeps only its own Sandbox's.
const FOREGROUND_SCAN: &str = r#"for f in /tmp/layerfs-exec-proc_*.record; do
[ -f "$f" ] || continue
read -r id path pid start < "$f" || continue
[ -n "$id" ] && [ -n "$path" ] && [ -n "$start" ] || continue
case "$pid" in ''|*[!0-9]*) continue ;; esac
s=$(cat "/proc/$pid/stat" 2>/dev/null) || continue
set -- ${s##*) }
[ "$1" != Z ] || continue
shift 19
[ "$1" = "$start" ] || continue
printf "%s %s %s %s\n" "$id" "$path" "$pid" "$start"
done"#;

/// One live foreground exec, as its `/tmp` record and its process name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Foreground {
    pub(super) id: String,
    pub(super) path: String,
    pub(super) pid: u32,
    pub(super) start: String,
}

/// The random half of a process id: 12 bytes as 24 lowercase hex digits.
/// `process_id` and the record parser share this one width, so a record the
/// wrapper writes is always one `parse_foreground` accepts.
fn process_token() -> Result<String, SdkRefusal> {
    Ok(layerfs_bridge::sdk_values::hex(&random()?[..12]))
}

/// The id one exec uses: `proc_<sandbox>_<24 hex>`, so a record names the
/// Sandbox whose Workspace it holds.
fn process_id(state: &SandboxState) -> Result<String, SdkRefusal> {
    Ok(format!("proc_{}_{}", state.id, process_token()?))
}

/// Whether this Sandbox's backend has `/proc`, so a record can be written and
/// read there: Docker's container is Linux, a namespace Sandbox runs on this
/// Linux kernel, and a macOS host process has neither.
fn records(state: &SandboxState) -> Result<bool, SdkRefusal> {
    Ok(matches!(
        state.runtime()?.backend(),
        Backend::Docker | Backend::Namespaces
    ))
}

/// The argv of one foreground exec: the recording wrapper when the Sandbox
/// has `/proc`, the plain command otherwise.
pub(super) fn foreground_arguments(
    state: &SandboxState,
    command: &str,
) -> Result<Vec<String>, SdkRefusal> {
    if !records(state)? {
        return Ok(vec!["bash".into(), "-c".into(), command.into()]);
    }
    let id = process_id(state)?;
    Ok(vec![
        "bash".into(),
        "-c".into(),
        FOREGROUND_WRAPPER.into(),
        "layerfs-exec".into(),
        id,
        command.into(),
    ])
}

/// Reads the foreground records in the Sandbox's `/tmp` once.
fn scan_foreground(state: &SandboxState) -> Result<String, SdkRefusal> {
    let (code, out) = runtime::capture(
        state,
        "/tmp",
        vec!["bash".into(), "-c".into(), FOREGROUND_SCAN.into()],
    )?;
    if code != 0 {
        return Err(refused(
            "Failed",
            format!("foreground record scan exited {code}"),
        ));
    }
    Ok(out)
}

/// One scanned record line: `<id> <path> <pid> <starttime>`, with an id this
/// binding generates. A malformed line is ignored.
fn parse_foreground(line: &str) -> Option<Foreground> {
    let mut fields = line.split(' ');
    let id = fields.next()?;
    let path = fields.next()?;
    let pid = fields.next()?.parse::<u32>().ok()?;
    let start = fields.next()?;
    if fields.next().is_some() || pid == 0 || path.is_empty() || start.is_empty() {
        return None;
    }
    let (sandbox, token) = id.strip_prefix("proc_")?.split_once('_')?;
    if sandbox.is_empty()
        || !sandbox.bytes().all(|byte| byte.is_ascii_digit())
        || token.len() != PROCESS_TOKEN_HEX
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    Some(Foreground {
        id: id.to_owned(),
        path: path.to_owned(),
        pid,
        start: start.to_owned(),
    })
}

/// Scanned records of Sandbox `sandbox`, and only those in `path` when it is
/// named.
fn records_in(output: &str, sandbox: &str, path: Option<&str>) -> Vec<Foreground> {
    output
        .lines()
        .filter_map(parse_foreground)
        .filter(|record| {
            record
                .id
                .strip_prefix("proc_")
                .and_then(|rest| rest.split_once('_'))
                .is_some_and(|(owner, _)| owner == sandbox)
        })
        .filter(|record| path.is_none_or(|path| record.path == path))
        .collect()
}

/// Foreground execs still running in the Workspace at `path` of `state`:
/// their record names this Sandbox and Workspace, and `/proc` still shows
/// the pid and start time it recorded.
pub(super) fn foreground_in(
    state: &SandboxState,
    path: &str,
) -> Result<Vec<Foreground>, SdkRefusal> {
    let output = scan_foreground(state)?;
    Ok(records_in(&output, &state.id, Some(path)))
}
