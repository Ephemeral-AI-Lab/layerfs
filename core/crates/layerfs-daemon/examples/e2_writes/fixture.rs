//! Closed independent E04 inputs and an actual authenticated host fixture.
use super::digest::{hex, Sha256};
use layerfs_bridge::{
    codec::{Message, ReassemblyConfig, ReceiveBudget},
    contract::MessageKind,
    native,
};
use layerfs_content::{
    filesystem::{attributes::PortableMetadata, PathName},
    object::InodeKind,
    ObjectId,
};
use layerfs_daemon::{
    upstream::{AttachRefusal, ExpectedBinding, PersistenceBootstrap, Upstream, UpstreamOperation},
    Completion, OwnerClient,
};
use layerfs_history::{
    BranchId, BranchRecord, BranchSnapshot, CatalogId, CommitId, HistoryName, LayerId,
    LayerStackId, WorkspaceId,
};
use layerfs_persistence::SqlitePersistenceProfile;
use layerfs_sdk::client::{Attachment, Calls};
use layerfs_storage::StoragePolicy;
use layerfs_workspace::BaseStat;
use std::{
    error::Error,
    fmt,
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

pub const BASE_BYTES: u64 = 16_777_216;
pub const WRITE_BYTES: usize = 4096;
pub const WRITES: usize = 1000;
pub const FILE_NAME: &str = "dense.bin";
const DOMAIN: &[u8] = b"LayerFS E1 fixture v1\0";
pub const CLIENT_PRIVATE: [u8; 32] = [1; 32];
pub const HOST_PRIVATE: [u8; 32] = [2; 32];

pub struct Config {
    pub endpoint: String,
    pub assignment: PathBuf,
    pub base_input: PathBuf,
    pub replacements: PathBuf,
}
pub fn write_offset(index: usize) -> u64 {
    4096 * ((104729 * index as u64) % (BASE_BYTES / 4096))
}
/// Independent generator identity; never used as the product's base provider.
pub fn payload(identity: u64, at: u64, output: &mut [u8]) {
    let mut written = 0;
    while written < output.len() {
        let offset = at + written as u64;
        let mut hash = Sha256::new();
        hash.update(DOMAIN);
        hash.update(&1u64.to_le_bytes());
        hash.update(&identity.to_le_bytes());
        hash.update(&(offset / 32).to_le_bytes());
        let unit = hash.finish();
        let start = (offset % 32) as usize;
        let count = (32 - start).min(output.len() - written);
        output[written..written + count].copy_from_slice(&unit[start..start + count]);
        written += count;
    }
}
fn hash_input(path: &Path, length: u64, replacements: bool) -> io::Result<String> {
    let mut file = File::open(path)?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() != length {
        return Err(io::Error::other("E04 closed input kind/length"));
    }
    let mut hash = Sha256::new();
    let mut bytes = [0; WRITE_BYTES];
    let mut expected = [0; WRITE_BYTES];
    for index in 0..length / WRITE_BYTES as u64 {
        file.read_exact(&mut bytes)?;
        if replacements {
            payload(index + 1, 0, &mut expected);
        } else {
            payload(0, index * WRITE_BYTES as u64, &mut expected);
        }
        if bytes != expected {
            return Err(io::Error::other(
                "E04 input differs from frozen independent generator",
            ));
        }
        hash.update(&bytes);
    }
    let after = file.metadata()?;
    if after.len() != before.len() || after.modified()? != before.modified()? {
        return Err(io::Error::other("E04 input changed during identity pass"));
    }
    Ok(hex(&hash.finish()))
}
pub struct Fixture {
    pub upstream: Upstream,
    pub calls: Arc<Calls>,
    pub serial: u64,
    pub stat: BaseStat,
    pub base_sha256: String,
    pub replacements_sha256: String,
    pub base_input: PathBuf,
    pub replacements: PathBuf,
    pub acquisition: Acquisition,
    receipts: Option<(Completion, Message, Message)>,
}
#[derive(Clone, Debug)]
pub struct Acquisition {
    pub base_sha256: String,
    pub metadata: PortableMetadata,
    pub nlink: u64,
    pub source_allocated_bytes: u64,
    /// Actual native copy count before Init; not an importer I/O counter.
    pub source_copied_bytes: u64,
    pub source_removed: bool,
}
impl Fixture {
    pub fn take_attach_receipts(&mut self) -> Option<(Completion, Message, Message)> {
        self.receipts.take()
    }
    pub fn write_input(&self, index: usize) -> io::Result<[u8; WRITE_BYTES]> {
        if index >= WRITES {
            return Err(io::Error::other("E04 write index"));
        }
        let mut file = File::open(&self.replacements)?;
        if file.metadata()?.len() != (WRITES * WRITE_BYTES) as u64 {
            return Err(io::Error::other("E04 replacement length changed"));
        }
        file.seek(SeekFrom::Start((index * WRITE_BYTES) as u64))?;
        let mut bytes = [0; WRITE_BYTES];
        file.read_exact(&mut bytes)?;
        let mut expected = [0; WRITE_BYTES];
        payload(index as u64 + 1, 0, &mut expected);
        if bytes != expected {
            return Err(io::Error::other(
                "E04 replacement changed after preparation",
            ));
        }
        Ok(bytes)
    }
}
/// Retains already created native/operation/result owners on original refusal.
pub struct FixtureFailure {
    pub cause: Box<dyn Error>,
    pub attachment: Option<Attachment>,
    pub attach: Option<Box<AttachRefusal>>,
    pub upstream: Option<Upstream>,
    pub operation: Option<UpstreamOperation>,
    pub receipts: Option<(Completion, Message, Message)>,
}
impl FixtureFailure {
    fn early(error: impl Into<Box<dyn Error>>) -> Self {
        Self {
            cause: error.into(),
            attachment: None,
            attach: None,
            upstream: None,
            operation: None,
            receipts: None,
        }
    }
}
impl fmt::Debug for FixtureFailure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut view = out.debug_struct("OriginalFixtureFailure");
        view.field("cause", &self.cause)
            .field("attach", &self.attach)
            .field("operation", &self.operation)
            .field("retained_upstream", &self.upstream.is_some())
            .field("retained_receipts", &self.receipts.is_some());
        if let Some(operation) = &self.operation {
            match operation.failure() {
                Ok(original) => view.field("first_object_failure", &*original),
                Err(original) => view.field("failure_owner_refusal", &original),
            };
        }
        view.finish()
    }
}
impl fmt::Display for FixtureFailure {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{self:?}")
    }
}
impl Error for FixtureFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

