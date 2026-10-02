//! One strict storage catalog client boundary, independent of local/native placement.
//! C5 conditional publication remains outside this storage-ready interface.
use crate::{
    strict_catalog::{
        CandidateRow, CatalogScope, Located, LogicalUse, PlacementDomain, Reference, Registration,
        SaveContext, StrictCatalog,
    },
    strict_remote::{NativeCatalog, RemoteError},
};
use layerfs_content::{ObjectId, ObjectRole};
use layerfs_storage::access::ValueGroupRow;

pub type Body = ([u8; 32], Option<Vec<u8>>);

/// A daemon-local catalog or its authenticated native proxy.
/// Native error strings retain explicit definite/unknown custody descriptions;
/// every error is terminal for the attempted operation, never an alternate route.
/// cache_namespace is0 until the proxy pins its first authenticated engine reply;
/// callers qualify cache answers only after required placement preflight.
pub trait Catalog: Send + Sync {
    fn cache_namespace(&self) -> u64;
    fn capture(&self, own_save: Option<i64>) -> Result<CatalogScope, String>;
    fn begin_save(&self) -> Result<i64, String>;
    fn begin_save_owned(&self, context: &SaveContext) -> Result<i64, String>;
    fn descriptor(&self, id: ObjectId) -> Result<Option<(ObjectRole, usize)>, String>;
    fn location(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        id: ObjectId,
    ) -> Result<Option<Located>, String>;
    fn locations(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        ids: &[ObjectId],
    ) -> Result<Vec<Option<Located>>, String>;
    fn has_use(&self, scope: CatalogScope, id: ObjectId, usage: LogicalUse)
        -> Result<bool, String>;
    fn body(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        order: i64,
    ) -> Result<Body, String>;
    fn body_domain(&self, scope: CatalogScope, order: i64) -> Result<PlacementDomain, String>;
    fn register_body(
        &self,
        domain: PlacementDomain,
        save: i64,
        digest: [u8; 32],
        metadata: Option<&[u8]>,
    ) -> Result<i64, String>;
    fn reserve_body(
        &self,
        domain: PlacementDomain,
        save: i64,
        digest: [u8; 32],
    ) -> Result<i64, String>;
    fn acknowledge_body(
        &self,
        save: i64,
        order: i64,
        digest: [u8; 32],
        metadata: Option<&[u8]>,
    ) -> Result<(), String>;
    fn discard_reserved_body(&self, save: i64, order: i64) -> Result<(), String>;
    fn register_use(
        &self,
        scope: CatalogScope,
        save: i64,
        domain: PlacementDomain,
        id: ObjectId,
        usage: LogicalUse,
        references: &[Reference],
    ) -> Result<(), String>;
    fn register(
        &self,
        scope: CatalogScope,
        save: i64,
        rows: &[Registration],
    ) -> Result<Vec<Located>, String>;
    fn reserve_ordinals(&self, save: i64, count: usize) -> Result<u32, String>;
    fn release_ordinals(&self, save: i64, used_end: u64, reserved_end: u64) -> Result<(), String>;
    fn note_window(&self, save: i64, used: usize, first: u32) -> Result<(), String>;
    fn insert_groups(
        &self,
        scope: CatalogScope,
        save: i64,
        groups: &[ValueGroupRow],
    ) -> Result<(), String>;
    fn group_for(&self, scope: CatalogScope, ordinal: u32)
        -> Result<Option<ValueGroupRow>, String>;
    fn group_page(
        &self,
        scope: CatalogScope,
        from: u32,
        limit: usize,
    ) -> Result<Vec<ValueGroupRow>, String>;
    fn metadata_window_start(&self, scope: CatalogScope) -> Result<u32, String>;
    fn candidate_page(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        after: u64,
        limit: usize,
    ) -> Result<Vec<CandidateRow>, String>;
    fn stage_candidates(
        &self,
        scope: CatalogScope,
        save: i64,
        domain: PlacementDomain,
        rows: &[CandidateRow],
    ) -> Result<(), String>;
    fn finish_storage(&self, save: i64) -> Result<(), String>;
    fn abandon(&self, save: i64) -> Result<(), String>;
    fn quarantine(&self, save: i64) -> Result<(), String>;
}

