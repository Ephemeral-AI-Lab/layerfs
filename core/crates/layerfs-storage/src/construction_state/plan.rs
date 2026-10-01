//! Closed private profiles and pre-effect sequential-phase arithmetic.

use crate::error::{StorageError, StorageResult};
use layerfs_content::file::edit::DraftCapacity;
use layerfs_content::filesystem::rows::BindingSourceId;
use layerfs_content::filesystem::state::{
    AliasCapacity, CanonicalCapacity, CanonicalOccupancy, FactCapacity, FactOccupancy,
    GraphCapacity, GraphSubject,
};

use super::profile::{RECORD_BYTES, ROW_LIMIT};

#[derive(Clone, Copy)]
pub(crate) enum Plan {
    Legacy,
    ClaimsThenRoots {
        directories: u64,
        bindings: u64,
    },
    SitesThenRoots {
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
    },
    SitesGraphThenRoots {
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
        capacity: GraphCapacity,
    },
    AliasesSitesGraphThenRoots {
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
        capacity: GraphCapacity,
        aliases: AliasCapacity,
    },
    NamespaceSitesGraphThenRoots {
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
        capacity: GraphCapacity,
        aliases: AliasCapacity,
        facts: FactCapacity,
    },
    CanonicalSitesGraphThenRoots {
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
        capacity: GraphCapacity,
        aliases: AliasCapacity,
        facts: FactCapacity,
        canonical: CanonicalCapacity,
    },
    Drafts {
        capacity: DraftCapacity,
    },
}

impl Plan {
    pub(crate) fn alias_graph(
        directories: u64,
        bindings: u64,
        subject: &GraphSubject,
        aliases: AliasCapacity,
    ) -> StorageResult<Self> {
        let _ = Self::graph(directories, bindings, subject)?;
        if aliases.site_bytes()
            != bindings
                .checked_mul(60)
                .ok_or(StorageError::Integrity("alias declaration bytes"))?
            || aliases.aggregate_bytes() > subject.capacity().scratch_bytes()
        {
            return Err(StorageError::Integrity("alias captured aggregate class"));
        }
        aliases.check(0, 0)?;
        Ok(Self::AliasesSitesGraphThenRoots {
            directories,
            bindings,
            source: subject.source_id(),
            capacity: subject.capacity(),
            aliases,
        })
    }

    pub(crate) fn namespace_graph(
        directories: u64,
        bindings: u64,
        subject: &GraphSubject,
        aliases: AliasCapacity,
        facts: FactCapacity,
    ) -> StorageResult<Self> {
        let _ = Self::alias_graph(directories, bindings, subject, aliases)?;
        let validated = FactCapacity::new(facts.facts(), facts.parents(), facts.bytes())?;
        if validated != facts || facts.bytes() > subject.capacity().scratch_bytes() {
            return Err(StorageError::Integrity("fact captured aggregate class"));
        }
        facts.check(FactOccupancy {
            sites: bindings,
            ..FactOccupancy::default()
        })?;
        Ok(Self::NamespaceSitesGraphThenRoots {
            directories,
            bindings,
            source: subject.source_id(),
            capacity: subject.capacity(),
            aliases,
            facts,
        })
    }

    pub(crate) fn canonical(
        directories: u64,
        bindings: u64,
        subject: &GraphSubject,
        aliases: AliasCapacity,
        facts: FactCapacity,
        canonical: CanonicalCapacity,
    ) -> StorageResult<Self> {
        let _ = Self::namespace_graph(directories, bindings, subject, aliases, facts)?;
        if CanonicalCapacity::new(
            canonical.counts(),
            canonical.zeros(),
            canonical.jobs(),
            canonical.frames(),
            canonical.bytes(),
        )? != canonical
            || canonical.bytes() > subject.capacity().scratch_bytes()
        {
            return Err(StorageError::Integrity(
                "canonical captured aggregate class",
            ));
        }
        canonical.check(CanonicalOccupancy {
            namespace: FactOccupancy {
                sites: bindings,
                ..Default::default()
            },
            ..Default::default()
        })?;
        Ok(Self::CanonicalSitesGraphThenRoots {
            directories,
            bindings,
            source: subject.source_id(),
            capacity: subject.capacity(),
            aliases,
            facts,
            canonical,
        })
    }
    pub(crate) const fn binding_limit(self) -> u64 {
        match self {
            Self::ClaimsThenRoots { bindings, .. }
            | Self::SitesThenRoots { bindings, .. }
            | Self::SitesGraphThenRoots { bindings, .. }
            | Self::AliasesSitesGraphThenRoots { bindings, .. }
            | Self::NamespaceSitesGraphThenRoots { bindings, .. }
            | Self::CanonicalSitesGraphThenRoots { bindings, .. } => bindings,
            _ => 0,
        }
    }
    pub(crate) const fn aliases(self) -> Option<AliasCapacity> {
        match self {
            Self::AliasesSitesGraphThenRoots { aliases, .. }
            | Self::NamespaceSitesGraphThenRoots { aliases, .. }
            | Self::CanonicalSitesGraphThenRoots { aliases, .. } => Some(aliases),
            _ => None,
        }
    }
    pub(crate) const fn facts(self) -> Option<FactCapacity> {
        match self {
            Self::NamespaceSitesGraphThenRoots { facts, .. }
            | Self::CanonicalSitesGraphThenRoots { facts, .. } => Some(facts),
            _ => None,
        }
    }
    pub(crate) const fn canonical_capacity(self) -> Option<CanonicalCapacity> {
        match self {
            Self::CanonicalSitesGraphThenRoots { canonical, .. } => Some(canonical),
            _ => None,
        }
    }
    pub(crate) fn profile_subject(self, graph: Option<&GraphSubject>) -> StorageResult<Vec<u8>> {
        let mut bytes = Vec::with_capacity(
            if matches!(self, Self::CanonicalSitesGraphThenRoots { .. }) {
                194
            } else if matches!(self, Self::NamespaceSitesGraphThenRoots { .. }) {
                154
            } else {
                130
            },
        );
        match self {
            Self::AliasesSitesGraphThenRoots { aliases, .. }
            | Self::NamespaceSitesGraphThenRoots { aliases, .. }
            | Self::CanonicalSitesGraphThenRoots { aliases, .. } => {
                bytes.extend_from_slice(
                    &graph
                        .ok_or(StorageError::Integrity("alias graph subject"))?
                        .encode(),
                );
                bytes.extend_from_slice(&aliases.records().to_be_bytes());
                bytes.extend_from_slice(&aliases.aggregate_bytes().to_be_bytes());
                bytes.extend_from_slice(&aliases.site_bytes().to_be_bytes());
                if let Some(facts) = self.facts() {
                    bytes.extend_from_slice(&facts.encode());
                }
                if let Some(canonical) = self.canonical_capacity() {
                    bytes.extend_from_slice(&canonical.encode());
                }
            }
            Self::Drafts { capacity } => {
                bytes.extend_from_slice(&capacity.records().to_be_bytes());
                bytes.extend_from_slice(&capacity.encoded_bytes().to_be_bytes());
                bytes.extend_from_slice(&capacity.scratch_bytes().to_be_bytes());
            }
            _ => {
                return Err(StorageError::Integrity(
                    "construction profile subject version",
                ))
            }
        }
        Ok(bytes)
    }

