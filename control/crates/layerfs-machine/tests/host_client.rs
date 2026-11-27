#![cfg(any(target_os = "macos", target_os = "linux"))]
use layerfs_home::{helpers::random, home};
use layerfs_machine::worker::{
    host_check_peer, read_host_frame, write_host_frame, HostHello, HostPaths, ATTACHED,
    HOST_MAX_REQUEST, MAX_HELLO,
};
use layerfs_protocol::sdk::{hex, SdkValue};
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const DEADLINE: Duration = Duration::from_secs(3);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let folder = PathBuf::from("/tmp").join(format!("lfw-{}", hex(&random().unwrap()[..8])));
        fs::DirBuilder::new().mode(0o700).create(&folder).unwrap();
        home::open(&folder).unwrap();
        home::private_dir(&home::paths::machine_dir(
            &folder,
            layerfs_home::names::HOST_MACHINE,
        ))
        .unwrap();
        Self(folder)
    }

    fn paths(&self) -> HostPaths {
        HostPaths::new(&self.0).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn accept(listener: &UnixListener) -> UnixStream {
    listener.set_nonblocking(true).unwrap();
    let end = Instant::now() + DEADLINE;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                return stream;
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock && Instant::now() < end => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("accept: {error}"),
        }
    }
}

#[test]
fn aliases_share_lifetime_lock_and_existing_only_does_not_start() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let alias = fixture.0.join("alias");
    std::os::unix::fs::symlink(&fixture.0, &alias).unwrap();
    let other_paths = HostPaths::new(&alias).unwrap();
    assert_eq!(paths.home, other_paths.home);
    assert_eq!(paths.control, other_paths.control);
    assert!(paths
        .call_existing(true, "ping", SdkValue::Null)
        .unwrap()
        .is_none());
    assert!(!home::paths::host_worker_startup_lock(&paths.home).exists());

    let guard = paths.try_hold().unwrap().unwrap();
    assert!(other_paths.try_hold().unwrap().is_none());
    let listener = paths.bind(true, &guard).unwrap();
    let mode = fs::symlink_metadata(&paths.control)
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    drop(listener);
    paths.remove_endpoints(&guard).unwrap();
    drop(guard);
    assert!(other_paths.try_hold().unwrap().is_some());
}

#[test]
fn mismatched_hello_refuses_before_request_bytes() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let guard = paths.try_hold().unwrap().unwrap();
    let listener = paths.bind(true, &guard).unwrap();
    let mut hello = HostHello::own(&paths).unwrap();
    hello.build = "other executable".to_owned();
    let server = std::thread::spawn(move || {
        let mut stream = accept(&listener);
        host_check_peer(&stream).unwrap();
        let end = Instant::now() + DEADLINE;
        write_host_frame(&stream, &hello.to_frame(), MAX_HELLO, end).unwrap();
        stream.set_read_timeout(Some(DEADLINE)).unwrap();
        assert_eq!(stream.read(&mut [0; 1]).unwrap(), 0);
    });
    let error = paths
        .call_existing(true, "ping", SdkValue::Null)
        .unwrap_err();
    assert_eq!(error.code, "Unavailable");
    assert!(error.detail.get("worker").is_some());
    server.join().unwrap();
    paths.remove_endpoints(&guard).unwrap();
}

#[test]
fn reply_loss_after_send_carries_request_id() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let guard = paths.try_hold().unwrap().unwrap();
    let listener = paths.bind(true, &guard).unwrap();
    let hello = HostHello::own(&paths).unwrap();
    let server = std::thread::spawn(move || {
        let stream = accept(&listener);
        host_check_peer(&stream).unwrap();
        let end = Instant::now() + DEADLINE;
        write_host_frame(&stream, &hello.to_frame(), MAX_HELLO, end).unwrap();
        let request = read_host_frame(&stream, HOST_MAX_REQUEST, Some(end))
            .unwrap()
            .unwrap();
        assert_eq!(
            request.get_text("generation"),
            Some(hello.generation.as_str())
        );
        assert_eq!(request.get_text("op"), Some("ping"));
        request.get_text("request_id").unwrap().to_owned()
    });
    let error = paths
        .call_existing(true, "ping", SdkValue::Null)
        .unwrap_err();
    assert_eq!(error.code, "Unknown");
    let request_id = server.join().unwrap();
    assert_eq!(
        error.detail.get_text("request_id"),
        Some(request_id.as_str())
    );
    paths.remove_endpoints(&guard).unwrap();
}

