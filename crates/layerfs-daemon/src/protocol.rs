use std::io::{self, Read, Write};

pub const SOCKET_PATH: &str = "/run/layerfs/daemon.sock";
pub const CAPABILITY_PATH: &str = "/run/layerfs/capability";
pub const WORKSPACE_ROOT: &str = "/workspace";
pub const MAGIC: [u8; 8] = *b"LFSDAEM3";
pub const VERSION: u16 = 1;
pub const MAX_CONTROL: usize = 1024 * 1024;
pub const MAX_OUTPUT: usize = 64 * 1024;
pub const MAX_ARG: usize = 128 * 1024;
pub const MAX_ARGS: usize = 4096;

pub const SERVER_HELLO_BYTES: usize = 8 + 2 + 16 + 32;
pub const CLIENT_AUTH_BYTES: usize = 32 + 32;
pub const AUTH_OK_BYTES: usize = 16 + 32;
pub const BOUND_AUTH_BYTES: usize = 32 + 32;
pub const BOUND_OK_BYTES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Kind {
    Exec = 1,
    Stop = 2,
    Started = 3,
    Stdout = 4,
    Stderr = 5,
    Exit = 6,
    Error = 7,
    Mount = 8,
    WorkspaceReady = 9,
    Close = 10,
    WorkspaceClosed = 11,
}

impl TryFrom<u8> for Kind {
    type Error = io::Error;

