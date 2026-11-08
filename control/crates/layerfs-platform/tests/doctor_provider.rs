//! Provider state is independent of worker health; observation executes no guest commands.
#![cfg(unix)]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{provider_within, Code, Platform, State};
use standin::Scratch;
use std::time::{Duration, Instant};

#[test]
fn docker_running_provider_does_not_require_a_worker() {
    let scratch = Scratch::new("doctor-running");
    scratch.tool(
        "docker",
        r#"case "$*" in
*version*) echo 29.0;;
*inspect*) printf 'running\tmachine-worker\n';;
*) echo unexpected >&2; exit 71;;
esac"#,
    );
    let spec = scratch.spec(Platform::Docker);
    let result = provider_within(&spec, Instant::now() + Duration::from_secs(10)).unwrap();
    assert_eq!(result.state, State::Running);
    assert_eq!(result.free_disk, None);
    assert!(!scratch.log().contains("exec"));
    assert_eq!(scratch.log().lines().count(), 2);
}

#[test]
fn docker_stopped_missing_foreign_and_transitional_are_distinct() {
    let scratch = Scratch::new("doctor-states");
    for (answer, expected) in [
        ("printf 'exited\\tmachine-worker\\n'", Some(State::Stopped)),
        ("echo no such object >&2; exit 1", Some(State::Missing)),
        ("printf 'running\\tother\\n'", None),
        ("printf 'paused\\tmachine-worker\\n'", None),
    ] {
        scratch.tool(
            "docker",
            &format!("case \"$*\" in *version*) echo 29.0;; *) {answer};; esac"),
        );
        let result = provider_within(
            &scratch.spec(Platform::Docker),
            Instant::now() + Duration::from_secs(10),
        );
        match expected {
            Some(state) => assert_eq!(result.unwrap().state, state),
            None => assert!(result.is_err()),
        }
    }
}

#[test]
fn missing_provider_tool_and_expired_budget_start_nothing() {
    let scratch = Scratch::new("doctor-missing");
    let error = provider_within(
        &scratch.spec(Platform::Docker),
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap_err();
    assert_eq!(error.code, Code::Unsupported);
    assert!(scratch.log().is_empty());
    scratch.tool("docker", "echo unexpected; exit 1");
    assert!(provider_within(&scratch.spec(Platform::Docker), Instant::now()).is_err());
    assert!(scratch.log().is_empty());
}

#[test]
fn provider_deadline_reaps_command_and_pipe_holding_descendant() {
    let scratch = Scratch::new("doctor-deadline");
    scratch.tool(
        "docker",
        "/bin/sleep 10 & echo $! > \"$SCRATCH/child\"; exit 0",
    );
    let started = Instant::now();
    assert!(provider_within(
        &scratch.spec(Platform::Docker),
        started + Duration::from_millis(100)
    )
    .is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[cfg(target_os = "macos")]
#[test]
fn lima_observes_provider_without_shell_or_worker() {
    let scratch = Scratch::new("doctor-lima");
    scratch.tool("limactl", "case \"$*\" in *--version*) echo 2.2;; *list*) printf 'Running\\t/tmp/lima\\n';; *) exit 71;; esac");
    assert_eq!(
        provider_within(
            &scratch.spec(Platform::Lima),
            Instant::now() + Duration::from_secs(10)
        )
        .unwrap()
        .state,
        State::Running
    );
    assert!(!scratch.log().contains("shell"));
}
