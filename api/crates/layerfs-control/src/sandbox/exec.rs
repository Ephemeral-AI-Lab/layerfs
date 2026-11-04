//! Commands in a mounted Workspace, run by the Sandbox runtime owner.
//!
//! `exec` runs the command as the Sandbox's command identity in the
//! Workspace directory and returns the exit code and output when it ends; the
//! daemon is not involved. A local Sandbox
//! (`layerfs_sandbox::Sandbox::HostProcess`) runs the same commands as
//! processes of this kernel.
//!
//! A foreground command writes a private record in the Sandbox's `/tmp`
//! naming its Workspace, its pid and that pid's start time before it runs, so
//! a later call can find and stop an orphan whose first client is gone. The
//! executor owns command launch and cancellation (AGENTS.md), so the record
//! is written where the command runs, by the same runtime owner: `bash -c`
//! under util-linux `setsid --wait`, and `/proc` read inside the Sandbox. A
//! backend without `/proc` (the macOS host process) runs the command plainly
//! and leaves no record; nothing else changes.
use crate::common::outcome::{refused, Outcome};
use crate::sandbox::output::Spool;
use crate::sandbox::registry::{random, sandbox, text, SandboxState};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_sandbox::Backend;
use std::time::Duration;

/// How many times a stopped process group is looked for.
const GONE_CHECKS: u32 = 50;
/// Pause between those checks.
const GONE_PAUSE: Duration = Duration::from_millis(100);
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