    fn try_from(value: u8) -> io::Result<Self> {
        match value {
            1 => Ok(Self::Exec),
            2 => Ok(Self::Stop),
            3 => Ok(Self::Started),
            4 => Ok(Self::Stdout),
            5 => Ok(Self::Stderr),
            6 => Ok(Self::Exit),
            7 => Ok(Self::Error),
            8 => Ok(Self::Mount),
            9 => Ok(Self::WorkspaceReady),
            10 => Ok(Self::Close),
            11 => Ok(Self::WorkspaceClosed),
            _ => Err(invalid("unknown daemon frame kind")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RemoteError {
    InvalidRequest = 1,
    LimitExceeded = 2,
    InfrastructureLost = 3,
    OutputFailed = 4,
    Unauthorized = 5,
}

impl TryFrom<u8> for RemoteError {
    type Error = io::Error;

    fn try_from(value: u8) -> io::Result<Self> {
        match value {
            1 => Ok(Self::InvalidRequest),
            2 => Ok(Self::LimitExceeded),
            3 => Ok(Self::InfrastructureLost),
            4 => Ok(Self::OutputFailed),
            5 => Ok(Self::Unauthorized),
            _ => Err(invalid("unknown daemon error")),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DaemonTiming {
    pub accept_bind_ns: u64,
    pub decode_ns: u64,
    pub spawn_ns: u64,
    pub runtime_ns: u64,
    pub drain_ns: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Exit {
    pub code: Option<i32>,
    pub signal: Option<i32>,
    pub stopped: bool,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub timing: DaemonTiming,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecRequest {
    pub owner_id: [u8; 16],
    pub workspace_id: [u8; 16],
    pub execution_id: [u8; 16],
    pub cwd: Vec<u8>,
    pub argv: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MountRequest {
    pub owner_id: [u8; 16],
    pub workspace_id: [u8; 16],
    pub root: Vec<u8>,
    pub endpoint: Vec<u8>,
    pub capability: [u8; 32],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub kind: Kind,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy)]
pub struct ServerHello {
    pub boot_id: [u8; 16],
    pub nonce: [u8; 32],
}

impl ServerHello {
    pub fn encode(self) -> [u8; SERVER_HELLO_BYTES] {
        let mut bytes = [0; SERVER_HELLO_BYTES];
        bytes[..8].copy_from_slice(&MAGIC);
        bytes[8..10].copy_from_slice(&VERSION.to_be_bytes());
        bytes[10..26].copy_from_slice(&self.boot_id);
        bytes[26..].copy_from_slice(&self.nonce);
        bytes
    }

    pub fn decode(mut reader: impl Read) -> io::Result<Self> {
        let mut bytes = [0; SERVER_HELLO_BYTES];
        reader.read_exact(&mut bytes)?;
        if bytes[..8] != MAGIC || u16::from_be_bytes([bytes[8], bytes[9]]) != VERSION {
            return Err(invalid("daemon protocol version"));
        }
        Ok(Self {
            boot_id: bytes[10..26].try_into().expect("boot id width"),
            nonce: bytes[26..].try_into().expect("nonce width"),
        })
    }
}

pub fn client_proof(
    capability: &[u8; 32],
    hello: ServerHello,
    client_nonce: &[u8; 32],
    pid: u32,
    uid: u32,
    gid: u32,
) -> [u8; 32] {
    proof(
        capability,
        b"layerfs-daemon-client-v1",
        hello,
        client_nonce,
        (pid, uid, gid),
        None,
    )
}

pub fn server_proof(
    capability: &[u8; 32],
    hello: ServerHello,
    client_nonce: &[u8; 32],
    pid: u32,
    uid: u32,
    gid: u32,
    owner_id: &[u8; 16],
) -> [u8; 32] {
    proof(
        capability,
        b"layerfs-daemon-server-v1",
        hello,
        client_nonce,
        (pid, uid, gid),
        Some(owner_id),
    )
}

pub fn tcp_client_proof(
    capability: &[u8; 32],
    hello: ServerHello,
    client_nonce: &[u8; 32],
) -> [u8; 32] {
    transcript_proof(
        capability,
        b"layerfs-daemon-tcp-client-v1",
        hello,
        client_nonce,
        &[],
    )
}

pub fn tcp_server_proof(
    capability: &[u8; 32],
    hello: ServerHello,
    client_nonce: &[u8; 32],
    owner_id: &[u8; 16],
) -> [u8; 32] {
    transcript_proof(
        capability,
        b"layerfs-daemon-tcp-server-v1",
        hello,
        client_nonce,
        owner_id,
    )
}

pub fn bound_client_proof(
    capability: &[u8; 32],
    hello: ServerHello,
    client_nonce: &[u8; 32],
    owner_id: &[u8; 16],
    kind: Kind,
    payload: &[u8],
) -> [u8; 32] {
    bound_proof(
        capability,
        b"layerfs-daemon-bound-client-v1",
        hello,
        client_nonce,
        owner_id,
        kind,
        payload,
    )
}

pub fn bound_server_proof(
    capability: &[u8; 32],
    hello: ServerHello,
    client_nonce: &[u8; 32],
    owner_id: &[u8; 16],
    kind: Kind,
    payload: &[u8],
) -> [u8; 32] {
    bound_proof(
        capability,
        b"layerfs-daemon-bound-server-v1",
        hello,
        client_nonce,
        owner_id,
        kind,
        payload,
    )
}

fn bound_proof(
    capability: &[u8; 32],
    domain: &[u8],
    hello: ServerHello,
    client_nonce: &[u8; 32],
    owner_id: &[u8; 16],
    kind: Kind,
    payload: &[u8],
) -> [u8; 32] {
    let mut hasher = transcript_hasher(capability, domain, hello, client_nonce);
    hasher.update(owner_id);
    hasher.update(&[kind as u8]);
    hasher.update(payload);
    *hasher.finalize().as_bytes()
}

fn transcript_proof(
    capability: &[u8; 32],
    domain: &[u8],
    hello: ServerHello,
    client_nonce: &[u8; 32],
    transcript: &[u8],
) -> [u8; 32] {
    let mut hasher = transcript_hasher(capability, domain, hello, client_nonce);
    hasher.update(transcript);
    *hasher.finalize().as_bytes()
}

fn transcript_hasher(
    capability: &[u8; 32],
    domain: &[u8],
    hello: ServerHello,
    client_nonce: &[u8; 32],
) -> blake3::Hasher {
    let mut hasher = blake3::Hasher::new_keyed(capability);
    hasher.update(domain);
    hasher.update(&MAGIC);
    hasher.update(&VERSION.to_be_bytes());
    hasher.update(&hello.boot_id);
    hasher.update(&hello.nonce);
    hasher.update(client_nonce);
    hasher
}

fn proof(
    capability: &[u8; 32],
    domain: &[u8],
    hello: ServerHello,
    client_nonce: &[u8; 32],
    credentials: (u32, u32, u32),
    owner_id: Option<&[u8; 16]>,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_keyed(capability);
    hasher.update(domain);
    hasher.update(&MAGIC);
    hasher.update(&VERSION.to_be_bytes());
    hasher.update(&hello.boot_id);
    hasher.update(&hello.nonce);
    hasher.update(client_nonce);
    hasher.update(&credentials.0.to_be_bytes());
    hasher.update(&credentials.1.to_be_bytes());
    hasher.update(&credentials.2.to_be_bytes());
    if let Some(owner_id) = owner_id {
        hasher.update(owner_id);
    }
    *hasher.finalize().as_bytes()
}

pub fn write_frame(mut writer: impl Write, kind: Kind, payload: &[u8]) -> io::Result<()> {
    validate_frame(kind, payload.len())?;
    writer.write_all(&(payload.len() as u32).to_be_bytes())?;
    writer.write_all(&[kind as u8])?;
    writer.write_all(payload)
}

pub fn read_frame(mut reader: impl Read) -> io::Result<Frame> {
    let mut length = [0; 4];
    reader.read_exact(&mut length)?;
    let length = u32::from_be_bytes(length) as usize;
    let mut kind = [0];
    reader.read_exact(&mut kind)?;
    let kind = Kind::try_from(kind[0])?;
    validate_frame(kind, length)?;
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(length)
        .map_err(|_| invalid("daemon frame allocation"))?;
    payload.resize(length, 0);
    reader.read_exact(&mut payload)?;
    Ok(Frame { kind, payload })
}

fn validate_frame(kind: Kind, length: usize) -> io::Result<()> {
    let limit = match kind {
        Kind::Stdout | Kind::Stderr => MAX_OUTPUT,
        _ => MAX_CONTROL,
    };
    if length > limit {
        return Err(invalid("daemon frame length"));
    }
    match kind {
        Kind::Stop | Kind::Started | Kind::Close | Kind::WorkspaceClosed if length != 0 => {
            Err(invalid("daemon empty frame payload"))
        }
        Kind::WorkspaceReady if length == 0 => Err(invalid("daemon WorkspaceReady payload")),
        Kind::Error if length != 1 => Err(invalid("daemon error frame payload")),
        Kind::Exit if length != 62 => Err(invalid("daemon Exit payload")),
        _ => Ok(()),
    }
}

impl ExecRequest {
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        validate_exec(&self.cwd, &self.argv)?;
        let mut bytes = Vec::with_capacity(52 + self.cwd.len() + argv_encoded_len(&self.argv)?);
        bytes.extend_from_slice(&self.owner_id);
        bytes.extend_from_slice(&self.workspace_id);
        bytes.extend_from_slice(&self.execution_id);
        push_bytes(&mut bytes, &self.cwd)?;
        push_u32(&mut bytes, self.argv.len())?;
        for arg in &self.argv {
            push_bytes(&mut bytes, arg)?;
        }
        if bytes.len() > MAX_CONTROL {
            return Err(invalid("daemon Exec aggregate"));
        }
        Ok(bytes)
    }

    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        if payload.len() > MAX_CONTROL {
            return Err(invalid("daemon Exec aggregate"));
        }
        let mut cursor = Cursor::new(payload);
        let owner_id = cursor.take(16)?.try_into().expect("owner id width");
        let workspace_id = cursor.take(16)?.try_into().expect("workspace id width");
        let execution_id = cursor.take(16)?.try_into().expect("execution id width");
        let cwd = cursor.bytes(MAX_CONTROL)?;
        let argc = cursor.u32()? as usize;
        if argc == 0 || argc > MAX_ARGS {
            return Err(invalid("daemon argv count"));
        }
        let mut argv = Vec::new();
        argv.try_reserve_exact(argc)
            .map_err(|_| invalid("daemon argv allocation"))?;
        for _ in 0..argc {
            argv.push(cursor.bytes(MAX_ARG)?);
        }
        if !cursor.done() {
            return Err(invalid("daemon Exec trailing bytes"));
        }
        validate_exec(&cwd, &argv)?;
        Ok(Self {
            owner_id,
            workspace_id,
            execution_id,
            cwd,
            argv,
        })
    }
}

impl MountRequest {
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        validate_mount(&self.root, &self.endpoint)?;
        let length = 72_usize
            .checked_add(self.root.len())
            .and_then(|length| length.checked_add(self.endpoint.len()))
            .ok_or_else(|| invalid("daemon Mount overflow"))?;
        if length > MAX_CONTROL {
            return Err(invalid("daemon Mount aggregate"));
        }
        let mut bytes = Vec::with_capacity(length);
        bytes.extend_from_slice(&self.owner_id);
        bytes.extend_from_slice(&self.workspace_id);
        push_bytes(&mut bytes, &self.root)?;
        push_bytes(&mut bytes, &self.endpoint)?;
        bytes.extend_from_slice(&self.capability);
        if bytes.len() > MAX_CONTROL {
            return Err(invalid("daemon Mount aggregate"));
        }
        Ok(bytes)
    }

    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        if payload.len() > MAX_CONTROL {
            return Err(invalid("daemon Mount aggregate"));
        }
        let mut cursor = Cursor::new(payload);
        let owner_id = cursor.take(16)?.try_into().expect("owner id width");
        let workspace_id = cursor.take(16)?.try_into().expect("workspace id width");
        let root = cursor.bytes(MAX_CONTROL)?;
        let endpoint = cursor.bytes(MAX_ARG)?;
        let capability = cursor.take(32)?.try_into().expect("Mount capability width");
        if !cursor.done() {
            return Err(invalid("daemon Mount trailing bytes"));
        }
        validate_mount(&root, &endpoint)?;
        Ok(Self {
            owner_id,
            workspace_id,
            root,
            endpoint,
            capability,
        })
    }
}

fn validate_mount(root: &[u8], endpoint: &[u8]) -> io::Result<()> {
    if root.is_empty()
        || root.len() > MAX_CONTROL
        || root.contains(&0)
        || endpoint.is_empty()
        || endpoint.len() > MAX_ARG
        || endpoint.contains(&0)
    {
        return Err(invalid("daemon Mount input"));
    }
    Ok(())
}

fn validate_exec(cwd: &[u8], argv: &[Vec<u8>]) -> io::Result<()> {
    if cwd.is_empty() || cwd.contains(&0) || cwd.len() > MAX_CONTROL {
        return Err(invalid("daemon cwd"));
    }
    if argv.is_empty() || argv.len() > MAX_ARGS || argv[0].is_empty() {
        return Err(invalid("daemon argv"));
    }
    let mut aggregate = 0_usize;
    for arg in argv {
        if arg.len() > MAX_ARG || arg.contains(&0) {
            return Err(invalid("daemon argument"));
        }
        aggregate = aggregate
            .checked_add(arg.len())
            .ok_or_else(|| invalid("daemon argv overflow"))?;
    }
    if aggregate > MAX_CONTROL {
        return Err(invalid("daemon argv aggregate"));
    }
    Ok(())
}

fn argv_encoded_len(argv: &[Vec<u8>]) -> io::Result<usize> {
    argv.iter().try_fold(4_usize, |total, arg| {
        total
            .checked_add(4)
            .and_then(|total| total.checked_add(arg.len()))
            .ok_or_else(|| invalid("daemon argv overflow"))
    })
}

impl Exit {
    pub fn encode(self) -> [u8; 62] {
        let mut bytes = [0; 62];
        let (kind, value) = match (self.code, self.signal) {
            (Some(code), None) => (0, code),
            (None, Some(signal)) => (1, signal),
            _ => (2, 0),
        };
        bytes[0] = kind;
        bytes[1..5].copy_from_slice(&value.to_be_bytes());
        bytes[5] = u8::from(self.stopped);
        let values = [
            self.stdout_bytes,
            self.stderr_bytes,
            self.timing.accept_bind_ns,
            self.timing.decode_ns,
            self.timing.spawn_ns,
            self.timing.runtime_ns,
            self.timing.drain_ns,
        ];
        for (index, value) in values.into_iter().enumerate() {
            let start = 6 + index * 8;
            bytes[start..start + 8].copy_from_slice(&value.to_be_bytes());
        }
        bytes
    }

    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        if payload.len() != 62 || payload[5] > 1 {
            return Err(invalid("daemon Exit"));
        }
        let value = i32::from_be_bytes(payload[1..5].try_into().expect("status width"));
        let mut values = [0_u64; 7];
        for (index, output) in values.iter_mut().enumerate() {
            let start = 6 + index * 8;
            *output = u64::from_be_bytes(
                payload[start..start + 8]
                    .try_into()
                    .expect("Exit value width"),
            );
        }
        let (code, signal) = match payload[0] {
            0 => (Some(value), None),
            1 => (None, Some(value)),
            2 => (None, None),
            _ => return Err(invalid("daemon Exit status")),
        };
        Ok(Self {
            code,
            signal,
            stopped: payload[5] == 1,
            stdout_bytes: values[0],
            stderr_bytes: values[1],
            timing: DaemonTiming {
                accept_bind_ns: values[2],
                decode_ns: values[3],
                spawn_ns: values[4],
                runtime_ns: values[5],
                drain_ns: values[6],
            },
        })
    }
}

fn push_u32(output: &mut Vec<u8>, value: usize) -> io::Result<()> {
    let value = u32::try_from(value).map_err(|_| invalid("daemon length"))?;
    output.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

fn push_bytes(output: &mut Vec<u8>, bytes: &[u8]) -> io::Result<()> {
    push_u32(output, bytes.len())?;
    output.extend_from_slice(bytes);
    Ok(())
}

struct Cursor<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    fn new(payload: &'a [u8]) -> Self {
        Self { payload, offset: 0 }
    }

    fn take(&mut self, length: usize) -> io::Result<&'a [u8]> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| invalid("daemon cursor overflow"))?;
        let bytes = self
            .payload
            .get(self.offset..end)
            .ok_or_else(|| invalid("truncated daemon payload"))?;
        self.offset = end;
        Ok(bytes)
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().expect("u32 width"),
        ))
    }

    fn bytes(&mut self, limit: usize) -> io::Result<Vec<u8>> {
        let length = self.u32()? as usize;
        if length > limit {
            return Err(invalid("daemon field length"));
        }
        Ok(self.take(length)?.to_vec())
    }

    fn done(&self) -> bool {
        self.offset == self.payload.len()
    }
}

pub fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor as IoCursor;

