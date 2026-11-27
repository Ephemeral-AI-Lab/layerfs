//! A real host worker and CLI callers sharing one SQLite Store.
#![allow(dead_code)]
use crate::fixture::Fixture;
use layerfs_api::home::{entity_lock, paths};
use layerfs_bridge::control::SdkValue;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const CLI: &str = env!("CARGO_BIN_EXE_layerfs");
const WAIT: Duration = Duration::from_secs(8);
pub const STORE: &str = "shop";
const SOURCE: &str = "the host worker proof\n";

const READ: &[&str] = &["file", "read", "shop/api", "main", "/hello"];
const STAT: &[&str] = &["file", "stat", "shop/api", "main", "/hello"];
const LIST: &[&str] = &["file", "list", "shop/api", "main", "/"];

pub struct Cli(Option<Child>);

impl Cli {
    pub fn spawn(home: &Path, words: &[&str]) -> Self {
        Self(Some(
            Command::new(CLI)
                .args(words)
                .env("LAYERFS_HOME", home)
                .env_remove("LAYERFS_TRACE")
                .env_remove("LAYERFS_TRACE_STEP")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        ))
    }

    #[cfg(target_os = "linux")]
    pub fn connected(&self) -> bool {
        sockets(i64::from(self.0.as_ref().unwrap().id())) > 0
    }

    fn output(&mut self) -> Output {
        let child = self.0.as_mut().unwrap();
        wait_for("CLI exit", || {
            child.try_wait().unwrap().is_some().then_some(())
        });
        self.0.take().unwrap().wait_with_output().unwrap()
    }

    pub fn kill(&mut self) {
        self.0.as_mut().unwrap().kill().unwrap();
        let output = self.output();
        assert!(!output.status.success(), "the killed caller succeeded");
    }

    pub fn ok(&mut self) -> SdkValue {
        let output = self.output();
        assert!(output.status.success(), "{output:?}");
        decode(&output.stdout)
    }

    pub fn refused(&mut self, code: &str) -> SdkValue {
        let output = self.output();
        assert!(!output.status.success(), "{output:?}");
        let bytes = if output.stdout.is_empty() {
            &output.stderr
        } else {
            &output.stdout
        };
        let refusal = decode(bytes);
        assert_eq!(refusal.get_text("error"), Some(code), "{refusal:?}");
        refusal
    }
}

impl Drop for Cli {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn decode(bytes: &[u8]) -> SdkValue {
    let text = std::str::from_utf8(bytes).unwrap();
    layerfs_api::json::parse(text).unwrap_or_else(|error| panic!("{error:?}: {text}"))
}

pub fn create() -> Fixture {
    let home = Fixture::new();
    let dir = home.paths.home.join("proof-input");
    std::fs::create_dir(&dir).unwrap();
    std::fs::write(dir.join("hello"), SOURCE).unwrap();
    Cli::spawn(&home.paths.home, &["store", "create", STORE]).ok();
    Cli::spawn(
        &home.paths.home,
        &["project", "init", "shop/api", dir.to_str().unwrap()],
    )
    .ok();
    home
}

pub fn lock_store(home: &Fixture) -> entity_lock::EntityLock {
    entity_lock::take(
        &paths::store_dir(&home.paths.home, STORE).join("lock"),
        WAIT,
        "proof Store lock",
    )
    .unwrap()
}

pub fn commits(home: &Fixture) -> SdkValue {
    let history = Cli::spawn(&home.paths.home, &["commit", "list", "shop/api", "main"]).ok();
    let Some(SdkValue::List(items)) = history.get("items") else {
        panic!("missing Commit list: {history:?}");
    };
    assert_eq!(items.len(), 1, "{history:?}");
    history
}

pub fn lease_file(home: &Fixture, pid: i64) -> PathBuf {
    paths::lease_dir(&home.paths.home, STORE).join(pid.to_string())
}

pub fn spawn_reads(home: &Fixture, count: usize, detailed: bool) -> Vec<Cli> {
    (0..count)
        .map(|index| {
            let method = [READ, STAT, LIST][index % 3];
            let mut words = method.to_vec();
            if detailed {
                words.push("--detail");
            }
            Cli::spawn(&home.paths.home, &words)
        })
        .collect()
}

pub fn assert_reads(mut reads: Vec<Cli>) {
    for (index, call) in reads.iter_mut().enumerate() {
        let value = call.ok();
        match index % 3 {
            0 => assert_eq!(value.get_text("text"), Some(SOURCE), "{value:?}"),
            1 => {
                assert_eq!(value.get_text("type"), Some("file"), "{value:?}");
                assert_eq!(
                    value.get_int("size"),
                    Some(SOURCE.len() as i64),
                    "{value:?}"
                );
            }
            _ => {
                let Some(SdkValue::List(items)) = value.get("items") else {
                    panic!("missing listing: {value:?}");
                };
                assert_eq!(items.len(), 1, "{value:?}");
                assert_eq!(items[0].get_text("path"), Some("/hello"), "{value:?}");
            }
        }
    }
}

pub fn wait_for<T>(description: &str, mut ready: impl FnMut() -> Option<T>) -> T {
    let until = Instant::now() + WAIT;
    loop {
        if let Some(value) = ready() {
            return value;
        }
        assert!(Instant::now() < until, "did not observe {description}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The admitted count includes the ping currently returning this snapshot.
pub fn pool(home: &Fixture, ordinary: i64, admitted: i64, queued: i64) -> SdkValue {
    let until = Instant::now() + WAIT;
    loop {
        let last = home.ping();
        if last.get_int("ordinary") == Some(ordinary)
            && last.get_int("admitted") == Some(admitted + 1)
            && last.get_int("queued") == Some(queued)
        {
            return last;
        }
        assert!(
            Instant::now() < until,
            "worker pool did not reach ({ordinary}, {admitted}, {queued}): {last:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "linux")]
pub fn sockets(pid: i64) -> usize {
    std::fs::read_dir(format!("/proc/{pid}/fd"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| std::fs::read_link(entry.path()).ok())
        .filter(|target| target.to_string_lossy().starts_with("socket:"))
        .count()
}

#[cfg(target_os = "linux")]
pub fn threads(pid: i64) -> usize {
    std::fs::read_dir(format!("/proc/{pid}/task"))
        .unwrap()
        .count()
}
