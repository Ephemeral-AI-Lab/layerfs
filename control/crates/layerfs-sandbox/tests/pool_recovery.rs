//! A Sandbox's connection limit after the daemon refused an open: callers
//! take turns on the connections open, and after `RECOVER` the limit is
//! `CONNECTIONS` again, so a long-lived process is not held to a spike's
//! limit. It never passes `CONNECTIONS`.
//!
//! Nothing runs: a stand-in daemon admits at most `cap` connections at a
//! time and answers the next with a capacity notice before any handshake,
//! as a full daemon does.
use layerfs_bridge::control::{Answer, Call, DaemonPhase, DaemonStatus, Reply, Request};
use layerfs_bridge::duplex;
use layerfs_bridge::duplex::admission::{self, AdmissionNotice, AdmissionReason};
use layerfs_sandbox::daemon::pool::{Controls, Dial, CONNECTIONS, RECOVER};
use std::io::Read;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const CLIENT_PRIVATE: [u8; 32] = [2; 32];
const DAEMON_PRIVATE: [u8; 32] = [1; 32];
const DAEMON_INSTANCE: [u8; 32] = [3; 32];

/// The stand-in's admission: how many it admits at a time, how many it
/// holds now, and how many it refused.
#[derive(Default)]
struct Admission {
    cap: AtomicUsize,
    live: AtomicUsize,
    refused: AtomicUsize,
}

fn daemon(cap: usize) -> (SocketAddr, Arc<Admission>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = listener.local_addr().unwrap();
    let admission = Arc::new(Admission::default());
    admission.cap.store(cap, Ordering::SeqCst);
    let counted = admission.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            if counted.live.load(Ordering::SeqCst) >= counted.cap.load(Ordering::SeqCst) {
                counted.refused.fetch_add(1, Ordering::SeqCst);
                let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
                let _ = stream.set_read_timeout(Some(Duration::from_millis(50)));
                let notice = AdmissionNotice::new(AdmissionReason::NoPoolSlot, 0, 0, 0);
                if admission::write(&mut stream, &notice).is_ok() {
                    let _ = stream.read(&mut [0u8; 512]);
                }
                continue;
            }
            counted.live.fetch_add(1, Ordering::SeqCst);
            let admitted = counted.clone();
            std::thread::spawn(move || {
                serve(stream);
                admitted.live.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    (endpoint, admission)
}

/// One admitted connection: its Hello, until the far side closes it
/// (bounded by its 30 s read wait).
fn serve(stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    let Ok(mut channel) = duplex::accept(
        stream,
        &DAEMON_PRIVATE,
        duplex::public_key(&CLIENT_PRIVATE).unwrap(),
    ) else {
        return;
    };
    while let Ok(bytes) = channel.receive.receive() {
        let Ok(call) = Call::decode(bytes) else { break };
        let reply = match call.request {
            Request::Hello(_) => Reply::Hello(DaemonStatus {
                instance: DAEMON_INSTANCE,
                phase: DaemonPhase::ControlReady,
                overlay_sqlite: Some("3.51.0".into()),
                store_sqlite: Some("3.51.0".into()),
            }),
            _ => continue,
        };
        let answer = Answer { id: call.id, reply }.encode().unwrap();
        if channel.send.send(&answer).is_err() {
            break;
        }
    }
}

fn dial(endpoint: SocketAddr) -> Dial {
    Dial {
        endpoint,
        secret: CLIENT_PRIVATE,
        peer: duplex::public_key(&DAEMON_PRIVATE).unwrap(),
        instance: DAEMON_INSTANCE,
    }
}

#[test]
fn a_lowered_limit_recovers_after_a_spike() {
    let (endpoint, admission) = daemon(2);
    let dial = dial(endpoint);
    let controls = Controls::new(dial.open().unwrap(), dial);
    let short = Duration::from_millis(300);

    // a spike: the daemon admits two; a third caller is refused an open,
    // waits its turn and is `Busy`
    let first = controls.lend_within(short).unwrap();
    let second = controls.lend_within(short).unwrap();
    let busy = controls
        .lend_within(short)
        .err()
        .expect("the daemon is full");
    assert_eq!(busy.code, "Busy", "{busy:?}");
    assert_eq!(admission.refused.load(Ordering::SeqCst), 1);

    // the daemon has room again, but the lowered limit holds: no open is
    // tried before `RECOVER`
    admission.cap.store(8, Ordering::SeqCst);
    let held = controls.lend_within(short).err().expect("the limit holds");
    assert_eq!(held.code, "Busy", "{held:?}");
    assert_eq!(admission.refused.load(Ordering::SeqCst), 1);

    // after it, the limit is `CONNECTIONS` again: a third, and up to six
    std::thread::sleep(RECOVER);
    let mut lent = vec![first, second];
    while lent.len() < CONNECTIONS {
        lent.push(controls.lend_within(short).unwrap());
    }
    assert_eq!(admission.live.load(Ordering::SeqCst), CONNECTIONS);
    // and never more
    let seventh = controls.lend_within(short).err().expect("six at most");
    assert_eq!(seventh.code, "Busy", "{seventh:?}");
    assert_eq!(admission.live.load(Ordering::SeqCst), CONNECTIONS);
    assert_eq!(admission.refused.load(Ordering::SeqCst), 1);
    drop(lent);
}
