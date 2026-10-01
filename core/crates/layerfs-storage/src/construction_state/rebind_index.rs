//! Fixed owner-row rebind CAS; no data migration, dynamic population or schema rewrite.
use super::{rebind_context::Context, session::Resource};
use crate::{StorageError, StorageResult};
use rusqlite::Connection;
fn one(changed: usize) -> StorageResult<()> {
    if changed == 1 {
        Ok(())
    } else {
        Err(StorageError::Integrity("scratch rebind fixed-row CAS"))
    }
}
pub(crate) fn apply(c: &Connection, r: &Resource, new: &Context) -> StorageResult<()> {
    if new.plan.version() == 6 {
        one(c.execute("UPDATE session_owner SET header=?1 WHERE id=1 AND header=?2 AND scope IS NULL AND sealed=0 AND records=0 AND record_bytes=0 AND digest IS NULL",rusqlite::params![new.header.as_bytes(),r.header.as_ref().unwrap().as_bytes()])?)?;
        one(c.execute("UPDATE draft_owner SET scope=?1 WHERE id=1 AND scope=?2 AND stage=0 AND records=0 AND bytes=0 AND next_job=1 AND selected IS NULL",rusqlite::params![new.draft.as_ref().unwrap().scope.as_bytes().as_slice(),r.draft.as_ref().unwrap().scope.as_bytes().as_slice()])?)?;
    } else {
        let capacity = new.subject.as_ref().unwrap().capacity();
        one(c.execute("UPDATE session_owner SET header=?1,declared_roots=?2,declared_sites=?3,selected_bytes=?4,graph_records=?5,graph_record_bytes=?6 WHERE id=1 AND header=?7 AND scope IS NULL AND sealed=0 AND records=0 AND record_bytes=0 AND digest IS NULL",rusqlite::params![new.header.as_bytes(),new.plan.root_limit() as i64,new.plan.binding_limit() as i64,capacity.scratch_bytes() as i64,capacity.records() as i64,capacity.encoded_bytes() as i64,r.header.as_ref().unwrap().as_bytes()])?)?;
        one(c.execute("UPDATE site_owner SET scope=?1 WHERE id=1 AND scope=?2 AND stage=0 AND records=0 AND remaining=0 AND birth_digest IS NULL AND birth_max IS NULL AND final_digest IS NULL AND final_max IS NULL AND after_key IS NULL",rusqlite::params![new.sites.as_ref().unwrap().scope.as_bytes().as_slice(),r.sites.as_ref().unwrap().scope.as_bytes().as_slice()])?)?;
        one(c.execute("UPDATE graph_owner SET scope=?1 WHERE id=1 AND scope=?2 AND stage=0 AND nodes=0 AND edges=0 AND record_bytes=0 AND remaining_nodes=0 AND remaining_edges=0 AND adjacency_seal IS NULL AND proof_seal IS NULL AND max_node IS NULL AND max_edge IS NULL AND after_node IS NULL AND after_edge IS NULL AND seed_count=0",rusqlite::params![new.graph.as_ref().unwrap().scope.as_bytes().as_slice(),r.graph.as_ref().unwrap().scope.as_bytes().as_slice()])?)?;
        one(c.execute("UPDATE solver_owner SET scope=?1 WHERE id=1 AND scope=?2 AND current_serial=0 AND dfs_root_serial=0 AND next_discovery=1 AND scc_root_serial=0 AND scc_boundary=0 AND cumulative_pop_count=0 AND last_stack_discovery=0 AND any_seed=0 AND singleton_self_loop=0",rusqlite::params![new.graph.as_ref().unwrap().scope.as_bytes().as_slice(),r.graph.as_ref().unwrap().scope.as_bytes().as_slice()])?)?;
        if new.aliases.is_some() {
            one(c.execute("UPDATE alias_owner SET scope=?1 WHERE id=1 AND scope=?2 AND stage=0 AND members IS NULL AND sequence=0 AND facts=0 AND jobs=0 AND expanded=0 AND current_serial IS NULL AND current_sequence IS NULL AND remaining=0 AND after_serial IS NULL",rusqlite::params![new.aliases.as_ref().unwrap().scope.as_bytes().as_slice(),r.aliases.as_ref().unwrap().scope.as_bytes().as_slice()])?)?;
        }
    }
    verify(c, new)
}
pub(crate) fn verify(c: &Connection, new: &Context) -> StorageResult<()> {
    if new.draft.is_some() {
        for table in [
            "draft_headers",
            "draft_bodies",
            "draft_references",
            "draft_predecessors",
            "draft_counts",
            "draft_jobs",
            "draft_committed",
            "draft_emissions",
        ] {
            if !super::count_index::empty(c, table)? {
                return Err(StorageError::Integrity("scratch rebind nonempty drafts"));
            }
        }
    } else {
        for table in [
            "directory_roots",
            "binding_sites",
            "graph_nodes",
            "graph_edges",
        ] {
            if !super::count_index::empty(c, table)? {
                return Err(StorageError::Integrity(
                    "scratch rebind nonempty graph projection",
                ));
            }
        }
        if new.aliases.is_some() {
            for table in ["alias_facts", "alias_jobs"] {
                if !super::count_index::empty(c, table)? {
                    return Err(StorageError::Integrity("scratch rebind nonempty aliases"));
                }
            }
        }
        if new.facts.is_some() {
            for table in ["base_facts", "parent_eligibility"] {
                if !super::count_index::empty(c, table)? {
                    return Err(StorageError::Integrity("scratch rebind nonempty facts"));
                }
            }
        }
    }
    super::profile::readback(c, new.plan)?;
    super::index::verify_header(c, new.header.as_bytes(), None)?;
    if let Some(sites) = &new.sites {
        super::site_index::verify(c, sites)?;
    }
    if let Some(graph) = &new.graph {
        super::graph_index::verify(c, graph)?;
    }
    if let Some(aliases) = &new.aliases {
        super::alias_index::verify(c, aliases)?;
    }
    if let Some(facts) = &new.facts {
        super::fact_index::verify(c, facts)?;
    }
    if let Some(draft) = &new.draft {
        super::draft_index::verify(c, draft)?;
    }
    if new.plan.canonical_capacity().is_some() {
        super::count_index::deferred(c)?;
        super::release_index::deferred(c)?;
    }
    Ok(())
}
