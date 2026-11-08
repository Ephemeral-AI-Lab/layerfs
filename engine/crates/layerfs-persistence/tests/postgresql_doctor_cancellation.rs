//! Cancellation signals interrupt actual connection/preflight work, without sleeps.
#![cfg(feature = "postgresql")]
mod pg_doctor;
use layerfs_persistence::{
    diagnostic::{Options, Outcome},
    PostgresSelection,
};
use pg_doctor::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
#[test]
fn pre_cancelled_connection_does_not_resolve_or_open_any_socket() {
    let cancelled = AtomicBool::new(true);
    let selection = PostgresSelection::new(
        "postgresql://unused@must-not-resolve.invalid/unused",
        "unused",
    );
    let mut out = Vec::new();
    layerfs_persistence::diagnostic::postgresql_connection(
        &selection,
        BINDING,
        Options {
            cancelled: Some(&cancelled),
            ..Options::default()
        },
        &mut |_, _, _| panic!("pre-cancelled request reached resolver"),
        &mut |o| out.push(o),
    )
    .unwrap();
    assert!(has(&out, "store.connection.cancelled", Outcome::Unknown));
    no_corruption(&out);
}
fn signal_from_stalled_peer(tls: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    let peer = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(2);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until =>
                {
                    std::thread::yield_now()
                }
                Err(_) => return false,
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        if tls {
            let mut request = [0; 8];
            if socket.read_exact(&mut request).is_err() {
                return false;
            }
            assert_eq!(request, [0, 0, 0, 8, 4, 210, 22, 47]);
            if socket.write_all(b"S").is_err() {
                return false;
            }
            let mut hello = [0; 4096];
            if !matches!(socket.read(&mut hello),Ok(n)if n>0) {
                return false;
            }
        } else {
            let mut length = [0; 4];
            if socket.read_exact(&mut length).is_err() {
                return false;
            }
            let length = u32::from_be_bytes(length) as usize;
            if !(8..=4096).contains(&length) {
                return false;
            }
            let mut startup = vec![0; length - 4];
            if socket.read_exact(&mut startup).is_err() {
                return false;
            }
        }
        signal.store(true, Ordering::Relaxed);
        let mut bytes = [0; 4096];
        loop {
            match socket.read(&mut bytes) {
                Ok(0) => return true,
                Ok(_) => {}
                Err(_) => return false,
            }
        }
    });
    let selected = PostgresSelection::new(
        format!(
            "postgresql://unused@{address}/unused?sslmode={}",
            if tls { "require" } else { "disable" }
        ),
        "unused",
    );
    let start = Instant::now();
    let mut out = Vec::new();
    layerfs_persistence::diagnostic::postgresql_connection(
        &selected,
        BINDING,
        Options {
            cancelled: Some(&cancelled),
            deadline: None,
            ..Options::default()
        },
        &mut resolver,
        &mut |o| out.push(o),
    )
    .unwrap();
    assert!(start.elapsed() < Duration::from_millis(1500));
    assert!(cancelled.load(Ordering::Relaxed));
    assert!(peer.join().unwrap(),"actual peer observed startup/ClientHello, signalled cancellation and independently observed socket EOF");
    assert!(has(&out, "store.connection.transport", Outcome::Unknown));
    no_corruption(&out);
}
#[test]
fn cancellation_during_stalled_authentication_closes_socket() {
    signal_from_stalled_peer(false);
}
#[test]
fn cancellation_during_stalled_tls_closes_socket() {
    signal_from_stalled_peer(true);
}
#[test]
fn blocked_preflight_identity_query_is_cancelled_and_backend_work_is_released() {
    let Some(mut f) = Fixture::new("preflight_cancel") else {
        return;
    };
    let before = f.fingerprint();
    let app = f.selection.schema.clone();
    let mut selected = f.selection.clone();
    selected.url.push_str(&format!("&application_name={app}"));
    f.admin
        .batch_execute(&format!(
            "BEGIN; LOCK TABLE {}.history_meta IN ACCESS EXCLUSIVE MODE",
            f.selection.schema
        ))
        .unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    let monitor_url = f.selection.url.clone();
    let monitor_app = app.clone();
    let monitor = std::thread::spawn(move || {
        let mut client = postgres::Client::connect(&monitor_url, postgres::NoTls).unwrap();
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            let blocked:bool=client.query_one("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE application_name=$1 AND state='active' AND wait_event_type='Lock')",&[&monitor_app]).unwrap().get(0);
            if blocked {
                signal.store(true, Ordering::Relaxed);
                return true;
            }
            if Instant::now() >= until {
                signal.store(true, Ordering::Relaxed);
                return false;
            }
            std::thread::yield_now();
        }
    });
    let start = Instant::now();
    let mut out = Vec::new();
    layerfs_persistence::diagnostic::postgresql_connection(
        &selected,
        BINDING,
        Options {
            deadline: None,
            cancelled: Some(&cancelled),
            ..Options::default()
        },
        &mut resolver,
        &mut |o| out.push(o),
    )
    .unwrap();
    assert!(
        monitor.join().unwrap(),
        "independent backend monitor observed actual lock wait before cancellation"
    );
    assert!(start.elapsed() < Duration::from_millis(1500));
    assert!(
        has(&out, "store.connection.cancellation.release", Outcome::Pass),
        "{out:?}"
    );
    no_corruption(&out);
    let count:i64=f.admin.query_one("SELECT count(*) FROM pg_stat_activity WHERE application_name=$1 AND (backend_xmin IS NOT NULL OR wait_event_type='Lock')",&[&app]).unwrap().get(0);
    assert_eq!(count, 0, "actual preflight query and snapshot released");
    f.admin.batch_execute("ROLLBACK").unwrap();
    assert_eq!(before, f.fingerprint());
}
#[test]
fn observer_panic_joins_the_preflight_cancellation_watcher() {
    let Some(f) = Fixture::new("panic_watch") else {
        return;
    };
    let cancelled = AtomicBool::new(false);
    let start = Instant::now();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = layerfs_persistence::diagnostic::postgresql_connection(
            &f.selection,
            BINDING,
            Options {
                cancelled: Some(&cancelled),
                ..Options::default()
            },
            &mut resolver,
            &mut |o| {
                if o.id == "store.connection.authentication" {
                    panic!("expected private fixture observer panic")
                }
            },
        );
    }));
    assert!(result.is_err());
    assert!(!cancelled.load(Ordering::Relaxed));
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "watcher must stop and join even when its observer unwinds"
    );
}
