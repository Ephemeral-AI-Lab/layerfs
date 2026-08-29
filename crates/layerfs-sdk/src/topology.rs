use crate::{BranchConnection, BranchConnectionId, ConnectionContext, StackConnectionId};

pub(crate) fn branch(
    context: &ConnectionContext,
    id: BranchConnectionId,
) -> Option<&BranchConnection> {
    context
        .branches
        .iter()
        .find(|connection| connection.id == id)
}

pub(crate) fn stack_dependents(context: &ConnectionContext, id: StackConnectionId) -> bool {
    context
        .branches
        .iter()
        .any(|branch| branch.parent == crate::connection::BranchParent::Stack(id))
}
