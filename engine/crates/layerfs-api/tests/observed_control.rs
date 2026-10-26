//! Normal and observed facades use one authenticated lifecycle with exact tokens.
use layerfs_api::{
    internal::{Control, ControlPhase},
    WorkspaceApi,
};
use layerfs_bridge::{
    control::{Answer, Call, CleanupObservation, Reply, Request, UnmountControl, WorkspaceToken},
    duplex,
};
use layerfs_history::WorkspaceId;
use std::{
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

fn pair(
    run: impl FnOnce(duplex::Connection) + Send + 'static,
) -> (Control, thread::JoinHandle<()>) {
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
fn facade_observation_is_explicit_and_does_not_persist_on_channel() {
    let (mut control, worker) = pair(|mut channel| {
        for observed in [false, true, false] {
            let bytes = channel.receive.receive().unwrap();
            let call = Call::decode(bytes).unwrap();
            assert_eq!(matches!(call.request, Request::Observed { .. }), observed);
            if let Request::Observed { scope, .. } = &call.request {
                assert_eq!(*scope, [7; 32]);
            }
            assert_eq!(
                call.request.without_observation().unwrap(),
                &Request::Cleanup(token())
            );
            let answer = Answer {
                id: call.id,
                reply: Reply::Cleanup {
                    token: token(),
                    state: CleanupObservation::Gone,
                },
            };
            channel.send.send(&answer.encode().unwrap()).unwrap();
        }
    });
    assert_eq!(
        layerfs_api::internal::workspace(&mut control)
            .cleanup(token())
            .unwrap(),
        CleanupObservation::Gone
    );
    assert_eq!(
        layerfs_api::internal::with_observations(&mut control, &[7; 32])
            .cleanup(token())
            .unwrap(),
        CleanupObservation::Gone
    );
    assert_eq!(
        layerfs_api::internal::workspace(&mut control)
            .cleanup(token())
            .unwrap(),
        CleanupObservation::Gone
    );
    worker.join().unwrap();
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
fn observed_facade_borrows_scope_with_same_two_pointer_layout() {
    assert_eq!(
        std::mem::size_of::<WorkspaceApi<'_>>(),
        2 * std::mem::size_of::<usize>()
    );
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
        layerfs_api::internal::workspace(&mut control)
            .unmount(token(), false)
            .unwrap(),
        None
    );
    let failure = layerfs_api::internal::workspace(&mut control)
        .unmount(token(), true)
        .unwrap_err();
    assert!(
        matches!(failure.cause, layerfs_api::OperationCause::Remote(ref cause) if cause.code == ControlCode::Busy && cause.phase == "force:admission")
    );
    worker.join().unwrap();
}
