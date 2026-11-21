//! Normal and observed facades use one authenticated lifecycle with exact tokens.
use layerfs_bridge::{
    control::{Answer, Call, CleanupObservation, Reply, Request, UnmountControl, WorkspaceToken},
    duplex,
};
use layerfs_history::WorkspaceId;
use layerfs_sandbox::internal::{Control, ControlPhase};
use std::{
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

fn pair<T: Send + 'static>(
    run: impl FnOnce(duplex::Connection) -> T + Send + 'static,
) -> (Control, thread::JoinHandle<T>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let client =
        TcpStream::connect_timeout(&listener.local_addr().unwrap(), Duration::from_secs(2))
            .unwrap();
    let (server, _) = listener.accept().unwrap();
    for socket in [&client, &server] {
        socket
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
    }
    let worker = thread::spawn(move || {
        run(duplex::accept(server, &[52; 32], duplex::public_key(&[51; 32]).unwrap()).unwrap())
    });
    let connection =
        duplex::initiate(client, &[51; 32], duplex::public_key(&[52; 32]).unwrap()).unwrap();
    (Control::new(connection), worker)
}
fn token() -> WorkspaceToken {
    WorkspaceToken {
        workspace: WorkspaceId::from_authority([3; 32]).unwrap(),
        namespace: 7,
    }
}
#[test]
fn observed_reply_keeps_original_exact_token_validation() {
    let (mut control, worker) = pair(|mut channel| {
        let call = Call::decode(channel.receive.receive().unwrap()).unwrap();
        let mut wrong = token();
        wrong.namespace += 1;
        let answer = Answer {
            id: call.id,
            reply: Reply::Cleanup {
                token: wrong,
                state: CleanupObservation::Gone,
            },
        };
        channel.send.send(&answer.encode().unwrap()).unwrap();
    });
    let failed = control
        .call(Request::Observed {
            scope: [7; 32],
            request: Box::new(Request::Cleanup(token())),
            traced: false,
        })
        .unwrap_err();
    assert!(failed.attempted);
    assert_eq!(failed.phase, ControlPhase::Validate);
    assert!(failed.received.is_some());
    worker.join().unwrap();
}
#[test]
fn unmount_force_selects_one_original_request_without_unknown_relinquishment() {
    use layerfs_bridge::control::{ControlCode, ControlRefusal};
    let (mut control, worker) = pair(|mut channel| {
        for force in [false, true] {
            let call = Call::decode(channel.receive.receive().unwrap()).unwrap();
            let reply = if force {
                assert_eq!(
                    call.request,
                    Request::ForceUnmount {
                        token: token(),
                        relinquish_unknown: false
                    }
                );
                Reply::Refused(ControlRefusal {
                    code: ControlCode::Busy,
                    phase: "force:admission".into(),
                    moved: None,
                    published: None,
                    detail: "original owner retained".into(),
                })
            } else {
                // The plain facade unmount discards uncommitted changes.
                assert_eq!(
                    call.request,
                    Request::Unmount(UnmountControl {
                        token: token(),
                        discard: true,
                    })
                );
                Reply::Unmounted(token())
            };
            channel
                .send
                .send(&Answer { id: call.id, reply }.encode().unwrap())
                .unwrap();
        }
    });
    assert_eq!(
        layerfs_sandbox::internal::workspace(&mut control)
            .unmount(token(), false)
            .unwrap(),
        None
    );
    let failure = layerfs_sandbox::internal::workspace(&mut control)
        .unmount(token(), true)
        .unwrap_err();
    assert!(
        matches!(failure.cause, layerfs_sandbox::OperationCause::Remote(ref cause) if cause.code == ControlCode::Busy && cause.phase == "force:admission")
    );
    worker.join().unwrap();
}

#[test]
fn issue_747_unknown_quarantines_the_session_after_delivering_the_refusal() {
    use layerfs_bridge::control::{ControlCode, ControlRefusal};
    for code in [ControlCode::Busy, ControlCode::Unknown] {
        let refusal = ControlRefusal {
            code,
            phase: "commit".into(),
            detail: "original outcome".into(),
            moved: None,
            published: None,
        };
        let reply = Reply::Refused(refusal);
        let original = reply.clone();
        let (mut control, worker) = pair(move |mut channel| {
            let call = Call::decode(channel.receive.receive().unwrap()).unwrap();
            channel
                .send
                .send(&Answer { id: call.id, reply }.encode().unwrap())
                .unwrap();
            if code == ControlCode::Busy {
                let next = Call::decode(channel.receive.receive().unwrap()).unwrap();
                assert_eq!(next.request, Request::EndSession);
                channel
                    .send
                    .send(
                        &Answer {
                            id: next.id,
                            reply: Reply::SessionEnded,
                        }
                        .encode()
                        .unwrap(),
                    )
                    .unwrap();
            }
            {
                assert!(
                    channel.receive.receive().is_err(),
                    "another request reached the terminal session"
                );
            }
        });
        assert_eq!(control.call(Request::Status(token())).unwrap(), original);
        if code == ControlCode::Unknown {
            assert!(
                control.is_failed(),
                "a terminal refusal must remove this connection from reuse"
            );
            let error = control.call(Request::Status(token())).unwrap_err();
            assert!(!error.attempted);
        } else {
            assert_eq!(
                control.call(Request::EndSession).unwrap(),
                Reply::SessionEnded
            );
        }
        worker.join().unwrap();
    }
}

