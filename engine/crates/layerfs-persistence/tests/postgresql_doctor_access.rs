//! Actual isolated PostgreSQL authority, privilege, transport and loss matrix.
#![cfg(feature = "postgresql")]
mod pg_doctor;
use layerfs_persistence::{
    diagnostic::{Options, Outcome},
    PostgresHandles,
};
use pg_doctor::*;
use std::time::{Duration, Instant};
#[test]
fn saved_authority_schema_authentication_tls_and_transport_are_distinct() {
    let Some(mut f) = Fixture::new("access") else {
        return;
    };
    let before = f.fingerprint();
    let out = connection(&f.selection, BINDING);
    assert!(
        has(&out, "store.connection.identity", Outcome::Pass),
        "{out:?}"
    );
    let out = connection(&f.selection, b"wrong-store");
    assert!(
        has(&out, "store.connection.identity", Outcome::Fail),
        "{out:?}"
    );
    let mut absent = f.selection.clone();
    absent.schema.push_str("_absent");
    let out = connection(&absent, BINDING);
    assert!(
        has(&out, "store.connection.schema", Outcome::Fail),
        "{out:?}"
    );
    let role = f.role();
    let restricted = f.restricted(&role);
    f.admin
        .batch_execute(&format!(
            "REVOKE USAGE ON SCHEMA {} FROM {role}",
            f.selection.schema
        ))
        .unwrap();
    let out = connection(&restricted, BINDING);
    assert!(
        has(&out, "store.connection.schema", Outcome::Fail),
        "{out:?}"
    );
    f.admin
        .batch_execute(&format!(
            "GRANT USAGE ON SCHEMA {} TO {role}; REVOKE SELECT ON {}.history_meta FROM {role}",
            f.selection.schema, f.selection.schema
        ))
        .unwrap();
    let out = connection(&restricted, BINDING);
    assert!(has(&out, "store.connection.read", Outcome::Fail), "{out:?}");
    let marker = "wrong-password-never-display-412d";
    let mut wrong = f.selection.clone();
    let password = std::env::var("LAYERFS_TEST_POSTGRES_PASSWORD").unwrap();
    wrong.url = wrong.url.replace(&password, marker);
    let out = connection(&wrong, BINDING);
    assert!(
        has(&out, "store.connection.authentication", Outcome::Fail),
        "{out:?}"
    );
    assert!(!format!("{out:?}").contains(marker));
    assert!(!format!("{out:?}").contains(&password));
    let mut tls = f.selection.clone();
    tls.url = tls.url.replace("sslmode=disable", "sslmode=require");
    let out = connection(&tls, BINDING);
    assert!(
        has(&out, "store.connection.transport", Outcome::Unknown),
        "self-signed fixture must fail normal trust validation: {out:?}"
    );
    no_corruption(&out);
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = socket.local_addr().unwrap().port();
    drop(socket);
    let unavailable = layerfs_persistence::PostgresSelection::new(
        format!("postgresql://unused@127.0.0.1:{port}/postgres?sslmode=disable"),
        "unused",
    );
    let out = connection(&unavailable, BINDING);
    assert!(has(&out, "store.connection.transport", Outcome::Unknown));
    no_corruption(&out);
    assert_eq!(before, f.fingerprint(), "Doctor source rows unchanged");
}
#[test]
fn namespace_phase_transport_loss_is_unknown_and_releases_its_snapshot() {
    let Some(mut f) = Fixture::new("disconnect") else {
        return;
    };
    pg_doctor::namespace::seed(f.writer.as_ref().unwrap());
    let before = f.fingerprint();
    let mut selection = f.selection.clone();
    let application = f.selection.schema.clone();
    selection
        .url
        .push_str(&format!("&application_name={application}"));
    let reader = PostgresHandles::open_diagnostic(
        &selection,
        BINDING,
        [71; 32],
        Some(Instant::now() + Duration::from_secs(10)),
        &mut resolver,
    )
    .unwrap();
    let mut out = Vec::new();
    let result=reader.diagnose(Options{quick:false,deadline:Some(Instant::now()+Duration::from_secs(5)),cancelled:None},&mut|o|{if o.id=="store.history.summary"{let row=f.admin.query_one("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE application_name=$1",&[&application]).unwrap();assert!(row.get::<_,bool>(0));}out.push(o)});
    assert!(result.is_err());
    assert!(has(&out, "store.namespace", Outcome::Unknown), "{out:?}");
    no_corruption(&out);
    let count:i64=f.admin.query_one("SELECT count(*) FROM pg_stat_activity WHERE application_name=$1 AND backend_xmin IS NOT NULL",&[&application]).unwrap().get(0);
    assert_eq!(count, 0, "server snapshot independently released");
    drop(reader);
    assert_eq!(before, f.fingerprint());
}
#[test]
fn live_constraint_trigger_and_index_damage_cannot_hide_behind_saved_schema_text() {
    let Some(mut f) = Fixture::new("schema") else {
        return;
    };
    let reader = f.reader();
    let (result, out) = audit(&reader, true);
    result.unwrap();
    no_corruption(&out);
    assert!(
        has(&out, "store.schema.objects.summary", Outcome::Pass),
        "{out:?}"
    );
    drop(reader);
    for mutation in [
        format!(
            "ALTER TABLE {}.pack DROP CONSTRAINT pack_length_check",
            f.selection.schema
        ),
        format!(
            "ALTER TABLE {}.pack DISABLE TRIGGER pack_immutable",
            f.selection.schema
        ),
        format!("DROP INDEX {}.object_location_prefix", f.selection.schema),
    ] {
        f.admin.batch_execute(&mutation).unwrap();
        let reader = f.reader();
        let (_, out) = audit(&reader, true);
        assert!(has(&out, "store.schema.objects", Outcome::Fail), "{out:?}");
    }
}