    pub(crate) fn graph(
        directories: u64,
        bindings: u64,
        subject: &GraphSubject,
    ) -> StorageResult<Self> {
        // Prepared D/B limits stay independent of the selected graph capacity.
        let _ = Self::sites(directories, bindings, subject.source_id())?;
        Ok(Self::SitesGraphThenRoots {
            directories,
            bindings,
            source: subject.source_id(),
            capacity: subject.capacity(),
        })
    }

    pub(crate) const fn scratch_bytes(self) -> u64 {
        match self {
            Self::SitesGraphThenRoots { capacity, .. }
            | Self::AliasesSitesGraphThenRoots { capacity, .. }
            | Self::NamespaceSitesGraphThenRoots { capacity, .. }
            | Self::CanonicalSitesGraphThenRoots { capacity, .. } => capacity.scratch_bytes(),
            Self::Drafts { capacity } => capacity.scratch_bytes(),
            _ => super::native::RESERVED_BYTES,
        }
    }

    pub(crate) fn sites(
        directories: u64,
        bindings: u64,
        source: BindingSourceId,
    ) -> StorageResult<Self> {
        // Sites and roots are sequential populations in one unchanged class.
        let _ = Self::phased(directories, bindings)?;
        let bytes = (directories * RECORD_BYTES).max(bindings * 60);
        if bytes > ROW_LIMIT * RECORD_BYTES {
            return Err(StorageError::CapacityExceeded {
                what: "construction scratch site record bytes",
                limit: ROW_LIMIT * RECORD_BYTES,
                actual: bytes,
            });
        }
        Ok(Self::SitesThenRoots {
            directories,
            bindings,
            source,
        })
    }
    pub(crate) fn phased(directories: u64, bindings: u64) -> StorageResult<Self> {
        let rows = directories.max(bindings);
        if rows > ROW_LIMIT {
            return Err(StorageError::CapacityExceeded {
                what: "construction scratch phased declared rows",
                limit: ROW_LIMIT,
                actual: rows,
            });
        }
        let bytes = directories
            .checked_mul(RECORD_BYTES)
            .zip(bindings.checked_mul(32))
            .map(|(roots, claims)| roots.max(claims))
            .ok_or(StorageError::Integrity(
                "construction scratch phase arithmetic",
            ))?;
        if bytes > ROW_LIMIT * RECORD_BYTES {
            return Err(StorageError::CapacityExceeded {
                what: "construction scratch phased declared record bytes",
                limit: ROW_LIMIT * RECORD_BYTES,
                actual: bytes,
            });
        }
        Ok(Self::ClaimsThenRoots {
            directories,
            bindings,
        })
    }

    pub(crate) const fn version(self) -> u16 {
        match self {
            Self::Legacy => 1,
            Self::ClaimsThenRoots { .. } => 2,
            Self::SitesThenRoots { .. } => 3,
            Self::SitesGraphThenRoots { .. } => 4,
            Self::AliasesSitesGraphThenRoots { .. } => 5,
            Self::Drafts { .. } => 6,
            Self::NamespaceSitesGraphThenRoots { .. } => 7,
            Self::CanonicalSitesGraphThenRoots { .. } => 8,
        }
    }

    pub(crate) const fn root_limit(self) -> u64 {
        match self {
            Self::Legacy => ROW_LIMIT,
            Self::ClaimsThenRoots { directories, .. }
            | Self::SitesThenRoots { directories, .. } => directories,
            Self::SitesGraphThenRoots { directories, .. }
            | Self::AliasesSitesGraphThenRoots { directories, .. }
            | Self::NamespaceSitesGraphThenRoots { directories, .. }
            | Self::CanonicalSitesGraphThenRoots { directories, .. } => directories,
            Self::Drafts { .. } => 0,
        }
    }
}
