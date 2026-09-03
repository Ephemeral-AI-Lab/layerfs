#![forbid(unsafe_code)]

pub mod protocol;

#[cfg(unix)]
mod client {
    use crate::protocol::{
        self, ExecRequest, Exit, Kind, MountRequest, RemoteError, ResourceSampleFinishRequest,
        ResourceSampleRequest, ServerHello, AUTH_OK_BYTES, BOUND_AUTH_BYTES, BOUND_OK_BYTES,
        CLIENT_AUTH_BYTES,
    };
    #[cfg(target_os = "linux")]
    use crate::protocol::{CAPABILITY_PATH, SOCKET_PATH};
    #[cfg(target_os = "linux")]
    use nix::sys::socket::{getsockopt, sockopt::PeerCredentials};
    use std::fs;
    use std::io::{self, Read, Write};
    use std::net::{Shutdown, SocketAddr, TcpStream};
    #[cfg(target_os = "linux")]
    use std::os::unix::fs::MetadataExt;
    #[cfg(target_os = "linux")]
    use std::os::unix::net::UnixStream;
    #[cfg(target_os = "linux")]
    use std::path::Path;
    use std::sync::{Arc, OnceLock};
    use std::time::Duration;

    #[cfg(target_os = "linux")]
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Peer {
        pid: u32,
        uid: u32,
        gid: u32,
    }

    #[derive(Clone, Copy)]
    enum Connector {
        #[cfg(target_os = "linux")]
        Unix,
        Tcp(SocketAddr),
    }

    enum Transport {
        #[cfg(target_os = "linux")]
        Unix(UnixStream),
        Tcp(TcpStream),
    }

    pub struct Stream(Transport);

    pub struct Owner {
        stream: Stream,
        owner_id: [u8; 16],
        connector: Connector,
        #[cfg(target_os = "linux")]
        daemon: Option<Peer>,
        capability: [u8; 32],
        boot_id: [u8; 16],
    }

    static OWNER: OnceLock<Arc<Owner>> = OnceLock::new();

    pub struct Exec {
        stream: Stream,
    }

    pub struct Mount {
        stream: Option<Stream>,
        mountinfo: Vec<u8>,
    }

    pub struct ResourceSampleClock {
        stream: Stream,
    }