#[test]
fn permission_revocation_does_not_invalidate_admitted_snapshot_reads() {
    let Some(mut f) = Fixture::new("namespace_denied") else {
        return;
    };
    pg_doctor::namespace::seed(f.writer.as_ref().unwrap());
    let before = f.fingerprint();
    let control = f.reader();
    let (result, out) = audit(&control, false);
    result.unwrap();
    no_corruption(&out);
    assert!(
        has(&out, "store.namespace.summary", Outcome::Pass),
        "valid control: {out:?}"
    );
    drop(control);
    let role = f.role();
    let selected = f.restricted(&role);
    let reader = PostgresHandles::open_diagnostic(
        &selected,
        BINDING,
        [71; 32],
        Some(Instant::now() + Duration::from_secs(20)),
        &mut resolver,
    )
    .unwrap();
    let mut out = Vec::new();
    let mut revoked = false;
    let _ = reader.diagnose(
        Options {
            quick: false,
            deadline: Some(Instant::now() + Duration::from_secs(10)),
            cancelled: None,
        },
        &mut |o| {
            if o.id == "store.history.summary" && !revoked {
                f.admin
                    .batch_execute(&format!(
                        "REVOKE SELECT ON {}.object_location FROM {role}",
                        f.selection.schema
                    ))
                    .unwrap();
                revoked = true;
            }
            out.push(o)
        },
    );
    assert!(revoked);
    assert!(
        has(&out, "store.namespace.summary", Outcome::Pass),
        "already admitted snapshot reads remain valid: {out:?}"
    );
    no_corruption(&out);
    drop(reader);
    let next = PostgresHandles::open_diagnostic(
        &selected,
        BINDING,
        [71; 32],
        Some(Instant::now() + Duration::from_secs(5)),
        &mut resolver,
    )
    .unwrap();
    let (result, blocked) = audit(&next, false);
    assert!(result.is_err(), "newly denied source reads must fail");
    no_corruption(&blocked);
    assert!(!has(&blocked, "store.namespace.summary", Outcome::Pass));
    drop(next);
    assert_eq!(before, f.fingerprint());
}

#[test]
fn configured_unix_then_tcp_fallback_preserves_saved_options() {
    use std::str::FromStr;
    let Some(f) = Fixture::new("unix_fallback") else {
        return;
    };
    let parsed = postgres::Config::from_str(&f.selection.url).unwrap();
    let port = parsed.get_ports()[0];
    let password = std::env::var("LAYERFS_TEST_POSTGRES_PASSWORD").unwrap();
    let selection=layerfs_persistence::PostgresSelection::new(format!("host=/tmp/doctor-no-such-unix-socket,127.0.0.1 port={port},{port} user=postgres password={password} dbname=postgres sslmode=disable options='-c application_name=doctor_unix_fallback'"),&f.selection.schema);
    let out = connection(&selection, BINDING);
    assert!(
        has(&out, "store.connection.identity", Outcome::Pass),
        "{out:?}"
    );
    let mut random = f.selection.clone();
    random
        .url
        .push_str("&load_balance_hosts=random&host=127.0.0.1,127.0.0.1");
    let out = connection(&random, BINDING);
    assert!(
        has(&out, "store.connection.configuration", Outcome::Unknown),
        "{out:?}"
    );
    no_corruption(&out);
}

#[test]
fn stalled_actual_tls_handshake_is_bounded_and_its_socket_is_closed() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let peer = std::thread::spawn(move || {
        let until = Instant::now() + Duration::from_secs(2);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until =>
                {
                    std::thread::sleep(Duration::from_millis(2))
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
        let mut request = [0; 8];
        if socket.read_exact(&mut request).is_err() {
            return false;
        }
        assert_eq!(request, [0, 0, 0, 8, 4, 210, 22, 47]);
        if socket.write_all(b"S").is_err() {
            return false;
        }
        let mut hello = false;
        let mut bytes = [0; 4096];
        loop {
            match socket.read(&mut bytes) {
                Ok(0) => return hello,
                Ok(_) => hello = true,
                Err(_) => return false,
            }
        }
    });
    let selection = layerfs_persistence::PostgresSelection::new(
        format!("postgresql://unused@{address}/unused?sslmode=require"),
        "unused",
    );
    let start = Instant::now();
    let mut out = Vec::new();
    layerfs_persistence::diagnostic::postgresql_connection(
        &selection,
        BINDING,
        Options {
            deadline: Some(start + Duration::from_millis(250)),
            ..Options::default()
        },
        &mut resolver,
        &mut |o| out.push(o),
    )
    .unwrap();
    assert!(start.elapsed() < Duration::from_millis(1500));
    assert!(has(&out, "store.connection.transport", Outcome::Unknown));
    assert!(
        peer.join().unwrap(),
        "TLS ClientHello observed and owned socket independently closed"
    );
}
