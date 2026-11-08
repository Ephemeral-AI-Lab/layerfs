//! Command construction for apple-container, docker and native through
//! stand-in tools. No VM or container is made.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Code, HostOs, Platform, State};
use standin::Scratch;

fn line<'a>(log: &'a str, prefix: &str) -> &'a str {
    log.lines()
        .find(|l| l.starts_with(prefix))
        .unwrap_or_else(|| panic!("no call {prefix:?} in:\n{log}"))
}

#[test]
fn apple_container_runs_one_container_with_its_data_volume_and_no_home() {
    let scratch = Scratch::new("apple-run");
    scratch.tool(
        "container",
        r#"case "$1" in
  inspect) echo "Error: container not found" >&2; exit 1 ;;
  volume) [ "$2" = inspect ] && exit 1; exit 0 ;;
  run) echo "image pull failed" >&2; exit 1 ;;
esac"#,
    );
    let error =
        machine::create(&scratch.spec(Platform::AppleContainer), &scratch.bundle()).unwrap_err();
    if HostOs::current() != HostOs::Macos || std::env::consts::ARCH != "aarch64" {
        assert_eq!(error.code, Code::Unsupported);
        assert!(scratch.log().is_empty());
        return;
    }
    assert_eq!(error.code, Code::Failed, "{error}");
    assert!(!error.vm_exists);
    let log = scratch.log();
    assert_eq!(
        line(&log, "container volume create"),
        "container volume create -s 10240M lfs-dev-data"
    );
    assert_eq!(
        line(&log, "container run"),
        "container run --detach --name lfs-dev --cpus 2 --memory 2048M --cap-add SYS_ADMIN \
         --cap-add SYS_RESOURCE --cap-add NET_ADMIN --mount type=volume,source=lfs-dev-data,target=/layerfs/data \
         --publish 127.0.0.1:45678:7070 --label layerfs.role=machine debian:trixie-slim \
         sh -c trap 'exit 0' TERM; sleep infinity & wait"
    );
    assert!(!log.contains("/layerfs/home"));
    // nothing exists under the name, so the volume this create made goes
    assert_eq!(
        log.lines().last(),
        Some("container volume delete lfs-dev-data")
    );
}

#[test]
fn apple_container_status_reads_status_state_from_inspect() {
    let scratch = Scratch::new("apple-status");
    scratch.tool(
        "container",
        r#"case "$1" in
  inspect) echo '[{"id":"lfs-dev","status":{"state":"running","networks":[]},"configuration":{"labels":{}}}]' ;;
  --version) echo "container CLI version 1.6.1" ;;
  exec) echo "Filesystem 1024-blocks Used Available Capacity Mounted on"; echo "/dev/vdb 10000 10 9990 1% /layerfs/data" ;;
esac"#,
    );
    let status = machine::status(&scratch.spec(Platform::AppleContainer)).unwrap();
    // the VM runs, but no worker answers at a recorded endpoint
    assert_eq!(status.state, State::Stopped);
    assert_eq!(status.free_disk, Some(9990 * 1024));
    assert_eq!(
        status.tool_version.as_deref(),
        Some("container CLI version 1.6.1")
    );
    assert!(status.down_reason.unwrap().contains("worker"));
    assert!(scratch
        .log()
        .contains("container exec lfs-dev df -kP /layerfs/data"));
}

#[test]
fn built_in_machines_are_never_created_or_removed() {
    let scratch = Scratch::new("built-in");
    for kind in [Platform::Native, Platform::Docker] {
        let spec = scratch.spec(kind);
        assert_eq!(
            machine::create(&spec, &scratch.bundle()).unwrap_err().code,
            Code::InvalidInput
        );
        assert_eq!(machine::remove(&spec).unwrap_err().code, Code::InvalidInput);
    }
    let native = scratch.spec(Platform::Native);
    assert!(machine::start(&native, &scratch.bundle())
        .unwrap()
        .is_none());
    machine::stop(&native).unwrap();
    assert_eq!(machine::status(&native).unwrap().state, State::Running);
    assert!(scratch.log().is_empty());
}

