#![allow(dead_code)]
//! Shared fixtures for the catalog's external tests.
//!
//! Everything here goes through the public API: `sqlite::create`,
//! `sqlite::open_read_only` and the `HistoryCatalog` methods. No private item is
//! reached and no statement is issued against the catalog by the tests.

use layerfs_content::ObjectId;
use layerfs_history::{
    BranchId, CatalogId, HistoryCatalogConfig, HistoryName, LayerId, LayerStackId, StageToken,
    WorkspaceId,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// A temporary directory removed when the test ends.
pub struct Temp(PathBuf);

impl Temp {
    /// Creates one uniquely named directory.
    pub fn new(name: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "layerfs-pg-history-{}-{}_{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            name
        ));
        std::fs::create_dir_all(&path).expect("temporary directory");
        Self(path)
    }

    /// The directory path.
    pub fn path(&self) -> &std::path::Path {
        &self.0
    }

    /// A file path inside this directory.
    pub fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The binding key every fixture catalog uses.
pub const BINDING: &[u8] = b"layerfs-history-test-binding";

/// Explicit PostgreSQL fixture config, derived only from this unique temp path.
pub fn config(path: &std::path::Path) -> layerfs_metadata::PgConfig {
    let mut config = layerfs_metadata::PgConfig::from_env().expect("owned PostgreSQL required");
    let hash = blake3::hash(path.to_string_lossy().as_bytes());
    config.schema = format!("lfs302_h_{}", &hash.to_hex()[..32]);
    config
}
pub fn control(path: &std::path::Path) -> postgres::Client {
    let config = config(path);
    let mut c = postgres::Config::new()
        .host(&config.host)
        .port(config.port)
        .user(&config.user)
        .password(&config.password)
        .dbname(&config.database)
        .connect(postgres::NoTls)
        .unwrap();
    c.batch_execute(&format!("SET search_path TO \"{}\"", config.schema))
        .unwrap();
    c
}
pub fn create(path: &std::path::Path) -> layerfs_metadata::PgHistory {
    layerfs_metadata::PgHistory::create(
        config(path),
        &HistoryCatalogConfig {
            cursor_key: [71; 32],
            binding_key: BINDING.to_vec(),
            incarnation: 7,
        },
    )
    .expect("catalog creation")
}
pub fn reopen(path: &std::path::Path) -> layerfs_metadata::PgHistory {
    layerfs_metadata::PgHistory::open_read_only(config(path), BINDING, [71; 32])
        .expect("read-only reopen")
}
/// The catalog identity of the fixture binding.
pub fn catalog_id() -> CatalogId {
    CatalogId::derive(BINDING).expect("catalog identity")
}

/// One deterministic root identity.
pub fn root(tag: u8) -> ObjectId {
    ObjectId::for_bytes(&[b"layerfs/test/root/".as_slice(), &[tag]].concat())
}

/// One deterministic stack identity.
pub fn stack(body: u8) -> LayerStackId {
    LayerStackId::from_authority([body; 16])
}

/// One deterministic Branch identity.
pub fn branch_id(body: u8) -> BranchId {
    BranchId::from_authority([body; 16])
}

/// One deterministic Workspace incarnation.
pub fn workspace(body: u8) -> WorkspaceId {
    WorkspaceId::from_authority([body; 32]).expect("Workspace incarnation")
}

/// One checked history name.
pub fn name(text: &str) -> HistoryName {
    HistoryName::new(text).expect("history name")
}

/// The genesis Layer identity of one stack and root.
pub fn genesis(stack: LayerStackId, root: ObjectId) -> LayerId {
    LayerId::derive(stack, None, root)
}

/// One checked stage token.
pub fn token(value: u64) -> StageToken {
    StageToken::new(value).expect("stage token")
}
