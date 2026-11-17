//! The worker's log on every machine kind (layerfs-project#178 step 0):
//! `/layerfs/data/log/worker.log`, appended to by the launcher's shell
//! redirect and rotated once, by the launcher only, when a start finds it
//! past 16 MiB. The launchers' commands and scripts are read from stand-in
//! tools, and the part every launcher shares is run under this host's `sh`
//! on a scratch log. No VM or container is made.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Code, HostOs, Platform, WORKER_LOG};
use standin::Scratch;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

const SIZE: u64 = 16 << 20;

/// The docker entry a start passed to `docker run`, as the stand-in logged it.
fn docker_entry(scratch: &Scratch) -> String {
    scratch.tool(
        "docker",
        r#"case "$3" in
  inspect) echo "Error: No such object: lfs-dev" >&2; exit 1 ;;
  run) echo "pull access denied" >&2; exit 1 ;;
esac"#,
    );
    let mut spec = scratch.spec(Platform::Docker);
    spec.os_image = Some("debian:trixie-slim".into());
    machine::start(&spec, &scratch.bundle()).unwrap_err();
    let log = scratch.log();
    let run = log
        .find("docker -H unix:///var/run/docker.sock run")
        .unwrap_or_else(|| panic!("no run in:\n{log}"));
    log[run..].to_owned()
}

/// Where `part` starts in `text`.
fn at(text: &str, part: &str) -> usize {
    text.find(part)
        .unwrap_or_else(|| panic!("{part:?} not in:\n{text}"))
}

