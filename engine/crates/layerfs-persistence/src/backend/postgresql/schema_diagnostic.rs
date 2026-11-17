//! Bounded diagnostic admission projections; ordinary writer checks are unchanged.
//! Normative schema-2 columns; healthy shipped-DDL fixtures detect drift.
const COLUMNS: &str = r#"absorbed run bytea NO
absorbed depth bigint NO
absorbed height bigint NO
binding_hint_block grp bigint NO
binding_hint_block seq bigint NO
binding_hint_block body bytea NO
branch branch_id bytea NO
branch layer_stack_id bytea NO
branch name text NO
branch base_layer_id bytea NO
branch forked_from_id bytea YES
branch head_commit_id bytea YES
branch_retired branch_id bytea NO
branch_retired removed_ms bigint NO
commit commit_id bytea NO
commit layer_stack_id bytea NO
commit root_id bytea NO
commit parent_commit_id bytea YES
commit base_layer_id bytea NO
commit second_parent_id bytea YES
commit branch_id bytea YES
commit via bigint NO
commit message text YES
commit merged_from_id bytea YES
commit created_ms bigint NO
commit logical bigint NO
commit new_canonical bigint NO
commit new_stored bigint NO
commit new_metadata bigint NO
commit lww bytea NO
commit displaced_id bytea YES
commit history_serial bigint NO
commit_lineage commit_id bytea NO
commit_lineage run bytea NO
commit_lineage depth bigint NO
commit_lineage newest bigint NO
commit_lineage merged bytea YES
content_signature slot bigint NO
content_signature stamp bigint NO
content_signature object_id bytea NO
content_signature signature bytea NO
file_digest digest bytea NO
file_digest length bigint NO
file_digest root bytea NO
gc_dead kind text NO
gc_dead id bytea NO
gc_intent op_id bytea NO
gc_intent kind text NO
gc_intent scope_kind text NO
gc_intent scope_id bytea YES
gc_intent workspace text YES
gc_intent sandbox text YES
gc_intent machine text YES
gc_intent project text YES
gc_intent branch text YES
gc_intent owner_runtime text YES
gc_intent owner_boot text YES
gc_intent state text NO
gc_intent uncertain boolean NO
gc_intent phase text YES
gc_intent generation bigint YES
gc_intent artifact text YES
gc_intent high_water bigint YES
gc_intent evidence text YES
gc_intent updated_ms bigint NO
gc_op id bigint NO
gc_op generation bigint NO
gc_op phase text NO
gc_op scratch_schema text YES
gc_op op_id bytea YES
gc_op mark_fingerprint text YES
gc_op mark_visited bigint YES
gc_op mark_serial bigint YES
gc_op mark_pack_id bigint YES
gc_op mark_digest bytea YES
gc_op mark_totals bytea YES
gc_op mark_stamp bigint YES
gc_op started_ms bigint NO
gc_op dead_canonical bigint NO
gc_op dead_stored bigint NO
gc_op dead_metadata bigint NO
gc_op dead_logical bigint NO
gc_op physical_before bigint NO
gc_op physical_after bigint NO
gc_op peak_temp bigint NO
gc_op kept_waste bytea YES
gc_op totals_applied boolean NO
gc_op artifact text YES
gc_op pid bigint YES
gc_op backend_start text YES
gc_op usename text YES
gc_op incarnation bigint YES
history_meta id bigint NO
history_meta catalog_id bytea NO
history_meta catalog_incarnation bigint NO
history_meta binding_key bytea NO
history_meta identity_format bigint NO
history_meta schema_version bigint NO
history_meta next_history_serial bigint NO
history_meta schema_source text NO
history_meta schema_definition text NO
init_entry operation_id bigint NO
init_entry parent_position bigint NO
init_entry name bytea NO
init_entry position bigint YES
init_entry kind bigint NO
init_entry canonical_position bigint YES
init_entry metadata_root bytea NO
init_entry content_root bytea YES
init_entry host_path bytea YES
init_entry host_identity bytea YES
init_host_file operation_id bigint NO
init_host_file canonical_position bigint NO
init_host_file device bytea NO
init_host_file inode bytea NO
init_host_file evidence bytea NO
init_host_file host_path bytea NO
init_host_file aliases bigint NO
init_host_file file_root bytea YES
init_operation operation_id bigint NO
init_operation owner_epoch bigint NO
init_operation phase bigint NO
init_operation source_device bytea NO
init_operation source_inode bytea NO
init_operation stack_id bytea NO
init_operation scope bytea NO
init_operation held_rows bigint NO
init_operation held_bytes bigint NO
init_operation peak_rows bigint NO
init_operation peak_bytes bigint NO
init_operation removed_rows bigint NO
init_operation removed_bytes bigint NO
inode_value_block first_ordinal bigint NO
inode_value_block last_ordinal bigint NO
inode_value_block body bytea NO
kept_node layer_stack_id bytea NO
kept_node kind text NO
kept_node node_id bytea NO
kept_node serial bigint NO
layer layer_id bytea NO
layer layer_stack_id bytea NO
layer parent_layer_id bytea YES
layer root_id bytea NO
layer source_branch_id bytea YES
layer logical bigint NO
layer overwrote bytea NO
layer source_commit_id bytea YES
layer history_serial bigint NO
layer_lineage layer_id bytea NO
layer_lineage layer_stack_id bytea NO
layer_lineage height bigint NO
layer_lineage overwrote_count bigint NO
layer_stack layer_stack_id bytea NO
layer_stack name text NO
layer_stack scope_id bytea NO
layer_stack profile_id bytea NO
layer_stack head_layer_id bytea NO
layer_stack_retired layer_stack_id bytea NO
layer_stack_retired deleted_ms bigint NO
object_location pack_id bigint NO
object_location canonical_length integer NO
object_location role smallint NO
object_location group_number smallint NO
object_location record_number smallint NO
object_location object_id bytea NO
pack pack_id bigint NO
pack domain bigint NO
pack digest bytea NO
pack length bigint NO
pack body bytea NO
path_removed layer_stack_id bytea NO
path_removed path_key bytea NO
path_removed path bytea NO
path_removed frontier_serial bigint NO
path_removed removed_ms bigint NO
project_epoch layer_stack_id bytea NO
project_epoch epoch bigint NO
reach key bytea NO
reach x bigint NO
reach height bigint NO
scope_allocator scope_id bytea NO
scope_allocator highwater bigint NO
scope_allocator authority_id bytea NO
store_lane id bigint NO
store_policy id bigint NO
store_policy schema_version bigint NO
store_policy format_profile bigint NO
store_policy small_file_threshold_bytes bigint NO
store_policy whole_file_delta_max_depth bigint NO
store_policy chunk_delta_max_depth bigint NO
store_policy metadata_delta_max_depth bigint NO
store_policy next_pack_id bigint NO
store_policy next_ordinal bigint NO
store_policy trace_salt bytea NO
store_totals id bigint NO
store_totals logical bigint NO
store_totals canonical bigint NO
store_totals stored bigint NO
store_totals metadata bigint NO
store_totals init_entries bigint NO
store_totals init_files bigint NO
store_totals init_bytes bigint NO
store_totals init_canonical bigint NO
store_totals init_stored bigint NO
store_totals init_metadata bigint NO
store_totals init_millis bigint NO
"#;
use super::{int, meta_columns, single, ACQUISITION, ACQUISITION_TABLES, SCHEMA, TABLES};
use crate::backend::{postgresql::session::connection::Session, records::BackendError};
use postgres::SimpleQueryRow;