    #[test]
    fn fragmented_and_coalesced_frames_and_exec_bounds() {
        let request = ExecRequest {
            owner_id: [1; 16],
            workspace_id: [2; 16],
            execution_id: [3; 16],
            cwd: b"/workspace/x".to_vec(),
            argv: vec![b"/bin/printf".to_vec(), Vec::new(), vec![0xff]],
        };
        let payload = request.encode().unwrap();
        assert_eq!(ExecRequest::decode(&payload).unwrap(), request);
        let mount = MountRequest {
            owner_id: [1; 16],
            workspace_id: [2; 16],
            root: b"/workspace/x".to_vec(),
            endpoint: b"127.0.0.1:1234".to_vec(),
            capability: [3; 32],
        };
        let mount_payload = mount.encode().unwrap();

        let mut bytes = Vec::new();
        write_frame(&mut bytes, Kind::Exec, &payload).unwrap();
        write_frame(&mut bytes, Kind::Started, &[]).unwrap();
        write_frame(&mut bytes, Kind::Mount, &mount_payload).unwrap();
        write_frame(&mut bytes, Kind::Close, &[]).unwrap();
        let mut one_byte = OneByte(IoCursor::new(bytes));
        assert_eq!(read_frame(&mut one_byte).unwrap().kind, Kind::Exec);
        assert_eq!(read_frame(&mut one_byte).unwrap().kind, Kind::Started);
        let frame = read_frame(&mut one_byte).unwrap();
        assert_eq!(frame.kind, Kind::Mount);
        assert_eq!(MountRequest::decode(&frame.payload).unwrap(), mount);
        assert_eq!(read_frame(&mut one_byte).unwrap().kind, Kind::Close);

        let mut trailing = payload.clone();
        trailing.push(0);
        assert!(ExecRequest::decode(&trailing).is_err());
        let mut bad = request.clone();
        bad.argv[0].clear();
        assert!(bad.encode().is_err());
        bad = request.clone();
        bad.argv[0] = vec![b'x'; MAX_ARG + 1];
        assert!(bad.encode().is_err());
        bad = request;
        bad.argv = vec![b"x".to_vec(); MAX_ARGS + 1];
        assert!(bad.encode().is_err());

        assert!(write_frame(Vec::new(), Kind::Close, &[0]).is_err());
        assert!(write_frame(Vec::new(), Kind::WorkspaceReady, &[]).is_err());
        let mut bad_mount = mount.clone();
        bad_mount.endpoint.clear();
        assert!(bad_mount.encode().is_err());
        bad_mount = mount.clone();
        bad_mount.root.push(0);
        assert!(bad_mount.encode().is_err());
        let mut trailing_mount = mount_payload;
        trailing_mount.push(0);
        assert!(MountRequest::decode(&trailing_mount).is_err());
    }

    #[test]
    fn tcp_bound_proof_binds_challenge_owner_kind_and_payload() {
        let capability = [9; 32];
        let hello = ServerHello {
            boot_id: [1; 16],
            nonce: [2; 32],
        };
        let client_nonce = [3; 32];
        let owner = [4; 16];
        let proof = bound_client_proof(
            &capability,
            hello,
            &client_nonce,
            &owner,
            Kind::Exec,
            b"request",
        );
        assert_ne!(
            proof,
            bound_client_proof(
                &capability,
                ServerHello {
                    boot_id: hello.boot_id,
                    nonce: [5; 32],
                },
                &client_nonce,
                &owner,
                Kind::Exec,
                b"request",
            )
        );
        assert_ne!(
            proof,
            bound_client_proof(
                &capability,
                hello,
                &client_nonce,
                &owner,
                Kind::Mount,
                b"request",
            )
        );
        assert_ne!(
            proof,
            bound_server_proof(
                &capability,
                hello,
                &client_nonce,
                &owner,
                Kind::Exec,
                b"request",
            )
        );
    }

    struct OneByte<R>(R);

    impl<R: Read> Read for OneByte<R> {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            let length = bytes.len().min(1);
            self.0.read(&mut bytes[..length])
        }
    }
}
