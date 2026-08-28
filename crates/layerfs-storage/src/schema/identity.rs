//! Frozen schema-family and immutable Store-role identities.

pub type SqlSchema = (&'static str, &'static str);

pub const LEGACY_FULL_FORMAT_MARKER: &str = "layerfs-phase4a-sqlite-blob";
pub const LEGACY_FULL_SCHEMA_VERSION: i64 = 2;
pub const FULL_FORMAT_MARKER: &str = "layerfs-full-sqlite";
pub const FULL_SCHEMA_VERSION: i64 = 1;
pub const WORKING_FORMAT_MARKER: &str = "layerfs-working-sqlite";
pub const WORKING_SCHEMA_VERSION: i64 = 1;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SchemaIdentity {
    LegacyFull,
    Full,
    Working,
}

impl SchemaIdentity {
    pub const fn format_marker(self) -> &'static str {
        match self {
            Self::LegacyFull => LEGACY_FULL_FORMAT_MARKER,
            Self::Full => FULL_FORMAT_MARKER,
            Self::Working => WORKING_FORMAT_MARKER,
        }
    }

    pub const fn schema_version(self) -> i64 {
        match self {
            Self::LegacyFull => LEGACY_FULL_SCHEMA_VERSION,
            Self::Full => FULL_SCHEMA_VERSION,
            Self::Working => WORKING_SCHEMA_VERSION,
        }
    }

    pub const fn admits_role(self, role: StoreRole) -> bool {
        matches!(
            (self, role),
            (Self::Full, StoreRole::Durable | StoreRole::DurableCache)
                | (Self::Working, StoreRole::Working)
        )
    }

    pub const fn is_explicit_migration_source(self) -> bool {
        matches!(self, Self::LegacyFull)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StoreRole {
    Working,
    Durable,
    DurableCache,
}

impl StoreRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Durable => "durable",
            Self::DurableCache => "durable_cache",
        }
    }

    pub const fn schema_identity(self) -> SchemaIdentity {
        match self {
            Self::Working => SchemaIdentity::Working,
            Self::Durable | Self::DurableCache => SchemaIdentity::Full,
        }
    }

    pub const fn is_authority(self) -> bool {
        matches!(self, Self::Durable)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SchemaContract {
    pub identity: SchemaIdentity,
    pub format_marker: &'static str,
    pub schema_version: i64,
    pub table_names: &'static [&'static str],
    pub table_partitions: &'static [&'static [SqlSchema]],
    pub index_schemas: &'static [SqlSchema],
}

impl SchemaContract {
    pub const fn admits_role(self, role: StoreRole) -> bool {
        self.identity.admits_role(role)
    }

    pub const fn admits_exact_role(self, expected: StoreRole, stored: StoreRole) -> bool {
        matches!(
            (expected, stored),
            (StoreRole::Working, StoreRole::Working)
                | (StoreRole::Durable, StoreRole::Durable)
                | (StoreRole::DurableCache, StoreRole::DurableCache)
        ) && self.admits_role(stored)
    }
}