#[test]
fn issue_747_only_explicit_discard_relinquishes_uncertain_commit() {
    use layerfs_bridge::control::*;
    use layerfs_history::{
        BranchId, BranchRecord, BranchSnapshot, CommitId, HistoryName, LayerId, LayerStackId,
    };
    use layerfs_sandbox::{
        daemon::pool::{Controls, Dial},
        host::Host,
        record::WorkspaceRecord,
        registry::{SandboxState, SANDBOXES},
    };
    use std::{fs, os::unix::fs::PermissionsExt, sync::Arc};
    for discard in [false, true] {
        let home = std::env::temp_dir().join(format!(
            "layerfs-747-force-{}-{discard}",
            std::process::id()
        ));
        fs::create_dir(&home).unwrap();
        fs::set_permissions(&home, fs::Permissions::from_mode(0o700)).unwrap();
        let home = home.canonicalize().unwrap();
        let host = Host {
            home: home.to_str().unwrap().into(),
            store: "shop".into(),
            project: "api".into(),
            sandbox: Some("s1".into()),
            store_record: None,
            store_path: None,
            deploy: None,
            postgresql: None,
            message: Default::default(),
            started: None,
        };
        let directory = layerfs_home::home::paths::sandbox_dir(&home, "s1");
        fs::create_dir_all(directory.join("workspace")).unwrap();
        for private in [&directory, &directory.join("workspace")] {
            fs::set_permissions(private, fs::Permissions::from_mode(0o700)).unwrap();
        }
        let id = "03".repeat(32);
        WorkspaceRecord::mounted(id.clone(), None, "main", "/work", 7)
            .write(&directory.join("workspace").join(format!("{id}.json")))
            .unwrap();
        let (control, worker) = pair(move |mut channel| {
            let query = Call::decode(channel.receive.receive().unwrap()).unwrap();
            assert_eq!(query.request, Request::Status(token()));
            let root = layerfs_content::ObjectId::for_bytes(b"root");
            let status = WorkspaceStatus {
                token: token(),
                binding: BranchSnapshot {
                    branch: BranchRecord {
                        forked_from: None,
                        id: BranchId::from_authority([1; 16]),
                        stack: LayerStackId::from_authority([2; 16]),
                        name: HistoryName::new("agent").unwrap(),
                        base_layer: LayerId::from_bytes([0x32; 33]).unwrap(),
                        head_commit: Some(CommitId::from_bytes([0x12; 33]).unwrap()),
                    },
                    head_root: Some(root),
                    base_root: root,
                    effective_root: root,
                    scope: layerfs_content::ObjectId::for_bytes(b"scope"),
                    profile: layerfs_content::filesystem::profile_id(),
                },
                activity: Activity::Uncertain,
                epoch: 19,
                epoch_saturated: false,
                published: None,
                local: None,
                local_failure: Some(ControlRefusal {
                    code: ControlCode::Busy,
                    phase: "observe".into(),
                    detail: "commit is uncertain".into(),
                    moved: None,
                    published: None,
                }),
                kernel: Some(MountStatus {
                    phase: MountPhase::Ready,
                    ready: Some(ReadyMount {
                        token: token(),
                        directory: "/work".into(),
                        receipt: MountReceipt {
                            mount: 1,
                            root: 1,
                            mount_id: 1,
                            device_major: 0,
                            device_minor: 1,
                            abi_major: 7,
                            abi_minor: 31,
                            offered: 0,
                            selected: 0,
                            max_write: 4096,
                            max_readahead: 4096,
                            max_background: 2,
                            congestion_threshold: 1,
                            page_size: 4096,
                            loops: 1,
                            abort_bound: true,
                        },
                    }),
                    detached: false,
                    work: None,
                }),
            };
            let mut command = query;
            while command.request == Request::Status(token()) {
                channel
                    .send
                    .send(
                        &Answer {
                            id: command.id,
                            reply: Reply::Status(Box::new(status.clone())),
                        }
                        .encode()
                        .unwrap(),
                    )
                    .unwrap();
                command = Call::decode(channel.receive.receive().unwrap()).unwrap();
            }
            channel
                .send
                .send(
                    &Answer {
                        id: command.id,
                        reply: Reply::Refused(ControlRefusal {
                            code: ControlCode::Busy,
                            phase: "force".into(),
                            detail: "test refusal".into(),
                            moved: None,
                            published: None,
                        }),
                    }
                    .encode()
                    .unwrap(),
                )
                .unwrap();
            command.request
        });
        let state = Arc::new(SandboxState {
            id: "s1".into(),
            project: "shop/api".into(),
            name: "s1".into(),
            directory: directory.clone(),
            runtime: None,
            identity: layerfs_sandbox_backend::CommandIdentity {
                uid: 1000,
                gid: 1000,
            },
            controls: Controls::new(
                control,
                Dial {
                    endpoint: "127.0.0.1:1".parse().unwrap(),
                    secret: [51; 32],
                    peer: duplex::public_key(&[52; 32]).unwrap(),
                    instance: [1; 32],
                },
            ),
            mounts: Default::default(),
        });
        SANDBOXES.lock().unwrap().insert(directory.clone(), state);
        let result =
            layerfs_sandbox::workspace::serve::unmount(&host, Some("/work"), discard, true);
        SANDBOXES.lock().unwrap().remove(&directory);
        fs::remove_dir_all(home).unwrap();
        assert_eq!(result.unwrap_err().message, "test refusal");
        assert_eq!(
            worker.join().unwrap(),
            Request::ForceUnmount {
                token: token(),
                relinquish_unknown: discard
            }
        );
    }
}
