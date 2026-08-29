use layerfs_sdk::{
    ExecutionId, FactId, FactKind, MonitorScope, OperationReceipt, OutputPage, WorkspaceDetail,
    WorkspaceDiff, WorkspaceSessionId, WorkspaceSummary,
};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ViewQuery {
    Topology,
    Workspaces,
    Workspace(WorkspaceSessionId),
    WorkspaceDiff(WorkspaceSessionId),
    Output {
        execution_id: ExecutionId,
        after: u64,
        follow: bool,
    },
    Monitor(MonitorScope),
    Store(StoreQuery),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreScope {
    Layer,
    Stack(PathBuf),
    Branch(PathBuf),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreQuery {
    Page {
        scope: StoreScope,
        kind: FactKind,
        after: Option<String>,
        limit: u16,
    },
    Fact {
        scope: StoreScope,
        id: FactId,
    },
    CommitDiff {
        branch: PathBuf,
        left: layerfs_sdk::CommitId,
        right: layerfs_sdk::CommitId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewScope {
    Topology,
    Workspaces,
    Workspace,
    Output,
    Monitor,
    Store,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TopologyEntry {
    pub role: String,
    pub location: String,
    pub parent: Option<String>,
    pub active: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitDiffEntry {
    pub inode: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreFact {
    pub kind: FactKind,
    pub id: String,
    pub fields: Vec<(String, String)>,
}

impl StoreFact {
    pub fn fact(&self) -> crate::CliResult<layerfs_sdk::Fact> {
        use layerfs_sdk::{
            AddResultRecord, BranchRecord, CommitRecord, Fact, FactKind, LayerHistoryRecord,
            LayerRecord, StackHistoryRecord, StackRecord,
        };

        let field = |name| {
            self.fields
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| value.as_str())
                .ok_or(crate::CliError::Integrity)
        };
        let fact = match self.kind {
            FactKind::Commit => Fact::Commit(CommitRecord {
                id: parse_value(&self.id)?,
                root_id: parse_value(field("root_id")?)?,
                parent_id: parse_option_value(field("parent_id")?)?,
                merge_parent_id: parse_option_value(field("merge_parent_id")?)?,
            }),
            FactKind::Branch => Fact::Branch(BranchRecord {
                id: parse_value(&self.id)?,
                head_commit_id: parse_value(field("head_commit_id")?)?,
                base_id: parse_base_id(field("base_id")?)?,
            }),
            FactKind::LayerHistory => Fact::LayerHistory(LayerHistoryRecord {
                id: parse_value(&self.id)?,
                head_layer_id: parse_value(field("head_layer_id")?)?,
            }),
            FactKind::Layer => Fact::Layer(LayerRecord {
                id: parse_value(&self.id)?,
                history_id: parse_value(field("history_id")?)?,
                parent_id: parse_option_value(field("parent_id")?)?,
                root_id: parse_value(field("root_id")?)?,
            }),
            FactKind::StackHistory => Fact::StackHistory(StackHistoryRecord {
                id: parse_value(&self.id)?,
                base_layer_id: parse_value(field("base_layer_id")?)?,
                head_stack_id: parse_value(field("head_stack_id")?)?,
            }),
            FactKind::Stack => Fact::Stack(StackRecord {
                id: parse_value(&self.id)?,
                history_id: parse_value(field("history_id")?)?,
                parent_id: parse_option_value(field("parent_id")?)?,
                root_id: parse_value(field("root_id")?)?,
            }),
            FactKind::AddResult => Fact::AddResult(AddResultRecord {
                source_id: parse_source_id(&self.id)?,
                result_id: parse_result_id(field("result_id")?)?,
            }),
        };
        Ok(fact)
    }
}

fn parse_value<T: std::str::FromStr>(value: &str) -> crate::CliResult<T> {
    value.parse().map_err(|_| crate::CliError::Integrity)
}

fn parse_option_value<T: std::str::FromStr>(value: &str) -> crate::CliResult<Option<T>> {
    if value == "None" {
        return Ok(None);
    }
    let value = value
        .strip_prefix("Some(")
        .and_then(|value| value.strip_suffix(')'))
        .ok_or(crate::CliError::Integrity)?;
    parse_debug_id(value)
}

fn parse_debug_id<T: std::str::FromStr>(value: &str) -> crate::CliResult<Option<T>> {
    let (_, value) = value.split_once('(').ok_or(crate::CliError::Integrity)?;
    let value = value.strip_suffix(')').ok_or(crate::CliError::Integrity)?;
    value
        .parse()
        .map(Some)
        .map_err(|_| crate::CliError::Integrity)
}

fn parse_nested_id<T: std::str::FromStr>(value: &str, wrapper: &str) -> crate::CliResult<T> {
    let value = value
        .strip_prefix(wrapper)
        .and_then(|value| value.strip_prefix('('))
        .and_then(|value| value.strip_suffix(')'))
        .ok_or(crate::CliError::Integrity)?;
    value.parse().map_err(|_| crate::CliError::Integrity)
}

fn parse_base_id(value: &str) -> crate::CliResult<layerfs_sdk::BaseId> {
    if let Some(value) = value.strip_prefix("Layer(") {
        return Ok(layerfs_sdk::BaseId::Layer(parse_nested_id(
            value.strip_suffix(')').ok_or(crate::CliError::Integrity)?,
            "LayerId",
        )?));
    }
    if let Some(value) = value.strip_prefix("Stack(") {
        return Ok(layerfs_sdk::BaseId::Stack(parse_nested_id(
            value.strip_suffix(')').ok_or(crate::CliError::Integrity)?,
            "StackId",
        )?));
    }
    Err(crate::CliError::Integrity)
}

fn parse_source_id(value: &str) -> crate::CliResult<layerfs_sdk::SourceId> {
    if let Some(value) = value.strip_prefix("Branch(") {
        return Ok(layerfs_sdk::SourceId::Branch(parse_nested_id(
            value.strip_suffix(')').ok_or(crate::CliError::Integrity)?,
            "BranchId",
        )?));
    }
    if let Some(value) = value.strip_prefix("Stack(") {
        return Ok(layerfs_sdk::SourceId::Stack(parse_nested_id(
            value.strip_suffix(')').ok_or(crate::CliError::Integrity)?,
            "StackId",
        )?));
    }
    Err(crate::CliError::Integrity)
}

fn parse_result_id(value: &str) -> crate::CliResult<layerfs_sdk::ResultId> {
    if let Some(value) = value.strip_prefix("Layer(") {
        return Ok(layerfs_sdk::ResultId::Layer(parse_nested_id(
            value.strip_suffix(')').ok_or(crate::CliError::Integrity)?,
            "LayerId",
        )?));
    }
    if let Some(value) = value.strip_prefix("Stack(") {
        return Ok(layerfs_sdk::ResultId::Stack(parse_nested_id(
            value.strip_suffix(')').ok_or(crate::CliError::Integrity)?,
            "StackId",
        )?));
    }
    Err(crate::CliError::Integrity)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreSnapshot {
    Page {
        facts: Vec<StoreFact>,
        next: Option<String>,
    },
    Fact(Option<StoreFact>),
    CommitDiff(Vec<CommitDiffEntry>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseView {
    pub role: String,
    pub location: String,
    pub database_bytes: u64,
    pub wal_bytes: u64,
    pub shm_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementView {
    pub role: String,
    pub object_count: u64,
    pub encoded_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DedupView {
    pub route: String,
    pub route_cas_bytes: u64,
    pub union_cas_bytes: u64,
    pub cross_store_placement_bytes: u64,
    pub placement_factor: f64,
    pub placements: Vec<PlacementView>,
}

#[derive(Clone, Debug)]
pub enum MonitorView {
    Databases(Vec<DatabaseView>),
    Dedup(Vec<DedupView>),
    Workspaces(Vec<WorkspaceSummary>),
    Branch(Option<StoreFact>),
    Operations(Vec<OperationReceipt>),
    Process {
        process_id: u32,
        resident_bytes: Option<u64>,
        available_parallelism: usize,
    },
}

#[derive(Clone, Debug)]
pub enum ViewSnapshot {
    Topology(Vec<TopologyEntry>),
    Workspaces(Vec<WorkspaceSummary>),
    Workspace(WorkspaceDetail),
    WorkspaceDiff(WorkspaceDiff),
    Output(OutputPage),
    Monitor(MonitorView),
    Store(StoreSnapshot),
}

pub(crate) fn store_fact(fact: layerfs_sdk::Fact) -> StoreFact {
    use layerfs_sdk::Fact;
    let (kind, id, fields) = match fact {
        Fact::Commit(value) => (
            FactKind::Commit,
            value.id.to_string(),
            vec![
                ("root_id".to_owned(), value.root_id.to_string()),
                ("parent_id".to_owned(), format!("{:?}", value.parent_id)),
                (
                    "merge_parent_id".to_owned(),
                    format!("{:?}", value.merge_parent_id),
                ),
            ],
        ),
        Fact::Branch(value) => (
            FactKind::Branch,
            value.id.to_string(),
            vec![
                (
                    "head_commit_id".to_owned(),
                    value.head_commit_id.to_string(),
                ),
                ("base_id".to_owned(), format!("{:?}", value.base_id)),
            ],
        ),
        Fact::LayerHistory(value) => (
            FactKind::LayerHistory,
            value.id.to_string(),
            vec![("head_layer_id".to_owned(), value.head_layer_id.to_string())],
        ),
        Fact::Layer(value) => (
            FactKind::Layer,
            value.id.to_string(),
            vec![
                ("history_id".to_owned(), value.history_id.to_string()),
                ("parent_id".to_owned(), format!("{:?}", value.parent_id)),
                ("root_id".to_owned(), value.root_id.to_string()),
            ],
        ),
        Fact::StackHistory(value) => (
            FactKind::StackHistory,
            value.id.to_string(),
            vec![
                ("base_layer_id".to_owned(), value.base_layer_id.to_string()),
                ("head_stack_id".to_owned(), value.head_stack_id.to_string()),
            ],
        ),
        Fact::Stack(value) => (
            FactKind::Stack,
            value.id.to_string(),
            vec![
                ("history_id".to_owned(), value.history_id.to_string()),
                ("parent_id".to_owned(), format!("{:?}", value.parent_id)),
                ("root_id".to_owned(), value.root_id.to_string()),
            ],
        ),
        Fact::AddResult(value) => (
            FactKind::AddResult,
            format!("{:?}", value.source_id),
            vec![("result_id".to_owned(), format!("{:?}", value.result_id))],
        ),
    };
    StoreFact { kind, id, fields }
}

pub(crate) fn monitor_view(snapshot: layerfs_sdk::MonitorSnapshot) -> MonitorView {
    match snapshot {
        layerfs_sdk::MonitorSnapshot::Databases(values) => MonitorView::Databases(
            values
                .into_iter()
                .map(|value| DatabaseView {
                    role: value.role,
                    location: value.location,
                    database_bytes: value.storage.database_bytes,
                    wal_bytes: value.storage.wal_bytes,
                    shm_bytes: value.storage.shm_bytes,
                })
                .collect(),
        ),
        layerfs_sdk::MonitorSnapshot::Dedup(values) => MonitorView::Dedup(
            values
                .into_iter()
                .map(|(route, value)| DedupView {
                    route: route.to_string(),
                    route_cas_bytes: value.route_cas_bytes,
                    union_cas_bytes: value.union_cas_bytes,
                    cross_store_placement_bytes: value.cross_store_placement_bytes,
                    placement_factor: value.placement_factor,
                    placements: value
                        .placements
                        .into_iter()
                        .map(|placement| PlacementView {
                            role: placement.role,
                            object_count: placement.object_count,
                            encoded_bytes: placement.encoded_bytes,
                        })
                        .collect(),
                })
                .collect(),
        ),
        layerfs_sdk::MonitorSnapshot::Workspaces(values) => MonitorView::Workspaces(values),
        layerfs_sdk::MonitorSnapshot::Branch(value) => {
            MonitorView::Branch(value.map(layerfs_sdk::Fact::Branch).map(store_fact))
        }
        layerfs_sdk::MonitorSnapshot::Operations(values) => MonitorView::Operations(values),
        layerfs_sdk::MonitorSnapshot::Process {
            process_id,
            resident_bytes,
            available_parallelism,
        } => MonitorView::Process {
            process_id,
            resident_bytes,
            available_parallelism,
        },
    }
}

pub(crate) fn put_query(
    output: &mut crate::control::WireWriter,
    query: &ViewQuery,
) -> crate::CliResult<()> {
    match query {
        ViewQuery::Topology => output.byte(0),
        ViewQuery::Workspaces => output.byte(1),
        ViewQuery::Workspace(id) => {
            output.byte(2);
            output.string(&id.to_string())?;
        }
        ViewQuery::WorkspaceDiff(id) => {
            output.byte(3);
            output.string(&id.to_string())?;
        }
        ViewQuery::Output {
            execution_id,
            after,
            follow,
        } => {
            output.byte(4);
            output.string(&execution_id.to_string())?;
            output.u64(*after);
            output.bool(*follow);
        }
        ViewQuery::Monitor(scope) => {
            output.byte(5);
            crate::event::put_monitor_scope(output, *scope)?;
        }
        ViewQuery::Store(query) => {
            output.byte(6);
            put_store_query(output, query)?;
        }
    }
    Ok(())
}

pub(crate) fn get_query(input: &mut crate::control::WireReader<'_>) -> crate::CliResult<ViewQuery> {
    Ok(match input.byte()? {
        0 => ViewQuery::Topology,
        1 => ViewQuery::Workspaces,
        2 => ViewQuery::Workspace(parse(&input.string()?, "workspace ID")?),
        3 => ViewQuery::WorkspaceDiff(parse(&input.string()?, "workspace ID")?),
        4 => ViewQuery::Output {
            execution_id: parse(&input.string()?, "execution ID")?,
            after: input.u64()?,
            follow: input.bool()?,
        },
        5 => ViewQuery::Monitor(crate::event::get_monitor_scope(input)?),
        6 => ViewQuery::Store(get_store_query(input)?),
        _ => return Err(crate::CliError::Context("view query".to_owned())),
    })
}

pub(crate) fn put_snapshot(
    output: &mut crate::control::WireWriter,
    snapshot: &ViewSnapshot,
) -> crate::CliResult<()> {
    match snapshot {
        ViewSnapshot::Topology(values) => {
            output.byte(0);
            put_count(output, values.len())?;
            for value in values {
                output.string(&value.role)?;
                output.string(&value.location)?;
                put_optional_string(output, value.parent.as_deref())?;
                output.bool(value.active);
            }
        }
        ViewSnapshot::Workspaces(values) => {
            output.byte(1);
            put_count(output, values.len())?;
            for value in values {
                put_workspace_summary(output, value)?;
            }
        }
        ViewSnapshot::Workspace(value) => {
            output.byte(2);
            put_workspace_detail(output, value)?;
        }
        ViewSnapshot::WorkspaceDiff(value) => {
            output.byte(3);
            put_workspace_diff(output, value)?;
        }
        ViewSnapshot::Output(value) => {
            output.byte(4);
            put_output_page(output, value)?;
        }
        ViewSnapshot::Monitor(value) => {
            output.byte(5);
            crate::event::put_monitor(output, value)?;
        }
        ViewSnapshot::Store(value) => {
            output.byte(6);
            put_store_snapshot(output, value)?;
        }
    }
    Ok(())
}

pub(crate) fn get_snapshot(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<ViewSnapshot> {
    Ok(match input.byte()? {
        0 => ViewSnapshot::Topology(
            (0..input.count()?)
                .map(|_| {
                    Ok(TopologyEntry {
                        role: input.string()?,
                        location: input.string()?,
                        parent: get_optional_string(input)?,
                        active: input.bool()?,
                    })
                })
                .collect::<crate::CliResult<Vec<_>>>()?,
        ),
        1 => ViewSnapshot::Workspaces(
            (0..input.count()?)
                .map(|_| get_workspace_summary(input))
                .collect::<crate::CliResult<Vec<_>>>()?,
        ),
        2 => ViewSnapshot::Workspace(get_workspace_detail(input)?),
        3 => ViewSnapshot::WorkspaceDiff(get_workspace_diff(input)?),
        4 => ViewSnapshot::Output(get_output_page(input)?),
        5 => ViewSnapshot::Monitor(crate::event::get_monitor(input)?),
        6 => ViewSnapshot::Store(get_store_snapshot(input)?),
        _ => return Err(crate::CliError::Context("view snapshot".to_owned())),
    })
}

pub(crate) fn put_scope(output: &mut crate::control::WireWriter, scope: ViewScope) {
    output.byte(match scope {
        ViewScope::Topology => 0,
        ViewScope::Workspaces => 1,
        ViewScope::Workspace => 2,
        ViewScope::Output => 3,
        ViewScope::Monitor => 4,
        ViewScope::Store => 5,
    });
}

pub(crate) fn get_scope(input: &mut crate::control::WireReader<'_>) -> crate::CliResult<ViewScope> {
    Ok(match input.byte()? {
        0 => ViewScope::Topology,
        1 => ViewScope::Workspaces,
        2 => ViewScope::Workspace,
        3 => ViewScope::Output,
        4 => ViewScope::Monitor,
        5 => ViewScope::Store,
        _ => return Err(crate::CliError::Context("view scope".to_owned())),
    })
}

pub(crate) fn put_workspace_session(
    output: &mut crate::control::WireWriter,
    value: &layerfs_sdk::WorkspaceSession,
) -> crate::CliResult<()> {
    output.string(&value.id.to_string())?;
    output.string(&value.branch_id.to_string())?;
    output.string(&value.pinned_head.to_string())?;
    put_placement(output, &value.placement)?;
    put_projection(output, value.projection);
    put_state(output, value.state);
    Ok(())
}

pub(crate) fn get_workspace_session(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<layerfs_sdk::WorkspaceSession> {
    Ok(layerfs_sdk::WorkspaceSession {
        id: parse(&input.string()?, "workspace ID")?,
        branch_id: parse(&input.string()?, "branch ID")?,
        pinned_head: parse(&input.string()?, "commit ID")?,
        placement: get_placement(input)?,
        projection: get_projection(input)?,
        state: get_state(input)?,
    })
}

fn put_store_query(
    output: &mut crate::control::WireWriter,
    query: &StoreQuery,
) -> crate::CliResult<()> {
    match query {
        StoreQuery::Page {
            scope,
            kind,
            after,
            limit,
        } => {
            output.byte(0);
            put_store_scope(output, scope)?;
            put_fact_kind(output, *kind);
            put_optional_string(output, after.as_deref())?;
            output.u16(*limit);
        }
        StoreQuery::Fact { scope, id } => {
            output.byte(1);
            put_store_scope(output, scope)?;
            put_fact_id(output, *id)?;
        }
        StoreQuery::CommitDiff {
            branch,
            left,
            right,
        } => {
            output.byte(2);
            output.path(branch)?;
            output.string(&left.to_string())?;
            output.string(&right.to_string())?;
        }
    }
    Ok(())
}

fn get_store_query(input: &mut crate::control::WireReader<'_>) -> crate::CliResult<StoreQuery> {
    Ok(match input.byte()? {
        0 => StoreQuery::Page {
            scope: get_store_scope(input)?,
            kind: get_fact_kind(input)?,
            after: get_optional_string(input)?,
            limit: input.u16()?,
        },
        1 => StoreQuery::Fact {
            scope: get_store_scope(input)?,
            id: get_fact_id(input)?,
        },
        2 => StoreQuery::CommitDiff {
            branch: input.path()?,
            left: parse(&input.string()?, "commit ID")?,
            right: parse(&input.string()?, "commit ID")?,
        },
        _ => return Err(crate::CliError::Context("store query".to_owned())),
    })
}

fn put_store_scope(
    output: &mut crate::control::WireWriter,
    scope: &StoreScope,
) -> crate::CliResult<()> {
    match scope {
        StoreScope::Layer => output.byte(0),
        StoreScope::Stack(path) => {
            output.byte(1);
            output.path(path)?;
        }
        StoreScope::Branch(path) => {
            output.byte(2);
            output.path(path)?;
        }
    }
    Ok(())
}

fn get_store_scope(input: &mut crate::control::WireReader<'_>) -> crate::CliResult<StoreScope> {
    Ok(match input.byte()? {
        0 => StoreScope::Layer,
        1 => StoreScope::Stack(input.path()?),
        2 => StoreScope::Branch(input.path()?),
        _ => return Err(crate::CliError::Context("store scope".to_owned())),
    })
}

fn put_store_snapshot(
    output: &mut crate::control::WireWriter,
    snapshot: &StoreSnapshot,
) -> crate::CliResult<()> {
    match snapshot {
        StoreSnapshot::Page { facts, next } => {
            output.byte(0);
            put_count(output, facts.len())?;
            for fact in facts {
                put_store_fact(output, fact)?;
            }
            put_optional_string(output, next.as_deref())?;
        }
        StoreSnapshot::Fact(fact) => {
            output.byte(1);
            output.bool(fact.is_some());
            if let Some(fact) = fact {
                put_store_fact(output, fact)?;
            }
        }
        StoreSnapshot::CommitDiff(values) => {
            output.byte(2);
            put_count(output, values.len())?;
            for value in values {
                output.string(&value.inode)?;
                put_optional_string(output, value.before.as_deref())?;
                put_optional_string(output, value.after.as_deref())?;
            }
        }
    }
    Ok(())
}

fn get_store_snapshot(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<StoreSnapshot> {
    Ok(match input.byte()? {
        0 => StoreSnapshot::Page {
            facts: (0..input.count()?)
                .map(|_| get_store_fact(input))
                .collect::<crate::CliResult<Vec<_>>>()?,
            next: get_optional_string(input)?,
        },
        1 => StoreSnapshot::Fact(input.bool()?.then(|| get_store_fact(input)).transpose()?),
        2 => StoreSnapshot::CommitDiff(
            (0..input.count()?)
                .map(|_| {
                    Ok(CommitDiffEntry {
                        inode: input.string()?,
                        before: get_optional_string(input)?,
                        after: get_optional_string(input)?,
                    })
                })
                .collect::<crate::CliResult<Vec<_>>>()?,
        ),
        _ => return Err(crate::CliError::Context("store snapshot".to_owned())),
    })
}

pub(crate) fn put_store_fact(
    output: &mut crate::control::WireWriter,
    fact: &StoreFact,
) -> crate::CliResult<()> {
    put_fact_kind(output, fact.kind);
    output.string(&fact.id)?;
    put_count(output, fact.fields.len())?;
    for (name, value) in &fact.fields {
        output.string(name)?;
        output.string(value)?;
    }
    Ok(())
}

pub(crate) fn get_store_fact(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<StoreFact> {
    Ok(StoreFact {
        kind: get_fact_kind(input)?,
        id: input.string()?,
        fields: (0..input.count()?)
            .map(|_| Ok((input.string()?, input.string()?)))
            .collect::<crate::CliResult<Vec<_>>>()?,
    })
}

pub(crate) fn put_workspace_summary(
    output: &mut crate::control::WireWriter,
    value: &WorkspaceSummary,
) -> crate::CliResult<()> {
    output.string(&value.id.to_string())?;
    output.string(&value.branch_id.to_string())?;
    output.string(&value.pinned_head.to_string())?;
    put_state(output, value.state);
    output.bool(value.dirty);
    Ok(())
}

pub(crate) fn get_workspace_summary(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<WorkspaceSummary> {
    Ok(WorkspaceSummary {
        id: parse(&input.string()?, "workspace ID")?,
        branch_id: parse(&input.string()?, "branch ID")?,
        pinned_head: parse(&input.string()?, "commit ID")?,
        state: get_state(input)?,
        dirty: input.bool()?,
    })
}

fn put_workspace_detail(
    output: &mut crate::control::WireWriter,
    value: &WorkspaceDetail,
) -> crate::CliResult<()> {
    put_workspace_session(output, &value.session)?;
    output.u64(value.mutation_generation);
    put_count(output, value.executions.len())?;
    for execution in &value.executions {
        output.string(&execution.id.to_string())?;
        output.bool(execution.running);
        output.bool(execution.receipt.is_some());
        if let Some(receipt) = &execution.receipt {
            put_execution_receipt(output, receipt)?;
        }
    }
    Ok(())
}

fn get_workspace_detail(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<WorkspaceDetail> {
    Ok(WorkspaceDetail {
        session: get_workspace_session(input)?,
        mutation_generation: input.u64()?,
        executions: (0..input.count()?)
            .map(|_| {
                Ok(layerfs_sdk::ExecutionSummary {
                    id: parse(&input.string()?, "execution ID")?,
                    running: input.bool()?,
                    receipt: input
                        .bool()?
                        .then(|| get_execution_receipt(input))
                        .transpose()?,
                })
            })
            .collect::<crate::CliResult<Vec<_>>>()?,
    })
}

fn put_workspace_diff(
    output: &mut crate::control::WireWriter,
    value: &WorkspaceDiff,
) -> crate::CliResult<()> {
    output.string(&value.session_id.to_string())?;
    output.bool(value.dirty);
    output.u64(value.mutation_generation);
    Ok(())
}

fn get_workspace_diff(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<WorkspaceDiff> {
    Ok(WorkspaceDiff {
        session_id: parse(&input.string()?, "workspace ID")?,
        dirty: input.bool()?,
        mutation_generation: input.u64()?,
    })
}

fn put_output_page(
    output: &mut crate::control::WireWriter,
    value: &OutputPage,
) -> crate::CliResult<()> {
    put_count(output, value.chunks.len())?;
    for chunk in &value.chunks {
        put_output_chunk(output, chunk)?;
    }
    output.u64(value.next_sequence);
    output.bool(value.truncated);
    output.bool(value.exited);
    output.bool(value.receipt.is_some());
    if let Some(receipt) = &value.receipt {
        put_execution_receipt(output, receipt)?;
    }
    Ok(())
}

fn get_output_page(input: &mut crate::control::WireReader<'_>) -> crate::CliResult<OutputPage> {
    Ok(OutputPage {
        chunks: (0..input.count()?)
            .map(|_| get_output_chunk(input))
            .collect::<crate::CliResult<Vec<_>>>()?,
        next_sequence: input.u64()?,
        truncated: input.bool()?,
        exited: input.bool()?,
        receipt: input
            .bool()?
            .then(|| get_execution_receipt(input))
            .transpose()?,
    })
}

pub(crate) fn put_output_chunk(
    output: &mut crate::control::WireWriter,
    value: &layerfs_sdk::OutputChunk,
) -> crate::CliResult<()> {
    output.u64(value.sequence);
    output.byte(match value.stream {
        layerfs_sdk::OutputStream::Stdout => 0,
        layerfs_sdk::OutputStream::Stderr => 1,
    });
    output.bytes(&value.bytes)
}

pub(crate) fn get_output_chunk(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<layerfs_sdk::OutputChunk> {
    let sequence = input.u64()?;
    let stream = match input.byte()? {
        0 => layerfs_sdk::OutputStream::Stdout,
        1 => layerfs_sdk::OutputStream::Stderr,
        _ => return Err(crate::CliError::Context("output stream".to_owned())),
    };
    Ok(layerfs_sdk::OutputChunk {
        sequence,
        stream,
        bytes: input.bytes()?.to_vec(),
    })
}

fn put_execution_receipt(
    output: &mut crate::control::WireWriter,
    value: &layerfs_sdk::ExecutionReceipt,
) -> crate::CliResult<()> {
    output.string(&value.execution_id.to_string())?;
    output.bool(value.exit_code.is_some());
    if let Some(code) = value.exit_code {
        output.i32(code);
    }
    output.u64(value.elapsed_ns);
    output.u64(value.stdout_bytes);
    output.u64(value.stderr_bytes);
    output.bool(value.stopped);
    Ok(())
}

fn get_execution_receipt(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<layerfs_sdk::ExecutionReceipt> {
    Ok(layerfs_sdk::ExecutionReceipt {
        execution_id: parse(&input.string()?, "execution ID")?,
        exit_code: input.bool()?.then(|| input.i32()).transpose()?,
        elapsed_ns: input.u64()?,
        stdout_bytes: input.u64()?,
        stderr_bytes: input.u64()?,
        stopped: input.bool()?,
    })
}

fn put_placement(
    output: &mut crate::control::WireWriter,
    value: &layerfs_sdk::WorkspacePlacement,
) -> crate::CliResult<()> {
    match value {
        layerfs_sdk::WorkspacePlacement::Host { root } => {
            output.byte(0);
            output.path(root)?;
        }
        layerfs_sdk::WorkspacePlacement::Container { container_id, root } => {
            output.byte(1);
            output.string(&container_id.0)?;
            output.path(root)?;
        }
    }
    Ok(())
}

fn get_placement(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<layerfs_sdk::WorkspacePlacement> {
    Ok(match input.byte()? {
        0 => layerfs_sdk::WorkspacePlacement::Host {
            root: input.path()?,
        },
        1 => layerfs_sdk::WorkspacePlacement::Container {
            container_id: layerfs_sdk::ContainerId(input.string()?),
            root: input.path()?,
        },
        _ => return Err(crate::CliError::Context("workspace placement".to_owned())),
    })
}

fn put_projection(
    output: &mut crate::control::WireWriter,
    value: layerfs_sdk::WorkspaceProjection,
) {
    output.byte(match value {
        layerfs_sdk::WorkspaceProjection::Fuse => 0,
        layerfs_sdk::WorkspaceProjection::Materialize => 1,
    });
}

fn get_projection(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<layerfs_sdk::WorkspaceProjection> {
    Ok(match input.byte()? {
        0 => layerfs_sdk::WorkspaceProjection::Fuse,
        1 => layerfs_sdk::WorkspaceProjection::Materialize,
        _ => return Err(crate::CliError::Context("workspace projection".to_owned())),
    })
}

fn put_state(output: &mut crate::control::WireWriter, value: layerfs_sdk::WorkspaceState) {
    output.byte(match value {
        layerfs_sdk::WorkspaceState::Active => 0,
        layerfs_sdk::WorkspaceState::Committed => 1,
        layerfs_sdk::WorkspaceState::Discarded => 2,
        layerfs_sdk::WorkspaceState::Ended => 3,
    });
}

fn get_state(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<layerfs_sdk::WorkspaceState> {
    Ok(match input.byte()? {
        0 => layerfs_sdk::WorkspaceState::Active,
        1 => layerfs_sdk::WorkspaceState::Committed,
        2 => layerfs_sdk::WorkspaceState::Discarded,
        3 => layerfs_sdk::WorkspaceState::Ended,
        _ => return Err(crate::CliError::Context("workspace state".to_owned())),
    })
}

fn put_fact_id(output: &mut crate::control::WireWriter, value: FactId) -> crate::CliResult<()> {
    let (kind, id) = match value {
        FactId::LayerHistory(id) => (0, id.to_string()),
        FactId::Layer(id) => (1, id.to_string()),
        FactId::StackHistory(id) => (2, id.to_string()),
        FactId::Stack(id) => (3, id.to_string()),
        FactId::Branch(id) => (4, id.to_string()),
        FactId::Commit(id) => (5, id.to_string()),
    };
    output.byte(kind);
    output.string(&id)
}

fn get_fact_id(input: &mut crate::control::WireReader<'_>) -> crate::CliResult<FactId> {
    let kind = input.byte()?;
    let id = input.string()?;
    Ok(match kind {
        0 => FactId::LayerHistory(parse(&id, "LayerHistory ID")?),
        1 => FactId::Layer(parse(&id, "Layer ID")?),
        2 => FactId::StackHistory(parse(&id, "StackHistory ID")?),
        3 => FactId::Stack(parse(&id, "Stack ID")?),
        4 => FactId::Branch(parse(&id, "Branch ID")?),
        5 => FactId::Commit(parse(&id, "Commit ID")?),
        _ => return Err(crate::CliError::Context("fact ID".to_owned())),
    })
}

pub(crate) fn put_fact_kind(output: &mut crate::control::WireWriter, value: FactKind) {
    output.byte(match value {
        FactKind::Commit => 0,
        FactKind::Branch => 1,
        FactKind::LayerHistory => 2,
        FactKind::Layer => 3,
        FactKind::StackHistory => 4,
        FactKind::Stack => 5,
        FactKind::AddResult => 6,
    });
}

pub(crate) fn get_fact_kind(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<FactKind> {
    Ok(match input.byte()? {
        0 => FactKind::Commit,
        1 => FactKind::Branch,
        2 => FactKind::LayerHistory,
        3 => FactKind::Layer,
        4 => FactKind::StackHistory,
        5 => FactKind::Stack,
        6 => FactKind::AddResult,
        _ => return Err(crate::CliError::Context("fact kind".to_owned())),
    })
}

pub(crate) fn put_optional_string(
    output: &mut crate::control::WireWriter,
    value: Option<&str>,
) -> crate::CliResult<()> {
    output.bool(value.is_some());
    if let Some(value) = value {
        output.string(value)?;
    }
    Ok(())
}

pub(crate) fn get_optional_string(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<Option<String>> {
    input.bool()?.then(|| input.string()).transpose()
}

pub(crate) fn put_optional_u64(output: &mut crate::control::WireWriter, value: Option<u64>) {
    output.bool(value.is_some());
    if let Some(value) = value {
        output.u64(value);
    }
}

pub(crate) fn get_optional_u64(
    input: &mut crate::control::WireReader<'_>,
) -> crate::CliResult<Option<u64>> {
    input.bool()?.then(|| input.u64()).transpose()
}

pub(crate) fn put_count(
    output: &mut crate::control::WireWriter,
    value: usize,
) -> crate::CliResult<()> {
    output.u32(
        value
            .try_into()
            .map_err(|_| crate::CliError::Context("collection length".to_owned()))?,
    );
    Ok(())
}

pub(crate) fn parse<T: std::str::FromStr>(value: &str, name: &str) -> crate::CliResult<T> {
    value
        .parse()
        .map_err(|_| crate::CliError::Context(name.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::StoreFact;
    use layerfs_sdk::{Fact, FactKind};

    fn id(tag: &str, bytes: usize) -> String {
        format!("{tag}{}", "0".repeat(bytes * 2))
    }

    #[test]
    fn store_fact_decodes_branch_and_base() {
        let fact = StoreFact {
            kind: FactKind::Branch,
            id: id("11", 16),
            fields: vec![
                ("head_commit_id".to_owned(), id("12", 32)),
                (
                    "base_id".to_owned(),
                    format!("Layer(LayerId({}))", id("32", 32)),
                ),
            ],
        };
        assert!(matches!(fact.fact().unwrap(), Fact::Branch(_)));
    }
}
