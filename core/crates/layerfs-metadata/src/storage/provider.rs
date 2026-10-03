//! Provider lifetime and direct port delegation.
use crate::{client::Client, config::PgConfig, schema, wire::PgDiagnostics};
use layerfs_content::ObjectId;
use layerfs_storage::{
    location::{LocatedObject, SignatureRow},
    policy::StoragePolicy,
    port::*,
};
use std::sync::Arc;
/// One PostgreSQL metadata namespace, independent of opaque payload storage.
pub struct PgMetadata {
    pub(crate) client: Arc<Client>,
}
impl PgMetadata {
    /// Creates schema 1 explicitly. Existing product tables are refused, not migrated.
    pub fn create(config: PgConfig, policy: StoragePolicy) -> Result<Self, MetadataError> {
        let client = Client::connect(config.clone())?;
        schema::create(&client, &config, policy)?;
        Ok(Self { client })
    }
    /// Opens and validates the existing namespace on one authenticated connection.
    pub fn open(config: PgConfig) -> Result<Self, MetadataError> {
        let result = Self {
            client: Client::connect(config)?,
        };
        result.read_policy()?;
        Ok(result)
    }
    /// Actual SQL/protocol/socket counts. No timing or admission claim.
    pub fn diagnostics(&self) -> Result<PgDiagnostics, MetadataError> {
        self.client.diagnostics()
    }
}
impl MetadataStore for PgMetadata {
    fn policy(&self) -> Result<StoragePolicy, MetadataError> {
        self.read_policy()
    }
    fn locate(&self, ids: &[ObjectId], out: &mut Vec<LocatedObject>) -> Result<(), MetadataError> {
        self.read_locations(ids, out)
    }
    fn read_packs(&self, ids: &[i64], out: &mut Vec<MetadataPack>) -> Result<(), MetadataError> {
        self.read_bodies(ids, out)
    }
    fn value_groups(&self, query: ValueGroupQuery<'_>) -> Result<ValueGroups, MetadataError> {
        self.read_groups(query)
    }
    fn signatures(&self, out: &mut Vec<SignatureRow>) -> Result<(), MetadataError> {
        self.read_signatures(out)
    }
    fn reserve(&self, request: Reserve) -> Result<Reserved, MetadataError> {
        self.allocate(request)
    }
    fn register(&self, batch: &Registration) -> Result<Registered, MetadataError> {
        self.write_registration(batch)
    }
}