pub fn attach(config: &Config, owner: OwnerClient) -> Result<Fixture, FixtureFailure> {
    let base_sha256 =
        hash_input(&config.base_input, BASE_BYTES, false).map_err(FixtureFailure::early)?;
    let replacements_sha256 = hash_input(&config.replacements, (WRITES * WRITE_BYTES) as u64, true)
        .map_err(FixtureFailure::early)?;
    let acquisition =
        acquisition(&config.assignment.with_extension("fixture")).map_err(FixtureFailure::early)?;
    if acquisition.base_sha256 != base_sha256
        || !acquisition.source_removed
        || acquisition.source_copied_bytes != BASE_BYTES
        || acquisition.source_allocated_bytes < BASE_BYTES
    {
        return Err(FixtureFailure::early(
            "E04 actual dense acquisition identity",
        ));
    }
    let (expected, bootstrap) = assignment(&config.assignment).map_err(FixtureFailure::early)?;
    if expected.local_peer != native::public_key(&CLIENT_PRIVATE).map_err(FixtureFailure::early)? {
        return Err(FixtureFailure::early("E04 provisioned client differs"));
    }
    let address = config
        .endpoint
        .to_socket_addrs()
        .map_err(FixtureFailure::early)?
        .next()
        .ok_or_else(|| FixtureFailure::early("E04 endpoint has no first address"))?;
    let socket = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(FixtureFailure::early)?;
    socket
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(FixtureFailure::early)?;
    socket
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(FixtureFailure::early)?;
    let budget = ReceiveBudget::new(ReassemblyConfig {
        kind: MessageKind::Reply,
        messages: 8,
        demand_messages: 1,
        control_messages: 1,
        message_bytes: 34 << 20,
        bytes: 96 << 20,
        demand_reserve: 34 << 20,
        control_reserve: 64 << 10,
    })
    .map_err(FixtureFailure::early)?;
    let connection = native::initiate(socket, &CLIENT_PRIVATE, expected.host_peer)
        .map_err(FixtureFailure::early)?;
    let attachment = match Attachment::new(connection, budget) {
        Ok(attachment) => attachment,
        Err((error, connection)) => {
            return Err(FixtureFailure::early(Box::new(AttachmentRefusal {
                error,
                connection,
            })))
        }
    };
    let calls = attachment.calls();
    let success = match Upstream::attach_with_receipts(attachment, expected, bootstrap, owner, 0) {
        Ok(success) => success,
        Err(original) => {
            return Err(FixtureFailure {
                cause: "E04 original Upstream attach refused".into(),
                attach: Some(Box::new(original)),
                ..FixtureFailure::early("E04 original Upstream attach refused")
            })
        }
    };
    let upstream = success.upstream;
    let receipts = (success.open, success.binding_reply, success.policy_reply);
    let operation = match upstream.operation() {
        Ok(operation) => operation,
        Err(original) => {
            return Err(FixtureFailure {
                cause: Box::new(ScopeRefusal(original)),
                upstream: Some(upstream),
                receipts: Some(receipts),
                ..FixtureFailure::early("E04 operation scope refused")
            })
        }
    };
    let result = operation.run(|operation| -> Result<BaseStat, Box<dyn Error>> {
        let base = operation.workspace().base()?;
        let file = base.child(
            base.root().root_inode().serial(),
            &PathName::new(FILE_NAME)?,
        )?;
        let stat = base.stat(file.serial)?;
        if stat.value.kind != InodeKind::RegularFile
            || stat.logical_len != BASE_BYTES
            || stat.metadata != acquisition.metadata
            || stat.value.namespace_ref_count != acquisition.nlink
        {
            return Err(
                "E04 authenticated actual dense file facts differ from independent source".into(),
            );
        }
        Ok(stat)
    });
    let stat = match result {
        Ok(result) => result.value,
        Err(original) => {
            return Err(FixtureFailure {
                cause: original.error,
                operation: Some(original.operation),
                upstream: Some(upstream),
                receipts: Some(receipts),
                ..FixtureFailure::early("E04 authenticated file refused")
            })
        }
    };
    Ok(Fixture {
        upstream,
        calls,
        serial: stat.serial,
        stat,
        base_sha256,
        replacements_sha256,
        base_input: config.base_input.clone(),
        replacements: config.replacements.clone(),
        acquisition,
        receipts: Some(receipts),
    })
}

