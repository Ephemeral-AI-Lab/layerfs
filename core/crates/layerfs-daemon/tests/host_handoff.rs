//! macOS Init -> authenticated stream -> Linux daemon on a named VM volume.
#[cfg(target_os = "macos")]
#[path = "support/install_docker.rs"]
mod docker;
#[allow(dead_code)]
#[path = "support/native_install.rs"]
mod support;

#[cfg(target_os = "macos")]
#[test]
fn macos_init_handoff_to_linux_daemon_has_no_host_data_path() {
    use layerfs_bridge::native;
    use std::{fs, net::TcpStream, time::Duration};
    let fixture = support::Fixture::new("docker", Some("/store/store.sqlite"));
    let mut daemon = docker::Daemon::start();
    let stream = support::socket(
        TcpStream::connect_timeout(&daemon.address(), Duration::from_secs(3)).unwrap(),
    );
    let mut connection = native::initiate(
        stream,
        &support::CLIENT_PRIVATE,
        native::public_key(&support::SERVER_PRIVATE).unwrap(),
    )
    .unwrap();
    let ack = layerfs_sdk::install(&fixture.project, &mut connection).unwrap();
    assert_eq!(ack.manifest.host_sqlite, "3.51.0");
    assert_eq!(ack.manifest.daemon_sqlite.as_deref(), Some("3.53.2"));
    assert_eq!(ack.work.sent_bytes, fixture.project.store.bytes);
    // The VM never received a bind mount of this path. Removing the sealed
    // original before the explicit oracle command rules out host access too.
    fs::remove_file(&fixture.project.store.path).unwrap();
    fs::write(
        fixture.directory.join("installed.manifest"),
        ack.manifest.encode().unwrap(),
    )
    .unwrap();
    connection.send.send(b"RUN_COMPLETE_ROOT_ORACLE").unwrap();
    assert_eq!(
        connection.receive.receive().unwrap(),
        b"COMPLETE_ROOT_ORACLE_PASSED"
    );
    let output = daemon.finish();
    assert!(output.contains("INSTALL_ROOT_ORACLE paths=6"));
    assert!(output.contains("host_sqlite=3.51.0 daemon_sqlite=3.53.2"));
    println!(
        "HOST_HANDOFF manifest={:?} work={:?} source_removed=true sealed_removed=true",
        ack.manifest, ack.work
    );
    // Keep the final install manifest as part of the host fixture receipt. The
    // owning test removes only the VM volume after the daemon closes it.
    println!(
        "HOST_HANDOFF_RETAINED_MANIFEST {}",
        fixture.directory.join("installed.manifest").display()
    );
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "external daemon role; invoked explicitly by the macOS handoff proof"]
fn linux_daemon_install_role() {
    use layerfs_bridge::native;
    use layerfs_daemon::{install::receive_install, install_types::StoreSettings};
    use std::{io::Write, net::TcpListener, path::Path};
    assert_eq!(std::env::var("LAYERFS_INSTALL_CHILD").as_deref(), Ok("1"));
    let listener = TcpListener::bind("0.0.0.0:43210").unwrap();
    println!("LAYERFS_INSTALL_LISTEN");
    std::io::stdout().flush().unwrap();
    // This entire child is additionally bounded by an eight-second external
    // process stop. Parent panic stops its exact owned container.
    let (socket, _) = listener.accept().unwrap();
    let mut channel = native::accept(
        support::socket(socket),
        &support::SERVER_PRIVATE,
        native::public_key(&support::CLIENT_PRIVATE).unwrap(),
    )
    .unwrap();
    let installed = receive_install(
        &mut channel,
        Path::new("/store/store.sqlite"),
        StoreSettings::default(),
    )
    .unwrap();
    assert_eq!(
        channel.receive.receive().unwrap(),
        b"RUN_COMPLETE_ROOT_ORACLE"
    );
    support::oracle(&installed, Path::new("/tmp/install-overlay.sqlite"));
    channel.send.send(b"COMPLETE_ROOT_ORACLE_PASSED").unwrap();
    drop(installed);
}