impl Catalog for StrictCatalog {
    fn cache_namespace(&self) -> u64 {
        StrictCatalog::cache_namespace(self)
    }
    fn capture(&self, own_save: Option<i64>) -> Result<CatalogScope, String> {
        StrictCatalog::capture(self, own_save)
    }
    fn begin_save(&self) -> Result<i64, String> {
        StrictCatalog::begin_save(self)
    }
    fn begin_save_owned(&self, context: &SaveContext) -> Result<i64, String> {
        StrictCatalog::begin_save_owned(self, context)
    }
    fn descriptor(&self, id: ObjectId) -> Result<Option<(ObjectRole, usize)>, String> {
        StrictCatalog::descriptor(self, id)
    }
    fn location(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        id: ObjectId,
    ) -> Result<Option<Located>, String> {
        StrictCatalog::location(self, scope, domain, id)
    }
    fn locations(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        ids: &[ObjectId],
    ) -> Result<Vec<Option<Located>>, String> {
        StrictCatalog::locations(self, scope, domain, ids)
    }
    fn has_use(
        &self,
        scope: CatalogScope,
        id: ObjectId,
        usage: LogicalUse,
    ) -> Result<bool, String> {
        StrictCatalog::has_use(self, scope, id, usage)
    }
    fn body(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        order: i64,
    ) -> Result<Body, String> {
        StrictCatalog::body(self, scope, domain, order)
    }
    fn body_domain(&self, scope: CatalogScope, order: i64) -> Result<PlacementDomain, String> {
        StrictCatalog::body_domain(self, scope, order)
    }
    fn register_body(
        &self,
        domain: PlacementDomain,
        save: i64,
        digest: [u8; 32],
        metadata: Option<&[u8]>,
    ) -> Result<i64, String> {
        StrictCatalog::register_body(self, domain, save, digest, metadata)
    }
    fn reserve_body(
        &self,
        domain: PlacementDomain,
        save: i64,
        digest: [u8; 32],
    ) -> Result<i64, String> {
        StrictCatalog::reserve_body(self, domain, save, digest)
    }
    fn acknowledge_body(
        &self,
        save: i64,
        order: i64,
        digest: [u8; 32],
        metadata: Option<&[u8]>,
    ) -> Result<(), String> {
        StrictCatalog::acknowledge_body_checked(self, save, order, digest, metadata)
    }
    fn discard_reserved_body(&self, save: i64, order: i64) -> Result<(), String> {
        StrictCatalog::discard_reserved_body(self, save, order)
    }
    fn register_use(
        &self,
        scope: CatalogScope,
        save: i64,
        domain: PlacementDomain,
        id: ObjectId,
        usage: LogicalUse,
        references: &[Reference],
    ) -> Result<(), String> {
        StrictCatalog::register_use(self, scope, save, domain, id, usage, references)
    }
    fn register(
        &self,
        scope: CatalogScope,
        save: i64,
        rows: &[Registration],
    ) -> Result<Vec<Located>, String> {
        StrictCatalog::register(self, scope, save, rows)
    }
    fn reserve_ordinals(&self, save: i64, count: usize) -> Result<u32, String> {
        StrictCatalog::reserve_ordinals(self, save, count)
    }
    fn release_ordinals(&self, save: i64, used_end: u64, reserved_end: u64) -> Result<(), String> {
        StrictCatalog::release_ordinals(self, save, used_end, reserved_end)
    }
    fn note_window(&self, save: i64, used: usize, first: u32) -> Result<(), String> {
        StrictCatalog::note_window(self, save, used, first)
    }
    fn insert_groups(
        &self,
        scope: CatalogScope,
        save: i64,
        groups: &[ValueGroupRow],
    ) -> Result<(), String> {
        StrictCatalog::insert_groups(self, scope, save, groups)
    }
    fn group_for(
        &self,
        scope: CatalogScope,
        ordinal: u32,
    ) -> Result<Option<ValueGroupRow>, String> {
        StrictCatalog::group_for(self, scope, ordinal)
    }
    fn group_page(
        &self,
        scope: CatalogScope,
        from: u32,
        limit: usize,
    ) -> Result<Vec<ValueGroupRow>, String> {
        StrictCatalog::group_page(self, scope, from, limit)
    }
    fn metadata_window_start(&self, scope: CatalogScope) -> Result<u32, String> {
        StrictCatalog::metadata_window_start(self, scope)
    }
    fn candidate_page(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        after: u64,
        limit: usize,
    ) -> Result<Vec<CandidateRow>, String> {
        StrictCatalog::candidate_page(self, scope, domain, after, limit)
    }
    fn stage_candidates(
        &self,
        scope: CatalogScope,
        save: i64,
        domain: PlacementDomain,
        rows: &[CandidateRow],
    ) -> Result<(), String> {
        StrictCatalog::stage_candidates(self, scope, save, domain, rows)
    }
    fn finish_storage(&self, save: i64) -> Result<(), String> {
        StrictCatalog::finish_storage(self, save)
    }
    fn abandon(&self, save: i64) -> Result<(), String> {
        StrictCatalog::abandon(self, save)
    }
    fn quarantine(&self, save: i64) -> Result<(), String> {
        StrictCatalog::quarantine(self, save)
    }
}

