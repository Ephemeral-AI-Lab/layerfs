#![allow(dead_code)]
use layerfs_bridge::control::{Answer, Reply, SdkValue};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt},
        net::UnixListener,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub mod engine;
pub const CID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
static NEXT: AtomicUsize = AtomicUsize::new(0);

pub struct Home(pub PathBuf);
impl Home {
    pub fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "layerfs-binding-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        for path in [
            root.clone(),
            root.join("proj"),
            root.join("proj/sandboxes"),
            root.join("proj/sandboxes/1"),
        ] {
            std::fs::DirBuilder::new().mode(0o700).create(path).unwrap();
        }
        let artifact = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("liblayerfs_node.so");
        std::fs::copy(artifact, root.join("layerfs.node"))
            .expect("build layerfs-node before running native tests");
        let home = Self(root);
        home.record(CID, [1; 32], [2; 32]);
        home
    }

    pub fn record(&self, container: &str, secret: [u8; 32], peer: [u8; 32]) {
        let bytes = |n: [u8; 32]| SdkValue::Bytes(n.to_vec());
        let value = SdkValue::map([
            ("id", SdkValue::text("1")),
            ("project", SdkValue::text("proj")),
            ("container", SdkValue::text(container)),
            (
                "docker",
                SdkValue::text(self.0.join("docker.sock").to_str().unwrap()),
            ),
            (
                "image",
                SdkValue::text(format!("sha256:{}", "a".repeat(64))),
            ),
            ("volume", SdkValue::text("lfs-test-borrowed")),
            ("secret", bytes(secret)),
            ("peer", bytes(peer)),
            ("instance", bytes([7; 32])),
        ]);
        let path = self.0.join("proj/sandboxes/1/sandbox.record");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(
            &Answer {
                id: 1,
                reply: Reply::Sdk(value),
            }
            .encode()
            .unwrap(),
        )
        .unwrap();
    }

    pub fn node(&self, script: &str, preload: Option<&Path>, timeout: Duration) {
        let mut command =
            Command::new(std::env::var_os("LAYERFS_TEST_NODE").unwrap_or_else(|| "node".into()));
        command
            .arg("-e")
            .arg(format!(
                r#"
            const assert = require('node:assert/strict');
            const native = require(process.env.LAYERFS_NATIVE);
            const args = {{home: process.env.LAYERFS_HOME, project:'proj', sandbox:'1'}};
            const reclaim = () => native.backendCall('runtime.sandbox.reclaim', args);
            (async () => {{ {script} }})().catch(e => {{ console.error(e); process.exitCode=1; }});
        "#
            ))
            .env("LAYERFS_NATIVE", self.0.join("layerfs.node"))
            .env("LAYERFS_HOME", &self.0)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        if let Some(preload) = preload {
            command
                .env("LD_PRELOAD", preload)
                .env("LAYERFS_FORK_HOME", &self.0);
        }
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + timeout;
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("native binding call exceeded {timeout:?}");
            }
            thread::sleep(Duration::from_millis(2));
        };
        assert!(status.success(), "Node binding assertions failed: {status}");
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        // The fork fixture observes this even during panic unwinding.
        let _ = std::fs::write(self.0.join("release"), b"release");
        if self.0.join("fork.pid").exists() {
            let deadline = Instant::now() + Duration::from_secs(1);
            while !self.0.join("fork.done").exists() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(2));
            }
            if !self.0.join("fork.done").exists() {
                return;
            }
        }
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