/// What every launcher runs before it starts a worker, cut out of `script`.
fn log_start(script: &str) -> &str {
    let last = r#"chmod 600 "$LAYERFS_WORKER_LOG" )"#;
    &script[at(script, "( umask 077")..at(script, last) + last.len()]
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

fn sized(path: &Path, size: u64) {
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_len(size).unwrap();
}

/// The launch lines a start wrote to `log`.
fn launches(log: &Path) -> usize {
    let text = String::from_utf8_lossy(&fs::read(log).unwrap()).into_owned();
    text.lines()
        .filter(|line| line.contains("layerfs worker: launched 20"))
        .count()
}

#[test]
fn the_log_is_on_the_data_disk_of_every_machine_kind() {
    assert_eq!(WORKER_LOG, "/layerfs/data/log/worker.log");
    assert!(WORKER_LOG.starts_with(machine::DATA));
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn docker_bounds_the_container_log_and_its_entry_appends_to_the_worker_log() {
    let scratch = Scratch::new("docker-worker-log");
    let run = docker_entry(&scratch);
    let command = run.lines().next().unwrap();
    // always the three together: max-size alone fails where the Engine's
    // default driver is journald
    assert!(
        command.contains(" --log-driver local --log-opt max-size=16m --log-opt max-file=2 "),
        "{command}"
    );
    assert_eq!(command.matches("--log-").count(), 3, "{command}");
    assert!(
        command.contains(" -e LAYERFS_WORKER_LOG=/layerfs/data/log/worker.log "),
        "{command}"
    );
    // the entry: the log's folder, the one rotation, the pid, then the
    // worker with its output appended to the log
    let order = [
        r#"mkdir -p "${LAYERFS_WORKER_LOG%/*}""#,
        r#"[ "$(wc -c < "$LAYERFS_WORKER_LOG")" -gt 16777216 ]"#,
        r#"mv -f "$LAYERFS_WORKER_LOG" "${LAYERFS_WORKER_LOG%.log}.1.log""#,
        "echo $$ > /layerfs/state/worker.pid",
        "while [ ! -s /layerfs/state/secret ]",
        r#"exec /layerfs/bundle/layerfs worker >> "$LAYERFS_WORKER_LOG" 2>&1"#,
    ];
    let places: Vec<usize> = order.iter().map(|part| at(&run, part)).collect();
    assert!(places.windows(2).all(|pair| pair[0] < pair[1]), "{run}");
    assert!(
        run.trim_end().ends_with(order[5]),
        "the worker is the entry's last command: {run}"
    );
    assert!(
        !run.contains("exec /layerfs/bundle/layerfs worker\n"),
        "{run}"
    );
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_start_appends_to_a_small_log_and_rotates_one_past_its_size_once() {
    let scratch = Scratch::new("worker-log-start");
    let script = log_start(&docker_entry(&scratch)).to_owned();
    let log = scratch.path("data/log/worker.log");
    let rotated = scratch.path("data/log/worker.1.log");
    fs::create_dir_all(scratch.path("data")).unwrap();
    let start = |script: &str| {
        let status = Command::new("sh")
            .args(["-c", script])
            .env("LAYERFS_WORKER_LOG", &log)
            .status()
            .unwrap();
        assert!(status.success(), "{script}");
    };
    // the first start makes the folder and the log, private
    start(&script);
    assert_eq!(mode(log.parent().unwrap()), 0o700);
    assert_eq!(mode(&log), 0o600);
    assert_eq!(launches(&log), 1);
    assert!(!rotated.exists());
    // a restart appends: the lines of the run before stay in the log
    let mut text = fs::read_to_string(&log).unwrap();
    text.push_str("layerfs worker: cannot listen on 0.0.0.0:7070: Address already in use\n");
    fs::write(&log, &text).unwrap();
    start(&script);
    assert!(fs::read_to_string(&log).unwrap().starts_with(&text));
    assert_eq!(launches(&log), 2);
    assert!(!rotated.exists(), "a small log is not rotated at a start");
    // exactly at the size is not past it
    fs::write(&rotated, "the oldest\n").unwrap();
    sized(&log, SIZE);
    start(&script);
    assert_eq!(fs::read_to_string(&rotated).unwrap(), "the oldest\n");
    assert_eq!(launches(&log), 3);
    // a start that finds it past the size rotates it, replacing the older
    // rotated file, and begins a new private log
    let before = fs::metadata(&log).unwrap().len();
    assert!(before > SIZE);
    start(&script);
    assert_eq!(fs::metadata(&rotated).unwrap().len(), before);
    assert_eq!(launches(&rotated), 3);
    assert_eq!(launches(&log), 1);
    assert!(fs::metadata(&log).unwrap().len() < 100);
    assert_eq!(mode(&log), 0o600);
    // once: the next start appends and leaves the rotated file
    start(&script);
    assert_eq!(launches(&log), 2);
    assert_eq!(fs::metadata(&rotated).unwrap().len(), before);
    let names = fs::read_dir(log.parent().unwrap()).unwrap().count();
    assert_eq!(names, 2, "the log and one rotated file");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_log_an_older_build_left_in_the_state_folder_moves_in_at_the_next_start() {
    let scratch = Scratch::new("worker-log-older");
    let old = scratch.path("state-worker.log");
    let shared = log_start(&docker_entry(&scratch)).to_owned();
    assert_eq!(shared.matches("/layerfs/state/worker.log").count(), 2);
    // the same lines, with the older build's path in this scratch folder
    let script = shared.replace("/layerfs/state/worker.log", old.to_str().unwrap());
    let log = scratch.path("data/log/worker.log");
    fs::create_dir_all(scratch.path("data")).unwrap();
    let start = || {
        let status = Command::new("sh")
            .args(["-c", &script])
            .env("LAYERFS_WORKER_LOG", &log)
            .status()
            .unwrap();
        assert!(status.success());
    };
    fs::write(
        &old,
        "layerfs worker: listening on 0.0.0.0:7070, root /layerfs/data\n",
    )
    .unwrap();
    fs::set_permissions(&old, fs::Permissions::from_mode(0o644)).unwrap();
    start();
    assert!(!old.exists(), "moved, not copied");
    let text = fs::read_to_string(&log).unwrap();
    assert!(text.starts_with("layerfs worker: listening on 0.0.0.0:7070"));
    assert_eq!(launches(&log), 1);
    assert_eq!(mode(&log), 0o600);
    // once the log exists, a file at the older path is left alone
    fs::write(&old, "not ours to move\n").unwrap();
    start();
    assert_eq!(fs::read_to_string(&old).unwrap(), "not ours to move\n");
    assert_eq!(launches(&log), 2);
}

/// A docker worker that ends stops its container, which `docker exec`
/// cannot enter: the start reports the log's end, copied out of the
/// stopped container, without waiting out the readiness time.
#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn a_docker_worker_that_ends_at_start_is_reported_with_its_log_s_end() {
    let scratch = Scratch::new("docker-worker-ended");
    scratch.tool(
        "docker",
        r#"case "$3" in
  inspect) [ -e "$SCRATCH/ran" ] || { echo "Error: No such object: lfs-dev" >&2; exit 1; }
    printf 'exited\tmachine-worker\n' ;;
  run) touch "$SCRATCH/ran" ;;
  port) echo "127.0.0.1:45999" ;;
  exec) case "$*" in
      *worker.pid*) echo "Error response from daemon: container lfs-dev is not running" >&2; exit 1 ;;
      *) cat > /dev/null ;;
    esac ;;
  cp) printf 'layerfs worker: launched 2026-11-16T00:00:00Z\nlayerfs worker: cannot listen on 0.0.0.0:7070: Address already in use\n' > "$5" ;;
