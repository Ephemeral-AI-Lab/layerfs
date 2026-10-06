//! Explicit real macOS Store/Runtime -> Linux public consumer functional proof.
#![cfg(target_os = "macos")]

#[path = "../examples/upstream_consumer/assignment.rs"]
#[allow(dead_code)]
mod assignment;
#[path = "support/upstream_host.rs"]
mod host;

use layerfs_bridge::native;
use layerfs_persistence::SqlitePersistenceProfile;
use layerfs_sdk::runtime::supervisor::{Supervisor, SupervisorConfig, SupervisorEvent};
use std::{
    fs::File,
    net::TcpListener,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const IMAGE: &str = "sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6";
struct DockerChild {
    child: Child,
    name: String,
    finished: bool,
}
impl Drop for DockerChild {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.child.kill();
            let _ = self.child.wait();
            // Only this proof's exact container, never another owner's resources.
            let _ = Command::new("docker")
                .args(["rm", "-f", &self.name])
                .status();
        }
    }
}

#[test]
#[ignore = "explicit Docker proof; requires prebuilt/pinned Linux consumer in LAYERFS_R4_LINUX_CONSUMER"]
fn real_host_store_and_supervisor_serve_source_removed_root_under_both_profiles() {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .unwrap();
    let binary = PathBuf::from(
        std::env::var_os("LAYERFS_R4_LINUX_CONSUMER")
            .expect("build Linux consumer first; supply its exact path"),
    )
    .canonicalize()
    .unwrap();
    let binary_relative = binary.strip_prefix(&repository).unwrap();
    let output_root = PathBuf::from(
        std::env::var_os("LAYERFS_R4_PROOF_OUTPUT")
            .expect("supply fresh append-only proof output directory"),
    );
    for profile in [
        SqlitePersistenceProfile::Durable,
        SqlitePersistenceProfile::Disposable,
    ] {
        let mut fixture = host::Host::new(profile);
        let output_directory = output_root.join(format!("{profile:?}"));
        std::fs::create_dir(&output_directory).expect("fresh profile proof output");
        assert!(!fixture.temp.0.join("native-source").exists());
        let assignment_path = fixture.temp.0.join("assignment.txt");
        assignment::write(&assignment_path, &fixture.expected, &fixture.bootstrap).unwrap();
        std::fs::write(
            assignment_path.with_extension("metadata"),
            &fixture.native_metadata,
        )
        .unwrap();
        std::fs::copy(&assignment_path, output_directory.join("assignment.txt")).unwrap();
        std::fs::write(
            output_directory.join("native-metadata.tsv"),
            &fixture.native_metadata,
        )
        .unwrap();
        std::fs::write(
            output_directory.join("raw-native-metadata.tsv"),
            &fixture.native_raw_metadata,
        )
        .unwrap();
        let assignment_relative = assignment_path.canonicalize().unwrap();
        let assignment_relative = assignment_relative.strip_prefix(&repository).unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = format!(
            "host.docker.internal:{}",
            listener.local_addr().unwrap().port()
        );
        let name = format!("layerfs-r4-proof-{}-{}", std::process::id(), profile as u8);
        let stdout = output_directory.join("consumer.stdout");
        let stderr = output_directory.join("consumer.stderr");
        let mount = format!("{}:/work:ro", repository.display());
        let mut command = Command::new("docker");
        command.args([
            "run",
            "--rm",
            "--name",
            &name,
            "--platform",
            "linux/arm64",
            "-v",
            &mount,
        ]);
        command.args([IMAGE, "timeout", "--kill-after=1s", "40s"]);
        command.arg(PathBuf::from("/work").join(binary_relative));
        command.arg(&endpoint);
        command.arg(PathBuf::from("/work").join(assignment_relative));
        command.stdout(Stdio::from(File::create(&stdout).unwrap()));
        command.stderr(Stdio::from(File::create(&stderr).unwrap()));
        let mut process = DockerChild {
            child: command.spawn().unwrap(),
            name,
            finished: false,
        };
        let deadline = Instant::now() + Duration::from_secs(45);
        let stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Some(status) = process.child.try_wait().unwrap() {
                        process.finished = true;
                        panic!(
                            "consumer exited before handshake {status}: {}",
                            std::fs::read_to_string(&stderr).unwrap()
                        );
                    }
                    assert!(Instant::now() < deadline, "bounded initial accept");
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("initial accept: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let connection = native::accept(stream, &host::HOST_PRIVATE, fixture.expected.local_peer)
            .unwrap_or_else(|error| {
                panic!(
                    "original handshake {error:?}: {}",
                    std::fs::read_to_string(&stderr).unwrap()
                )
            });
        let mut sessions = fixture.runtime.sessions();
        let binding = sessions
            .bind(
                &connection.peer,
                fixture.expected.workspace,
                fixture.expected.snapshot.branch.id,
            )
            .unwrap();
        assert_eq!(binding.snapshot(), &fixture.expected.snapshot);
        let mut supervisor = Supervisor::new(&mut sessions, SupervisorConfig::default()).unwrap();
        let attachment = supervisor
            .attach_bound(connection, binding)
            .map_err(|_| "attach original connection")
            .unwrap();
        let mut joined = false;
        let mut deliveries = 0;
        let status = loop {
            if let Some(event) = supervisor.step() {
                match event {
                    SupervisorEvent::Delivered(delivery) => {
                        deliveries += 1;
                        assert!(delivery.output.complete, "original reply-send outcome");
                        drop(delivery);
                    }
                    SupervisorEvent::Fenced(fence) => {
                        assert_eq!(fence.attachment, attachment);
                        joined = true;
                        drop(fence);
                    }
                }
            }
            if let Some(status) = process.child.try_wait().unwrap() {
                process.finished = true;
                break status;
            }
            assert!(Instant::now() < deadline, "bounded host serving/child wait");
            thread::sleep(Duration::from_millis(1));
        };
        if !joined {
            supervisor.fence(attachment).unwrap();
            while !joined {
                joined = supervisor.try_join(attachment).unwrap().is_some();
                assert!(
                    Instant::now() < deadline,
                    "bounded original attachment join"
                );
                if !joined {
                    thread::sleep(Duration::from_millis(1));
                }
            }
        }
        drop(supervisor);
        let custody = sessions.fence().unwrap();
        assert!(custody.slots().iter().all(Option::is_none));
        let output = std::fs::read_to_string(stdout).unwrap();
        let errors = std::fs::read_to_string(stderr).unwrap();
        assert!(
            status.success(),
            "original Docker consumer status {status}: {errors}"
        );
        assert!(
            output.contains("R4_REAL_HOST_CONSUMER_PASS files=7 source_removed=true"),
            "{output}\n{errors}"
        );
        assert!(
            deliveries > 2,
            "real Binding/Policy/object/length/serial service"
        );
        println!(
            "profile={profile:?} source_removed=true real_host_deliveries={deliveries}\n{output}"
        );
    }
}