struct AttachmentRefusal {
    error: layerfs_sdk::client::AttachmentError,
    connection: native::Connection,
}
impl fmt::Debug for AttachmentRefusal {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        let error: &dyn Error = match &self.error {
            layerfs_sdk::client::AttachmentError::Native(error) => error,
            layerfs_sdk::client::AttachmentError::Frame(error) => error,
        };
        out.debug_struct("OriginalAttachmentRefusal")
            .field("error", &error)
            .field("peer", &self.connection.peer.public_key())
            .finish()
    }
}
#[derive(Debug)]
struct ScopeRefusal(layerfs_daemon::upstream::OperationRefusal);
impl fmt::Display for ScopeRefusal {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{self:?}")
    }
}
impl Error for ScopeRefusal {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0.error)
    }
}
impl fmt::Display for AttachmentRefusal {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(out, "{self:?}")
    }
}
impl Error for AttachmentRefusal {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match &self.error {
            layerfs_sdk::client::AttachmentError::Native(error) => error,
            layerfs_sdk::client::AttachmentError::Frame(error) => error,
        })
    }
}

fn read_fields(path: &Path, maximum: u64) -> Result<Vec<String>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(maximum + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err("E04 independent assignment window".into());
    }
    Ok(std::str::from_utf8(&bytes)?
        .lines()
        .map(str::to_owned)
        .collect())
}
fn bytes<const N: usize>(value: &str) -> Result<[u8; N], Box<dyn Error>> {
    if !value.is_ascii() || value.len() != 2 * N {
        return Err("E04 assignment field width".into());
    }
    let mut out = [0; N];
    for (index, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(out)
}
fn profile(value: &str) -> Result<SqlitePersistenceProfile, Box<dyn Error>> {
    match value {
        "durable" => Ok(SqlitePersistenceProfile::Durable),
        "disposable" => Ok(SqlitePersistenceProfile::Disposable),
        _ => Err("E04 persistence profile".into()),
    }
}
fn assignment(path: &Path) -> Result<(ExpectedBinding, PersistenceBootstrap), Box<dyn Error>> {
    let f = read_fields(path, 8192)?;
    if f.len() != 20 || f[0] != "layerfs-r4-functional-v1" {
        return Err("E04 complete host assignment schema".into());
    }
    let object = |v: &str| -> Result<ObjectId, Box<dyn Error>> {
        Ok(ObjectId::from_bytes(&bytes::<32>(v)?)?)
    };
    let expected = ExpectedBinding {
        host_peer: bytes(&f[1])?,
        local_peer: bytes(&f[2])?,
        runtime: bytes(&f[3])?,
        catalog: CatalogId::from_bytes(bytes(&f[4])?),
        provider_incarnation: f[5].parse()?,
        workspace: WorkspaceId::from_slice(&bytes::<32>(&f[6])?)?,
        snapshot: BranchSnapshot {
            branch: BranchRecord {
                id: BranchId::from_slice(&bytes::<17>(&f[7])?)?,
                stack: LayerStackId::from_slice(&bytes::<17>(&f[8])?)?,
                name: HistoryName::new(&f[9])?,
                base_layer: LayerId::from_slice(&bytes::<33>(&f[10])?)?,
                head_commit: if f[11] == "-" {
                    None
                } else {
                    Some(CommitId::from_slice(&bytes::<33>(&f[11])?)?)
                },
            },
            head_root: if f[12] == "-" {
                None
            } else {
                Some(object(&f[12])?)
            },
            base_root: object(&f[13])?,
            effective_root: object(&f[14])?,
            scope: object(&f[15])?,
            profile: object(&f[16])?,
        },
        root_serial: f[17].parse()?,
        policy: StoragePolicy::frozen_default(),
        persistence: profile(&f[18])?,
    };
    let bootstrap = PersistenceBootstrap {
        host_peer: expected.host_peer,
        runtime: expected.runtime,
        catalog: expected.catalog,
        provider_incarnation: expected.provider_incarnation,
        profile: profile(&f[19])?,
    };
    Ok((expected, bootstrap))
}
fn acquisition(path: &Path) -> Result<Acquisition, Box<dyn Error>> {
    let f = read_fields(path, 4096)?;
    if f.len() != 9 || f[0] != "layerfs-e04-acquisition-v1" || f[8] != "source_removed=true" {
        return Err("E04 independent acquisition schema".into());
    }
    bytes::<32>(&f[1])?;
    Ok(Acquisition {
        base_sha256: f[1].clone(),
        metadata: PortableMetadata {
            mode: f[2].parse()?,
            mtime_seconds: f[3].parse()?,
            mtime_nanoseconds: f[4].parse()?,
        },
        nlink: f[5].parse()?,
        source_allocated_bytes: f[6].parse()?,
        source_copied_bytes: f[7].parse()?,
        source_removed: true,
    })
}

/// macOS owns the real global Store. The caller runs its actual Runtime Sessions
/// through Supervisor on the host thread, retains original delivery/fence reports,
/// and never substitutes a daemon-local object map. All acquisition is separate
/// prepared-input work, not E04 Workspace-write work or a cold-cache claim.
/// Project's supported native Init profile has four constructor workers; the
/// construction-workers environment declaration does not override that profile.
#[cfg(target_os = "macos")]
pub struct HostFixture {
    pub runtime: layerfs_sdk::Runtime,
    pub expected: ExpectedBinding,
    pub bootstrap: PersistenceBootstrap,
    pub acquisition: Acquisition,
}
#[cfg(target_os = "macos")]
struct Authority {
    peer: [u8; 32],
    workspace: WorkspaceId,
    branch: BranchId,
}
#[cfg(target_os = "macos")]
impl layerfs_sdk::Authorization for Authority {
    fn workspace(
        &self,
        peer: [u8; 32],
        workspace: WorkspaceId,
        branch: BranchId,
    ) -> layerfs_sdk::RuntimeResult<()> {
        if (peer, workspace, branch) == (self.peer, self.workspace, self.branch) {
            Ok(())
        } else {
            Err(layerfs_sdk::RuntimeError::Denied)
        }
    }
    fn objects(
        &self,
        peer: [u8; 32],
        workspace: WorkspaceId,
        branch: BranchId,
        _: &[ObjectId],
    ) -> layerfs_sdk::RuntimeResult<()> {
        self.workspace(peer, workspace, branch)
    }
}
/// Creates only fresh owned paths; failures leave acquired artifacts untouched.
/// The closed independent base input is copied densely, closed, observed and
/// imported through actual Project/Storage Save. It is never a consumer provider.
#[cfg(target_os = "macos")]
pub fn prepare_host(
    global_store: &Path,
    source: &Path,
    assignment_path: &Path,
    base_input: &Path,
    profile: SqlitePersistenceProfile,
    incarnation: [u8; 32],
    deadline: std::time::Instant,
) -> Result<HostFixture, Box<dyn Error>> {
    use layerfs_history::{ForkRequest, ForkSource, HistoryCatalog, HistoryCatalogConfig};
    use layerfs_persistence::{Handles, PersistenceConfig, SqliteAcquisitionSchema};
    use layerfs_storage::Storage;
    use std::{fs::OpenOptions, io::Write, os::unix::fs::MetadataExt};
    if global_store.exists()
        || source.exists()
        || assignment_path.exists()
        || assignment_path.with_extension("fixture").exists()
    {
        return Err("E04 requires fresh owned host paths".into());
    }
    if std::env::var("LAYERFS_CONSTRUCTION_WORKERS")
        .ok()
        .as_deref()
        != Some("1")
    {
        return Err("E04 construction-workers environment declaration must equal1".into());
    }
    let base_sha256 = hash_input(base_input, BASE_BYTES, false)?;
    std::fs::create_dir(source)?;
    let target = source.join(FILE_NAME);
    let mut input = File::open(base_input)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)?;
    let mut copied = 0u64;
    let mut buffer = [0; 65_536];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
        copied = copied
            .checked_add(count as u64)
            .ok_or("E04 dense input copy overflow")?;
    }
    if copied != BASE_BYTES {
        return Err("E04 actual dense copied byte count".into());
    }
    drop(output);
    drop(input);
    if hash_input(&target, BASE_BYTES, false)? != base_sha256 {
        return Err("E04 closed dense copy differs from sealed input".into());
    }
    let metadata = std::fs::symlink_metadata(&target)?;
    let allocated = metadata
        .blocks()
        .checked_mul(512)
        .ok_or("E04 source allocation overflow")?;
    if !metadata.is_file() || metadata.len() != BASE_BYTES || allocated < BASE_BYTES {
        return Err("E04 native input is not physically dense".into());
    }
    let portable = PortableMetadata {
        mode: metadata.mode() & 0o777,
        mtime_seconds: metadata.mtime(),
        mtime_nanoseconds: u32::try_from(metadata.mtime_nsec())?,
    };
    portable.validate(InodeKind::RegularFile)?;
    let policy = StoragePolicy::frozen_default();
    let handles = Handles::create(
        PersistenceConfig::sqlite(global_store)
            .with_sqlite_profile(profile)
            .with_sqlite_acquisition(SqliteAcquisitionSchema::Tables),
        policy,
        &HistoryCatalogConfig {
            binding_key: incarnation.to_vec(),
            cursor_key: [9; 32],
            incarnation: 1,
        },
    )?;
    let storage = Storage::new(handles.storage.clone())?;
    let layer_name = HistoryName::new("e04-dense")?;
    let initialized = layerfs_telemetry::timer::Timing::disabled(
        "E04 actual native fixture acquisition",
        |scope| {
            layerfs_project::init(
                &storage,
                &handles.history,
                layerfs_project::InitRequest {
                    source,
                    acquisition: &handles.acquisition,
                    stack: LayerStackId::from_authority([5; 16]),
                    name: layer_name,
                    scope_seed: incarnation,
                    deadline,
                },
                scope,
            )
        },
    )
    .0?;
    if initialized.entries != 2 {
        return Err("E04 source projection changed".into());
    }
    let branch = BranchId::from_authority([6; 16]);
    handles.history.fork(&ForkRequest {
        stack: initialized.stack.id,
        branch,
        name: HistoryName::new("main")?,
        source: ForkSource::Layer(initialized.stack.head_layer),
    })?;
    let workspace = WorkspaceId::from_authority(incarnation)?;
    let expected = ExpectedBinding {
        host_peer: native::public_key(&HOST_PRIVATE)?,
        local_peer: native::public_key(&CLIENT_PRIVATE)?,
        runtime: incarnation,
        catalog: handles.history.catalog_id(),
        provider_incarnation: handles.history.incarnation(),
        workspace,
        snapshot: handles
            .history
            .branch_snapshot(branch)?
            .ok_or("E04 actual fork missing")?,
        root_serial: initialized.root_serial,
        policy: storage.policy(),
        persistence: profile,
    };
    let bootstrap = PersistenceBootstrap {
        host_peer: expected.host_peer,
        runtime: expected.runtime,
        catalog: expected.catalog,
        provider_incarnation: expected.provider_incarnation,
        profile: handles.profile().persistence,
    };
    if bootstrap.profile != profile {
        return Err("E04 actual profile differs".into());
    }
    let acquisition = Acquisition {
        base_sha256,
        metadata: portable,
        nlink: metadata.nlink(),
        source_allocated_bytes: allocated,
        source_copied_bytes: copied,
        source_removed: true,
    };
    // Remove only this newly created acquisition copy after successful Save/Init.
    // The independent closed input remains available solely to the oracle.
    std::fs::remove_dir_all(source)?;
    if source.exists() {
        return Err("E04 acquisition source remains".into());
    }
    write_assignment(assignment_path, &expected, &bootstrap)?;
    let values = [
        "layerfs-e04-acquisition-v1".into(),
        acquisition.base_sha256.clone(),
        portable.mode.to_string(),
        portable.mtime_seconds.to_string(),
        portable.mtime_nanoseconds.to_string(),
        acquisition.nlink.to_string(),
        allocated.to_string(),
        copied.to_string(),
        "source_removed=true".into(),
    ];
    let mut fact = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(assignment_path.with_extension("fixture"))?;
    fact.write_all((values.join("\n") + "\n").as_bytes())?;
    drop(fact);
    drop(storage);
    let runtime = layerfs_sdk::Runtime::new(
        handles,
        layerfs_sdk::Config {
            incarnation,
            save_slots: 4,
        },
        Box::new(Authority {
            peer: expected.local_peer,
            workspace,
            branch,
        }),
    )?;
    Ok(HostFixture {
        runtime,
        expected,
        bootstrap,
        acquisition,
    })
}
#[cfg(target_os = "macos")]
fn write_assignment(
    path: &Path,
    expected: &ExpectedBinding,
    bootstrap: &PersistenceBootstrap,
) -> Result<(), Box<dyn Error>> {
    use std::{fs::OpenOptions, io::Write};
    let raw = |bytes: &[u8]| -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(bytes.len() * 2);
        for &byte in bytes {
            output.push(DIGITS[usize::from(byte >> 4)] as char);
            output.push(DIGITS[usize::from(byte & 15)] as char);
        }
        output
    };
    let name = |profile| match profile {
        SqlitePersistenceProfile::Durable => "durable",
        SqlitePersistenceProfile::Disposable => "disposable",
    };
    let s = &expected.snapshot;
    let fields = [
        "layerfs-r4-functional-v1".into(),
        raw(&expected.host_peer),
        raw(&expected.local_peer),
        raw(&expected.runtime),
        raw(&expected.catalog.to_bytes()),
        expected.provider_incarnation.to_string(),
        raw(&expected.workspace.to_bytes()),
        raw(&s.branch.id.to_bytes()),
        raw(&s.branch.stack.to_bytes()),
        s.branch.name.as_str().into(),
        raw(&s.branch.base_layer.to_bytes()),
        s.branch
            .head_commit
            .map_or_else(|| "-".into(), |v| raw(&v.to_bytes())),
        s.head_root
            .map_or_else(|| "-".into(), |v| raw(&v.to_bytes())),
        raw(&s.base_root.to_bytes()),
        raw(&s.effective_root.to_bytes()),
        raw(&s.scope.to_bytes()),
        raw(&s.profile.to_bytes()),
        expected.root_serial.to_string(),
        name(expected.persistence).into(),
        name(bootstrap.profile).into(),
    ];
    let mut output = OpenOptions::new().write(true).create_new(true).open(path)?;
    output.write_all((fields.join("\n") + "\n").as_bytes())?;
    Ok(())
}