esac"#,
    );
    let mut spec = scratch.spec(Platform::Docker);
    spec.os_image = Some("debian:trixie-slim".into());
    let started = Instant::now();
    let error = machine::start(&spec, &scratch.bundle()).unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(30), "{error}");
    assert_eq!(error.code, Code::Failed, "{error}");
    assert_eq!(
        error.message,
        "the worker of dev ended: layerfs worker: launched 2026-11-16T00:00:00Z; \
         layerfs worker: cannot listen on 0.0.0.0:7070: Address already in use"
    );
    let copy = spec.work_dir.join("worker.log.ended");
    let log = scratch.log();
    let cp = format!(
        "docker -H unix:///var/run/docker.sock cp lfs-dev:/layerfs/data/log/worker.log {}",
        copy.display()
    );
    assert_eq!(log.lines().filter(|line| *line == cp).count(), 1, "{log}");
    assert!(!copy.exists(), "the copy is removed once read");
    assert!(!log.contains(" logs "), "nothing reads the container's log");
}

#[test]
fn a_vm_launcher_rotates_at_start_and_appends_the_worker_s_output() {
    if HostOs::current() != HostOs::Macos {
        return;
    }
    let scratch = Scratch::new("lima-worker-log");
    scratch.tool(
        "limactl",
        r#"case "$1" in
  list) [ -e "$SCRATCH/created" ] && printf 'Running\t%s\n' "$SCRATCH/lima-host"; exit 0 ;;
  create) cat > "$SCRATCH/template.json"; touch "$SCRATCH/created"; exit 0 ;;
  --version) echo "limactl version 9.9.9" ;;
esac
exit 0"#,
    );
    scratch.tool(
        "ssh",
        r#"case "$*" in
  *"tar -C"*) cat > /dev/null ;;
  *max_user_namespaces*) echo 0 ;;
  *"setsid nohup"*) exit 7 ;;
esac
exit 0"#,
    );
    scratch.tool("tar", "exit 0");
    machine::create(&scratch.spec(Platform::Lima), &scratch.bundle()).unwrap_err();
    let log = scratch.log();
    let launch = log
        .split("ssh ")
        .find(|call| call.contains("setsid nohup"))
        .unwrap_or_else(|| panic!("no launch in:\n{log}"));
    // an earlier worker is ended first, so no writer holds the log when
    // it is rotated; then the worker starts with its output appended
    let order = [
        r#"kill "$p" 2>/dev/null"#,
        r#"LAYERFS_WORKER_LOG="$3""#,
        r#"mkdir -p "${LAYERFS_WORKER_LOG%/*}""#,
        r#"[ "$(wc -c < "$LAYERFS_WORKER_LOG")" -gt 16777216 ]"#,
        r#"mv -f "$LAYERFS_WORKER_LOG" "${LAYERFS_WORKER_LOG%.log}.1.log""#,
        r#"setsid nohup "$2/layerfs" worker >> "$LAYERFS_WORKER_LOG" 2>&1 < /dev/null &"#,
        "echo $! > $s/worker.pid",
    ];
    let places: Vec<usize> = order.iter().map(|part| at(launch, part)).collect();
    assert!(places.windows(2).all(|pair| pair[0] < pair[1]), "{launch}");
    // the same lines as docker's entry, and the log's path as its argument
    assert!(launch.contains("( umask 077"), "{launch}");
    assert!(
        launch.contains(" /layerfs/bundle /layerfs/data/log/worker.log"),
        "{launch}"
    );
    assert!(!launch.contains(">> $s/worker.log"), "{launch}");
}
