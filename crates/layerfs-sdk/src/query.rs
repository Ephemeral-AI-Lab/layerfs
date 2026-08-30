use layerfs_storage::LayerStackId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueryKind {
    LayerStacks,
    AuthorityLayerStacks,
    Layers,
    AuthorityLayers,
    Branches,
    AuthorityBranches,
    Commits,
    AuthorityCommits,
    Workspaces,
    Monitor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Query {
    kind: QueryKind,
    after: Option<Vec<u8>>,
    limit: u16,
    layer_stack_id: Option<LayerStackId>,
}

impl Query {
    pub fn new(kind: QueryKind) -> Self {
        Self {
            kind,
            after: None,
            limit: 512,
            layer_stack_id: None,
        }
    }

    pub fn after(mut self, continuation: Vec<u8>) -> Self {
        self.after = Some(continuation);
        self
    }

    pub fn limit(mut self, limit: u16) -> Self {
        self.limit = limit;
        self
    }

    pub fn in_layer_stack(mut self, layer_stack_id: LayerStackId) -> Self {
        self.layer_stack_id = Some(layer_stack_id);
        self
    }

    pub const fn kind(&self) -> QueryKind {
        self.kind
    }

    pub fn continuation(&self) -> Option<&[u8]> {
        self.after.as_deref()
    }

    pub const fn page_limit(&self) -> u16 {
        self.limit
    }

    pub const fn layer_stack_id(&self) -> Option<LayerStackId> {
        self.layer_stack_id
    }
}

#[derive(Clone, Debug)]
pub enum QueryItem {
    LayerStack(layerfs_storage::LayerStackRecord),
    LayerStackScope(
        layerfs_storage::LayerStackFact,
        layerfs_storage::LayerStackScopeRecord,
    ),
    Branch(layerfs_storage::BranchRecord),
    BranchScope(
        layerfs_storage::BranchRecord,
        layerfs_storage::BranchScopeRecord,
    ),
    Fact(layerfs_storage::Fact),
    Workspace(WorkspaceQueryItem),
    Monitor(layerfs_monitor::MonitorSnapshot),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceQueryItem {
    pub summary: layerfs_workspace::WorkspaceSummary,
    pub layer_stack_id: LayerStackId,
    pub layer_stack_name: layerfs_storage::EntityName,
    pub branch_name: layerfs_storage::EntityName,
}

#[derive(Clone, Debug)]
pub struct QueryPage {
    pub items: Vec<QueryItem>,
    pub continuation: Option<Vec<u8>>,
}

impl QueryPage {
    pub fn into_next_query(self, prior: &Query) -> Option<Query> {
        self.continuation
            .map(|continuation| prior.clone().after(continuation))
    }
}