fn projection() -> String {
    let base = TABLES.join(",");
    let mut base_columns = String::new();
    for line in COLUMNS.lines().filter(|line| !line.starts_with("init_")) {
        base_columns.push_str(line);
        base_columns.push('\n');
    }
    let mut full = [TABLES.as_slice(), ACQUISITION_TABLES.as_slice()].concat();
    full.sort_unstable();
    let full = full.join(",");
    let tables="SELECT string_agg(c.relname,',' ORDER BY c.relname) FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relkind IN ('r','p')";
    let columns="SELECT string_agg(c.relname||' '||a.attname||' '||pg_catalog.format_type(a.atttypid,a.atttypmod)||' '||CASE WHEN a.attnotnull THEN 'NO' ELSE 'YES' END||chr(10),'' ORDER BY c.relname,a.attnum) FROM pg_catalog.pg_attribute a JOIN pg_catalog.pg_class c ON c.oid=a.attrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=current_schema() AND c.relkind IN ('r','p') AND a.attnum>0 AND NOT a.attisdropped";
    let meta=meta_columns("").replace("encode(h.catalog_id,'hex')","encode(CASE WHEN octet_length(h.catalog_id)=32 THEN h.catalog_id END,'hex')").replace("encode(h.binding_key,'hex')","encode(CASE WHEN octet_length(h.binding_key) BETWEEN 1 AND 128 THEN h.binding_key END,'hex')");
    format!("SELECT CASE WHEN ({tables})='{base}' THEN 0 WHEN ({tables})='{full}' THEN 1 ELSE -1 END,CASE WHEN (({tables})='{base}' AND h.schema_source=$doctor_source${SCHEMA}$doctor_source$ AND h.schema_definition=$doctor_columns${base_columns}$doctor_columns$) OR (({tables})='{full}' AND h.schema_source=$doctor_source${SCHEMA}{ACQUISITION}$doctor_source$ AND h.schema_definition=$doctor_columns${COLUMNS}$doctor_columns$) THEN CASE WHEN h.schema_definition=({columns}) THEN 1 ELSE 0 END ELSE 0 END,{meta} AND ((({tables})='{base}' AND ({columns})=$doctor_columns${base_columns}$doctor_columns$) OR (({tables})='{full}' AND ({columns})=$doctor_columns${COLUMNS}$doctor_columns$))")
}

pub(super) fn row(session: &Session) -> Result<(SimpleQueryRow, bool), BackendError> {
    let sql = format!(
        "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY; SET LOCAL search_path TO {}; {}; COMMIT",
        session.settings.schema,
        projection()
    );
    let mut groups = session.simple(&sql, true)?;
    if groups.len() != 4 {
        return Err(BackendError::Integrity);
    }
    let row = single(groups.swap_remove(2))?;
    let acquisition = match int(&row, 0)? {
        0 => false,
        1 => true,
        _ => return Err(BackendError::Integrity),
    };
    if int(&row, 1)? != 1 {
        return Err(BackendError::Integrity);
    }
    Ok((row, acquisition))
}

pub(super) fn guard(
    tx: &crate::backend::postgresql::session::transaction::Transaction<'_>,
) -> Result<(), BackendError> {
    let rows = tx.query(&projection(), vec![])?;
    let row = rows
        .first()
        .filter(|_| rows.len() == 1)
        .ok_or(BackendError::Integrity)?;
    if !matches!(row.get::<i64>(0)?, 0 | 1) || row.get::<i64>(1)? != 1 {
        return Err(BackendError::Integrity);
    }
    Ok(())
}
