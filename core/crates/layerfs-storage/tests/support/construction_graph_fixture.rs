//! Real issued immutable source capabilities and native provider ownership helpers.

use layerfs_content::filesystem::identity::InodeScope;
use layerfs_content::filesystem::input::{DirectoryUpdate, FilesystemInput, FilesystemResources};
use layerfs_content::filesystem::path::PathName;
use layerfs_content::filesystem::root::FilesystemRootId;
use layerfs_content::filesystem::rows::{BindingRows, DirectoryHeader, SliceBindingRows};
use layerfs_content::filesystem::state::{
    GraphAdjacencySeal, GraphCapacity, GraphConstructionScopes, GraphNodeKey, GraphSubject,
    SiteBirthLedger, SiteObservation, StateRecord,
};
use layerfs_content::ObjectId;
use layerfs_storage::construction_state::{ScratchAuthority, ScratchOwnerStatus, ScratchSession};
use rusqlite::{Connection, OpenFlags};
use std::path::Path;

pub const DEFAULT: u64 = 16 * 1024 * 1024;
pub const CONFIGURED: u64 = 48 * 1024 * 1024;
pub const ROWS: u64 = 65536;

pub fn with_source<T>(
    count: u32,
    update: bool,
    body: impl FnOnce(&SliceBindingRows<'_>, DirectoryHeader) -> T,
) -> T {
    let directories = [DirectoryUpdate {
        parent: i64::MAX as u64,
        changes: (0..count)
            .map(|ordinal| {
                (
                    PathName::new(&format!("n{ordinal:08x}")).unwrap(),
                    Some(u64::from(count) + 1 - u64::from(ordinal)),
                )
            })
            .collect(),
    }];
    let input = FilesystemInput {
        base: if update { Some(base()) } else { None },
        scope: namespace(),
        root_serial: 1,
        directories: &directories,
        inodes: &[],
        new_inodes: &[],
        resources: FilesystemResources::default(),
    };
    let source = SliceBindingRows::new(&input).unwrap();
    let header = source.directory_header(i64::MAX as u64).unwrap().unwrap();
    body(&source, header)
}

pub fn namespace() -> InodeScope {
    InodeScope::from_object(ObjectId::from_bytes(&[0x42; 32]).unwrap())
}
pub fn base() -> FilesystemRootId {
    FilesystemRootId(ObjectId::from_bytes(&[0x43; 32]).unwrap())
}
pub fn subject(source: &impl BindingRows, update: bool, bytes: u64) -> GraphSubject {
    GraphSubject::new(
        source.binding_source_id().unwrap(),
        namespace(),
        update.then(base),
        1,
        GraphCapacity::new(bytes).unwrap(),
    )
    .unwrap()
}
pub fn selected(session: &ScratchSession) -> GraphConstructionScopes {
    GraphConstructionScopes::new(
        session.selection().clone(),
        session.graph_subject().unwrap().clone(),
    )
    .unwrap()
}
pub fn begin(
    authority: &ScratchAuthority,
    source: &impl BindingRows,
    update: bool,
    bytes: u64,
    directories: u64,
    bindings: u64,
) -> (ScratchSession, GraphConstructionScopes) {
    let session = authority
        .begin_graph(
            [0x64; 32],
            directories,
            bindings,
            subject(source, update, bytes),
        )
        .unwrap();
    let scopes = selected(&session);
    (session, scopes)
}
pub fn empty_sites(session: &mut ScratchSession, scopes: &GraphConstructionScopes) {
    let expected = SiteBirthLedger::new(scopes.sites().clone()).unwrap().seal();
    let members = session.site_close_membership(&expected).unwrap();
    assert_eq!(members.birth(), &expected);
    let seal = session.site_final_seal(&members).unwrap();
    session.site_retire(&seal).unwrap();
}
pub fn full_sites(
    session: &mut ScratchSession,
    scopes: &GraphConstructionScopes,
    header: DirectoryHeader,
    count: u32,
) {
    let mut ledger = SiteBirthLedger::new(scopes.sites().clone()).unwrap();
    let mut batch = Vec::with_capacity(128);
    for ordinal in 0..count {
        batch.push(
            layerfs_content::filesystem::state::SiteRecord::birth(
                scopes.sites(),
                u64::from(count) + 1 - u64::from(ordinal),
                layerfs_content::filesystem::rows::BindingPoint::new(&header, ordinal).unwrap(),
                true,
            )
            .unwrap(),
        );
        if batch.len() == 128 || ordinal + 1 == count {
            assert_eq!(
                session.site_insert_batch(scopes.sites(), &batch).unwrap(),
                layerfs_content::filesystem::state::ClaimAdmission::Fresh
            );
            ledger.acknowledge(&batch).unwrap();
            batch.clear();
        }
    }
    let members = session.site_close_membership(&ledger.seal()).unwrap();
    let mut observed = Vec::with_capacity(128);
    for serial in 2..=u64::from(count) + 1 {
        observed.push(SiteObservation::new(
            layerfs_content::filesystem::state::SiteKey::new(scopes.sites(), serial).unwrap(),
            true,
        ));
        if observed.len() == 128 || serial == u64::from(count) + 1 {
            session
                .site_observe_base_batch(&members, &observed)
                .unwrap();
            observed.clear();
        }
    }
    let seal = session.site_final_seal(&members).unwrap();
    session.site_retire(&seal).unwrap();
}
pub fn seeds(
    session: &mut ScratchSession,
    scopes: &GraphConstructionScopes,
    serials: impl IntoIterator<Item = u64>,
) {
    let mut batch = Vec::with_capacity(128);
    for serial in serials {
        batch.push(GraphNodeKey::new(scopes.graph(), serial).unwrap());
        if batch.len() == 128 {
            session.graph_seed_batch(scopes.graph(), &batch).unwrap();
            batch.clear();
        }
    }
    if !batch.is_empty() {
        session.graph_seed_batch(scopes.graph(), &batch).unwrap();
    }
}
pub fn graph(
    session: &mut ScratchSession,
    scopes: &GraphConstructionScopes,
    seed: &[u64],
    outgoing: impl Fn(u64) -> Vec<u64>,
) -> GraphAdjacencySeal {
    seeds(session, scopes, seed.iter().copied());
    while let Some(mut parent) = session.graph_unexpanded(scopes.graph()).unwrap() {
        let children = outgoing(parent.key().serial());
        for chunk in children.chunks(63) {
            let keys: Vec<_> = chunk
                .iter()
                .map(|serial| GraphNodeKey::new(scopes.graph(), *serial).unwrap())
                .collect();
            parent = session
                .graph_append(scopes.graph(), &parent, &keys)
                .unwrap()
                .parent()
                .unwrap();
        }
        session.graph_expanded(scopes.graph(), &parent).unwrap();
    }
    session.graph_seal(scopes.graph()).unwrap()
}
pub fn status(authority: &ScratchAuthority, token: u64) -> ScratchOwnerStatus {
    authority
        .status()
        .unwrap()
        .into_iter()
        .find(|owner| owner.token == token)
        .unwrap()
}
pub fn external(path: &Path) -> Connection {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )
    .unwrap();
    connection.busy_timeout(std::time::Duration::ZERO).unwrap();
    connection
        .execute_batch("PRAGMA synchronous=OFF;PRAGMA journal_mode=MEMORY;")
        .unwrap();
    connection
}
pub fn scalar(connection: &Connection, sql: &str) -> u64 {
    let value: i64 = connection.query_row(sql, [], |row| row.get(0)).unwrap();
    u64::try_from(value).expect("nonnegative native scalar")
}
pub fn root(scope: &layerfs_content::filesystem::state::StateScope, serial: u64) -> StateRecord {
    StateRecord::directory_root(
        scope,
        serial,
        ObjectId::from_bytes(&super::roots_oracle::root(serial)).unwrap(),
    )
    .unwrap()
}
pub fn assert_header(
    base: &Path,
    owner: &ScratchOwnerStatus,
    scope: &layerfs_content::filesystem::state::GraphScope,
) {
    let header: Vec<u8> = external(&owner.path)
        .query_row("SELECT header FROM session_owner WHERE id=1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(header.len(), 298);
    assert_eq!(&header[..8], b"LFCSOWN4");
    assert_eq!(&header[8..10], &4u16.to_be_bytes());
    assert_eq!(&header[10..16], &[0; 6]);
    assert_eq!(&header[16..24], &owner.token.to_be_bytes());
    assert_eq!(&header[24..56], &owner.selector);
    let subject = super::graph_oracle::subject(
        scope.subject().source_id().as_bytes(),
        scope.subject().base().is_some(),
        scope.subject().root_serial(),
        scope.capacity().scratch_bytes(),
    );
    assert_eq!(&header[192..], &subject);
    let parent = identity(&std::fs::metadata(base).unwrap());
    let directory = identity(&std::fs::metadata(owner.path.parent().unwrap()).unwrap());
    let file = identity(&std::fs::metadata(&owner.path).unwrap());
    assert_eq!(&header[120..144], &parent);
    assert_eq!(&header[144..168], &directory);
    assert_eq!(&header[168..192], &file);
    let mut hash = blake3::Hasher::new();
    hash.update(b"layerfs/construction-state/native/v4\0");
    hash.update(&header[88..120]);
    hash.update(&parent);
    hash.update(&directory);
    hash.update(&file);
    hash.update(&owner.selector);
    hash.update(&owner.token.to_be_bytes());
    hash.update(&subject);
    assert_eq!(&header[56..88], hash.finalize().as_bytes());
    assert_eq!(
        &header[56..88],
        scope.nodes().selection().owner_binding().unwrap()
    );
}
pub fn identity(metadata: &std::fs::Metadata) -> [u8; 24] {
    use std::os::unix::fs::MetadataExt;
    let mut b = [0; 24];
    b[..8].copy_from_slice(&metadata.dev().to_be_bytes());
    b[8..16].copy_from_slice(&metadata.ino().to_be_bytes());
    b[16..20].copy_from_slice(&metadata.uid().to_be_bytes());
    b[20..].copy_from_slice(&metadata.mode().to_be_bytes());
    b
}