fn native_error(error: RemoteError) -> String {
    match error {
        RemoteError::Definite(message)
            if crate::strict_catalog::is_definite_catalog_error(&message) =>
        {
            message
        }
        other => other.to_string(),
    }
}

impl Catalog for NativeCatalog {
    fn cache_namespace(&self) -> u64 {
        NativeCatalog::cache_namespace(self)
    }
    fn capture(&self, own_save: Option<i64>) -> Result<CatalogScope, String> {
        NativeCatalog::capture(self, own_save).map_err(native_error)
    }
    fn begin_save(&self) -> Result<i64, String> {
        NativeCatalog::begin_save(self).map_err(native_error)
    }
    fn begin_save_owned(&self, context: &SaveContext) -> Result<i64, String> {
        NativeCatalog::begin_save_owned(self, context).map_err(native_error)
    }
    fn descriptor(&self, id: ObjectId) -> Result<Option<(ObjectRole, usize)>, String> {
        NativeCatalog::descriptor(self, id).map_err(native_error)
    }
    fn location(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        id: ObjectId,
    ) -> Result<Option<Located>, String> {
        NativeCatalog::location(self, scope, domain, id).map_err(native_error)
    }
    fn locations(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        ids: &[ObjectId],
    ) -> Result<Vec<Option<Located>>, String> {
        NativeCatalog::locations(self, scope, domain, ids).map_err(native_error)
    }
    fn has_use(
        &self,
        scope: CatalogScope,
        id: ObjectId,
        usage: LogicalUse,
    ) -> Result<bool, String> {
        NativeCatalog::has_use(self, scope, id, usage).map_err(native_error)
    }
    fn body(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        order: i64,
    ) -> Result<Body, String> {
        NativeCatalog::body(self, scope, domain, order).map_err(native_error)
    }
    fn body_domain(&self, scope: CatalogScope, order: i64) -> Result<PlacementDomain, String> {
        NativeCatalog::body_domain(self, scope, order).map_err(native_error)
    }
    fn register_body(
        &self,
        domain: PlacementDomain,
        save: i64,
        digest: [u8; 32],
        metadata: Option<&[u8]>,
    ) -> Result<i64, String> {
        NativeCatalog::register_body(self, domain, save, digest, metadata).map_err(native_error)
    }
    fn reserve_body(
        &self,
        domain: PlacementDomain,
        save: i64,
        digest: [u8; 32],
    ) -> Result<i64, String> {
        NativeCatalog::reserve_body(self, domain, save, digest).map_err(native_error)
    }
    fn acknowledge_body(
        &self,
        save: i64,
        order: i64,
        digest: [u8; 32],
        metadata: Option<&[u8]>,
    ) -> Result<(), String> {
        NativeCatalog::acknowledge_body(self, save, order, digest, metadata).map_err(native_error)
    }
    fn discard_reserved_body(&self, save: i64, order: i64) -> Result<(), String> {
        NativeCatalog::discard_reserved_body(self, save, order).map_err(native_error)
    }
    fn register_use(
        &self,
        scope: CatalogScope,
        save: i64,
        domain: PlacementDomain,
        id: ObjectId,
        usage: LogicalUse,
        references: &[Reference],
    ) -> Result<(), String> {
        NativeCatalog::register_use(self, scope, save, domain, id, usage, references)
            .map_err(native_error)
    }
    fn register(
        &self,
        scope: CatalogScope,
        save: i64,
        rows: &[Registration],
    ) -> Result<Vec<Located>, String> {
        NativeCatalog::register(self, scope, save, rows).map_err(native_error)
    }
    fn reserve_ordinals(&self, save: i64, count: usize) -> Result<u32, String> {
        NativeCatalog::reserve_ordinals(self, save, count).map_err(native_error)
    }
    fn release_ordinals(&self, save: i64, used_end: u64, reserved_end: u64) -> Result<(), String> {
        NativeCatalog::release_ordinals(self, save, used_end, reserved_end).map_err(native_error)
    }
    fn note_window(&self, save: i64, used: usize, first: u32) -> Result<(), String> {
        NativeCatalog::note_window(self, save, used, first).map_err(native_error)
    }
    fn insert_groups(
        &self,
        scope: CatalogScope,
        save: i64,
        groups: &[ValueGroupRow],
    ) -> Result<(), String> {
        NativeCatalog::insert_groups(self, scope, save, groups).map_err(native_error)
    }
    fn group_for(
        &self,
        scope: CatalogScope,
        ordinal: u32,
    ) -> Result<Option<ValueGroupRow>, String> {
        NativeCatalog::group_for(self, scope, ordinal).map_err(native_error)
    }
    fn group_page(
        &self,
        scope: CatalogScope,
        from: u32,
        limit: usize,
    ) -> Result<Vec<ValueGroupRow>, String> {
        NativeCatalog::group_page(self, scope, from, limit).map_err(native_error)
    }
    fn metadata_window_start(&self, scope: CatalogScope) -> Result<u32, String> {
        NativeCatalog::metadata_window_start(self, scope).map_err(native_error)
    }
    fn candidate_page(
        &self,
        scope: CatalogScope,
        domain: PlacementDomain,
        after: u64,
        limit: usize,
    ) -> Result<Vec<CandidateRow>, String> {
        NativeCatalog::candidate_page(self, scope, domain, after, limit).map_err(native_error)
    }
    fn stage_candidates(
        &self,
        scope: CatalogScope,
        save: i64,
        domain: PlacementDomain,
        rows: &[CandidateRow],
    ) -> Result<(), String> {
        NativeCatalog::stage_candidates(self, scope, save, domain, rows).map_err(native_error)
    }
    fn finish_storage(&self, save: i64) -> Result<(), String> {
        NativeCatalog::finish_storage(self, save).map_err(native_error)
    }
    fn abandon(&self, save: i64) -> Result<(), String> {
        NativeCatalog::abandon(self, save).map_err(native_error)
    }
    fn quarantine(&self, save: i64) -> Result<(), String> {
        NativeCatalog::quarantine(self, save).map_err(native_error)
    }
}
