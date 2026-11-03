//! Commands in a mounted Workspace, run by the Sandbox runtime owner.
//!
//! `exec` runs the command as the Sandbox's command identity in the
//! Workspace directory and returns the exit code and output when it ends; the
//! daemon is not involved. A background command is started in its own session
//! with its output in a log file, and this binding keeps its process id. A
//! foreground command writes a private record in `/tmp` naming its Workspace,
//! its pid and that pid's start time before it runs, so a second client can
//! find and cancel an orphan whose first client is gone. The Engine offers no
//! per-exec cancel, so `cancel` signals the process group from a second
//! command and waits (bounded) until it is gone.
use crate::job::{failed, refused, Outcome};
use crate::output::Spool;
use crate::registry::{lock, naming, sandbox, sandbox_live, text, Process, SandboxState};
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::time::Duration;

/// Home of the command identity, which has no account in the image.
const HOME: &str = "HOME=/tmp";
/// Checks that a cancelled process group is gone.
const GONE_CHECKS: u32 = 50;
/// Pause between those checks.
const GONE_PAUSE: Duration = Duration::from_millis(100);
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

/// The id one exec uses: `proc_<sandbox>_<12 hex>`, shared by foreground and
/// background commands so `cancel` can find either.
fn process_id(state: &SandboxState) -> Result<String, SdkRefusal> {
    Ok(format!(
        "proc_{}_{}",
        state.id,
        layerfs_bridge::sdk_values::hex(&crate::registry::random()?[..12])
    ))
}

/// The argv of a foreground exec, with its command as one separate argument.
fn foreground_arguments(id: &str, command: &str) -> Vec<String> {
    vec![
        "bash".into(),
        "-c".into(),
        FOREGROUND_WRAPPER.into(),
        "layerfs-exec".into(),
        id.into(),
        command.into(),
    ]
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

/// One command to its end: exit code, stdout, stderr. `spool` sends every
/// non-empty stream to its file instead of keeping it inline.
fn run(
    state: &SandboxState,
    directory: &str,
    arguments: Vec<String>,
    spool: bool,
) -> Result<(i32, Spool, Spool), SdkRefusal> {
    let created = layerfs_api::internal::exec(
        &state.runtime,
        state.identity,
        arguments,
        vec![HOME.to_owned()],
        directory.to_owned(),
        false,
    )
    .map_err(failed)?;
    let attached = created.start().map_err(failed)?;
    let (_input, output, handle) = attached.into_parts().map_err(failed)?;
    let (mut stdout, mut stderr) = (Spool::new(spool), Spool::new(spool));
    output.copy_to(&mut stdout, &mut stderr).map_err(failed)?;
    let inspected = handle.inspect().map_err(failed)?;
    let code = inspected
        .known_root_exit()
        .ok_or_else(|| refused("Unknown", "the command's exit code is not known"))?;
    Ok((code, stdout, stderr))
}

/// `workspace.exec(ws, command, { background?, spool? })`; with
/// `background`, `backend.daemon.process.start(ws, command)`.
///
/// A foreground result is `kind`, `exitCode`, `stdout`, `stderr`, `message`
/// and `truncated` (always false: no output is cut). A stream over the
/// inline window (1 MiB), or any non-empty stream with `spool`, is null
/// here and spooled; the result then also names its `output`, read with
/// `workspace.exec_output` and closed with `sandbox.exec_release`, and the
/// `stdoutBytes` and `stderrBytes` sizes.
pub(crate) fn exec(args: &SdkValue) -> Outcome {
    let state = sandbox(args)?;
    let path = text(args, "path")?;
    let command = text(args, "command")?;
    let directory = state.directory(path)?;
    if !args.get_bool("background") {
        let id = process_id(&state)?;
        let (code, stdout, stderr) = run(
            &state,
            &directory,
            foreground_arguments(&id, command),
            args.get_bool("spool"),
        )?;
        let mut streams = crate::output::fields(stdout, stderr)?.into_iter();
        let mut result = vec![
            ("kind", SdkValue::Text(format!("exit:{code}"))),
            ("exitCode", SdkValue::Int(i64::from(code))),
        ];
        result.extend(streams.by_ref().take(2));
        result.push(("message", SdkValue::opt_text(args.get_text("message"))));
        result.push(("truncated", SdkValue::Bool(false)));
        result.extend(streams);
        return Ok(SdkValue::Map(
            result
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value))
                .collect(),
        ));
    }
    let _guard = crate::discovery::guard(&state.directory)?;
    // the id names its Sandbox, so `cancel` needs nothing else
    let id = process_id(&state)?;
    let script = format!("setsid bash -c \"$0\" >/tmp/layerfs-{id}.log 2>&1 </dev/null & echo $!");
    let (code, stdout, stderr) = run(
        &state,
        &directory,
        vec!["bash".into(), "-c".into(), script, command.into()],
        false,
    )?;
    let pid = String::from_utf8_lossy(stdout.kept())
        .trim()
        .parse::<u32>()
        .map_err(|_| {
            refused(
                "Failed",
                format!(
                    "background start exited {code}: {}",
                    String::from_utf8_lossy(stderr.kept())
                ),
            )
        })?;
    let start = start_time(&state, pid)?.ok_or_else(|| {
        refused(
            "Unknown",
            "background process exited before its identity was observed",
        )
    })?;
    let process = Process {
        path: path.to_owned(),
        command: command.to_owned(),
        pid,
        stopped: false,
        start,
    };
    crate::discovery::save_process(&state, &id, &process)?;
    lock(&state.processes)?.insert(id.clone(), process);
    Ok(SdkValue::map([
        ("kind", SdkValue::text("running")),
        ("process", SdkValue::Text(id)),
        ("command", SdkValue::text(command)),
    ]))
}

