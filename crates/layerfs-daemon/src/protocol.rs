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
    ResourceSampleStart = 12,
    ResourceSampleStarted = 13,
    ResourceSampleFinish = 14,
    ResourceSample = 15,
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
            12 => Ok(Self::ResourceSampleStart),
            13 => Ok(Self::ResourceSampleStarted),
            14 => Ok(Self::ResourceSampleFinish),
            15 => Ok(Self::ResourceSample),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceSampleRequest {
    pub owner_id: [u8; 16],
    pub workspace_id: [u8; 16],
}

impl ResourceSampleRequest {
    pub fn encode(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        bytes[..16].copy_from_slice(&self.owner_id);
        bytes[16..].copy_from_slice(&self.workspace_id);
        bytes
    }

    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        if payload.len() != 32 {
            return Err(invalid("daemon resource sample request"));
        }
        Ok(Self {
            owner_id: payload[..16].try_into().expect("owner id width"),
            workspace_id: payload[16..].try_into().expect("workspace id width"),
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceSampleFinishRequest {
    pub owner_id: [u8; 16],
    pub workspace_id: [u8; 16],
    pub t0_unix_ns: u64,
    pub t3_unix_ns: u64,
    pub uncertainty_ns: u64,
}

impl ResourceSampleFinishRequest {
    pub fn encode(self) -> [u8; 56] {
        let mut bytes = [0; 56];
        bytes[..16].copy_from_slice(&self.owner_id);
        bytes[16..32].copy_from_slice(&self.workspace_id);
        bytes[32..40].copy_from_slice(&self.t0_unix_ns.to_be_bytes());
        bytes[40..48].copy_from_slice(&self.t3_unix_ns.to_be_bytes());
        bytes[48..].copy_from_slice(&self.uncertainty_ns.to_be_bytes());
        bytes
    }

    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        if payload.len() != 56 {
            return Err(invalid("daemon resource sample finish request"));
        }
        let request = Self {
            owner_id: payload[..16].try_into().expect("owner id width"),
            workspace_id: payload[16..32].try_into().expect("workspace id width"),
            t0_unix_ns: u64::from_be_bytes(payload[32..40].try_into().expect("T0 width")),
            t3_unix_ns: u64::from_be_bytes(payload[40..48].try_into().expect("T3 width")),
            uncertainty_ns: u64::from_be_bytes(
                payload[48..].try_into().expect("uncertainty width"),
            ),
        };
        if request.t0_unix_ns == 0
            || request.t3_unix_ns < request.t0_unix_ns
            || request.uncertainty_ns > 1_000_000
        {
            return Err(invalid("daemon resource sample boundaries"));
        }
        Ok(request)
    }
}

pub const CGROUP_STAT_FIELDS: usize = 8;
const CGROUP_SAMPLE_VALUES: usize = 27 + CGROUP_STAT_FIELDS * 3;
const CGROUP_SAMPLE_BYTES: usize = CGROUP_SAMPLE_VALUES * 8 + 4;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CgroupResourceSample {
    pub memory_current_baseline: u64,
    pub memory_current_peak: u64,
    pub memory_current_final: u64,
    pub memory_incremental_peak: u64,
    pub memory_lifetime_peak_baseline: u64,
    pub memory_lifetime_peak_final: u64,
    pub swap_baseline: u64,
    pub swap_peak: u64,
    pub swap_final: u64,
    pub oom_baseline: u64,
    pub oom_final: u64,
    pub oom_delta: u64,
    pub oom_kill_baseline: u64,
    pub oom_kill_final: u64,
    pub oom_kill_delta: u64,
    pub dirty_writeback_baseline: u64,
    pub dirty_writeback_peak: u64,
    pub dirty_writeback_incremental_peak: u64,
    pub sample_interval_ns: u64,
    pub sample_count: u64,
    pub first_sample_ns: u64,
    pub last_sample_ns: u64,
    pub maximum_sample_gap_ns: u64,
    pub started_ns: u64,
    pub finished_ns: u64,
    pub sampler_thread_count: u64,
    pub interior_sample_ns: u64,
    pub stat_baseline: [u64; CGROUP_STAT_FIELDS],
    pub stat_peak: [u64; CGROUP_STAT_FIELDS],
    pub stat_final: [u64; CGROUP_STAT_FIELDS],
    pub t0_boundary_sampled: bool,
    pub t3_boundary_sampled: bool,
    pub interior_sampled: bool,
    pub sample_overflow: bool,
}

impl CgroupResourceSample {
    pub fn encode(self) -> [u8; CGROUP_SAMPLE_BYTES] {
        let mut bytes = [0; CGROUP_SAMPLE_BYTES];
        let mut values = [0_u64; CGROUP_SAMPLE_VALUES];
        values[..27].copy_from_slice(&[
            self.memory_current_baseline,
            self.memory_current_peak,
            self.memory_current_final,
            self.memory_incremental_peak,
            self.memory_lifetime_peak_baseline,
            self.memory_lifetime_peak_final,
            self.swap_baseline,
            self.swap_peak,
            self.swap_final,
            self.oom_baseline,
            self.oom_final,
            self.oom_delta,
            self.oom_kill_baseline,
            self.oom_kill_final,
            self.oom_kill_delta,
            self.dirty_writeback_baseline,
            self.dirty_writeback_peak,
            self.dirty_writeback_incremental_peak,
            self.sample_interval_ns,
            self.sample_count,
            self.first_sample_ns,
            self.last_sample_ns,
            self.maximum_sample_gap_ns,
            self.started_ns,
            self.finished_ns,
            self.sampler_thread_count,
            self.interior_sample_ns,
        ]);
        values[27..35].copy_from_slice(&self.stat_baseline);
        values[35..43].copy_from_slice(&self.stat_peak);
        values[43..51].copy_from_slice(&self.stat_final);
        for (index, value) in values.into_iter().enumerate() {
            let start = index * 8;
            bytes[start..start + 8].copy_from_slice(&value.to_be_bytes());
        }
        bytes[CGROUP_SAMPLE_VALUES * 8] = u8::from(self.t0_boundary_sampled);
        bytes[CGROUP_SAMPLE_VALUES * 8 + 1] = u8::from(self.t3_boundary_sampled);
        bytes[CGROUP_SAMPLE_VALUES * 8 + 2] = u8::from(self.interior_sampled);
        bytes[CGROUP_SAMPLE_VALUES * 8 + 3] = u8::from(self.sample_overflow);
        bytes
    }

    pub fn decode(payload: &[u8]) -> io::Result<Self> {
        if payload.len() != CGROUP_SAMPLE_BYTES
            || payload[CGROUP_SAMPLE_VALUES * 8..]
                .iter()
                .any(|value| *value > 1)
        {
            return Err(invalid("daemon cgroup resource sample"));
        }
        let mut values = [0_u64; CGROUP_SAMPLE_VALUES];
        for (index, value) in values.iter_mut().enumerate() {
            let start = index * 8;
            *value = u64::from_be_bytes(payload[start..start + 8].try_into().expect("u64 width"));
        }
        Ok(Self {
            memory_current_baseline: values[0],
            memory_current_peak: values[1],
            memory_current_final: values[2],
            memory_incremental_peak: values[3],
            memory_lifetime_peak_baseline: values[4],
            memory_lifetime_peak_final: values[5],
            swap_baseline: values[6],
            swap_peak: values[7],
            swap_final: values[8],
            oom_baseline: values[9],
            oom_final: values[10],
            oom_delta: values[11],
            oom_kill_baseline: values[12],
            oom_kill_final: values[13],
            oom_kill_delta: values[14],
            dirty_writeback_baseline: values[15],
            dirty_writeback_peak: values[16],
            dirty_writeback_incremental_peak: values[17],
            sample_interval_ns: values[18],
            sample_count: values[19],
            first_sample_ns: values[20],
            last_sample_ns: values[21],
            maximum_sample_gap_ns: values[22],
            started_ns: values[23],
            finished_ns: values[24],
            sampler_thread_count: values[25],
            interior_sample_ns: values[26],
            stat_baseline: values[27..35].try_into().expect("cgroup baseline width"),
            stat_peak: values[35..43].try_into().expect("cgroup peak width"),
            stat_final: values[43..51].try_into().expect("cgroup final width"),
            t0_boundary_sampled: payload[CGROUP_SAMPLE_VALUES * 8] == 1,
            t3_boundary_sampled: payload[CGROUP_SAMPLE_VALUES * 8 + 1] == 1,
            interior_sampled: payload[CGROUP_SAMPLE_VALUES * 8 + 2] == 1,
            sample_overflow: payload[CGROUP_SAMPLE_VALUES * 8 + 3] == 1,
        })
    }
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
        Kind::ResourceSampleStart if length != 32 => {
            Err(invalid("daemon resource sample request payload"))
        }
        Kind::ResourceSampleStarted if length != 16 => {
            Err(invalid("daemon resource sample clock calibration payload"))
        }
        Kind::ResourceSampleFinish if length != 56 => {
            Err(invalid("daemon resource sample finish payload"))
        }
        Kind::ResourceSample if length != CGROUP_SAMPLE_BYTES => {
            Err(invalid("daemon cgroup resource sample payload"))
        }
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
        let resource = ResourceSampleRequest {
            owner_id: [1; 16],
            workspace_id: [2; 16],
        };
        let resource_payload = resource.encode();
        assert_eq!(
            ResourceSampleRequest::decode(&resource_payload).unwrap(),
            resource
        );
        let finish = ResourceSampleFinishRequest {
            owner_id: [1; 16],
            workspace_id: [2; 16],
            t0_unix_ns: 10,
            t3_unix_ns: 20,
            uncertainty_ns: 1,
        };
        assert_eq!(
            ResourceSampleFinishRequest::decode(&finish.encode()).unwrap(),
            finish
        );
        let mut invalid_finish = finish.encode();
        invalid_finish[40..48].copy_from_slice(&9_u64.to_be_bytes());
        assert!(ResourceSampleFinishRequest::decode(&invalid_finish).is_err());
        assert!(validate_frame(Kind::ResourceSampleStarted, 16).is_ok());
        assert!(validate_frame(Kind::ResourceSampleStarted, 0).is_err());
        let sample = CgroupResourceSample {
            memory_current_baseline: 1,
            memory_current_peak: 2,
            sample_count: 3,
            stat_peak: [4; CGROUP_STAT_FIELDS],
            t0_boundary_sampled: true,
            t3_boundary_sampled: true,
            interior_sampled: true,
            ..Default::default()
        };
        assert_eq!(
            CgroupResourceSample::decode(&sample.encode()).unwrap(),
            sample
        );

        let mut bytes = Vec::new();
        write_frame(&mut bytes, Kind::Exec, &payload).unwrap();
        write_frame(&mut bytes, Kind::Started, &[]).unwrap();
        write_frame(&mut bytes, Kind::Mount, &mount_payload).unwrap();
        write_frame(&mut bytes, Kind::ResourceSampleStart, &resource_payload).unwrap();
        write_frame(&mut bytes, Kind::ResourceSample, &sample.encode()).unwrap();
        write_frame(&mut bytes, Kind::Close, &[]).unwrap();
        let mut one_byte = OneByte(IoCursor::new(bytes));
        assert_eq!(read_frame(&mut one_byte).unwrap().kind, Kind::Exec);
        assert_eq!(read_frame(&mut one_byte).unwrap().kind, Kind::Started);
        let frame = read_frame(&mut one_byte).unwrap();
        assert_eq!(frame.kind, Kind::Mount);
        assert_eq!(MountRequest::decode(&frame.payload).unwrap(), mount);
        assert_eq!(
            read_frame(&mut one_byte).unwrap().kind,
            Kind::ResourceSampleStart
        );
        assert_eq!(
            read_frame(&mut one_byte).unwrap().kind,
            Kind::ResourceSample
        );
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
