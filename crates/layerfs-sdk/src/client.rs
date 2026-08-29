use crate::branch::Parent;
use crate::{
    BranchConnectionId, ConnectionContext, LayerConnection, SdkError, StackConnectionId,
    StoreLocation,
};
use layerfs_storage::{
    BranchId, CommitId, Fact, FactKind, LayerHistoryId, LayerId, StackHistoryId, StackId,
    StorageError,
};

#[derive(Default)]
pub struct Client {
    context: Option<ConnectionContext>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryScope {
    Layer,
    Stack(StackConnectionId),
    Branch(BranchConnectionId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryCursor(Vec<u8>);

impl std::fmt::Display for QueryCursor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FactId {
    LayerHistory(LayerHistoryId),
    Layer(LayerId),
    StackHistory(StackHistoryId),
    Stack(StackId),
    Branch(BranchId),
    Commit(CommitId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Query {
    Topology,
    Page {
        scope: QueryScope,
        kind: FactKind,
        after: Option<QueryCursor>,
        limit: u16,
    },
    Fact {
        scope: QueryScope,
        id: FactId,
    },
    CommitDiff {
        connection: BranchConnectionId,
        left: CommitId,
        right: CommitId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryPage {
    pub facts: Vec<Fact>,
    pub next: Option<QueryCursor>,
}

#[derive(Clone)]
pub enum QueryResult {
    Topology(ConnectionContext),
    Page(QueryPage),
    Fact(Option<Fact>),
    CommitDiff(Vec<layerfs_branch_store::RootDiff>),
}

impl std::fmt::Debug for QueryResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Topology(value) => formatter
                .debug_struct("Topology")
                .field("stacks", &value.stacks.len())
                .field("branches", &value.branches.len())
                .finish(),
            Self::Page(value) => formatter.debug_tuple("Page").field(value).finish(),
            Self::Fact(value) => formatter.debug_tuple("Fact").field(value).finish(),
            Self::CommitDiff(value) => formatter.debug_tuple("CommitDiff").field(value).finish(),
        }
    }
}

impl Client {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_layer(&mut self, location: StoreLocation) -> Result<LayerConnection, SdkError> {
        self.install_layer(crate::layer::create(location)?)
    }

    pub fn connect_layer(&mut self, location: StoreLocation) -> Result<LayerConnection, SdkError> {
        self.install_layer(crate::layer::connect(location)?)
    }

    fn install_layer(&mut self, layer: LayerConnection) -> Result<LayerConnection, SdkError> {
        if self.context.is_some() {
            return Err(SdkError::ActiveDependents);
        }
        self.context = Some(ConnectionContext {
            layer: layer.clone(),
            stacks: Vec::new(),
            branches: Vec::new(),
            active_stack: None,
            active_branch: None,
        });
        Ok(layer)
    }

    pub fn create_stack(&mut self, location: StoreLocation) -> Result<StackConnectionId, SdkError> {
        self.open_stack(location, true)
    }

    pub fn connect_stack(
        &mut self,
        location: StoreLocation,
    ) -> Result<StackConnectionId, SdkError> {
        self.open_stack(location, false)
    }

    fn open_stack(
        &mut self,
        location: StoreLocation,
        create: bool,
    ) -> Result<StackConnectionId, SdkError> {
        let context = self.context.as_mut().ok_or(SdkError::MissingLayer)?;
        let stack = if create {
            crate::stack::create(location, &context.layer)?
        } else {
            crate::stack::connect(location, &context.layer)?
        };
        let id = stack.id;
        context.stacks.push(stack);
        context.active_stack = Some(id);
        Ok(id)
    }

    pub fn create_branch(
        &mut self,
        location: StoreLocation,
    ) -> Result<BranchConnectionId, SdkError> {
        self.open_branch(location, true)
    }

    pub fn connect_branch(
        &mut self,
        location: StoreLocation,
    ) -> Result<BranchConnectionId, SdkError> {
        self.open_branch(location, false)
    }

    fn open_branch(
        &mut self,
        location: StoreLocation,
        create: bool,
    ) -> Result<BranchConnectionId, SdkError> {
        let context = self.context.as_mut().ok_or(SdkError::MissingLayer)?;
        let parent = match context.active_stack {
            Some(id) => Parent::Stack(
                context
                    .stacks
                    .iter()
                    .find(|stack| stack.id == id)
                    .ok_or(SdkError::MissingStack)?,
            ),
            None => Parent::Layer(&context.layer),
        };
        let branch = if create {
            crate::branch::create(location, parent)?
        } else {
            crate::branch::connect(location, parent)?
        };
        let id = branch.id;
        context.branches.push(branch);
        context.active_branch = Some(id);
        Ok(id)
    }

    pub fn context(&self) -> Result<&ConnectionContext, SdkError> {
        self.context.as_ref().ok_or(SdkError::MissingLayer)
    }

    pub fn subsystems(
        &self,
        runtime_root: impl AsRef<std::path::Path>,
    ) -> Result<
        (
            std::sync::Arc<layerfs_workspace::Workspaces>,
            std::sync::Arc<layerfs_monitor::Monitor>,
        ),
        SdkError,
    > {
        let context = self.context()?;
        let workspaces = std::sync::Arc::new(layerfs_workspace::Workspaces::new(
            runtime_root.as_ref().join("workspaces"),
            context.branches.iter().map(|branch| branch.store.clone()),
        )?);
        let routes = context.branches.iter().map(|branch| {
            let stack = match branch.parent {
                crate::connection::BranchParent::Layer(_) => None,
                crate::connection::BranchParent::Stack(id) => context
                    .stacks
                    .iter()
                    .find(|stack| stack.id == id)
                    .map(|stack| stack.store.clone()),
            };
            layerfs_monitor::MonitoredRoute::new(
                branch.store.clone(),
                stack,
                context.layer.store.clone(),
            )
        });
        let monitor = std::sync::Arc::new(
            layerfs_monitor::Monitor::new(
                runtime_root.as_ref().join("monitor"),
                routes,
                workspaces.clone(),
            )
            .map_err(SdkError::Monitor)?,
        );
        Ok((workspaces, monitor))
    }

    pub fn use_stack(&mut self, id: Option<StackConnectionId>) -> Result<(), SdkError> {
        let context = self.context.as_mut().ok_or(SdkError::MissingLayer)?;
        if id.is_some_and(|id| context.stacks.iter().all(|stack| stack.id != id)) {
            return Err(SdkError::MissingStack);
        }
        context.active_stack = id;
        context.active_branch = None;
        Ok(())
    }

    pub fn use_branch(&mut self, id: BranchConnectionId) -> Result<(), SdkError> {
        let context = self.context.as_mut().ok_or(SdkError::MissingLayer)?;
        if context.branches.iter().all(|branch| branch.id != id) {
            return Err(SdkError::MissingBranch);
        }
        context.active_branch = Some(id);
        Ok(())
    }

    pub fn disconnect_branch(&mut self, id: BranchConnectionId) -> Result<(), SdkError> {
        let context = self.context.as_mut().ok_or(SdkError::MissingLayer)?;
        let position = context
            .branches
            .iter()
            .position(|branch| branch.id == id)
            .ok_or(SdkError::MissingBranch)?;
        context.branches.remove(position);
        if context.active_branch == Some(id) {
            context.active_branch = None;
        }
        Ok(())
    }

    pub fn disconnect_stack(&mut self, id: StackConnectionId) -> Result<(), SdkError> {
        let context = self.context.as_mut().ok_or(SdkError::MissingLayer)?;
        if crate::topology::stack_dependents(context, id) {
            return Err(SdkError::ActiveDependents);
        }
        let position = context
            .stacks
            .iter()
            .position(|stack| stack.id == id)
            .ok_or(SdkError::MissingStack)?;
        context.stacks.remove(position);
        if context.active_stack == Some(id) {
            context.active_stack = None;
        }
        Ok(())
    }

    pub fn disconnect_layer(&mut self) -> Result<(), SdkError> {
        let context = self.context.as_ref().ok_or(SdkError::MissingLayer)?;
        if !context.stacks.is_empty() || !context.branches.is_empty() {
            return Err(SdkError::ActiveDependents);
        }
        self.context = None;
        Ok(())
    }

    pub fn query(&self, query: Query) -> Result<QueryResult, SdkError> {
        let context = self.context()?;
        Ok(match query {
            Query::Topology => QueryResult::Topology(context.clone()),
            Query::Page {
                scope,
                kind,
                after,
                limit,
            } => {
                let after = after.as_ref().map(|cursor| cursor.0.as_slice());
                let facts = match scope {
                    QueryScope::Layer => context.layer.store.fact_page(kind, after, limit)?,
                    QueryScope::Stack(id) => {
                        stack(context, id)?.store.fact_page(kind, after, limit)?
                    }
                    QueryScope::Branch(id) => crate::topology::branch(context, id)
                        .ok_or(SdkError::MissingBranch)?
                        .store
                        .fact_page(kind, after, limit)?,
                };
                let next = (facts.len() == usize::from(limit))
                    .then(|| facts.last().map(|fact| QueryCursor(fact.id())))
                    .flatten();
                QueryResult::Page(QueryPage { facts, next })
            }
            Query::Fact { scope, id } => QueryResult::Fact(query_fact(context, scope, id)?),
            Query::CommitDiff {
                connection,
                left,
                right,
            } => QueryResult::CommitDiff(
                crate::topology::branch(context, connection)
                    .ok_or(SdkError::MissingBranch)?
                    .store
                    .commit_diff(left, right)?,
            ),
        })
    }
}

fn stack(
    context: &ConnectionContext,
    id: StackConnectionId,
) -> Result<&crate::StackConnection, SdkError> {
    context
        .stacks
        .iter()
        .find(|stack| stack.id == id)
        .ok_or(SdkError::MissingStack)
}

fn query_fact(
    context: &ConnectionContext,
    scope: QueryScope,
    id: FactId,
) -> Result<Option<Fact>, SdkError> {
    let fact = match (scope, id) {
        (QueryScope::Layer, FactId::LayerHistory(id)) => context
            .layer
            .store
            .layer_history(id)?
            .map(Fact::LayerHistory),
        (QueryScope::Layer, FactId::Layer(id)) => context.layer.store.layer(id)?.map(Fact::Layer),
        (QueryScope::Stack(connection), FactId::StackHistory(id)) => stack(context, connection)?
            .store
            .stack_history(id)?
            .map(Fact::StackHistory),
        (QueryScope::Stack(connection), FactId::Stack(id)) => stack(context, connection)?
            .store
            .stack(id)?
            .map(Fact::Stack),
        (QueryScope::Branch(connection), FactId::Branch(id)) => {
            crate::topology::branch(context, connection)
                .ok_or(SdkError::MissingBranch)?
                .store
                .branch(id)?
                .map(Fact::Branch)
        }
        (QueryScope::Branch(connection), FactId::Commit(id)) => {
            crate::topology::branch(context, connection)
                .ok_or(SdkError::MissingBranch)?
                .store
                .commit_record(id)?
                .map(Fact::Commit)
        }
        _ => return Err(SdkError::Storage(StorageError::WrongSourceRoute)),
    };
    Ok(fact)
}