#[test]
fn a_bundle_without_its_binaries_is_refused_before_any_effect() {
    let scratch = Scratch::new("bundle");
    let mut bundle = scratch.bundle();
    std::fs::remove_file(bundle.dir.join("layerfs")).unwrap();
    let error = machine::create(&scratch.spec(Platform::AppleContainer), &bundle).unwrap_err();
    assert_eq!(error.code, Code::InvalidInput);
    assert_eq!(error.missing, ["bundle:layerfs"]);
    bundle.hash = "not hex".into();
    assert_eq!(
        machine::create(&scratch.spec(Platform::AppleContainer), &bundle)
            .unwrap_err()
            .code,
        Code::InvalidInput
    );
    assert!(scratch.log().is_empty());
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn docker_on_macos_and_linux_starts_one_privileged_worker_container_without_the_home() {
    let scratch = Scratch::new("docker-macos");
    scratch.tool(
        "docker",
        r#"case "$3" in
  inspect) echo "Error: No such object: lfs-dev" >&2; exit 1 ;;
  run) echo "pull access denied" >&2; exit 1 ;;
esac"#,
    );
    let mut spec = scratch.spec(Platform::Docker);
    let bundle = scratch.bundle();
    let error = machine::start(&spec, &bundle).unwrap_err();
    assert_eq!(
        error.code,
        Code::InvalidInput,
        "the worker container needs an image"
    );
    spec.os_image = Some("debian:trixie-slim".into());
    let error = machine::start(&spec, &bundle).unwrap_err();
    assert_eq!(error.code, Code::Failed, "{error}");
    let log = scratch.log();
    let run = line(&log, "docker -H unix:///var/run/docker.sock run");
    for part in [
        // it runs namespaces Sandboxes inside itself (D-8)
        "-d --pull never --name lfs-dev --init --privileged --cgroupns private",
        "--label layerfs.role=machine-worker --label layerfs.bundle=0123abcd",
        "-v lfs-dev-data:/layerfs/data",
        &format!("-v {}:/layerfs/bundle:ro", bundle.dir.display()),
        "--tmpfs /layerfs/state:mode=0700",
        "-e LAYERFS_WORKER_SECRET=/layerfs/state/secret",
        "--entrypoint sh debian:trixie-slim -c",
    ] {
        assert!(run.contains(part), "{part} not in {run}");
    }
    if cfg!(target_os = "linux") {
        // a Linux Engine's worker shares the host's network, listens on a
        // host loopback port and is given the name `reach` uses (#535 §7.4)
        assert!(
            run.contains("--network host --add-host host.docker.internal:host-gateway -v "),
            "{run}"
        );
        assert!(run.contains("-e LAYERFS_WORKER_LISTEN=127.0.0.1:"), "{run}");
        assert!(!run.contains(" -p "), "{run}");
    } else {
        assert!(run.contains("-p 127.0.0.1::7070"), "{run}");
        assert!(
            run.contains("-e LAYERFS_WORKER_LISTEN=0.0.0.0:7070"),
            "{run}"
        );
        assert!(!run.contains("--network host"), "{run}");
    }
    assert_eq!(
        run.matches(" -v ").count(),
        2,
        "data volume and bundle only, never Docker's socket: {run}"
    );
    assert!(!run.contains("docker.sock:"), "{run}");
    // its folders are made as a VM guest's are, and the host's user
    // namespace limit is left alone: the command filter blocks them
    assert!(
        log.contains("mkdir -p /layerfs/data/store /layerfs/data/sandbox"),
        "{log}"
    );
    assert!(!log.contains("max_user_namespaces"), "{log}");
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn docker_leaves_a_foreign_container_alone() {
    let scratch = Scratch::new("docker-foreign");
    scratch.tool(
        "docker",
        r#"[ "$3" = inspect ] && printf 'running\tsomething-else\n'; exit 0"#,
    );
    let mut spec = scratch.spec(Platform::Docker);
    spec.os_image = Some("debian:trixie-slim".into());
    let error = machine::start(&spec, &scratch.bundle()).unwrap_err();
    assert_eq!(error.code, Code::Exists);
    assert_eq!(scratch.log().lines().count(), 1);
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn docker_loads_the_bundle_image_once_and_answers_its_id() {
    let scratch = Scratch::new("docker-load");
    let id = format!("sha256:{}", "ab".repeat(32));
    scratch.tool(
        "docker",
        &format!(
            r#"case "$3" in
  load) [ -f "$5" ] && echo "Loaded image ID: {}" ;;
esac"#,
            id.to_uppercase().replace("SHA256", "sha256")
        ),
    );
    let archive = scratch.path("sandbox-image.tar");
    std::fs::write(&archive, b"").unwrap();
    let spec = scratch.spec(Platform::Docker);
    assert_eq!(machine::load_image(&spec, &archive).unwrap(), id);
    assert_eq!(
        scratch.log().trim(),
        format!(
            "docker -H unix:///var/run/docker.sock load -i {}",
            archive.display()
        )
    );
    // an answer with no image id is Failed
    let error = machine::load_image(&spec, &scratch.path("absent.tar")).unwrap_err();
    assert_eq!(error.code, Code::Failed, "{error}");
    // only the docker machine loads images
    let error = machine::load_image(&scratch.spec(Platform::Lima), &archive).unwrap_err();
    assert_eq!(error.code, Code::InvalidInput, "{error}");
}