/// True while the process group of `pid` has a member that is not a zombie.
/// The container's first process is the daemon, which does not reap orphans,
/// so a finished member can stay a zombie that `kill -0` still finds.
fn start_time(state: &SandboxState, pid: u32) -> Result<Option<String>, SdkRefusal> {
    let (code, out, _) = run(state, "/tmp", vec!["bash".into(), "-c".into(), format!("s=$(cat /proc/{pid}/stat 2>/dev/null) || exit 1; set -- ${{s##*) }}; shift 19; printf '%s' \"$1\"")], false)?;
    if code == 0 {
        Ok(Some(String::from_utf8_lossy(out.kept()).into_owned()))
    } else {
        Ok(None)
    }
}
fn alive(state: &SandboxState, pid: u32) -> Result<bool, SdkRefusal> {
    // After the `(comm) ` field: state, parent, process group.
    let probe = format!(
        "for f in /proc/[0-9]*/stat; do s=$(cat \"$f\" 2>/dev/null) || continue; \
         set -- ${{s##*) }}; [ \"$1\" != Z ] && [ \"$3\" = {pid} ] && exit 0; done; exit 1"
    );
    let (code, _, _) = run(
        state,
        "/tmp",
        vec!["bash".into(), "-c".into(), probe],
        false,
    )?;
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

/// Reads the foreground records in the Sandbox's `/tmp` once.
fn scan_foreground(state: &SandboxState) -> Result<String, SdkRefusal> {
    let (code, out, _) = run(
        state,
        "/tmp",
        vec!["bash".into(), "-c".into(), FOREGROUND_SCAN.into()],
        false,
    )?;
    if code != 0 {
        return Err(refused(
            "Failed",
            format!("foreground record scan exited {code}"),
        ));
    }
    Ok(String::from_utf8_lossy(out.kept()).into_owned())
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
        || token.len() != 12
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

/// Foreground execs of `state` whose record still names a live session
/// leader: their first client may be gone.
pub(crate) fn foreground_records(state: &SandboxState) -> Result<Vec<Foreground>, SdkRefusal> {
    let output = scan_foreground(state)?;
    Ok(records_in(&output, &state.id, None))
}

/// Foreground execs still running in the Workspace at `path`.
pub(crate) fn foreground_in(
    state: &SandboxState,
    path: &str,
) -> Result<Vec<Foreground>, SdkRefusal> {
    let output = scan_foreground(state)?;
    Ok(records_in(&output, &state.id, Some(path)))
}

/// Background processes still running in the Workspace at `path`.
pub(crate) fn running_in(state: &SandboxState, path: &str) -> Result<Vec<String>, SdkRefusal> {
    let started = lock(&state.processes)?
        .iter()
        .filter(|(_, process)| process.path == path && !process.stopped)
        .map(|(id, process)| (id.clone(), process.pid, process.start.clone()))
        .collect::<Vec<_>>();
    let mut running = Vec::new();
    for (id, pid, start) in started {
        if owned_alive(state, pid, &start)? {
            running.push(id);
        }
    }
    Ok(running)
}

/// `backend.daemon.process.list()`: the background processes started in the
/// project's Sandboxes that still run (a gone Sandbox's went with it), each
/// with its `sandbox`.
pub(crate) fn processes(args: &SdkValue) -> Outcome {
    let mut items = Vec::new();
    for id in crate::discovery::sandbox_ids(args)? {
        let named = naming(args, &id);
        if !crate::discovery::sandbox_dir(&named, &id)?
            .join("sandbox.record")
            .exists()
        {
            continue;
        }
        match sandbox_live(&named) {
            Ok(state) => {
                crate::discovery::refresh(&state)?;
                items.extend(listed(&state)?);
            }
            Err(refusal) if crate::discovery::gone_container(&refusal).is_some() => {}
            Err(refusal) => return Err(refusal),
        }
    }
    Ok(SdkValue::map([
        ("kind", SdkValue::text("ok")),
        ("items", SdkValue::List(items)),
    ]))
}

/// One Sandbox's processes, as `processes` lists them.
fn listed(state: &SandboxState) -> Result<Vec<SdkValue>, SdkRefusal> {
    let listed = lock(&state.processes)?
        .iter()
        .map(|(id, process)| {
            (
                id.clone(),
                process.path.clone(),
                process.command.clone(),
                process.pid,
                process.stopped,
                process.start.clone(),
            )
        })
        .collect::<Vec<_>>();
    let mut items = Vec::new();
    for (id, path, command, pid, stopped, start) in listed {
        let running = !stopped && owned_alive(state, pid, &start)?;
        items.push(SdkValue::map([
            ("id", SdkValue::Text(id)),
            ("sandbox", SdkValue::text(&state.id)),
            ("path", SdkValue::Text(path)),
            ("command", SdkValue::Text(command)),
            (
                "status",
                SdkValue::text(if running {
                    "running"
                } else if stopped {
                    "stopped"
                } else {
                    "exited"
                }),
            ),
        ]));
    }
    Ok(items)
}

/// `backend.daemon.process.cancel(process)`: TERM to the process group, then a
/// bounded wait until it is gone. The process id names its Sandbox. A
/// foreground exec is not in this process's map; its `/tmp` record stands in
/// for it, so a second client can cancel the orphan of a first client that
/// exited before its unmount.
pub(crate) fn cancel(args: &SdkValue) -> Outcome {
    let id = text(args, "process")?;
    let sandbox = id
        .strip_prefix("proc_")
        .and_then(|rest| rest.split_once('_'))
        .map(|(sandbox, _)| sandbox)
        .ok_or_else(|| refused("NoSuchProcess", format!("{id} is not a process id")))?;
    let state = sandbox_live(&naming(args, sandbox))?;
    crate::discovery::refresh(&state)?;
    let _guard = crate::discovery::guard(&state.directory)?;
    let (pid, start, stopped) = match lock(&state.processes)?
        .get(id)
        .map(|process| (process.pid, process.start.clone(), process.stopped))
    {
        Some(process) => process,
        // not one this process started: a foreground exec leaves its record
        None => {
            let record = foreground_records(&state)?
                .into_iter()
                .find(|record| record.id == id)
                .ok_or_else(|| {
                    refused(
                        "NoSuchProcess",
                        format!("{id} is not owned by {}", state.id),
                    )
                })?;
            (record.pid, record.start, false)
        }
    };
    if stopped || !owned_alive(&state, pid, &start)? {
        return Ok(SdkValue::map([
            ("kind", SdkValue::text("Stopped")),
            ("process", SdkValue::text(id)),
            ("status", SdkValue::text("stopped")),
        ]));
    }
    run(
        &state,
        "/tmp",
        vec![
            "bash".into(),
            "-c".into(),
            format!("kill -TERM -- -{pid} 2>/dev/null; true"),
        ],
        false,
    )?;
    let mut gone = false;
    for _ in 0..GONE_CHECKS {
        if !alive(&state, pid)? {
            gone = true;
            break;
        }
        std::thread::sleep(GONE_PAUSE);
    }
    if let Some(process) = lock(&state.processes)?.get_mut(id) {
        process.stopped = gone;
        crate::discovery::save_process(&state, id, process)?;
    }
    Ok(SdkValue::map([
        ("kind", SdkValue::text("Stopped")),
        ("process", SdkValue::text(id)),
        (
            "status",
            SdkValue::text(if gone { "stopped" } else { "stopping" }),
        ),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreground_argv_passes_the_command_as_its_own_argument() {
        let id = "proc_2_0123456789ab";
        let args = foreground_arguments(id, "printf '%s' 'a b'");
        assert_eq!(
            args,
            vec![
                "bash".to_owned(),
                "-c".to_owned(),
                FOREGROUND_WRAPPER.to_owned(),
                "layerfs-exec".to_owned(),
                id.to_owned(),
                "printf '%s' 'a b'".to_owned(),
            ]
        );
        assert!(FOREGROUND_WRAPPER.contains("setsid --wait bash -c 'id=$1\ncmd=$2\n"));
        assert!(FOREGROUND_WRAPPER.contains("/tmp/layerfs-exec-$id.record"));
        assert!(FOREGROUND_WRAPPER.contains("mv -f \"$t\" \"$r\""));
    }

    #[test]
    fn a_record_line_names_its_id_path_pid_and_start() {
        let record = parse_foreground("proc_2_0123456789ab /workspaces/3 4242 123456")
            .expect("a well-formed line");
        assert_eq!(
            record,
            Foreground {
                id: "proc_2_0123456789ab".to_owned(),
                path: "/workspaces/3".to_owned(),
                pid: 4242,
                start: "123456".to_owned(),
            }
        );
    }

    #[test]
    fn malformed_record_lines_are_ignored() {
        for line in [
            "",
            "proc_2_0123456789ab /workspaces/3 4242",
            "proc_2_0123456789ab /workspaces/3 not-a-pid 123456",
            "proc_2_0123456789ab /workspaces/3 0 123456",
            "proc_2_0123456789ab /workspaces/3 4242 ",
            "proc_2_0123456789ab /workspaces/3 4242 12 34",
            "process_2_0123456789ab /workspaces/3 4242 123456",
            "proc__0123456789ab /workspaces/3 4242 123456",
            "proc_2_0123456789 /workspaces/3 4242 123456",
            "proc_2_0123456789AB /workspaces/3 4242 123456",
            "proc_2_0123456789zz /workspaces/3 4242 123456",
            "garbage",
        ] {
            assert_eq!(parse_foreground(line), None, "accepted {line:?}");
        }
    }

    #[test]
    fn a_scan_keeps_only_its_sandbox_and_workspace() {
        let scanned = "proc_2_111111111111 /workspaces/3 10 1\n\
                       proc_3_222222222222 /workspaces/3 11 2\n\
                       proc_2_333333333333 /workspaces/4 12 3\n\
                       proc_2_444444444444 /workspaces/3 13 4\n";
        let ids = |path| -> Vec<String> {
            records_in(scanned, "2", path)
                .into_iter()
                .map(|record| record.id)
                .collect()
        };
        assert_eq!(
            ids(None),
            vec![
                "proc_2_111111111111",
                "proc_2_333333333333",
                "proc_2_444444444444",
            ]
        );
        assert_eq!(
            ids(Some("/workspaces/3")),
            vec!["proc_2_111111111111", "proc_2_444444444444"]
        );
        assert_eq!(ids(Some("/workspaces/4")), vec!["proc_2_333333333333"]);
        assert!(records_in(scanned, "9", None).is_empty());
    }
}
