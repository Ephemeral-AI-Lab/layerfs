//! Scalar catalog authority, allocator, accounting and table applicability.
use super::{emit, transaction::View, Observation, Options, Outcome};
use crate::backend::records::BackendError;

pub(super) fn count(view: &dyn View, sql: &str) -> Result<i64, BackendError> {
    view.query(sql, vec![])?
        .first()
        .ok_or(BackendError::Integrity)?
        .get(0)
}
pub(super) fn check(
    view: &dyn View,
    options: Options,
    observer: &mut dyn FnMut(Observation),
    id: &'static str,
    sql: &str,
    problem: &str,
) -> Result<(), BackendError> {
    options.check()?;
    let n = count(view, sql)?;
    emit(
        observer,
        id,
        if n == 0 { Outcome::Pass } else { Outcome::Fail },
        format!("{n} {problem}"),
    );
    Ok(())
}
pub(super) fn run(
    view: &dyn View,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    for(id,sql,problem)in[
        ("store.catalog.authority","SELECT count(*) FROM history_meta WHERE id!=1 OR blob_length(catalog_id)!=32 OR catalog_incarnation<1 OR blob_length(binding_key) NOT BETWEEN 1 AND 128 OR identity_format!=3","malformed catalog authority rows"),
        ("store.catalog.singleton","SELECT CASE WHEN count(*)=1 THEN 0 ELSE 1 END FROM history_meta","catalog authority singleton violations"),
        ("store.catalog.policy","SELECT count(*) FROM store_policy WHERE id!=1 OR next_pack_id<1 OR next_ordinal NOT BETWEEN 1 AND 4294967296","malformed allocation policy rows"),
        ("store.catalog.policy.singleton","SELECT CASE WHEN count(*)=1 THEN 0 ELSE 1 END FROM store_policy","allocation policy singleton violations"),
        ("store.catalog.pack_allocator","SELECT count(*) FROM pack WHERE pack_id>=(SELECT next_pack_id FROM store_policy WHERE id=1)","pack rows beyond their reserved allocation boundary"),
        ("store.catalog.ordinal_allocator","SELECT count(*) FROM inode_value_block WHERE last_ordinal>=(SELECT next_ordinal FROM store_policy WHERE id=1)","ordinal blocks beyond their reserved allocation boundary"),
        ("store.catalog.pack_shape","SELECT count(*) FROM pack WHERE pack_id<1 OR domain NOT IN (0,1) OR blob_length(digest)!=32 OR length NOT BETWEEN 32 AND 16781312 OR (domain=0 AND length>262144)","invalid pack descriptor shapes; payload extraction remains bounded by actual length"),
        ("store.catalog.location_shape","SELECT count(*) FROM object_location WHERE blob_length(object_id)!=32 OR role NOT BETWEEN 1 AND 13 OR canonical_length NOT BETWEEN 1 AND 16777216 OR pack_id<1 OR group_number NOT BETWEEN 0 AND 255 OR record_number NOT BETWEEN 0 AND 8190","invalid winning locator shapes"),
        ("store.catalog.ordinal_shape","SELECT count(*) FROM inode_value_block WHERE first_ordinal NOT BETWEEN 1 AND 4294967295 OR last_ordinal NOT BETWEEN first_ordinal AND 4294967295 OR last_ordinal-first_ordinal>=255 OR blob_length(body) NOT BETWEEN 9 AND 4000","malformed ordinal block key/range/body widths"),
        ("store.catalog.ordinal_overlap","SELECT count(*) FROM inode_value_block b WHERE EXISTS(SELECT 1 FROM inode_value_block p WHERE p.first_ordinal<b.first_ordinal AND p.last_ordinal>=b.first_ordinal)","overlapping authoritative ordinal blocks; reservation gaps remain valid"),
        ("store.advisory.binding_hints.shape","SELECT count(*) FROM binding_hint_block WHERE grp<0 OR seq<1 OR blob_length(body) NOT BETWEEN 9 AND 4000","malformed hint block key/body widths"),
        ("store.catalog.lineage_shape","SELECT (SELECT count(*) FROM layer_lineage WHERE blob_length(layer_id)!=33 OR blob_length(layer_stack_id)!=17 OR height<0 OR overwrote_count<0)+(SELECT count(*) FROM commit_lineage WHERE blob_length(commit_id)!=33 OR blob_length(run)!=33 OR depth<1 OR newest<0 OR blob_length(merged)!=33)+(SELECT count(*) FROM absorbed WHERE blob_length(run)!=33 OR depth<1 OR height<0)+(SELECT count(*) FROM reach WHERE blob_length(key) NOT IN (17,33) OR x<0 OR height<0)","malformed derived lineage key/value shapes"),
        ("store.catalog.scope","SELECT count(*) FROM scope_allocator WHERE blob_length(scope_id)!=32 OR highwater<0 OR blob_length(authority_id)!=32 OR authority_id!=(SELECT catalog_id FROM history_meta WHERE id=1)","invalid scope allocation/authority rows; unused retained scopes are allowed"),
        ("store.catalog.totals","SELECT count(*) FROM store_totals WHERE id!=1 OR logical<0 OR canonical<0 OR stored<0 OR metadata<0 OR init_entries<0 OR init_files<0 OR init_bytes<0 OR init_canonical<0 OR init_stored<0 OR init_metadata<0 OR init_millis<0","invalid nonnegative accounting rows"),
        ("store.catalog.totals.singleton","SELECT CASE WHEN count(*)=1 THEN 0 ELSE 1 END FROM store_totals","accounting singleton violations"),
        ("store.catalog.logical_total","SELECT count(*) FROM store_totals WHERE logical!=(SELECT COALESCE(sum(logical),0) FROM layer)+(SELECT COALESCE(sum(logical),0) FROM \"commit\")","logical accounting totals disagree with retained history rows"),
        ("store.catalog.canonical_total","SELECT count(*) FROM store_totals WHERE canonical!=(SELECT COALESCE(sum(canonical_length),0) FROM object_location)","canonical accounting totals disagree with winning locators"),
        ("store.catalog.stored_total","SELECT count(*) FROM store_totals WHERE stored!=(SELECT COALESCE(sum(length),0) FROM pack) OR metadata!=(SELECT COALESCE(sum(length),0) FROM pack WHERE domain=0)","stored accounting totals disagree with retained physical packs"),
    ]{check(view,options,observer,id,sql,problem)?;}
    if view.has_table("store_lane")? {
        check(view,options,observer,"store.catalog.lane","SELECT CASE WHEN count(*)=1 AND min(id)=1 AND max(id)=1 THEN 0 ELSE 1 END FROM store_lane","malformed PostgreSQL publication-lane singleton; no lane lock taken")?;
    }
    if view.has_table("sqlite_sequence")? {
        // sqlite_sequence records allocation history, not a demand that current
        // rows be contiguous or that a deleted Init owner still survives.
        check(
            view,
            options,
            observer,
            "store.catalog.sequence",
            "SELECT count(*) FROM sqlite_sequence WHERE seq<0",
            "negative SQLite AUTOINCREMENT sequence values",
        )?;
    }
    Ok(())
}
