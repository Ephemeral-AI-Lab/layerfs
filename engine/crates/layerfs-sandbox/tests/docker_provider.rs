//! Docker provider contracts exercised through bounded Unix HTTP peers, without an Engine.
#![cfg(unix)]
#[allow(dead_code)]
#[path = "support/engine.rs"]
mod peer;
use layerfs_sandbox::{
    backend::docker::{DockerProvider, ProviderFailure, Sandbox, SandboxRequest},
    provider::{
        LaunchPhase, LaunchRequest, ProviderLaunchOptions, SandboxInstance, SandboxProvider,
    },
    CommandIdentity, ContainerId, ExecRequest, RuntimeError,
};
use peer::{fixed, frame, upgrade, Peer};
use std::{
    fs, io,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
const CID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const EID: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
const IMAGE: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
static NEXT: AtomicUsize = AtomicUsize::new(0);
fn deployment() -> SandboxRequest {
    SandboxRequest {
        image: IMAGE.into(),
        store_volume: "provider-store".into(),
        port: 30421,
        labels: vec![],
    }
}
struct Binary(PathBuf);
impl Binary {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "h22-provider-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, b"sealed executable").unwrap();
        Self(path)
    }
    fn launch(&self) -> LaunchRequest {
        LaunchRequest {
            name: "h22-sandbox-1".into(),
            daemon_executable: self.0.clone(),
            daemon_config: b"private config".to_vec(),
            labels: vec![],
            startup_timeout: Duration::from_secs(2),
            options: ProviderLaunchOptions::Local {
                command_identity: CommandIdentity { uid: 501, gid: 20 },
                environment: vec![],
            },
        }
    }
}
impl Drop for Binary {
    fn drop(&mut self) {
        fs::remove_file(&self.0).unwrap();
    }
}
fn logs(payload: &[u8]) -> Vec<u8> {
    let mut bytes = format!("HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.multiplexed-stream\r\nContent-Length: {}\r\n\r\n", payload.len()).into_bytes();
    bytes.extend(payload);
    bytes
}
fn launch_responses() -> Vec<Vec<u8>> {
    vec![
        fixed(
            200,
            br#"{"Name":"provider-store","Driver":"local","Scope":"local","Options":null}"#,
        ),
        fixed(201, format!(r#"{{"Id":"{CID}"}}"#).as_bytes()),
        fixed(200, b""),
        fixed(204, b""),
        logs(&frame(1, b"LAYERFS_DAEMON_LISTEN 0.0.0.0:30421\n")),
    ]
}
fn exec_request() -> ExecRequest {
    ExecRequest {
        identity: CommandIdentity { uid: 501, gid: 20 },
        arguments: vec!["bash".into(), "-c".into(), "printf ordinary".into()],
        environment: vec![],
        directory: "/workspaces/1".into(),
        stdin: false,
    }
}
fn exec_inspection(running: bool, exit: &str) -> Vec<u8> {
    fixed(200, format!(r#"{{"ID":"{EID}","ContainerID":"{CID}","Running":{running},"ExitCode":{exit},"Pid":42}}"#).as_bytes())
}
#[test]
fn launch_stream_log_stop_delete_and_negotiated_versions_use_the_original_runtime() {
    let binary = Binary::new();
    for minor in [48, 54] {
        let mut responses = vec![fixed(
            200,
            format!(r#"{{"ApiVersion":"1.{minor}"}}"#).as_bytes(),
        )];
        responses.extend(launch_responses());
        let mut payload = frame(1, &vec![b'x'; 1024 * 1024 + 3]);
        payload.extend(frame(2, b"err"));
        responses.extend([
            fixed(201, format!(r#"{{"Id":"{EID}"}}"#).as_bytes()),
            upgrade(&payload),
            exec_inspection(false, "7"),
            logs(&frame(1, b"daemon log\n")),
            fixed(204, b""),
            fixed(204, b""),
        ]);
        let (docker, peer) = Peer::scripted(responses, 8192);
        let provider = DockerProvider::new(docker, deployment());
        assert_eq!(provider.capabilities().name, "docker");
        assert!(provider.local_directory("anything").unwrap().is_none());
        let mut instance = provider.launch(binary.launch()).unwrap();
        assert_eq!(instance.identity().to_string(), CID);
        assert_eq!(instance.scratch_directory(), "/tmp");
        assert!(instance.local_record().is_none());
        instance.track(42, 1).unwrap();
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let exit = instance
            .run(&exec_request(), &mut stdout, &mut stderr)
            .unwrap();
        assert_eq!((exit.code, exit.pid), (7, Some(42)));
        assert_eq!(exit.stdout_bytes, 1024 * 1024 + 3);
        assert_eq!(stdout.len() as u64, exit.stdout_bytes);
        assert_eq!(stderr, b"err");
        assert_eq!(instance.log_tail(20, 2048).unwrap(), b"daemon log\n");
        instance.stop(1).unwrap();
        instance.delete().unwrap();
        assert!(instance.stop(1).is_err());
        assert!(instance.delete().is_err());
        let calls = peer.finish();
        assert_eq!(calls.len(), 12);
        assert!(calls[0].starts_with(b"GET /version "));
        for call in &calls[1..] {
            assert!(String::from_utf8_lossy(call)
                .split(' ')
                .nth(1)
                .unwrap()
                .starts_with(&format!("/v1.{minor}/")));
        }
        assert!(String::from_utf8_lossy(&calls[2]).contains("create?name=h22-sandbox-1"));
        assert!(String::from_utf8_lossy(calls.last().unwrap()).contains("force=false&v=false"));
    }
}
#[test]
fn uncertain_create_retains_partial_id_and_never_adopts_or_cleans_it() {
    let binary = Binary::new();
    let mut responses = launch_responses();
    responses.truncate(2);
    responses[1] = fixed(201, format!(r#"{{"Id":"{CID}","Id":"{CID}"}}"#).as_bytes());
    let (docker, peer) = Peer::new(responses, 1);
    let failure = DockerProvider::new(docker, deployment())
        .launch(binary.launch())
        .err()
        .unwrap();
    assert_eq!(failure.phase, LaunchPhase::Create);
    assert!(failure.instance.is_none());
    let RuntimeError::Docker(cause) = failure.cause else {
        panic!("typed original cause")
    };
    let ProviderFailure::Create(cause) = *cause else {
        panic!("create custody")
    };
    assert_eq!(cause.transfer.observed_container.unwrap().to_string(), CID);
    assert!(cause.transfer.attempted);
    assert_eq!(peer.finish().len(), 2);
}
#[test]
fn upload_start_and_listener_failures_keep_acknowledged_instance_and_original_receipts() {
    let binary = Binary::new();
    for (stage, expected) in [
        (2, LaunchPhase::Create),
        (3, LaunchPhase::Start),
        (4, LaunchPhase::Listener),
    ] {
        let mut responses = launch_responses();
        responses.truncate(stage + 1);
        responses[stage] = fixed(409, b"refused");
        let (docker, peer) = Peer::new(responses, 8192);
        let mut failure = DockerProvider::new(docker, deployment())
            .launch(binary.launch())
            .err()
            .unwrap();
        assert_eq!(failure.phase, expected);
        assert_eq!(failure.instance.take().unwrap().identity().to_string(), CID);
        let RuntimeError::Docker(cause) = failure.cause else {
            panic!("typed receipt")
        };
        match (*cause, stage) {
            (ProviderFailure::Upload(f), 2) => {
                assert!(f.work.tar_bytes > 0);
                assert_eq!(f.transfer.status, Some(409));
            }
            (ProviderFailure::Start(f), 3) => assert_eq!(f.status, Some(409)),
            (ProviderFailure::Listener(f), 4) => assert_eq!(f.transfer.status, Some(409)),
            _ => panic!("original stage lost"),
        }
        assert_eq!(peer.finish().len(), stage + 1);
    }
}
#[test]
fn validation_and_stdin_refuse_without_an_engine_attempt() {
    let binary = Binary::new();
    let (docker, peer) = Peer::new(vec![], 1);
    let provider = DockerProvider::new(docker.clone(), deployment());
    let mut request = binary.launch();
    request.name = "../escape".into();
    assert_eq!(
        provider.launch(request).err().unwrap().phase,
        LaunchPhase::Validate
    );
    let stopped = Sandbox::stopped(docker, ContainerId::parse(CID).unwrap(), deployment());
    let mut request = exec_request();
    request.stdin = true;
    assert!(
        !SandboxInstance::run(&stopped, &request, &mut Vec::new(), &mut Vec::new())
            .unwrap_err()
            .attempted
    );
    assert!(peer.finish().is_empty());
}
#[test]
fn eof_with_unknown_exit_preserves_the_handle_without_polling_or_replay() {
    let binary = Binary::new();
    let mut responses = launch_responses();
    responses.extend([
        fixed(201, format!(r#"{{"Id":"{EID}"}}"#).as_bytes()),
        upgrade(&frame(1, b"ok")),
        exec_inspection(true, "null"),
    ]);
    let (docker, peer) = Peer::new(responses, 8192);
    let instance = DockerProvider::new(docker, deployment())
        .launch(binary.launch())
        .unwrap();
    let failure = instance
        .run(&exec_request(), &mut Vec::new(), &mut Vec::new())
        .unwrap_err();
    assert!(failure.attempted);
    assert_eq!(failure.requested_exec.unwrap().to_string(), EID);
    let RuntimeError::Docker(cause) = failure.cause else {
        panic!("typed custody")
    };
    let ProviderFailure::ExecExitUnknown {
        handle,
        inspection,
        progress,
    } = *cause
    else {
        panic!("EOF must not invent exit")
    };
    assert_eq!(progress.stdout, 2);
    assert_eq!(progress.stderr, 0);
    assert!(inspection.running);
    assert_eq!(handle.identity().1.to_string(), EID);
    assert_eq!(peer.finish().len(), 8);
}
#[test]
fn exec_stage_failures_keep_deciding_wire_facts_and_delivered_progress() {
    let binary = Binary::new();
    for stage in 0..3 {
        let mut responses = launch_responses();
        let mut exec = vec![
            fixed(201, format!(r#"{{"Id":"{EID}"}}"#).as_bytes()),
            upgrade(&frame(1, b"delivered")),
            exec_inspection(false, "0"),
        ];
        exec.truncate(stage + 1);
        exec[stage] = fixed(409, b"original refusal");
        responses.extend(exec);
        let (docker, peer) = Peer::new(responses, 8192);
        let owner = DockerProvider::new(docker, deployment())
            .launch(binary.launch())
            .unwrap();
        let failure = owner
            .run(&exec_request(), &mut Vec::new(), &mut Vec::new())
            .unwrap_err();
        assert!(failure.attempted);
        assert_eq!(failure.status, Some(409));
        assert!(failure.sent_bytes > 0);
        let RuntimeError::Docker(cause) = failure.cause else {
            panic!("original custody")
        };
        match (*cause, stage) {
            (ProviderFailure::ExecCreate(f), 0) => {
                assert_eq!(failure.sent_bytes, f.transfer.sent_bytes)
            }
            (ProviderFailure::ExecStart(f), 1) => {
                assert_eq!(failure.sent_bytes, f.transfer.sent_bytes);
                assert_eq!(f.created.identity().1.to_string(), EID);
            }
            (
                ProviderFailure::ExecInspect {
                    failure: f,
                    handle,
                    progress,
                },
                2,
            ) => {
                assert_eq!(failure.sent_bytes, f.sent_bytes);
                assert_eq!(handle.identity().1.to_string(), EID);
                assert_eq!(progress.stdout, 9);
                assert_eq!(progress.stderr, 0);
            }
            _ => panic!("wrong stage"),
        }
        assert_eq!(peer.finish().len(), 6 + stage);
    }
}
struct BrokenSink;
impl io::Write for BrokenSink {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[test]
fn failed_sink_retains_pending_output_and_exec_identity_without_inspection_or_cleanup() {
    let binary = Binary::new();
    let mut responses = launch_responses();
    responses.extend([
        fixed(201, format!(r#"{{"Id":"{EID}"}}"#).as_bytes()),
        upgrade(&frame(1, b"undelivered")),
    ]);
    let (docker, peer) = Peer::new(responses, 8192);
    let instance = DockerProvider::new(docker, deployment())
        .launch(binary.launch())
        .unwrap();
    let failure = instance
        .run(&exec_request(), &mut BrokenSink, &mut Vec::new())
        .unwrap_err();
    assert!(failure.attempted);
    assert_eq!(failure.requested_exec.unwrap().to_string(), EID);
    let RuntimeError::Docker(cause) = failure.cause else {
        panic!("typed custody")
    };
    let ProviderFailure::ExecOutput { handle, failure } = *cause else {
        panic!("stream owner retained")
    };
    assert_eq!(handle.identity().1.to_string(), EID);
    assert_eq!(failure.output.progress().stdout, 0);
    assert_eq!(failure.output.progress().wire_payload, 11);
    assert_eq!(peer.finish().len(), 7);
}
