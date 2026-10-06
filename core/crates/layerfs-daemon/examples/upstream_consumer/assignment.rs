//! Bounded external proof assignment; public types, no product serialization hook.
use layerfs_daemon::upstream::{ExpectedBinding, PersistenceBootstrap};
use layerfs_history::{
    BranchId, BranchRecord, BranchSnapshot, CatalogId, CommitId, HistoryName, LayerId,
    LayerStackId, WorkspaceId,
};
use layerfs_persistence::SqlitePersistenceProfile;
use layerfs_storage::StoragePolicy;
use std::{error::Error, path::Path};

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(DIGITS[usize::from(byte >> 4)] as char);
        output.push(DIGITS[usize::from(byte & 15)] as char);
    }
    output
}
fn bytes<const N: usize>(value: &str) -> Result<[u8; N], Box<dyn Error>> {
    if !value.is_ascii() || value.len() != 2 * N {
        return Err("assignment field width".into());
    }
    let mut output = [0; N];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(output)
}
fn profile(value: &str) -> Result<SqlitePersistenceProfile, Box<dyn Error>> {
    match value {
        "durable" => Ok(SqlitePersistenceProfile::Durable),
        "disposable" => Ok(SqlitePersistenceProfile::Disposable),
        _ => Err("assignment persistence profile".into()),
    }
}
fn profile_name(value: SqlitePersistenceProfile) -> &'static str {
    match value {
        SqlitePersistenceProfile::Durable => "durable",
        SqlitePersistenceProfile::Disposable => "disposable",
    }
}
pub fn write(
    path: &Path,
    expected: &ExpectedBinding,
    bootstrap: &PersistenceBootstrap,
) -> Result<(), Box<dyn Error>> {
    let snapshot = &expected.snapshot;
    let fields = vec![
        "layerfs-r4-functional-v1".into(),
        hex(&expected.host_peer),
        hex(&expected.local_peer),
        hex(&expected.runtime),
        hex(&expected.catalog.to_bytes()),
        expected.provider_incarnation.to_string(),
        hex(&expected.workspace.to_bytes()),
        hex(&snapshot.branch.id.to_bytes()),
        hex(&snapshot.branch.stack.to_bytes()),
        snapshot.branch.name.as_str().into(),
        hex(&snapshot.branch.base_layer.to_bytes()),
        snapshot
            .branch
            .head_commit
            .map_or_else(|| "-".into(), |id| hex(&id.to_bytes())),
        snapshot
            .head_root
            .map_or_else(|| "-".into(), |id| hex(&id.to_bytes())),
        hex(&snapshot.base_root.to_bytes()),
        hex(&snapshot.effective_root.to_bytes()),
        hex(&snapshot.scope.to_bytes()),
        hex(&snapshot.profile.to_bytes()),
        expected.root_serial.to_string(),
        profile_name(expected.persistence).into(),
        profile_name(bootstrap.profile).into(),
    ];
    assert_eq!(expected.policy, StoragePolicy::frozen_default());
    assert_eq!(expected.host_peer, bootstrap.host_peer);
    assert_eq!(expected.runtime, bootstrap.runtime);
    assert_eq!(expected.catalog, bootstrap.catalog);
    assert_eq!(
        expected.provider_incarnation,
        bootstrap.provider_incarnation
    );
    std::fs::write(path, fields.join("\n") + "\n")?;
    Ok(())
}
pub fn read(path: &Path) -> Result<(ExpectedBinding, PersistenceBootstrap), Box<dyn Error>> {
    let input = std::fs::read(path)?;
    if input.len() > 8192 {
        return Err("external proof assignment byte window".into());
    }
    let text = std::str::from_utf8(&input)?;
    let fields: Vec<_> = text.lines().collect();
    if fields.len() != 20 || fields[0] != "layerfs-r4-functional-v1" {
        return Err("external proof assignment schema".into());
    }
    let object = |value: &str| -> Result<layerfs_content::ObjectId, Box<dyn Error>> {
        Ok(layerfs_content::ObjectId::from_bytes(&bytes::<32>(value)?)?)
    };
    let expected = ExpectedBinding {
        host_peer: bytes(fields[1])?,
        local_peer: bytes(fields[2])?,
        runtime: bytes(fields[3])?,
        catalog: CatalogId::from_bytes(bytes(fields[4])?),
        provider_incarnation: fields[5].parse()?,
        workspace: WorkspaceId::from_slice(&bytes::<32>(fields[6])?)?,
        snapshot: BranchSnapshot {
            branch: BranchRecord {
                id: BranchId::from_slice(&bytes::<17>(fields[7])?)?,
                stack: LayerStackId::from_slice(&bytes::<17>(fields[8])?)?,
                name: HistoryName::new(fields[9])?,
                base_layer: LayerId::from_slice(&bytes::<33>(fields[10])?)?,
                head_commit: if fields[11] == "-" {
                    None
                } else {
                    Some(CommitId::from_slice(&bytes::<33>(fields[11])?)?)
                },
            },
            head_root: if fields[12] == "-" {
                None
            } else {
                Some(object(fields[12])?)
            },
            base_root: object(fields[13])?,
            effective_root: object(fields[14])?,
            scope: object(fields[15])?,
            profile: object(fields[16])?,
        },
        root_serial: fields[17].parse()?,
        policy: StoragePolicy::frozen_default(),
        persistence: profile(fields[18])?,
    };
    let bootstrap = PersistenceBootstrap {
        host_peer: expected.host_peer,
        runtime: expected.runtime,
        catalog: expected.catalog,
        provider_incarnation: expected.provider_incarnation,
        profile: profile(fields[19])?,
    };
    Ok((expected, bootstrap))
}