    impl ResourceSampleClock {
        pub fn probe(&mut self) -> io::Result<(u64, u64)> {
            protocol::write_clock_probe(&mut self.stream)?;
            let response = protocol::read_frame(&mut self.stream)?;
            if response.kind == Kind::Error {
                return Err(remote_error(&response.payload));
            }
            if response.kind != Kind::ResourceSampleStarted {
                return Err(protocol::invalid("daemon resource clock response"));
            }
            Ok((
                u64::from_be_bytes(
                    response.payload[..8]
                        .try_into()
                        .expect("clock receive width"),
                ),
                u64::from_be_bytes(response.payload[8..].try_into().expect("clock send width")),
            ))
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct MountExit;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum Event {
        Stdout(Vec<u8>),
        Stderr(Vec<u8>),
        Exit(Exit),
        Error(RemoteError),
    }

    impl Owner {
        fn connect() -> io::Result<Self> {
            let (connector, capability) = configuration()?;
            Self::connect_with(connector, capability)
        }

        fn connect_with(connector: Connector, capability: [u8; 32]) -> io::Result<Self> {
            let mut stream = connector.connect()?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            stream.set_write_timeout(Some(Duration::from_secs(2)))?;
            let hello = ServerHello::decode(&mut stream)?;
            let client_nonce = random()?;
            #[cfg(target_os = "linux")]
            let daemon = match connector {
                Connector::Unix => Some(peer(&stream)?),
                Connector::Tcp(_) => None,
            };
            let proof = match connector {
                #[cfg(target_os = "linux")]
                Connector::Unix => {
                    let daemon = daemon.expect("Unix daemon peer");
                    if daemon.uid != nix::unistd::geteuid().as_raw()
                        || daemon.gid != nix::unistd::getegid().as_raw()
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "daemon peer identity",
                        ));
                    }
                    protocol::client_proof(
                        &capability,
                        hello,
                        &client_nonce,
                        std::process::id(),
                        nix::unistd::geteuid().as_raw(),
                        nix::unistd::getegid().as_raw(),
                    )
                }
                Connector::Tcp(_) => protocol::tcp_client_proof(&capability, hello, &client_nonce),
            };
            let mut auth = [0; CLIENT_AUTH_BYTES];
            auth[..32].copy_from_slice(&client_nonce);
            auth[32..].copy_from_slice(&proof);
            stream.write_all(&auth)?;
            let mut ok = [0; AUTH_OK_BYTES];
            stream.read_exact(&mut ok)?;
            let owner_id = ok[..16].try_into().expect("owner id width");
            let expected = match connector {
                #[cfg(target_os = "linux")]
                Connector::Unix => protocol::server_proof(
                    &capability,
                    hello,
                    &client_nonce,
                    std::process::id(),
                    nix::unistd::geteuid().as_raw(),
                    nix::unistd::getegid().as_raw(),
                    &owner_id,
                ),
                Connector::Tcp(_) => {
                    protocol::tcp_server_proof(&capability, hello, &client_nonce, &owner_id)
                }
            };
            if !constant_time_equal(&ok[16..], &expected) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "daemon server proof",
                ));
            }
            stream.set_read_timeout(None)?;
            stream.set_write_timeout(None)?;
            Ok(Self {
                stream,
                owner_id,
                connector,
                #[cfg(target_os = "linux")]
                daemon,
                capability,
                boot_id: hello.boot_id,
            })
        }

        pub fn start(
            &self,
            workspace_id: [u8; 16],
            execution_id: [u8; 16],
            cwd: Vec<u8>,
            argv: Vec<Vec<u8>>,
        ) -> io::Result<Exec> {
            let payload = ExecRequest {
                owner_id: self.owner_id,
                workspace_id,
                execution_id,
                cwd,
                argv,
            }
            .encode()?;
            let mut stream = self.connect_bound(Kind::Exec, &payload, Duration::from_secs(2))?;
            let started = protocol::read_frame(&mut stream)?;
            if started.kind == Kind::Error {
                return Err(remote_error(&started.payload));
            }
            if started.kind != Kind::Started {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "daemon did not start execution",
                ));
            }
            stream.set_read_timeout(None)?;
            stream.set_write_timeout(None)?;
            Ok(Exec { stream })
        }

        pub fn mount(
            &self,
            workspace_id: [u8; 16],
            root: Vec<u8>,
            endpoint: Vec<u8>,
            capability: [u8; 32],
        ) -> io::Result<Mount> {
            let payload = MountRequest {
                owner_id: self.owner_id,
                workspace_id,
                root,
                endpoint,
                capability,
            }
            .encode()?;
            let mut stream = self.connect_bound(Kind::Mount, &payload, Duration::from_secs(10))?;
            let ready = protocol::read_frame(&mut stream)?;
            if ready.kind == Kind::Error {
                return Err(remote_error(&ready.payload));
            }
            if ready.kind != Kind::WorkspaceReady {
                return Err(protocol::invalid("daemon did not mount Workspace"));
            }
            stream.set_read_timeout(None)?;
            stream.set_write_timeout(None)?;
            Ok(Mount {
                stream: Some(stream),
                mountinfo: ready.payload,
            })
        }

        pub fn start_resource_sample(
            &self,
            workspace_id: [u8; 16],
        ) -> io::Result<ResourceSampleClock> {
            let request = ResourceSampleRequest {
                owner_id: self.owner_id,
                workspace_id,
            }
            .encode();
            let mut stream =
                self.connect_bound(Kind::ResourceSampleStart, &request, Duration::from_secs(2))?;
            let response = protocol::read_frame(&mut stream)?;
            if response.kind == Kind::Error {
                return Err(remote_error(&response.payload));
            }
            if response.kind != Kind::ResourceSampleStarted {
                return Err(protocol::invalid("daemon did not start resource sample"));
            }
            Ok(ResourceSampleClock { stream })
        }

        pub fn finish_resource_sample(
            &self,
            workspace_id: [u8; 16],
            t0_unix_ns: u64,
            t3_unix_ns: u64,
            uncertainty_ns: u64,
        ) -> io::Result<protocol::CgroupResourceSample> {
            let request = ResourceSampleFinishRequest {
                owner_id: self.owner_id,
                workspace_id,
                t0_unix_ns,
                t3_unix_ns,
                uncertainty_ns,
            }
            .encode();
            let mut stream =
                self.connect_bound(Kind::ResourceSampleFinish, &request, Duration::from_secs(2))?;
            let response = protocol::read_frame(&mut stream)?;
            if response.kind == Kind::Error {
                return Err(remote_error(&response.payload));
            }
            if response.kind != Kind::ResourceSample {
                return Err(protocol::invalid("daemon did not finish resource sample"));
            }
            protocol::CgroupResourceSample::decode(&response.payload)
        }

        fn connect_bound(
            &self,
            kind: Kind,
            payload: &[u8],
            timeout: Duration,
        ) -> io::Result<Stream> {
            let mut stream = self.connector.connect()?;
            stream.set_read_timeout(Some(timeout))?;
            stream.set_write_timeout(Some(timeout))?;
            match self.connector {
                #[cfg(target_os = "linux")]
                Connector::Unix => {
                    let expected = self.daemon.expect("Unix daemon peer");
                    if peer(&stream)? != expected {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "daemon bound-stream peer",
                        ));
                    }
                    protocol::write_frame(&mut stream, kind, payload)?;
                }
                Connector::Tcp(_) => {
                    let hello = ServerHello::decode(&mut stream)?;
                    if hello.boot_id != self.boot_id {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "daemon boot identity",
                        ));
                    }
                    let client_nonce = random()?;
                    let proof = protocol::bound_client_proof(
                        &self.capability,
                        hello,
                        &client_nonce,
                        &self.owner_id,
                        kind,
                        payload,
                    );
                    let mut auth = [0; BOUND_AUTH_BYTES];
                    auth[..32].copy_from_slice(&client_nonce);
                    auth[32..].copy_from_slice(&proof);
                    stream.write_all(&auth)?;
                    protocol::write_frame(&mut stream, kind, payload)?;
                    let mut ok = [0; BOUND_OK_BYTES];
                    stream.read_exact(&mut ok)?;
                    let expected = protocol::bound_server_proof(
                        &self.capability,
                        hello,
                        &client_nonce,
                        &self.owner_id,
                        kind,
                        payload,
                    );
                    if !constant_time_equal(&ok, &expected) {
                        return Err(io::Error::new(
                            io::ErrorKind::PermissionDenied,
                            "daemon bound-stream proof",
                        ));
                    }
                }
            }
            Ok(stream)
        }
    }

    pub fn prepare_owner() -> io::Result<Arc<Owner>> {
        if let Some(owner) = OWNER.get() {
            return Ok(owner.clone());
        }
        let owner = Arc::new(Owner::connect()?);
        OWNER
            .set(owner.clone())
            .map_err(|_| io::Error::other("daemon owner initialization"))?;
        Ok(owner)
    }

    pub fn connect_tcp(endpoint: SocketAddr, capability: [u8; 32]) -> io::Result<Arc<Owner>> {
        Owner::connect_with(Connector::Tcp(endpoint), capability).map(Arc::new)
    }

    impl Exec {
        pub fn try_clone(&self) -> io::Result<Stream> {
            self.stream.try_clone()
        }

        pub fn stop(&mut self) -> io::Result<()> {
            protocol::write_frame(&mut self.stream, Kind::Stop, &[])
        }

        pub fn disconnect(&self) -> io::Result<()> {
            self.stream.shutdown(std::net::Shutdown::Both)
        }

        pub fn read(stream: &mut Stream) -> io::Result<Event> {
            let frame = protocol::read_frame(stream)?;
            match frame.kind {
                Kind::Stdout => Ok(Event::Stdout(frame.payload)),
                Kind::Stderr => Ok(Event::Stderr(frame.payload)),
                Kind::Exit => Ok(Event::Exit(Exit::decode(&frame.payload)?)),
                Kind::Error => Ok(Event::Error(RemoteError::try_from(frame.payload[0])?)),
                _ => Err(protocol::invalid("unexpected daemon frame")),
            }
        }
    }

    impl Mount {
        pub fn mountinfo(&self) -> &[u8] {
            &self.mountinfo
        }

        pub fn close(&mut self) -> io::Result<MountExit> {
            let Some(mut stream) = self.stream.take() else {
                return Ok(MountExit);
            };
            let result = (|| {
                stream.set_read_timeout(Some(Duration::from_secs(10)))?;
                stream.set_write_timeout(Some(Duration::from_secs(10)))?;
                protocol::write_frame(&mut stream, Kind::Close, &[])?;
                let closed = protocol::read_frame(&mut stream)?;
                if closed.kind == Kind::Error {
                    return Err(remote_error(&closed.payload));
                }
                if closed.kind != Kind::WorkspaceClosed {
                    return Err(protocol::invalid("daemon did not close Workspace"));
                }
                Ok(MountExit)
            })();
            let _ = stream.shutdown(std::net::Shutdown::Both);
            result
        }

        pub fn disconnect(&self) -> io::Result<()> {
            self.stream
                .as_ref()
                .map_or(Ok(()), |stream| stream.shutdown(std::net::Shutdown::Both))
        }
    }

    impl Drop for Owner {
        fn drop(&mut self) {
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
        }
    }

    impl Drop for Mount {
        fn drop(&mut self) {
            if let Some(stream) = self.stream.take() {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            }
        }
    }

    impl Connector {
        fn connect(self) -> io::Result<Stream> {
            match self {
                #[cfg(target_os = "linux")]
                Self::Unix => {
                    UnixStream::connect(SOCKET_PATH).map(|stream| Stream(Transport::Unix(stream)))
                }
                Self::Tcp(endpoint) => {
                    let stream = TcpStream::connect(endpoint)?;
                    stream.set_nodelay(true)?;
                    Ok(Stream(Transport::Tcp(stream)))
                }
            }
        }
    }

    impl Stream {
        pub fn try_clone(&self) -> io::Result<Self> {
            match &self.0 {
                #[cfg(target_os = "linux")]
                Transport::Unix(stream) => stream
                    .try_clone()
                    .map(|stream| Self(Transport::Unix(stream))),
                Transport::Tcp(stream) => stream
                    .try_clone()
                    .map(|stream| Self(Transport::Tcp(stream))),
            }
        }

        fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
            match &self.0 {
                #[cfg(target_os = "linux")]
                Transport::Unix(stream) => stream.set_read_timeout(timeout),
                Transport::Tcp(stream) => stream.set_read_timeout(timeout),
            }
        }

        fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
            match &self.0 {
                #[cfg(target_os = "linux")]
                Transport::Unix(stream) => stream.set_write_timeout(timeout),
                Transport::Tcp(stream) => stream.set_write_timeout(timeout),
            }
        }

        fn shutdown(&self, how: Shutdown) -> io::Result<()> {
            match &self.0 {
                #[cfg(target_os = "linux")]
                Transport::Unix(stream) => stream.shutdown(how),
                Transport::Tcp(stream) => stream.shutdown(how),
            }
        }
    }

    impl Read for Stream {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            match &mut self.0 {
                #[cfg(target_os = "linux")]
                Transport::Unix(stream) => stream.read(bytes),
                Transport::Tcp(stream) => stream.read(bytes),
            }
        }
    }

    impl Write for Stream {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            match &mut self.0 {
                #[cfg(target_os = "linux")]
                Transport::Unix(stream) => stream.write(bytes),
                Transport::Tcp(stream) => stream.write(bytes),
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            match &mut self.0 {
                #[cfg(target_os = "linux")]
                Transport::Unix(stream) => stream.flush(),
                Transport::Tcp(stream) => stream.flush(),
            }
        }
    }

    fn remote_error(payload: &[u8]) -> io::Error {
        payload
            .first()
            .copied()
            .ok_or_else(|| protocol::invalid("missing daemon error"))
            .and_then(RemoteError::try_from)
            .map_or_else(
                |error| error,
                |error| io::Error::other(format!("{error:?}")),
            )
    }

    #[cfg(target_os = "linux")]
    fn peer(stream: &Stream) -> io::Result<Peer> {
        let Transport::Unix(stream) = &stream.0 else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "daemon Unix peer",
            ));
        };
        let credentials = getsockopt(stream, PeerCredentials).map_err(io::Error::other)?;
        Ok(Peer {
            pid: credentials
                .pid()
                .try_into()
                .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "daemon peer PID"))?,
            uid: credentials.uid(),
            gid: credentials.gid(),
        })
    }

    fn configuration() -> io::Result<(Connector, [u8; 32])> {
        match std::env::var("LAYERFS_DAEMON_TCP_ENDPOINT") {
            Ok(endpoint) => {
                let endpoint = endpoint
                    .parse()
                    .map_err(|_| protocol::invalid("daemon TCP endpoint"))?;
                let capability = std::env::var("LAYERFS_DAEMON_CAPABILITY")
                    .map_err(|_| protocol::invalid("daemon TCP capability"))?;
                Ok((Connector::Tcp(endpoint), decode_capability(&capability)?))
            }
            Err(std::env::VarError::NotPresent) => {
                #[cfg(target_os = "linux")]
                {
                    let capability = read_capability(Path::new(CAPABILITY_PATH))?;
                    Ok((Connector::Unix, capability))
                }
                #[cfg(not(target_os = "linux"))]
                {
                    Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "daemon TCP endpoint is required",
                    ))
                }
            }
            Err(_) => Err(protocol::invalid("daemon TCP endpoint")),
        }
    }

    fn decode_capability(value: &str) -> io::Result<[u8; 32]> {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(protocol::invalid("daemon TCP capability"));
        }
        let mut output = [0; 32];
        for (index, byte) in output.iter_mut().enumerate() {
            *byte = (hex_digit(value.as_bytes()[index * 2])? << 4)
                | hex_digit(value.as_bytes()[index * 2 + 1])?;
        }
        Ok(output)
    }

    fn hex_digit(value: u8) -> io::Result<u8> {
        match value {
            b'0'..=b'9' => Ok(value - b'0'),
            b'a'..=b'f' => Ok(value - b'a' + 10),
            b'A'..=b'F' => Ok(value - b'A' + 10),
            _ => Err(protocol::invalid("daemon TCP capability")),
        }
    }

    #[cfg(target_os = "linux")]
    fn read_capability(path: &Path) -> io::Result<[u8; 32]> {
        let metadata = fs::metadata(path)?;
        if !metadata.is_file() || metadata.mode() & 0o077 != 0 || metadata.len() != 32 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "daemon capability protection",
            ));
        }
        fs::read(path)?
            .try_into()
            .map_err(|_| protocol::invalid("daemon capability length"))
    }

    fn random<const N: usize>() -> io::Result<[u8; N]> {
        let mut bytes = [0; N];
        fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    fn constant_time_equal(left: &[u8], right: &[u8; 32]) -> bool {
        left.len() == 32
            && left
                .iter()
                .zip(right)
                .fold(0_u8, |different, (left, right)| different | (left ^ right))
                == 0
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[cfg(target_os = "linux")]
        use std::os::unix::fs::PermissionsExt;

        #[test]
        fn prepared_clock_stream_reuses_transport_and_rejects_non_clock_reply() {
            let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
            let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (mut remote, _) = listener.accept().unwrap();
            let server = std::thread::spawn(move || {
                for sent in [10, 20] {
                    assert_eq!(
                        protocol::read_frame(&mut remote).unwrap().kind,
                        Kind::ResourceSampleClock
                    );
                    protocol::write_clock_response(&mut remote, sent - 1, sent).unwrap();
                }
                assert_eq!(
                    protocol::read_frame(&mut remote).unwrap().kind,
                    Kind::ResourceSampleClock
                );
                protocol::write_frame(&mut remote, Kind::Started, &[]).unwrap();
            });
            let mut clock = ResourceSampleClock {
                stream: Stream(Transport::Tcp(stream)),
            };
            assert_eq!(clock.probe().unwrap(), (9, 10));
            assert_eq!(clock.probe().unwrap(), (19, 20));
            assert!(clock.probe().is_err());
            server.join().unwrap();
        }

        #[cfg(target_os = "linux")]
        #[test]
        fn capability_requires_exact_private_file() {
            let root = std::env::temp_dir().join(format!(
                "layerfs-capability-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::write(&root, [7; 32]).unwrap();
            fs::set_permissions(&root, fs::Permissions::from_mode(0o600)).unwrap();
            assert_eq!(read_capability(&root).unwrap(), [7; 32]);
            fs::set_permissions(&root, fs::Permissions::from_mode(0o644)).unwrap();
            assert!(read_capability(&root).is_err());
            fs::remove_file(root).unwrap();
        }

        #[test]
        fn tcp_capability_is_exact_hex() {
            assert_eq!(decode_capability(&"07".repeat(32)).unwrap(), [7; 32]);
            assert!(decode_capability("07").is_err());
            assert!(decode_capability(&"gg".repeat(32)).is_err());
        }

        #[test]
        fn tcp_owner_and_bound_stream_authenticate() {
            let capability = [9; 32];
            let owner_id = [8; 16];
            let boot_id = [7; 16];
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let endpoint = listener.local_addr().unwrap();
            let server = std::thread::spawn(move || {
                let (mut owner, _) = listener.accept().unwrap();
                owner.set_nodelay(true).unwrap();
                let hello = ServerHello {
                    boot_id,
                    nonce: [6; 32],
                };
                owner.write_all(&hello.encode()).unwrap();
                let mut auth = [0; CLIENT_AUTH_BYTES];
                owner.read_exact(&mut auth).unwrap();
                let nonce: [u8; 32] = auth[..32].try_into().unwrap();
                assert_eq!(
                    &auth[32..],
                    protocol::tcp_client_proof(&capability, hello, &nonce)
                );
                let mut ok = [0; AUTH_OK_BYTES];
                ok[..16].copy_from_slice(&owner_id);
                ok[16..].copy_from_slice(&protocol::tcp_server_proof(
                    &capability,
                    hello,
                    &nonce,
                    &owner_id,
                ));
                owner.write_all(&ok).unwrap();

                let (mut bound, _) = listener.accept().unwrap();
                bound.set_nodelay(true).unwrap();
                let hello = ServerHello {
                    boot_id,
                    nonce: [5; 32],
                };
                bound.write_all(&hello.encode()).unwrap();
                let mut auth = [0; BOUND_AUTH_BYTES];
                bound.read_exact(&mut auth).unwrap();
                let nonce: [u8; 32] = auth[..32].try_into().unwrap();
                let frame = protocol::read_frame(&mut bound).unwrap();
                assert_eq!(frame.kind, Kind::Exec);
                assert_eq!(
                    &auth[32..],
                    protocol::bound_client_proof(
                        &capability,
                        hello,
                        &nonce,
                        &owner_id,
                        frame.kind,
                        &frame.payload,
                    )
                );
                bound
                    .write_all(&protocol::bound_server_proof(
                        &capability,
                        hello,
                        &nonce,
                        &owner_id,
                        frame.kind,
                        &frame.payload,
                    ))
                    .unwrap();
                protocol::write_frame(&mut bound, Kind::Started, &[]).unwrap();
                let mut byte = [0];
                assert_eq!(owner.read(&mut byte).unwrap(), 0);
            });

            let owner = Owner::connect_with(Connector::Tcp(endpoint), capability).unwrap();
            let exec = owner
                .start(
                    [1; 16],
                    [2; 16],
                    b"/workspace/test".to_vec(),
                    vec![b"/bin/true".to_vec()],
                )
                .unwrap();
            match &exec.stream.0 {
                Transport::Tcp(stream) => {
                    assert_eq!(stream.read_timeout().unwrap(), None);
                    assert_eq!(stream.write_timeout().unwrap(), None);
                }
                #[cfg(target_os = "linux")]
                Transport::Unix(_) => panic!("expected TCP execution stream"),
            }
            drop(exec);
            drop(owner);
            server.join().unwrap();
        }
    }
}

#[cfg(unix)]
pub use client::{
    connect_tcp, prepare_owner, Event, Exec, Mount, MountExit, Owner, ResourceSampleClock, Stream,
};
