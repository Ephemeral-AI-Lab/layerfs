use crate::StackStore;
use layerfs_storage::{BranchId, FactKind, Result};

pub(crate) fn visit_commits(
    store: &StackStore,
    branch_id: BranchId,
    membership: &mut crate::branch_transfer::Membership<'_>,
    visitor: &mut dyn FnMut(&[layerfs_storage::CommitRecord]) -> Result<()>,
) -> Result<()> {
    if let Some(branch) = store.db.branch(branch_id)? {
        store
            .db
            .visit_commit_ancestry(branch.head_commit_id, None, &mut |_, page| {
                crate::branch_transfer::missing_page(
                    FactKind::Commit,
                    page,
                    |row| row.id.to_bytes(),
                    membership,
                    visitor,
                )
            })
    } else {
        store.parent.visit_commits(branch_id, membership, visitor)
    }
}
