//! Profile4/5 fixed owner rows reset only after checked empty projections.

use rusqlite::Connection;

use super::session::Resource;
use super::{graph_index, profile, root_retire, site_index};
use crate::error::{StorageError, StorageResult};

fn empty(connection: &Connection, resource: &Resource) -> StorageResult<()> {
    root_retire::empty(connection)?;
    if !site_index::empty(connection)? || !graph_index::empty(connection)? {
        return Err(StorageError::Integrity(
            "construction reset nonempty state/index",
        ));
    }
    if resource.aliases.is_some()
        && (!super::alias_index::empty(connection, "alias_facts")?
            || !super::alias_index::empty(connection, "alias_jobs")?)
    {
        return Err(StorageError::Integrity(
            "construction reset nonempty aliases",
        ));
    }
    if resource.facts.is_some() {
        for table in ["base_facts", "parent_eligibility"] {
            let count: i64 =
                connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?;
            if count != 0 {
                return Err(StorageError::Integrity("construction reset nonempty facts"));
            }
        }
    }
    if resource.canonical_capacity.is_some() {
        for table in [
            "canonical_counts",
            "zero_seeds",
            "release_jobs",
            "release_frames",
        ] {
            if !super::count_index::empty(connection, table)? {
                return Err(StorageError::Integrity(
                    "construction reset nonempty canonical state",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn commit(connection: &Connection, resource: &Resource) -> StorageResult<()> {
    if !connection.is_autocommit() {
        return Err(StorageError::Integrity(
            "construction reset open transaction",
        ));
    }
    empty(connection, resource)?;
    crate::sqlite::write::begin_immediate(connection)?;
    let result = (|| {
        resource.verify()?;
        empty(connection, resource)?;
        for sql in [
            "UPDATE session_owner SET scope=NULL,sealed=0,records=0,record_bytes=0,digest=NULL WHERE id=1 AND sealed=1",
            "UPDATE site_owner SET stage=0,records=0,remaining=0,birth_digest=NULL,birth_max=NULL,final_digest=NULL,final_max=NULL,after_key=NULL WHERE id=1 AND stage=4 AND remaining=0",
            "UPDATE graph_owner SET stage=0,nodes=0,edges=0,record_bytes=0,source_multiplicity=x'0000000000000000',remaining_nodes=0,remaining_edges=0,adjacency_seal=NULL,proof_seal=NULL,max_node=NULL,max_edge=NULL,after_node=NULL,after_edge=NULL,seed_count=0 WHERE id=1 AND stage=7 AND remaining_nodes=0 AND remaining_edges=0",
            "UPDATE solver_owner SET current_serial=0,dfs_root_serial=0,next_discovery=1,scc_root_serial=0,scc_boundary=0,cumulative_pop_count=0,last_stack_discovery=0,any_seed=0,singleton_self_loop=0 WHERE id=1",
        ] {
            if connection.execute(sql,[])? != 1 {
                return Err(StorageError::Integrity("construction reset fixed-row acknowledgement"));
            }
        }
        if resource.aliases.is_some()
            && connection.execute("UPDATE alias_owner SET members=NULL,stage=0,sequence=0,facts=0,jobs=0,expanded=0,current_serial=NULL,current_sequence=NULL,progress=?1,remaining=0,after_serial=NULL WHERE id=1 AND stage=4 AND remaining=0",[[0u8;264].as_slice()])?!=1
        {
            return Err(StorageError::Integrity("construction reset alias fixed-row acknowledgement"));
        }
        if resource.facts.is_some() {
            for (table, stage) in [(17, 4), (18, 5)] {
                if connection.execute("UPDATE fact_owner SET scope=NULL,stage=0,records=0,bound=0,record_bytes=0,remaining=0,after_serial=NULL,maximum=NULL,digest=NULL WHERE table_id=?1 AND stage=?2 AND remaining=0",rusqlite::params![table,stage])?!=1 {
                    return Err(StorageError::Integrity("construction reset fact fixed-row acknowledgement"));
                }
            }
        }
        if resource.canonical_capacity.is_some() {
            if connection.execute("UPDATE count_owner SET scope=NULL,stage=0,records=0,touched=0,maximum=NULL,zeros=0,zero_maximum=NULL,count_remaining=0,zero_remaining=0,after_count=NULL,after_zero=NULL,effects=NULL,final_seal=NULL,seeds=NULL WHERE id=1 AND stage=6 AND count_remaining=0 AND zero_remaining=0",[])? != 1 {
                return Err(StorageError::Integrity("construction reset count fixed-row acknowledgement"));
            }
            if resource.releasing.is_some() {
                if connection.execute("UPDATE release_owner SET scope=NULL,stage=0,seeds=NULL,seeded=0,seed_after=NULL,sequence=0,pending=0,current_key=NULL,current_value=NULL,frames=0,completed=0,directories=0,maximum_depth=0 WHERE id=1 AND stage=5 AND pending=0 AND current_key IS NULL AND current_value IS NULL AND frames=0",[])? != 1 {
                    return Err(StorageError::Integrity("construction reset release fixed-row acknowledgement"));
                }
            } else {
                super::release_index::deferred(connection)?;
            }
        }
        verify_rows(connection, resource)?;
        Ok(())
    })();
    profile::finish_write_guarded(connection, result, resource.engine)
}

pub(crate) fn verify(connection: &Connection, resource: &Resource) -> StorageResult<()> {
    if !connection.is_autocommit() {
        return Err(StorageError::Integrity(
            "construction known-clean open transaction",
        ));
    }
    verify_rows(connection, resource)?;
    empty(connection, resource)
}

/// Recheck the known-clean old context within an already owned rebind transaction.
pub(crate) fn verify_transaction(
    connection: &Connection,
    resource: &Resource,
) -> StorageResult<()> {
    resource.check_engine()?;
    if connection.is_autocommit() {
        return Err(StorageError::Integrity(
            "construction rebind transaction unavailable",
        ));
    }
    verify_rows(connection, resource)?;
    empty(connection, resource)
}

fn verify_rows(connection: &Connection, resource: &Resource) -> StorageResult<()> {
    let graph = resource
        .graph
        .as_ref()
        .ok_or(StorageError::Integrity("construction reset graph owner"))?;
    let sites = resource
        .sites
        .as_ref()
        .ok_or(StorageError::Integrity("construction reset sites owner"))?;
    let header = resource
        .header
        .as_ref()
        .ok_or(StorageError::Integrity("construction reset native header"))?;
    let capacity = graph.scope.capacity();
    let owner: i64 = connection.query_row(
        "SELECT COUNT(*) FROM session_owner WHERE id=1 AND header=?1 AND scope IS NULL AND sealed=0 AND records=0 AND record_bytes=0 AND digest IS NULL AND declared_roots=?2 AND declared_sites=?3 AND selected_bytes=?4 AND graph_records=?5 AND graph_record_bytes=?6",
        rusqlite::params![header.as_bytes(),sites.declared_roots as i64,sites.declared_sites as i64,
            capacity.scratch_bytes() as i64,capacity.records() as i64,capacity.encoded_bytes() as i64],|row|row.get(0))?;
    let site: i64 = connection.query_row(
        "SELECT COUNT(*) FROM site_owner WHERE id=1 AND scope=?1 AND stage=0 AND records=0 AND remaining=0 AND birth_digest IS NULL AND birth_max IS NULL AND final_digest IS NULL AND final_max IS NULL AND after_key IS NULL",
        [sites.scope.as_bytes().as_slice()],|row|row.get(0))?;
    let graph_row: i64 = connection.query_row(
        "SELECT COUNT(*) FROM graph_owner WHERE id=1 AND scope=?1 AND stage=0 AND nodes=0 AND edges=0 AND record_bytes=0 AND source_multiplicity=x'0000000000000000' AND remaining_nodes=0 AND remaining_edges=0 AND adjacency_seal IS NULL AND proof_seal IS NULL AND max_node IS NULL AND max_edge IS NULL AND after_node IS NULL AND after_edge IS NULL AND seed_count=0",
        [graph.scope.as_bytes().as_slice()],|row|row.get(0))?;
    let solver: i64 = connection.query_row(
        "SELECT COUNT(*) FROM solver_owner WHERE id=1 AND scope=?1 AND current_serial=0 AND dfs_root_serial=0 AND next_discovery=1 AND scc_root_serial=0 AND scc_boundary=0 AND cumulative_pop_count=0 AND last_stack_discovery=0 AND any_seed=0 AND singleton_self_loop=0",
        [graph.scope.as_bytes().as_slice()],|row|row.get(0))?;
    if (owner, site, graph_row, solver) != (1, 1, 1, 1) {
        return Err(StorageError::Integrity(
            "construction known-clean fixed rows",
        ));
    }
    if let Some(aliases) = &resource.aliases {
        let count:i64=connection.query_row("SELECT COUNT(*) FROM alias_owner WHERE id=1 AND scope=?1 AND members IS NULL AND stage=0 AND sequence=0 AND facts=0 AND jobs=0 AND expanded=0 AND current_serial IS NULL AND current_sequence IS NULL AND progress=?2 AND remaining=0 AND after_serial IS NULL",rusqlite::params![aliases.scope.as_bytes().as_slice(),[0u8;264].as_slice()],|row|row.get(0))?;
        if count != 1 {
            return Err(StorageError::Integrity(
                "construction known-clean alias fixed row",
            ));
        }
    }
    if resource.facts.is_some() {
        let rows:i64=connection.query_row("SELECT COUNT(*) FROM fact_owner WHERE table_id IN(17,18) AND scope IS NULL AND stage=0 AND records=0 AND bound=0 AND record_bytes=0 AND remaining=0 AND after_serial IS NULL AND maximum IS NULL AND digest IS NULL",[],|row|row.get(0))?;
        if rows != 2 {
            return Err(StorageError::Integrity(
                "construction known-clean fact fixed rows",
            ));
        }
    }
    if resource.canonical_capacity.is_some() {
        super::count_index::deferred(connection)?;
        super::release_index::deferred(connection)?;
    }
    Ok(())
}
