//! Force stop uses only provider power operations, retaining machines and disks.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Code, Platform};
use standin::Scratch;

#[test]
fn native_force_stop_never_stops_the_host() {
    let scratch = Scratch::new("force-native");
    machine::stop_force(&scratch.spec(Platform::Native)).unwrap();
    assert!(scratch.log().is_empty());
}

#[test]
fn docker_force_stops_only_the_owned_worker_with_zero_grace() {
    for state in ["running", "paused", "restarting"] {
        let scratch = Scratch::new("force-docker");
        scratch.tool("docker", &format!(
            "case \"$3\" in inspect) printf '{state}\\tmachine-worker\\n';; stop) exit 0;; *) exit 99;; esac"
        ));
        machine::stop_force(&scratch.spec(Platform::Docker)).unwrap();
        let log = scratch.log();
        let calls: Vec<_> = log.lines().collect();
        assert_eq!(calls.len(), 2, "{log}");
        assert!(calls[0].contains(" inspect -f "), "{log}");
        assert_eq!(
            calls[1],
            "docker -H unix:///var/run/docker.sock stop -t 0 lfs-dev"
        );
    }
}

#[test]
fn docker_force_preserves_unowned_containers_and_reports_inspect_failure() {
    for (answer, code) in [
        ("printf 'running\\tother-role\\n'", Code::InvalidInput),
        ("echo 'engine unavailable' >&2; exit 1", Code::Failed),
    ] {
        let scratch = Scratch::new("force-docker-refuse");
        scratch.tool("docker", answer);
        let error = machine::stop_force(&scratch.spec(Platform::Docker)).unwrap_err();
        assert_eq!(error.code, code, "{error}");
        assert_eq!(scratch.log().lines().count(), 1);
    }
}

#[test]
fn docker_missing_or_stopped_worker_does_not_boot_or_remove_anything() {
    for answer in [
        "echo 'No such object: lfs-dev' >&2; exit 1",
        "printf 'exited\\tmachine-worker\\n'",
    ] {
        let scratch = Scratch::new("force-docker-idle");
        scratch.tool("docker", answer);
        machine::stop_force(&scratch.spec(Platform::Docker)).unwrap();
        assert_eq!(scratch.log().lines().count(), 1);
    }
}

#[test]
fn ordinary_docker_stop_retains_grace_period() {
    let scratch = Scratch::new("normal-docker-stop");
    scratch.tool("docker", "case \"$3\" in inspect) printf 'running\\tmachine-worker\\n';; stop) exit 0;; *) exit 99;; esac");
    machine::stop(&scratch.spec(Platform::Docker)).unwrap();
    assert!(scratch.log().ends_with(" stop -t 5 lfs-dev\n"));
}

#[test]
fn lima_force_stops_running_and_broken_instances_without_guest_calls() {
    for state in ["Running", "Broken"] {
        let scratch = Scratch::new("force-lima");
        scratch.tool(
            "limactl",
            &format!(
            "case \"$1\" in list) printf '{state}\\t/unused\\n';; stop) exit 0;; *) exit 99;; esac"
        ),
        );
        machine::stop_force(&scratch.spec(Platform::Lima)).unwrap();
        let log = scratch.log();
        assert_eq!(log.lines().count(), 2, "{log}");
        assert!(log.ends_with("limactl stop --force lfs-dev\n"), "{log}");
    }
}

#[test]
fn lima_idle_force_and_normal_stop_do_not_change_disks() {
    for state in ["Stopped", ""] {
        let scratch = Scratch::new("force-lima-idle");
        scratch.tool("limactl", &format!("printf '{state}\\n'"));
        machine::stop_force(&scratch.spec(Platform::Lima)).unwrap();
        assert_eq!(scratch.log().lines().count(), 1);
    }
    let scratch = Scratch::new("normal-lima-stop");
    scratch.tool(
        "limactl",
        "case \"$1\" in list) printf 'Running\\t/unused\\n';; stop) exit 0;; *) exit 99;; esac",
    );
    machine::stop(&scratch.spec(Platform::Lima)).unwrap();
    assert!(scratch.log().ends_with("limactl stop lfs-dev\n"));
}

#[test]
fn apple_force_stops_running_or_failed_container_without_guest_calls() {
    for state in ["running", "failed"] {
        let scratch = Scratch::new("force-apple");
        scratch.tool("container", &format!(
            "case \"$1\" in inspect) echo '[{{\"status\":{{\"state\":\"{state}\"}}}}]';; stop) exit 0;; *) exit 99;; esac"
        ));
        machine::stop_force(&scratch.spec(Platform::AppleContainer)).unwrap();
        assert_eq!(
            scratch.log(),
            "container inspect lfs-dev\ncontainer stop lfs-dev\n"
        );
    }
}

#[test]
fn apple_missing_or_stopped_container_does_not_start_or_delete() {
    for answer in ["[]", "[{\"status\":{\"state\":\"stopped\"}}]"] {
        let scratch = Scratch::new("force-apple-idle");
        scratch.tool("container", &format!("echo '{answer}'"));
        machine::stop_force(&scratch.spec(Platform::AppleContainer)).unwrap();
        assert_eq!(scratch.log(), "container inspect lfs-dev\n");
    }
}
