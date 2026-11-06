//! `layerfs project init --progress` (#37): each progress event is one JSON
//! line on stderr, before the result, which is one JSON value on stdout; the
//! command says nothing on stderr without `--progress`; a closed stderr does
//! not fail the call; a call that declares no `on_progress` has no
//! `--progress`. Init into a Store on this host needs no machine, daemon or
//! bucket, and reports `check`, `install` and `record` with null counts.
//! The page that states this: docs/architecture/api/call-table/events.md.
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

const LAYERFS: &str = env!("CARGO_BIN_EXE_layerfs");

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-cli-progress-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

/// A home with a Store `shop` and a folder of `files` small files.
fn fixture(tag: &str, files: usize) -> (PathBuf, PathBuf) {
    let home = tempdir(&format!("{tag}-home"));
    let folder = tempdir(&format!("{tag}-folder"));
    for i in 0..files {
        std::fs::write(folder.join(format!("f{i}.txt")), format!("file {i}\n")).unwrap();
    }
    let made = layerfs(&home, &["store", "create", "shop"]);
    assert_eq!(made.status.code(), Some(0), "{made:?}");
    (home, folder)
}

fn layerfs(home: &Path, words: &[&str]) -> std::process::Output {
    Command::new(LAYERFS)
        .args(words)
        .env("LAYERFS_HOME", home)
        .output()
        .unwrap()
}

fn lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn progress_is_json_lines_on_stderr_before_the_result_on_stdout() {
    let (home, folder) = fixture("order", 400);
    let folder_text = folder.display().to_string();
    // read both pipes as they fill, to see which line comes when
    let mut child = Command::new(LAYERFS)
        .args(["project", "init", "shop/api", &folder_text, "--progress"])
        .env("LAYERFS_HOME", &home)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    let mut stderr = child.stderr.take().unwrap();
    let reader = std::thread::spawn(move || {
        // each read returns what one write put there: one line at a time
        let mut seen = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            match stderr.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(n) => seen.push((started.elapsed(), buffer[..n].to_vec())),
            }
        }
        seen
    });
    let mut stdout = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    let status = child.wait().unwrap();
    let ended = started.elapsed();
    let stderr_reads = reader.join().unwrap();
    assert_eq!(status.code(), Some(0));

    // stdout: the result alone, one JSON value
    let result = String::from_utf8(stdout).unwrap();
    assert_eq!(result.lines().count(), 1, "{result}");
    assert!(result.contains("\"project\":\"shop/api\""), "{result}");
    assert!(!result.contains("progress"), "{result}");

    // stderr: one `{"progress":{phase,done,total}}` line per event, in order
    let all: Vec<u8> = stderr_reads.iter().flat_map(|(_, b)| b.clone()).collect();
    assert_eq!(
        lines(&all),
        [
            r#"{"progress":{"phase":"check","done":null,"total":null}}"#,
            r#"{"progress":{"phase":"install","done":null,"total":null}}"#,
            r#"{"progress":{"phase":"record","done":null,"total":null}}"#,
        ]
    );
    // the first event arrived before the command ended: it is not replayed
    // at the end
    let first = stderr_reads.first().expect("a stderr read").0;
    assert!(first < ended, "first event at {first:?}, exit at {ended:?}");
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn without_progress_stderr_is_empty() {
    let (home, folder) = fixture("quiet", 3);
    let folder_text = folder.display().to_string();
    let run = layerfs(&home, &["project", "init", "shop/api", &folder_text]);
    assert_eq!(run.status.code(), Some(0));
    assert!(
        run.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(String::from_utf8_lossy(&run.stdout).contains("\"project\":\"shop/api\""));
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_closed_stderr_does_not_fail_the_call() {
    let (home, folder) = fixture("closed", 3);
    let folder_text = folder.display().to_string();
    // the shell closes fd 2 for the command: its progress writes fail and are
    // ignored (run.rs), so the call is not failed by its observer
    let run = Command::new("sh")
        .args(["-c", "exec \"$@\" 2>&-", "sh", LAYERFS])
        .args(["project", "init", "shop/api", &folder_text, "--progress"])
        .env("LAYERFS_HOME", &home)
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(0), "{run:?}");
    assert!(String::from_utf8_lossy(&run.stdout).contains("\"project\":\"shop/api\""));
    assert!(run.stderr.is_empty());
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn a_refusal_before_the_first_phase_prints_no_progress() {
    let (home, folder) = fixture("refused", 0);
    let run = layerfs(
        &home,
        &["project", "init", "shop/api", "relative/path", "--progress"],
    );
    assert_eq!(run.status.code(), Some(125), "{run:?}");
    assert!(run.stdout.is_empty());
    let err = lines(&run.stderr);
    assert_eq!(err.len(), 1, "{err:?}");
    assert!(err[0].contains("\"code\":\"InvalidInput\""), "{err:?}");
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn only_the_five_calls_that_declare_on_progress_take_progress() {
    let home = tempdir("flag-home");
    for words in [
        ["store", "list", "--progress"].as_slice(),
        ["machine", "create", "m1", "lima", "--progress"].as_slice(),
    ] {
        let run = layerfs(&home, words);
        assert_eq!(run.status.code(), Some(2), "{words:?}: {run:?}");
        let err = String::from_utf8_lossy(&run.stderr);
        assert!(err.contains("unknown option --progress"), "{err}");
    }
    for words in [
        ["project", "init", "--help"],
        ["store", "push", "--help"],
        ["store", "pull", "--help"],
        ["backup", "create", "--help"],
        ["backup", "restore", "--help"],
    ] {
        let help = layerfs(&home, &words);
        let text = String::from_utf8_lossy(&help.stdout).into_owned()
            + &String::from_utf8_lossy(&help.stderr);
        assert!(text.contains("--progress"), "{words:?}: {text}");
    }
    let _ = std::fs::remove_dir_all(&home);
}
