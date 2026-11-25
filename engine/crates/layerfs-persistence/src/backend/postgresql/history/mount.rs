//! The statements of a Workspace mount and unmount on PostgreSQL (#768),
//! prepared when a session opens so neither call prepares one.
use super::statements;
use crate::backend::postgresql::{
    metadata::{chains, reads},
    objects::read,
    session::{connection::Session, prepare::fenced},
};
use crate::backend::records::BackendError;

/// The writer's statements of a mount's admission and of the owner's
/// release: the session keys taken and given back, the Branch probe, the
/// conflict scan, the intent insert and the release's `DELETE`.
fn writer_texts(schema: &str) -> Result<Vec<String>, BackendError> {
    let mut texts = Vec::new();
    for key in [
        "gc_lock_session",
        "gc_unlock_session",
        "gc_intent_release_owner",
    ] {
        let sql = statements::bare(key).ok_or(BackendError::Integrity)?;
        texts.push(sql.replace("{s}", schema));
    }
    for key in [
        "branch_branch_by_id",
        "gc_intent_ops_branch",
        "gc_intent_insert",
    ] {
        texts.push(statements::statement(key)?.to_owned());
    }
    Ok(texts)
}

/// A Workspace reader's statements at mount, each a fenced autocommit read:
/// the snapshot by Branch id or by name, the bound node's markers, and the
/// first demand of the root (the chain locate, the pack ranges and the inode
/// value blocks).
fn reader_texts(schema: &str) -> Result<Vec<String>, BackendError> {
    let mut texts = Vec::new();
    for key in [
        "branch_snapshot",
        "branch_snapshot_named",
        "paths_node_markers",
    ] {
        let sql = statements::qualified(key).ok_or(BackendError::Integrity)?;
        texts.push(fenced(&sql.replace("{s}", schema), schema));
    }
    texts.push(fenced(&chains::ROOTS.replace("{schema}", schema), schema));
    texts.push(fenced(&read::ranges_sql(schema), schema));
    texts.push(fenced(&reads::values_sql(schema), schema));
    Ok(texts)
}

impl Session {
    /// Prepares the writer's mount and unmount statements in one flight.
    pub(crate) fn prepare_mount_writes(&self) -> Result<(), BackendError> {
        self.prepare(&writer_texts(&self.settings.schema)?)
    }

    /// Prepares a Workspace reader's mount statements in one flight.
    pub(crate) fn prepare_mount_reads(&self) -> Result<(), BackendError> {
        self.prepare(&reader_texts(&self.settings.schema)?)
    }
}
