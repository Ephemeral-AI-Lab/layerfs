//! Finding and stopping the holders of one Workspace: the same-group `/proc`
//! scan, the scope probe, TERM then KILL, and the bounded waits between them.
use super::record::{self, Foreground};
use super::runtime;
use crate::common::outcome::refused;
use crate::sandbox::registry::SandboxState;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use std::time::Duration;

/// How many times a stopped process group is looked for.
const GONE_CHECKS: u32 = 50;
/// Pause between those checks.
const GONE_PAUSE: Duration = Duration::from_millis(100);
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

/// What a workspace-scoped kill made of one holder.
#[derive(Debug, PartialEq, Eq)]
enum Terminated {
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
pub(super) fn cancel(state: &SandboxState, path: &str) -> Result<(), SdkRefusal> {
    let records = record::foreground_in(state, path)?;
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
    let remaining = record::foreground_in(state, path)?;
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
fn terminate(
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
    let (code, out) = runtime::capture(
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
    let (code, out) = runtime::capture(
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
    let (code, _) = runtime::capture(state, "/tmp", vec!["bash".into(), "-c".into(), probe])?;
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
    runtime::capture(
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
