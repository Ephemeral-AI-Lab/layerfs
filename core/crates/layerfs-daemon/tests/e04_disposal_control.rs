//! External diagnostic-control proofs; no product source or test-only API.
#[allow(dead_code)]
#[path = "../examples/e2_writes/control.rs"]
mod control;
#[allow(dead_code, clippy::needless_range_loop)]
#[path = "../../../benchmark/fs-bench-pro-storage-content/src/workload/digest.rs"]
mod digest;

use control::{Config, FinalBinding, Role};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    config: PathBuf,
    directory: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "layerfs-e04-control-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let directory = root.join("control");
        fs::create_dir(&directory).unwrap();
        let config = root.join("config");
        fs::write(
            &config,
            format!(
                "layerfs-e04-disposal-control-v1\n{}\n{}\n{}\nexplicit-host-fence-v1\n",
                "a".repeat(64),
                directory.display(),
                directory.display()
            ),
        )
        .unwrap();
        Self {
            root,
            config,
            directory,
        }
    }
    fn read(&self, role: Role) -> Config {
        Config::read(&self.config, role).unwrap()
    }
    fn binding(&self) -> FinalBinding {
        FinalBinding::new(17, b"exact original Binding reply").unwrap()
    }
    fn body(&self, name: &str, value: &[u8]) {
        fs::write(self.directory.join(format!("{name}.body")), value).unwrap();
        fs::create_dir(self.directory.join(format!("{name}.complete"))).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn exact_original_ready_and_ack_are_one_shot() {
    let f = Fixture::new();
    let host = f.read(Role::Host);
    let consumer = f.read(Role::Consumer);
    let binding = f.binding();
    let published = consumer.publish_ready(&binding).unwrap();
    assert!(published.marker_path.is_dir());
    let ready = host.try_ready().unwrap().unwrap();
    assert_eq!(ready.binding, binding);
    assert_eq!(ready.receipt.payload_sha256, published.payload_sha256);
    let acknowledged = host.publish_ack(&ready).unwrap();
    let received = consumer
        .wait_ack(&binding, Instant::now() + Duration::from_secs(5))
        .unwrap();
    assert_eq!(received.payload_sha256, acknowledged.payload_sha256);
    assert_eq!(received.correlation, 17);
    assert!(host.try_ready().is_err());
    assert!(host.publish_ack(&ready).is_err());
    assert!(consumer.publish_ready(&binding).is_err());
    assert!(consumer.wait_ack(&binding, Instant::now()).is_err());
}

#[test]
fn partial_body_without_complete_marker_is_not_parsed() {
    let f = Fixture::new();
    let host = f.read(Role::Host);
    let path = f.directory.join("consumer-ready.body");
    fs::write(&path, b"partial").unwrap();
    assert!(host.try_ready().unwrap().is_none());
    let binding = f.binding();
    fs::write(
        &path,
        format!(
            "layerfs-e04-disposal-ready-v1\n{}\n17\n{}\n",
            "a".repeat(64),
            control::bytes_hex(&binding.bytes)
        ),
    )
    .unwrap();
    fs::create_dir(f.directory.join("consumer-ready.complete")).unwrap();
    assert_eq!(host.try_ready().unwrap().unwrap().binding, binding);
}

#[test]
fn wrong_run_is_terminal_without_second_read() {
    let f = Fixture::new();
    let host = f.read(Role::Host);
    f.body(
        "consumer-ready",
        format!(
            "layerfs-e04-disposal-ready-v1\n{}\n17\n61\n",
            "b".repeat(64)
        )
        .as_bytes(),
    );
    let original = host.try_ready().unwrap_err();
    assert_eq!(original.phase, "validation");
    fs::remove_file(f.directory.join("consumer-ready.body")).unwrap();
    assert_eq!(host.try_ready().unwrap_err().phase, "validation");
}

#[test]
fn changed_ack_correlation_or_binding_is_refused() {
    for (correlation, digest) in [(18, "a".repeat(64)), (17, "b".repeat(64))] {
        let f = Fixture::new();
        let consumer = f.read(Role::Consumer);
        let binding = f.binding();
        consumer.publish_ready(&binding).unwrap();
        f.body(
            "host-ack",
            format!(
                "layerfs-e04-disposal-ack-v1\n{}\n{correlation}\n{digest}\n",
                "a".repeat(64)
            )
            .as_bytes(),
        );
        assert!(consumer
            .wait_ack(&binding, Instant::now() + Duration::from_secs(5))
            .is_err());
    }
}

#[test]
fn unacknowledged_ready_times_out_without_refresh() {
    let f = Fixture::new();
    let consumer = f.read(Role::Consumer);
    let binding = f.binding();
    consumer.publish_ready(&binding).unwrap();
    let original = consumer.wait_ack(&binding, Instant::now()).unwrap_err();
    assert_eq!(original.primary.kind(), std::io::ErrorKind::TimedOut);
    assert!(consumer
        .wait_ack(&binding, Instant::now() + Duration::from_secs(5))
        .is_err());
}

#[test]
fn existing_output_refusal_preserves_original_bytes() {
    let f = Fixture::new();
    let consumer = f.read(Role::Consumer);
    let body = f.directory.join("consumer-ready.body");
    fs::write(&body, b"original output").unwrap();
    let error = consumer.publish_ready(&f.binding()).unwrap_err();
    assert_eq!(error.primary.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(error.acknowledged_bytes, 0);
    assert_eq!(fs::read(&body).unwrap(), b"original output");
    assert!(!f.directory.join("consumer-ready.complete").exists());
    assert!(consumer.publish_ready(&f.binding()).is_err());
}

#[test]
fn malformed_complete_marker_cannot_authorize_body_read() {
    let f = Fixture::new();
    let host = f.read(Role::Host);
    fs::write(
        f.directory.join("consumer-ready.complete"),
        b"partial marker",
    )
    .unwrap();
    let error = host.try_ready().unwrap_err();
    assert_eq!(error.phase, "validation");
}

#[test]
fn preexisting_marker_cannot_expose_a_partly_written_body() {
    let f = Fixture::new();
    let consumer = f.read(Role::Consumer);
    fs::create_dir(f.directory.join("consumer-ready.complete")).unwrap();
    let original = consumer.publish_ready(&f.binding()).unwrap_err();
    assert_eq!(original.phase, "complete-precondition");
    assert_eq!(original.primary.kind(), std::io::ErrorKind::AlreadyExists);
    assert_eq!(original.acknowledged_bytes, 0);
    assert!(!f.directory.join("consumer-ready.body").exists());
    assert!(!original.close_disposition_unknown);
    assert!(consumer.publish_ready(&f.binding()).is_err());
}

#[test]
fn published_truncated_body_is_terminal() {
    let f = Fixture::new();
    let host = f.read(Role::Host);
    f.body("consumer-ready", b"layerfs-e04-disposal-ready-v1\npartial");
    assert!(host.try_ready().is_err());
    assert!(host.try_ready().is_err());
}

#[test]
fn closed_config_bytes_are_observed_before_control_use() {
    let f = Fixture::new();
    let raw = fs::read(&f.config).unwrap();
    let config = f.read(Role::Host);
    assert_eq!(config.source_bytes, raw.len() as u64);
    assert_eq!(config.source_sha256, digest::hex(&digest::sha256(&raw)));
    assert_eq!(config.source_path, f.config);
}

#[test]
fn invalid_binding_cannot_publish_control() {
    let f = Fixture::new();
    let consumer = f.read(Role::Consumer);
    let mut binding = f.binding();
    binding.bytes[0] ^= 1;
    assert!(consumer.publish_ready(&binding).is_err());
    assert!(!f.directory.join("consumer-ready.body").exists());
    assert!(FinalBinding::new(0, b"value").is_err());
    assert!(FinalBinding::new(1, &[0; 4097]).is_err());
}