/// One live foreground exec, as its `/tmp` record and its process name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Foreground {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) pid: u32,
    pub(crate) start: String,
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
fn foreground_arguments(state: &SandboxState, command: &str) -> Result<Vec<String>, SdkRefusal> {
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

/// The cwd and open files of every member of process group `$1`, when any is
/// outside `$2`: one `pid path` line each, read from `/proc`. Outside means an
/// absolute path that is not the Workspace or below it, not `/dev`, and not a
/// framework file under `/tmp/layerfs-` (the exec's own record and log); a
/// pipe, socket or deleted-or-anonymous file has no path and is not a
/// conflict.
const GROUP_OUTSIDE: &str = r#"pgid=$1
root=$2
outside() {
  case "$1" in
    "$root"|"$root"/*) return 1 ;;
    /dev|/dev/*) return 1 ;;
    /tmp/layerfs-*) return 1 ;;
    /*) return 0 ;;
    *) return 1 ;;
  esac
}
for f in /proc/[0-9]*/stat; do
  s=$(cat "$f" 2>/dev/null) || continue
  set -- ${s##*) }
  [ "$3" = "$pgid" ] || continue
  pid=${f#/proc/}
  pid=${pid%/stat}
  c=$(readlink "/proc/$pid/cwd" 2>/dev/null) || continue
  if outside "$c"; then printf "%s %s\n" "$pid" "$c"; continue; fi
  for l in /proc/$pid/fd/*; do
    t=$(readlink "$l" 2>/dev/null) || continue
    if outside "$t"; then printf "%s %s\n" "$pid" "$t"; break; fi
  done
done
exit 0"#;

/// One command to its end: exit code, stdout, stderr. `spool` sends every
/// non-empty stream to its file instead of keeping it inline. Its home is
/// the Sandbox's scratch directory (`/tmp` in a container), since the
/// command identity has no account there.
fn run(
    state: &SandboxState,
    directory: &str,
    arguments: Vec<String>,
    spool: bool,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    let runtime = state.runtime()?;
    crate::sandbox::runtime::run(
        runtime,
        state.identity,
        arguments,
        vec![format!(
            "HOME={}",
            crate::sandbox::runtime::scratch(runtime)
        )],
        directory,
        spool,
    )
}

/// [`run`] of a short probe whose whole stdout is its answer.
fn capture(
    state: &SandboxState,
    directory: &str,
    arguments: Vec<String>,
) -> Result<(i32, String), SdkRefusal> {
    let runtime = state.runtime()?;
    let (code, stdout, _) = crate::sandbox::runtime::capture(
        runtime,
        state.identity,
        arguments,
        vec![format!(
            "HOME={}",
            crate::sandbox::runtime::scratch(runtime)
        )],
        directory,
    )?;
    Ok((code, String::from_utf8_lossy(&stdout).into_owned()))
}

/// `workspace.exec(ws, command, { spool? })`.
///
/// A foreground result is `kind`, `exitCode`, `stdout`, `stderr`, `message`
/// and `truncated` (always false: no output is cut). A stream over the
/// inline window (1 MiB), or any non-empty stream with `spool`, is null
/// here and spooled; the result then also names its `output`, read with
/// `workspace.exec_output` and closed with `sandbox.exec_release`, and the
/// `stdoutBytes` and `stderrBytes` sizes. A `directory` from a caller that
/// keeps the Workspace's record (#535 G4) is the one the command runs in.
pub(crate) fn exec(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let path = text(args, "path")?;
    let command = text(args, "command")?;
    // a caller that reaches the Sandbox's daemon directly keeps its
    // Workspace records and sends the directory it checked there (#535 G4)
    let directory = match args.get_text("directory") {
        Some(directory) => directory.to_owned(),
        None => state.directory(path)?,
    };
    let arguments = foreground_arguments(&state, command)?;
    let (code, stdout, stderr) = run(&state, &directory, arguments, args.get_bool("spool"))?;
    let mut streams = crate::sandbox::output::fields(stdout, stderr)?.into_iter();
    let mut result = vec![
        ("kind", SdkValue::Text(format!("exit:{code}"))),
        ("exitCode", SdkValue::Int(i64::from(code))),
    ];
    result.extend(streams.by_ref().take(2));
    result.push(("message", SdkValue::opt_text(args.get_text("message"))));
    result.push(("truncated", SdkValue::Bool(false)));
    result.extend(streams);
    Ok(SdkValue::Map(
        result
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    ))
}

/// Reads the foreground records in the Sandbox's `/tmp` once.
fn scan_foreground(state: &SandboxState) -> Result<String, SdkRefusal> {
    let (code, out) = capture(
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
pub(crate) fn foreground_in(
    state: &SandboxState,
    path: &str,
) -> Result<Vec<Foreground>, SdkRefusal> {
    let output = scan_foreground(state)?;
    Ok(records_in(&output, &state.id, Some(path)))
}

/// What a workspace-scoped kill made of one holder.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Terminated {
    /// The holder is gone (or already was).
    Gone,
    /// Signals were sent but the process group is still there.
    Survived,
    /// The process group has a member whose cwd or open file is outside the
    /// target Workspace: nothing was signalled. Each entry is `pid path`.
    Outside(Vec<String>),
}

/// A `Busy` refusal naming the foreground execs that still run in the
/// Workspace.
pub(crate) fn busy(sandbox: &str, path: &str, processes: Vec<String>) -> SdkRefusal {
    SdkRefusal {
        code: "Busy".to_owned(),
        message: format!(
            "{sandbox}:{path} is busy: {} still running",
            processes.join(", ")
        ),
        detail: SdkValue::map([(
            "processes",
            SdkValue::List(processes.into_iter().map(SdkValue::Text).collect()),
        )]),
    }
}

/// A `Busy` refusal naming holders whose process group reaches outside the
/// Workspace, so a workspace-scoped kill left them alone.
fn scope_conflict(
    state: &SandboxState,
    path: &str,
    conflicts: Vec<(String, Vec<String>)>,
) -> SdkRefusal {
    let ids = conflicts
        .iter()
        .map(|(id, _)| SdkValue::text(id.as_str()))
        .collect::<Vec<_>>();
    let outside = conflicts
        .iter()
        .flat_map(|(_, paths)| paths.iter())
        .map(|path| SdkValue::text(path.as_str()))
        .collect::<Vec<_>>();
    SdkRefusal {
        code: "Busy".to_owned(),
        message: format!(
            "{}:{path} has a holder whose process group reaches outside the Workspace; nothing was killed",
            state.id
        ),
        detail: SdkValue::map([
            ("processes", SdkValue::List(ids)),
            ("scope_conflict", SdkValue::List(outside)),
        ]),
    }
}

/// Cancels every foreground exec still running in the Workspace at `path`:
/// TERM to its process group, a bounded wait, then KILL only if it is still
/// there. A group that reaches outside the Workspace is never signalled; a
/// holder that survives answers `Busy` with its id.
fn cancel_holders(state: &SandboxState, path: &str) -> Result<(), SdkRefusal> {
    let records = foreground_in(state, path)?;
    if records.is_empty() {
        return Ok(());
    }
    let mut conflicts = Vec::new();
    for record in &records {
        if let Terminated::Outside(outside) = terminate(state, path, record)? {
            conflicts.push((record.id.clone(), outside));
        }
    }
    // Never kill a group that reaches outside this Workspace: leave it and
    // name it, so the caller decides. No silent collateral.
    if !conflicts.is_empty() {
        return Err(scope_conflict(state, path, conflicts));
    }
    let remaining = foreground_in(state, path)?;
    if !remaining.is_empty() {
        return Err(busy(
            &state.id,
            path,
            remaining.into_iter().map(|record| record.id).collect(),
        ));
    }
    Ok(())
}

/// Stops one holder of the Workspace at `path` for a workspace-scoped kill:
/// TERM to its process group, a bounded wait, and KILL only if it is still
/// there, then a second bounded wait. A group with a member outside the
/// Workspace is left alone. A foreground exec's wrapper removes its `/tmp`
/// record once that session ends.
pub(crate) fn terminate(
    state: &SandboxState,
    path: &str,
    record: &Foreground,
) -> Result<Terminated, SdkRefusal> {
    if !owned_alive(state, record.pid, &record.start)? {
        return Ok(Terminated::Gone);
    }
    let outside = outside_scope(state, record.pid, path)?;
    if !outside.is_empty() {
        return Ok(Terminated::Outside(outside));
    }
    signal(state, record.pid, "TERM")?;
    let mut gone = wait_gone(state, record.pid)?;
    if !gone {
        signal(state, record.pid, "KILL")?;
        gone = wait_gone(state, record.pid)?;
    }
    Ok(if gone {
        Terminated::Gone
    } else {
        Terminated::Survived
    })
}

/// The members of process group `pid` whose cwd or open file is outside the
/// Workspace at `path`, as `pid path` lines.
fn outside_scope(state: &SandboxState, pid: u32, path: &str) -> Result<Vec<String>, SdkRefusal> {
    let (code, out) = capture(
        state,
        "/tmp",
        vec![
            "bash".into(),
            "-c".into(),
            GROUP_OUTSIDE.into(),
            "layerfs-kill".into(),
            pid.to_string(),
            path.into(),
        ],
    )?;
    if code != 0 {
        return Err(refused(
            "Failed",
            format!("process-group scope probe exited {code}"),
        ));
    }
    Ok(out
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

/// The start time of `pid`, as `/proc` names it.
fn start_time(state: &SandboxState, pid: u32) -> Result<Option<String>, SdkRefusal> {
    let (code, out) = capture(
        state,
        "/tmp",
        vec![
            "bash".into(),
            "-c".into(),
            format!("s=$(cat /proc/{pid}/stat 2>/dev/null) || exit 1; set -- ${{s##*) }}; shift 19; printf '%s' \"$1\""),
        ],
    )?;
    Ok((code == 0).then_some(out))
}

/// True while the process group of `pid` has a member that is not a zombie.
/// The container's first process is the daemon, which does not reap orphans,
/// so a finished member can stay a zombie that `kill -0` still finds.
fn alive(state: &SandboxState, pid: u32) -> Result<bool, SdkRefusal> {
    // After the `(comm) ` field: state, parent, process group.
    let probe = format!(
        "for f in /proc/[0-9]*/stat; do s=$(cat \"$f\" 2>/dev/null) || continue; \
         set -- ${{s##*) }}; [ \"$1\" != Z ] && [ \"$3\" = {pid} ] && exit 0; done; exit 1"
    );
    let (code, _) = capture(state, "/tmp", vec!["bash".into(), "-c".into(), probe])?;
    Ok(code == 0)
}

fn owned_alive(state: &SandboxState, pid: u32, start: &str) -> Result<bool, SdkRefusal> {
    if !alive(state, pid)? {
        return Ok(false);
    }
    if start_time(state, pid)?.as_deref() != Some(start) {
        return Err(refused(
            "Unknown",
            "process group identity is no longer provable; no signal sent",
        ));
    }
    Ok(true)
}

/// One signal to the process group `pid` leads. The command's own failure is
/// ignored: a group that is already gone is the outcome we want.
fn signal(state: &SandboxState, pid: u32, name: &str) -> Result<(), SdkRefusal> {
    capture(
        state,
        "/tmp",
        vec![
            "bash".into(),
            "-c".into(),
            format!("kill -{name} -- -{pid} 2>/dev/null; true"),
        ],
    )?;
    Ok(())
}

/// Waits (bounded) until the process group `pid` leads has no live member.
fn wait_gone(state: &SandboxState, pid: u32) -> Result<bool, SdkRefusal> {
    for _ in 0..GONE_CHECKS {
        if !alive(state, pid)? {
            return Ok(true);
        }
        std::thread::sleep(GONE_PAUSE);
    }
    Ok(false)
}

/// `workspace.holders(ws)`: the foreground execs still running in a
/// Workspace, read, or cancelled first with `kill`. This op runs where the
/// Sandbox's runtime is (its machine's worker, or this process), so a caller
/// that reaches only its daemon's forwarded port can still cancel an orphan.
/// Answers `{ processes }`; with `kill`, each own process group gets TERM and
/// then KILL, and a group that reaches outside the Workspace answers `Busy`.
pub(crate) fn holders(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let path = text(args, "path")?;
    if args.get_bool("kill") {
        cancel_holders(&state, path)?;
    }
    let records = foreground_in(&state, path)?;
    Ok(SdkValue::map([
        ("path", SdkValue::text(path)),
        (
            "processes",
            SdkValue::List(
                records
                    .into_iter()
                    .map(|record| SdkValue::Text(record.id))
                    .collect(),
            ),
        ),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One id shaped as `process_id` makes it.
    fn id_for(sandbox: &str) -> String {
        format!(
            "proc_{sandbox}_{}",
            process_token().expect("a random token")
        )
    }

    #[test]
    fn a_process_token_is_the_width_the_parser_accepts() {
        let token = process_token().expect("a random token");
        assert_eq!(token.len(), PROCESS_TOKEN_HEX);
        assert!(token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    }

    #[test]
    fn a_record_line_parses_the_id_process_id_produces() {
        let id = id_for("2");
        let record = parse_foreground(&format!("{id} /workspaces/3 4242 123456"))
            .expect("an id this binding generates");
        assert_eq!(
            record,
            Foreground {
                id,
                path: "/workspaces/3".to_owned(),
                pid: 4242,
                start: "123456".to_owned(),
            }
        );
    }

    #[test]
    fn foreground_argv_passes_the_command_as_its_own_argument() {
        let wrapper = FOREGROUND_WRAPPER;
        assert!(wrapper.contains("setsid --wait bash -c 'id=$1\ncmd=$2\n"));
        assert!(wrapper.contains("/tmp/layerfs-exec-$id.record"));
        assert!(wrapper.contains("mv -f \"$t\" \"$r\""));
    }

    #[test]
    fn malformed_record_lines_are_ignored() {
        let token = process_token().expect("a random token");
        let good = format!("proc_2_{token}");
        let tail = &good["proc_2_".len()..];
        for line in [
            String::new(),
            format!("{good} /workspaces/3 4242"),
            format!("{good} /workspaces/3 not-a-pid 123456"),
            format!("{good} /workspaces/3 0 123456"),
            format!("{good} /workspaces/3 4242 "),
            format!("{good} /workspaces/3 4242 12 34"),
            format!("process_2_{tail} /workspaces/3 4242 123456"),
            format!("proc__{tail} /workspaces/3 4242 123456"),
            // A narrower token is not an id this binding generates: the record
            // wrapper writes the width `process_id` gives it.
            format!("proc_2_{} /workspaces/3 4242 123456", &tail[..12]),
            "proc_2_0123456789ab /workspaces/3 4242 123456".to_owned(),
            format!("{good} /workspaces/3 4242 123456 "),
            format!("proc_2_{}a /workspaces/3 4242 123456", tail),
            format!(
                "proc_2_{} /workspaces/3 4242 123456",
                tail[..PROCESS_TOKEN_HEX - 1].to_owned()
            ),
            format!("proc_2_F{} /workspaces/3 4242 123456", &tail[1..]),
            format!(
                "proc_2_{}z /workspaces/3 4242 123456",
                &tail[..PROCESS_TOKEN_HEX - 2]
            ),
            format!(
                "proc_2_{} /workspaces/3 4242 123456",
                "0123456789ab_0123456789ab"
            ),
            "garbage".to_owned(),
        ] {
            assert_eq!(parse_foreground(&line), None, "accepted {line:?}");
        }
    }

    #[test]
    fn a_scan_keeps_only_its_sandbox_and_workspace() {
        let scanned = "proc_2_111111111111111111111111 /workspaces/3 10 1\n\
                       proc_3_222222222222222222222222 /workspaces/3 11 2\n\
                       proc_2_333333333333333333333333 /workspaces/4 12 3\n\
                       proc_2_444444444444444444444444 /workspaces/3 13 4\n";
        let ids = |path| -> Vec<String> {
            records_in(scanned, "2", path)
                .into_iter()
                .map(|record| record.id)
                .collect()
        };
        assert_eq!(
            ids(None),
            vec![
                "proc_2_111111111111111111111111",
                "proc_2_333333333333333333333333",
                "proc_2_444444444444444444444444",
            ]
        );
        assert_eq!(
            ids(Some("/workspaces/3")),
            vec![
                "proc_2_111111111111111111111111",
                "proc_2_444444444444444444444444"
            ]
        );
        assert_eq!(
            ids(Some("/workspaces/4")),
            vec!["proc_2_333333333333333333333333"]
        );
        assert!(records_in(scanned, "9", None).is_empty());
    }
}
