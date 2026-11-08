//! Read-only Init and merge-working-state checks in the pinned audit view.
use super::{catalog::check, emit, transaction::View, Observation, Options, Outcome};
use crate::backend::records::{BackendError, Param};
use layerfs_content::{filesystem::PathName, inode_leaf::InodeKind};

pub(super) fn run(
    view: &dyn View,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    let tables = [
        view.has_table("init_operation")?,
        view.has_table("init_entry")?,
        view.has_table("init_host_file")?,
    ];
    if tables.iter().any(|v| *v) && !tables.iter().all(|v| *v) {
        emit(
            observer,
            "store.acquisition.schema",
            Outcome::Fail,
            "partial acquisition table set",
        );
    } else if tables[0] {
        init(view, options, observer)?;
    } else {
        emit(
            observer,
            "store.acquisition",
            Outcome::Skipped,
            "this schema has no acquisition working tables",
        );
    }
    merge(view, options, observer)
}
fn init(
    view: &dyn View,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    for(id,sql,problem)in[
        ("store.acquisition.operations","SELECT count(*) FROM init_operation WHERE operation_id<1 OR owner_epoch<1 OR phase NOT BETWEEN 1 AND 6 OR blob_length(source_device)!=8 OR blob_length(source_inode)!=8 OR blob_length(stack_id)!=16 OR blob_length(scope)!=32 OR held_rows<0 OR held_bytes<0 OR peak_rows<held_rows OR peak_bytes<held_bytes OR removed_rows<0 OR removed_bytes<0","malformed Init owner/phase/identity/accounting rows"),
        ("store.acquisition.owners","SELECT count(*) FROM init_entry e LEFT JOIN init_operation o ON o.operation_id=e.operation_id WHERE o.operation_id IS NULL","Init entries without their owning operation"),
        ("store.acquisition.host_owners","SELECT count(*) FROM init_host_file f LEFT JOIN init_operation o ON o.operation_id=f.operation_id WHERE o.operation_id IS NULL","Init host identities without their owning operation"),
        ("store.acquisition.entry_shape","SELECT count(*) FROM init_entry WHERE operation_id<1 OR parent_position< -1 OR blob_length(name) NOT BETWEEN 0 AND 255 OR position<0 OR kind NOT BETWEEN 1 AND 3 OR canonical_position<1 OR blob_length(metadata_root)!=32 OR blob_length(content_root)!=32 OR blob_length(host_path) NOT BETWEEN 1 AND 4096 OR blob_length(host_identity)!=60","malformed Init entry scalar/blob fields"),
        ("store.acquisition.root_shape","SELECT count(*) FROM init_entry WHERE (parent_position=-1 AND (blob_length(name)!=0 OR position IS NULL OR position!=0 OR kind!=2)) OR (parent_position>=0 AND (blob_length(name)=0 OR position=0))","invalid Init source-root/name/position shapes"),
        ("store.acquisition.kind_shape","SELECT count(*) FROM init_entry WHERE (kind=1 AND (host_path IS NOT NULL OR (position IS NULL AND (host_identity IS NULL OR canonical_position IS NOT NULL)) OR (position IS NOT NULL AND (host_identity IS NOT NULL OR canonical_position IS NULL)) OR content_root IS NOT NULL)) OR (kind=2 AND (host_path IS NULL OR host_identity IS NOT NULL OR canonical_position IS NOT NULL)) OR (kind=3 AND (host_path IS NOT NULL OR host_identity IS NOT NULL OR canonical_position IS NOT NULL OR content_root IS NULL))","Init kind/placement/host/root shapes disagree"),
        ("store.acquisition.host_shape","SELECT count(*) FROM init_host_file WHERE operation_id<1 OR canonical_position<1 OR blob_length(device)!=8 OR blob_length(inode)!=8 OR blob_length(evidence)!=44 OR blob_length(host_path) NOT BETWEEN 1 AND 4096 OR aliases<0 OR blob_length(file_root)!=32","malformed Init canonical host-file rows"),
        ("store.acquisition.parent","SELECT count(*) FROM init_entry e JOIN init_operation o ON o.operation_id=e.operation_id LEFT JOIN init_entry p ON p.operation_id=e.operation_id AND p.kind=2 AND p.position=e.parent_position WHERE o.phase<=3 AND e.parent_position>=0 AND (p.operation_id IS NULL OR (e.position IS NOT NULL AND e.parent_position>=e.position))","active Init entries have absent/nonpreceding directory parent"),
        ("store.acquisition.canonical","SELECT count(*) FROM init_entry e JOIN init_operation o ON o.operation_id=e.operation_id LEFT JOIN init_host_file f ON f.operation_id=e.operation_id AND f.canonical_position=e.canonical_position WHERE o.phase<=3 AND e.kind=1 AND e.position IS NOT NULL AND (f.operation_id IS NULL OR e.canonical_position>e.position)","active Init positioned files have invalid canonical host identity"),
        ("store.acquisition.positions","SELECT count(*) FROM (SELECT e.operation_id,e.position FROM init_entry e JOIN init_operation o ON o.operation_id=e.operation_id WHERE o.phase<=3 AND e.position IS NOT NULL GROUP BY e.operation_id,e.position HAVING count(*)!=1) bad","active Init entry positions are reused"),
        ("store.acquisition.aliases","SELECT count(*) FROM init_host_file f JOIN init_operation o ON o.operation_id=f.operation_id WHERE o.phase<=3 AND f.aliases!=(SELECT count(*)-1 FROM init_entry e WHERE e.operation_id=f.operation_id AND e.kind=1 AND e.canonical_position=f.canonical_position)","active Init alias counts disagree with positioned bindings"),
        ("store.acquisition.host_identity","SELECT count(*) FROM (SELECT operation_id,device,inode FROM init_host_file GROUP BY operation_id,device,inode HAVING count(*)!=1) bad","duplicate Init canonical host identities"),
        ("store.acquisition.charges","SELECT count(*) FROM init_operation o WHERE o.held_rows!=(SELECT count(*) FROM init_entry e WHERE e.operation_id=o.operation_id)+(SELECT count(*) FROM init_host_file f WHERE f.operation_id=o.operation_id) OR o.held_bytes!=(SELECT COALESCE(sum(blob_length(name)+COALESCE(blob_length(host_path),0)+COALESCE(blob_length(host_identity),0)+32+COALESCE(blob_length(content_root),0)),0) FROM init_entry e WHERE e.operation_id=o.operation_id)+(SELECT COALESCE(sum(blob_length(host_path)+60+COALESCE(blob_length(file_root),0)),0) FROM init_host_file f WHERE f.operation_id=o.operation_id)","Init held-row/payload charges disagree with actual retained working rows"),
    ]{check(view,options,observer,id,sql,problem)?;}
    // Metadata is a private inline portable-metadata blob, not an object CID.
    let (mut operation, mut parent, mut name, mut checked, mut failed) =
        (0i64, -2i64, Vec::new(), 0u64, 0u64);
    loop {
        options.check()?;
        let rows=view.query("SELECT operation_id,parent_position,name,kind,CASE WHEN blob_length(metadata_root)=32 THEN metadata_root END FROM init_entry WHERE blob_length(name) BETWEEN 0 AND 255 AND (operation_id,parent_position,name)>(?1,?2,?3) ORDER BY operation_id,parent_position,name LIMIT 64",vec![Param::I64(operation),Param::I64(parent),Param::Bytes(name.clone())])?;
        if rows.is_empty() {
            break;
        }
        emit(
            observer,
            "store.acquisition.progress",
            Outcome::Pass,
            "checking next bounded Init metadata/name window",
        );
        for row in rows {
            options.check()?;
            operation = row.get(0)?;
            parent = row.get(1)?;
            name = row.get(2)?;
            checked += 1;
            let result = (|| -> Result<(), BackendError> {
                if parent != -1 {
                    PathName::from_bytes(&name).map_err(|_| BackendError::Integrity)?;
                }
                let kind = u8::try_from(row.get::<i64>(3)?)
                    .ok()
                    .and_then(|v| InodeKind::from_code(v).ok())
                    .ok_or(BackendError::Integrity)?;
                crate::backend::sqlite::acquisition::metadata_parse(&row.get::<Vec<u8>>(4)?, kind)?;
                Ok(())
            })();
            if result.is_err() {
                failed += 1;
                emit(observer,"store.acquisition.metadata",Outcome::Fail,format!("Init operation {operation}, parent {parent}: invalid name or inline portable metadata"));
            }
        }
    }
    emit(observer,"store.acquisition.metadata.summary",if failed==0{Outcome::Pass}else{Outcome::Fail},format!("{checked} Init entry metadata/name rows decoded; {failed} malformed; oversized names counted structurally without extraction"));
    emit(observer,"store.acquisition.roots",Outcome::Pass,"Init roots may precede Save publication or survive interrupted cleanup; no fabricated dangling-object failure; existing winning objects receive the selected canonical audit");
    Ok(())
}
fn merge(
    view: &dyn View,
    options: Options,
    observer: &mut dyn FnMut(Observation),
) -> Result<(), BackendError> {
    let owner = view.has_table("merge_operation")?;
    let records = view.has_table("merge_record")?;
    if owner != records {
        emit(
            observer,
            "store.merge.schema",
            Outcome::Fail,
            "partial persisted merge-working table set",
        );
        return Ok(());
    }
    if !owner {
        emit(observer,"store.merge",Outcome::Skipped,"no persisted merge-working tables in this Store schema; another PostgreSQL session's private temporary tables are outside this Store view");
        return Ok(());
    }
    for(id,sql,problem)in[
        ("store.merge.owner","SELECT count(*) FROM merge_operation WHERE operation<1 OR blob_length(token)!=16 OR touched<0","malformed merge-working owners"),
        ("store.merge.records","SELECT count(*) FROM merge_record WHERE operation<1 OR kind<0 OR kind>4294967295 OR blob_length(key)!=32 OR blob_length(value) NOT BETWEEN 0 AND 65536","malformed merge-working keys or values beyond the existing 64KiB apply envelope"),
        ("store.merge.references","SELECT count(*) FROM merge_record r LEFT JOIN merge_operation o ON o.operation=r.operation WHERE o.operation IS NULL","merge-working records without their owner"),
    ]{check(view,options,observer,id,sql,problem)?;}
    emit(observer,"store.merge.values",Outcome::Pass,"merge values are opaque operation-private indexed records; structural size/key/owner contract checked without interpreting private algorithm state or expiring owners");
    Ok(())
}
