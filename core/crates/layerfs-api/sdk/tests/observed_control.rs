//! Normal and observed facades use one authenticated lifecycle with exact tokens.
use layerfs_bridge::{
    control::{Answer, Call, CleanupObservation, Reply, Request, WorkspaceToken},
    native,
};
use layerfs_history::WorkspaceId;
use layerfs_sdk::{
    control::{Control, ControlPhase},
    WorkspaceApi,
};
use std::{
    net::{TcpListener, TcpStream},
    thread,
    time::Duration,
};

fn pair(
    run: impl FnOnce(native::Connection) + Send + 'static,
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
        run(native::accept(server, &[52; 32], native::public_key(&[51; 32]).unwrap()).unwrap())
    });
    let connection =
        native::initiate(client, &[51; 32], native::public_key(&[52; 32]).unwrap()).unwrap();
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
        WorkspaceApi::new(&mut control).cleanup(token()).unwrap(),
        CleanupObservation::Gone
    );
    assert_eq!(
        WorkspaceApi::with_observations(&mut control, &[7; 32])
            .cleanup(token())
            .unwrap(),
        CleanupObservation::Gone
    );
    assert_eq!(
        WorkspaceApi::new(&mut control).cleanup(token()).unwrap(),
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