#[test]
fn host_rejects_overlength_and_attachment_headers_without_reading_a_body() {
    for (header, code) in [
        (HOST_MAX_REQUEST as u32 + 1, "TooLarge"),
        (ATTACHED, "InvalidInput"),
    ] {
        let (mut sender, receiver) = UnixStream::pair().unwrap();
        sender.write_all(&header.to_be_bytes()).unwrap();
        let refusal = read_host_frame(&receiver, HOST_MAX_REQUEST, Some(Instant::now() + DEADLINE))
            .unwrap_err();
        assert_eq!(refusal.code, code);
    }
}

#[test]
fn cleanup_refuses_a_replaced_native_machine_directory() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let native = home::paths::machine_dir(&paths.home, layerfs_home::names::HOST_MACHINE);
    let old = paths.try_hold().unwrap().unwrap();
    let old_socket = paths.bind(true, &old).unwrap();

    fs::rename(&native, native.with_file_name("old-native")).unwrap();
    home::private_dir(&native).unwrap();
    let replacement = paths.try_hold().unwrap().unwrap();
    let new_socket = paths.bind(true, &replacement).unwrap();

    drop(old_socket);
    assert!(paths.remove_endpoints(&old).is_err());
    assert!(paths.control.exists());
    drop(new_socket);
    paths.remove_endpoints(&replacement).unwrap();
}

#[test]
fn buffered_frame_survives_peer_close() {
    let reply = SdkValue::map([("ok", SdkValue::text("completed"))]);
    for end in [Some(Instant::now() + DEADLINE), None] {
        let (sender, receiver) = UnixStream::pair().unwrap();
        write_host_frame(&sender, &reply, MAX_HELLO, Instant::now() + DEADLINE).unwrap();
        drop(sender);
        let received = read_host_frame(&receiver, MAX_HELLO, end).unwrap();
        assert_eq!(received, Some(reply.clone()));
    }
}

#[test]
fn management_rejects_oversize_before_dialing() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let refusal = paths
        .call_existing(true, "store.close", SdkValue::text("x".repeat(MAX_HELLO)))
        .unwrap_err();
    assert_eq!(refusal.code, "TooLarge");
    assert!(!paths.control.exists());
}

#[test]
fn incomplete_header_obeys_the_transfer_deadline() {
    let (mut sender, receiver) = UnixStream::pair().unwrap();
    sender.write_all(&[0]).unwrap();
    let end = Instant::now() + Duration::from_millis(30);
    let refusal = read_host_frame(&receiver, MAX_HELLO, Some(end)).unwrap_err();
    assert_eq!(refusal.code, "Unavailable");
    assert!(Instant::now() >= end);
}

/// An ordinary caller that takes the startup lock but starts nothing (here
/// the socket is not private yet while a worker holds the lifetime lock;
/// a full listen queue takes the same branch) reached a running worker: it
/// waits for its hello as any queued caller does, 30 s from its connect,
/// and its refusal names the load, with nothing sent. Runs about 31 s.
#[test]
fn a_caller_that_started_nothing_waits_as_a_queued_caller() {
    let fixture = Fixture::new();
    let paths = fixture.paths();
    let guard = paths.try_hold().unwrap().unwrap();
    let listener = paths.bind(false, &guard).unwrap();
    // between bind and chmod, as a starting worker is
    fs::set_permissions(&paths.socket, fs::Permissions::from_mode(0o755)).unwrap();
    let socket = paths.socket.clone();
    let private = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
    });
    let began = Instant::now();
    let refusal = paths.call("ping", SdkValue::map([])).unwrap_err();
    let waited = began.elapsed();
    private.join().unwrap();
    assert_eq!(refusal.code, "Unavailable", "{refusal:?}");
    assert!(
        refusal.message.contains("no call thread was free"),
        "{refusal:?}"
    );
    assert!(waited >= Duration::from_secs(30), "{waited:?}");
    // nothing was sent: the caller closed a connection that holds no bytes
    let mut stream = accept(&listener);
    match stream.read(&mut [0; 1]) {
        Ok(0) => {}
        Err(error) if error.kind() == io::ErrorKind::ConnectionReset => {}
        other => panic!("the caller sent bytes: {other:?}"),
    }
    paths.remove_endpoints(&guard).unwrap();
}
