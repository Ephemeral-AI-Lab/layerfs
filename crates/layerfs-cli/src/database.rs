use crate::fixture::{commit_id, layer_for, BranchRecord, MockState};
use crate::workspace::CanonicalObject;
use crate::{
    BranchOrigin, BranchRelation, CliError, CliResult, CommandKind, CommandResult, ContextProfile,
    ObjectId, RemotePlacement, StoreRole,
};
use rusqlite::{params, Connection, Transaction};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

const SCHEMA_VERSION: i64 = 3;
const LAYERSTACK_APPLICATION_ID: i64 = 0x4c46_534c;
const BRANCH_APPLICATION_ID: i64 = 0x4c46_5342;
const LAYERSTACK_SCHEMA: &str = include_str!("layerstack.sql");
const BRANCH_SCHEMA: &str = include_str!("branch.sql");

const LAYERSTACK_TABLES: &[(&str, usize)] = &[
    ("branches", 8),
    ("commits", 4),
    ("layer_stacks", 3),
    ("layers", 6),
    ("objects", 2),
    ("store", 2),
];
const BRANCH_TABLES: &[(&str, usize)] = &[
    ("branch_scopes", 4),
    ("branches", 8),
    ("commits", 4),
    ("complete_roots", 1),
    ("layer_stack_scopes", 3),
    ("layer_stacks", 2),
    ("layers", 6),
    ("objects", 2),
    ("store", 3),
];
const LAYERSTACK_INDEXES: &[&str] = &[
    "branch_identity",
    "branch_names",
    "branches_fork",
    "branches_head",
    "commits_parent",
    "layer_identity",
    "layer_stack_names",
    "layers_child",
    "layers_genesis",
    "layers_parent",
    "layers_source",
];
const BRANCH_INDEXES: &[&str] = &[
    "branch_identity",
    "branch_names",
    "branch_pointer",
    "branches_fork",
    "branches_head",
    "commits_parent",
    "layer_identity",
    "layer_stack_names",
    "layers_child",
    "layers_genesis",
    "layers_parent",
    "layers_source",
];

pub(crate) struct Databases {
    context_path: PathBuf,
    profile: Mutex<Option<ContextProfile>>,
    temporary_root: Option<PathBuf>,
}

impl Databases {
    pub(crate) fn open(context: &Path, empty: bool) -> CliResult<Self> {
        if matches!(context.to_str(), Some("mock" | "mock-empty")) {
            let root = temporary_root()?;
            let profile = ContextProfile {
                layerstack: root.join("layerstack.sqlite"),
                branch: root.join("branch.sqlite"),
            };
            create_layerstack(&profile.layerstack)?;
            let parent = store_id(&profile.layerstack, StoreRole::LayerStack)?;
            create_branch(&profile.branch, &parent)?;
            let databases = Self {
                context_path: root.join("context"),
                profile: Mutex::new(Some(profile)),
                temporary_root: Some(root),
            };
            databases.save_profile()?;
            let state = if empty {
                MockState::empty()
            } else {
                MockState::demo()
            };
            databases.sync(&state)?;
            return Ok(databases);
        }

        let profile = match load_profile(context) {
            Ok(profile) => Some(profile),
            Err(CliError::NotFound(_)) => None,
            Err(error) => return Err(error),
        };
        Ok(Self {
            context_path: context.to_owned(),
            profile: Mutex::new(profile),
            temporary_root: None,
        })
    }

    pub(crate) fn profile(&self) -> CliResult<ContextProfile> {
        self.profile
            .lock()
            .map_err(|_| CliError::Integrity("context profile lock".into()))?
            .clone()
            .ok_or_else(|| CliError::NotFound("active context".into()))
    }

    pub(crate) fn execute_control(
        &self,
        role: Option<StoreRole>,
        create: bool,
        location: Option<&str>,
        parent: Option<&str>,
        use_pair: Option<(&str, &str)>,
    ) -> CliResult<CommandResult> {
        if let Some((layerstack, branch)) = use_pair {
            let profile = ContextProfile {
                layerstack: absolute(layerstack)?,
                branch: absolute(branch)?,
            };
            validate_pair(&profile)?;
            save_profile(&self.context_path, &profile)?;
            *self
                .profile
                .lock()
                .map_err(|_| CliError::Integrity("context profile lock".into()))? =
                Some(profile.clone());
            return Ok(CommandResult::Context(profile));
        }

        let Some(role) = role else {
            return Ok(CommandResult::Context(self.profile()?));
        };
        let path = absolute(location.ok_or_else(|| CliError::Parse("Store location".into()))?)?;
        match (create, role) {
            (true, StoreRole::LayerStack) => create_layerstack(&path)?,
            (true, StoreRole::Branch) => {
                let parent =
                    absolute(parent.ok_or_else(|| CliError::Parse("BranchStore parent".into()))?)?;
                let parent_id = store_id(&parent, StoreRole::LayerStack)?;
                create_branch(&path, &parent_id)?;
            }
            (false, StoreRole::LayerStack) => {
                store_id(&path, StoreRole::LayerStack)?;
            }
            (false, StoreRole::Branch) => {
                let parent =
                    absolute(parent.ok_or_else(|| CliError::Parse("BranchStore parent".into()))?)?;
                let expected = store_id(&parent, StoreRole::LayerStack)?;
                let actual = parent_store_id(&path)?;
                if actual != expected {
                    return Err(CliError::Integrity("BranchStore parent StoreId".into()));
                }
            }
        }
        Ok(CommandResult::Query(format!(
            "{} {}",
            match role {
                StoreRole::LayerStack => "LayerStackStore",
                StoreRole::Branch => "BranchStore",
            },
            path.display()
        )))
    }

    pub(crate) fn apply_control(
        &self,
        state: &mut MockState,
        command: &CommandKind,
    ) -> Option<CliResult<CommandResult>> {
        match command {
            CommandKind::DbCreate {
                role,
                location,
                parent,
            } => Some(self.execute_control(
                Some(*role),
                true,
                Some(location),
                parent.as_deref(),
                None,
            )),
            CommandKind::DbConnect {
                role,
                location,
                parent,
            } => Some(self.execute_control(
                Some(*role),
                false,
                Some(location),
                parent.as_deref(),
                None,
            )),
            CommandKind::ContextUse { layerstack, branch } => Some(
                self.execute_control(None, false, None, None, Some((layerstack, branch)))
                    .inspect(|_| {
                        *state = MockState::empty();
                    }),
            ),
            CommandKind::ContextShow => Some(self.execute_control(None, false, None, None, None)),
            _ => None,
        }
    }

    pub(crate) fn sync(&self, state: &MockState) -> CliResult<()> {
        let profile = self.profile()?;
        sync_layerstack(&profile.layerstack, state)?;
        sync_branch(&profile.branch, state)?;
        Ok(())
    }

    pub(crate) fn publish_workspace_commit(
        &self,
        state: &MockState,
        workspace_id: &str,
    ) -> CliResult<()> {
        let workspace = state
            .workspaces
            .iter()
            .find(|workspace| workspace.id.as_str() == workspace_id)
            .ok_or_else(|| CliError::NotFound(workspace_id.into()))?;
        let published_id = workspace
            .published_commit
            .as_ref()
            .ok_or_else(|| CliError::Integrity("published Workspace Commit".into()))?;
        let branch = state
            .branch(&workspace.branch_id)
            .ok_or_else(|| CliError::Integrity("published Workspace Branch".into()))?;
        let commit = branch
            .commits
            .iter()
            .find(|commit| &commit.id == published_id)
            .ok_or_else(|| CliError::Integrity("published Commit fact".into()))?;
        let parent = commit
            .number
            .checked_sub(1)
            .filter(|number| *number > 0)
            .map(|number| commit_id_bytes(commit_id(branch.id.as_str(), number).as_str()))
            .or_else(|| {
                branch
                    .boundary_commit
                    .as_ref()
                    .map(|id| commit_id_bytes(id.as_str()))
            });
        let profile = self.profile()?;
        let mut connection = open(&profile.branch, StoreRole::Branch)?;
        let transaction = connection.transaction().map_err(database_error)?;
        insert_objects(&transaction, &commit.objects)?;
        transaction
            .execute(
                "INSERT INTO commits(commit_id,root_id,parent_commit_id,base_layer_id)
                 VALUES(?1,?2,?3,?4)",
                params![
                    commit_id_bytes(commit.id.as_str()),
                    object_id(commit.root.as_str()),
                    parent,
                    layer_id(state.branch_base_layer(branch).as_str())
                ],
            )
            .map_err(database_error)?;
        transaction
            .execute(
                "UPDATE branches SET head_commit_id=?1 WHERE branch_id=?2",
                params![
                    commit_id_bytes(commit.id.as_str()),
                    branch_id(branch.id.as_str())
                ],
            )
            .map_err(database_error)?;
        transaction
            .execute(
                "INSERT OR IGNORE INTO complete_roots(root_id) VALUES(?1)",
                [object_id(commit.root.as_str())],
            )
            .map_err(database_error)?;
        check_foreign_keys(&transaction)?;
        transaction.commit().map_err(database_error)
    }

    pub(crate) fn decorate_storage(&self, state: &mut MockState) -> CliResult<()> {
        let profile = match self.profile() {
            Ok(profile) => profile,
            Err(CliError::NotFound(_)) => return Ok(()),
            Err(error) => return Err(error),
        };
        state.storage.layerstack_store_id =
            short_hex(&store_id(&profile.layerstack, StoreRole::LayerStack)?);
        state.storage.branch_store_id = short_hex(&store_id(&profile.branch, StoreRole::Branch)?);
        state.storage.authority_bytes = file_bytes(&profile.layerstack)?;
        state.storage.branch_bytes = file_bytes(&profile.branch)?;
        let authority_objects = object_sizes(&profile.layerstack, StoreRole::LayerStack)?;
        let branch_objects = object_sizes(&profile.branch, StoreRole::Branch)?;
        state.storage.shared_objects = authority_objects
            .keys()
            .filter(|id| branch_objects.contains_key(*id))
            .count() as u64;
        let mut union = authority_objects;
        union.extend(branch_objects);
        state.storage.unique_bytes = union.values().sum::<i64>() as u64;
        let branch = open(&profile.branch, StoreRole::Branch)?;
        state.storage.replica_roots = count(&branch, "complete_roots", None)? as u16;
        state.storage.reference_scopes = count(
            &branch,
            "layer_stack_scopes",
            Some("serving_mode='reference'"),
        )?
        .saturating_add(count(
            &branch,
            "branch_scopes",
            Some("serving_mode='reference'"),
        )?) as u16;
        Ok(())
    }

    pub(crate) fn branch_objects(
        &self,
        objects: &[CanonicalObject],
    ) -> CliResult<HashSet<ObjectId>> {
        let profile = self.profile()?;
        let connection = open(&profile.branch, StoreRole::Branch)?;
        let mut statement = connection
            .prepare("SELECT EXISTS(SELECT 1 FROM objects WHERE object_id=?1)")
            .map_err(database_error)?;
        objects
            .iter()
            .filter_map(|object| {
                match statement
                    .query_row([object_id(object.id.as_str())], |row| row.get::<_, bool>(0))
                {
                    Ok(true) => Some(Ok(object.id.clone())),
                    Ok(false) => None,
                    Err(error) => Some(Err(database_error(error))),
                }
            })
            .collect()
    }

    pub(crate) fn branch_storage_bytes(&self) -> CliResult<u64> {
        let profile = self.profile()?;
        let mut bytes = file_bytes(&profile.branch)?;
        let sidecar = PathBuf::from(format!("{}-wal", profile.branch.display()));
        if sidecar.is_file() {
            bytes = bytes.saturating_add(file_bytes(&sidecar)?);
        }
        Ok(bytes)
    }

    fn save_profile(&self) -> CliResult<()> {
        save_profile(&self.context_path, &self.profile()?)
    }
}

impl Drop for Databases {
    fn drop(&mut self) {
        if let Some(root) = &self.temporary_root {
            let _ = std::fs::remove_dir_all(root);
        }
    }
}

fn create_layerstack(path: &Path) -> CliResult<()> {
    create_store(path, StoreRole::LayerStack, None)
}

fn create_branch(path: &Path, parent_store_id: &[u8]) -> CliResult<()> {
    create_store(path, StoreRole::Branch, Some(parent_store_id))
}

fn create_store(path: &Path, role: StoreRole, parent: Option<&[u8]>) -> CliResult<()> {
    if path.exists() {
        return Err(CliError::Database(format!(
            "Store already exists: {}",
            path.display()
        )));
    }
    if let Some(directory) = path.parent() {
        std::fs::create_dir_all(directory).map_err(database_error)?;
    }
    let connection = Connection::open(path).map_err(database_error)?;
    configure(&connection)?;
    connection
        .pragma_update(None, "application_id", application_id(role))
        .map_err(database_error)?;
    connection
        .pragma_update(None, "user_version", SCHEMA_VERSION)
        .map_err(database_error)?;
    connection
        .execute_batch(schema(role))
        .map_err(database_error)?;
    let id = random_id()?;
    match role {
        StoreRole::LayerStack => connection
            .execute("INSERT INTO store(singleton,store_id) VALUES(1,?1)", [&id])
            .map_err(database_error)?,
        StoreRole::Branch => connection
            .execute(
                "INSERT INTO store(singleton,store_id,parent_store_id) VALUES(1,?1,?2)",
                params![
                    id,
                    parent.ok_or_else(|| CliError::Integrity("Store parent".into()))?
                ],
            )
            .map_err(database_error)?,
    };
    verify(&connection, role)?;
    Ok(())
}

fn open(path: &Path, role: StoreRole) -> CliResult<Connection> {
    if !path.is_file() {
        return Err(CliError::NotFound(format!("Store {}", path.display())));
    }
    let connection = Connection::open(path).map_err(database_error)?;
    configure(&connection)?;
    verify(&connection, role)?;
    Ok(connection)
}

fn configure(connection: &Connection) -> CliResult<()> {
    connection
        .execute_batch(
            "PRAGMA foreign_keys=ON;
             PRAGMA journal_mode=WAL;
             PRAGMA synchronous=FULL;
             PRAGMA temp_store=FILE;
             PRAGMA cache_size=-8192;
             PRAGMA wal_autocheckpoint=1000;
             PRAGMA busy_timeout=5000;",
        )
        .map_err(database_error)
}

fn verify(connection: &Connection, role: StoreRole) -> CliResult<()> {
    let application: i64 = connection
        .query_row("PRAGMA application_id", [], |row| row.get(0))
        .map_err(database_error)?;
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(database_error)?;
    if application != application_id(role) || version != SCHEMA_VERSION {
        return Err(CliError::Integrity("Store role or schema version".into()));
    }
    let tables = expected_tables(role);
    let names = connection
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .map_err(database_error)?
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    if names
        != tables
            .iter()
            .map(|(name, _)| (*name).to_owned())
            .collect::<Vec<_>>()
    {
        return Err(CliError::Integrity("Store table manifest".into()));
    }
    for (table, count) in tables {
        let actual: i64 = connection
            .query_row(
                "SELECT count(*) FROM pragma_table_info(?1)",
                [table],
                |row| row.get(0),
            )
            .map_err(database_error)?;
        if actual != *count as i64 {
            return Err(CliError::Integrity("Store column manifest".into()));
        }
    }
    let indexes = connection
        .prepare(
            "SELECT name FROM sqlite_schema
             WHERE type='index' AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .map_err(database_error)?
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(database_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(database_error)?;
    if indexes
        != expected_indexes(role)
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    {
        return Err(CliError::Integrity("Store index manifest".into()));
    }
    Ok(())
}

fn validate_pair(profile: &ContextProfile) -> CliResult<()> {
    let expected = store_id(&profile.layerstack, StoreRole::LayerStack)?;
    if parent_store_id(&profile.branch)? != expected {
        return Err(CliError::Integrity("BranchStore parent StoreId".into()));
    }
    Ok(())
}

fn store_id(path: &Path, role: StoreRole) -> CliResult<Vec<u8>> {
    open(path, role)?
        .query_row("SELECT store_id FROM store WHERE singleton=1", [], |row| {
            row.get(0)
        })
        .map_err(database_error)
}

fn parent_store_id(path: &Path) -> CliResult<Vec<u8>> {
    open(path, StoreRole::Branch)?
        .query_row(
            "SELECT parent_store_id FROM store WHERE singleton=1",
            [],
            |row| row.get(0),
        )
        .map_err(database_error)
}

fn sync_layerstack(path: &Path, state: &MockState) -> CliResult<()> {
    let mut connection = open(path, StoreRole::LayerStack)?;
    let transaction = connection.transaction().map_err(database_error)?;
    transaction
        .pragma_update(None, "defer_foreign_keys", true)
        .map_err(database_error)?;
    clear(&transaction, false)?;
    for project in &state.projects {
        transaction
            .execute(
                "INSERT INTO layer_stacks(layer_stack_id,name,head_layer_id) VALUES(?1,?2,?3)",
                params![
                    stack_id(project.id.as_str()),
                    project.name.as_str(),
                    layer_id(layer_for(project, project.authority_layers).as_str())
                ],
            )
            .map_err(database_error)?;
    }
    insert_authority_facts(&transaction, state)?;
    check_foreign_keys(&transaction)?;
    transaction.commit().map_err(database_error)
}

fn sync_branch(path: &Path, state: &MockState) -> CliResult<()> {
    let mut connection = open(path, StoreRole::Branch)?;
    let transaction = connection.transaction().map_err(database_error)?;
    transaction
        .pragma_update(None, "defer_foreign_keys", true)
        .map_err(database_error)?;
    clear(&transaction, true)?;
    for project in &state.projects {
        let visible = project.work_layers.is_some()
            || state
                .branches
                .iter()
                .any(|branch| branch.project_id == project.id && branch_present(branch));
        if !visible {
            continue;
        }
        transaction
            .execute(
                "INSERT INTO layer_stacks(layer_stack_id,name) VALUES(?1,?2)",
                params![stack_id(project.id.as_str()), project.name.as_str()],
            )
            .map_err(database_error)?;
        if let (Some(boundary), Some(mode)) = (project.work_layers, project.mode) {
            transaction
                .execute(
                    "INSERT INTO layer_stack_scopes(layer_stack_id,through_layer_id,serving_mode)
                     VALUES(?1,?2,?3)",
                    params![
                        stack_id(project.id.as_str()),
                        layer_id(layer_for(project, boundary).as_str()),
                        placement(mode)
                    ],
                )
                .map_err(database_error)?;
        }
    }
    insert_work_facts(&transaction, state)?;
    check_foreign_keys(&transaction)?;
    transaction.commit().map_err(database_error)
}

fn check_foreign_keys(transaction: &Transaction<'_>) -> CliResult<()> {
    let violation = transaction
        .prepare("PRAGMA foreign_key_check")
        .map_err(database_error)?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .map_err(database_error)?
        .next()
        .transpose()
        .map_err(database_error)?;
    if let Some((table, row, parent, key)) = violation {
        return Err(CliError::Integrity(format!(
            "foreign key {table}[{row:?}] -> {parent} constraint {key}"
        )));
    }
    Ok(())
}

fn clear(transaction: &Transaction<'_>, branch: bool) -> CliResult<()> {
    if branch {
        transaction
            .execute_batch(
                "DELETE FROM branch_scopes;
                 DELETE FROM layer_stack_scopes;
                 DELETE FROM complete_roots;",
            )
            .map_err(database_error)?;
    }
    transaction
        .execute_batch(
            "DELETE FROM branches;
             DELETE FROM commits;
             DELETE FROM layers;
             DELETE FROM layer_stacks;
             DELETE FROM objects;",
        )
        .map_err(database_error)
}

fn insert_authority_facts(transaction: &Transaction<'_>, state: &MockState) -> CliResult<()> {
    for layer in &state.layers {
        let project = state
            .projects
            .iter()
            .find(|project| project.id == layer.project_id)
            .ok_or_else(|| CliError::Integrity("fixture LayerStack".into()))?;
        insert_objects(transaction, &layer.objects)?;
        let source = layer.source.as_ref();
        transaction
            .execute(
                "INSERT INTO layers(layer_id,layer_stack_id,parent_layer_id,root_id,source_branch_id,source_commit_id)
                 VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    layer_id(layer.id.as_str()),
                    stack_id(layer.project_id.as_str()),
                    (layer.number > 1)
                        .then(|| layer_id(layer_for(project, layer.number - 1).as_str())),
                    object_id(layer.root.as_str()),
                    source.map(|(branch, _)| branch_id(branch.as_str())),
                    source.map(|(_, commit)| commit_id_bytes(commit.as_str())),
                ],
            )
            .map_err(database_error)?;
    }
    insert_commits(transaction, state, true)?;
    for branch in state
        .branches
        .iter()
        .filter(|branch| branch.authority_head.is_some())
    {
        insert_branch(transaction, state, branch, branch.authority_head, false)?;
    }
    Ok(())
}

fn insert_work_facts(transaction: &Transaction<'_>, state: &MockState) -> CliResult<()> {
    for layer in &state.layers {
        let project = state
            .projects
            .iter()
            .find(|project| project.id == layer.project_id)
            .ok_or_else(|| CliError::Integrity("fixture LayerStack".into()))?;
        if project
            .work_layers
            .is_none_or(|boundary| layer.number > boundary)
        {
            continue;
        }
        let source = layer.source.as_ref();
        transaction
            .execute(
                "INSERT INTO layers(layer_id,layer_stack_id,parent_layer_id,root_id,source_branch_id,source_commit_id)
                 VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    layer_id(layer.id.as_str()),
                    stack_id(layer.project_id.as_str()),
                    (layer.number > 1)
                        .then(|| layer_id(layer_for(project, layer.number - 1).as_str())),
                    object_id(layer.root.as_str()),
                    source.map(|(branch, _)| branch_id(branch.as_str())),
                    source.map(|(_, commit)| commit_id_bytes(commit.as_str())),
                ],
            )
            .map_err(database_error)?;
        if layer.number <= project.complete_roots {
            insert_objects(transaction, &layer.objects)?;
            transaction
                .execute(
                    "INSERT OR IGNORE INTO complete_roots(root_id) VALUES(?1)",
                    [object_id(layer.root.as_str())],
                )
                .map_err(database_error)?;
        }
    }
    insert_commits(transaction, state, false)?;
    for branch in state
        .branches
        .iter()
        .filter(|branch| branch_present(branch))
    {
        insert_branch(transaction, state, branch, branch.work_head, true)?;
        match &branch.relation {
            BranchRelation::RemoteCurrent { mode }
            | BranchRelation::RemotePullBehind { mode, .. } => {
                let through = branch
                    .work_head
                    .ok_or_else(|| CliError::Integrity("remote Branch boundary".into()))?;
                transaction
                    .execute(
                        "INSERT INTO branch_scopes(branch_id,scope_kind,through_commit_id,serving_mode)
                         VALUES(?1,'remote',?2,?3)",
                        params![
                            branch_id(branch.id.as_str()),
                            commit_id_bytes(commit_id(branch.id.as_str(), through).as_str()),
                            placement(*mode)
                        ],
                    )
                    .map_err(database_error)?;
            }
            _ => {
                transaction
                    .execute(
                        "INSERT INTO branch_scopes(branch_id,scope_kind,through_commit_id,serving_mode)
                         VALUES(?1,'local',NULL,NULL)",
                        [branch_id(branch.id.as_str())],
                    )
                    .map_err(database_error)?;
            }
        }
    }
    Ok(())
}

fn insert_commits(
    transaction: &Transaction<'_>,
    state: &MockState,
    authority: bool,
) -> CliResult<()> {
    for branch in &state.branches {
        let required_boundary = (!authority).then(|| inherited_boundary(state, branch, false));
        let replica_boundary = (!authority).then(|| inherited_boundary(state, branch, true));
        let visible = branch
            .commits
            .iter()
            .filter(|commit| match required_boundary {
                Some(boundary) => commit.work || commit.number <= boundary,
                None => commit.authority,
            })
            .collect::<Vec<_>>();
        for (index, commit) in visible.iter().enumerate() {
            let complete = !authority
                && (branch_is_local(branch)
                    || commit.number <= branch.remote_complete_through.unwrap_or(0)
                    || commit.number <= replica_boundary.unwrap_or(0));
            if authority || complete {
                insert_objects(transaction, &commit.objects)?;
            }
            let parent = index
                .checked_sub(1)
                .and_then(|index| visible.get(index))
                .map(|parent| commit_id(branch.id.as_str(), parent.number))
                .or_else(|| branch.boundary_commit.clone());
            transaction
                .execute(
                    "INSERT OR IGNORE INTO commits(commit_id,root_id,parent_commit_id,base_layer_id)
                     VALUES(?1,?2,?3,?4)",
                    params![
                        commit_id_bytes(commit.id.as_str()),
                        object_id(commit.root.as_str()),
                        parent.map(|id| commit_id_bytes(id.as_str())),
                        layer_id(state.branch_base_layer(branch).as_str())
                    ],
                )
                .map_err(database_error)?;
            if complete {
                transaction
                    .execute(
                        "INSERT OR IGNORE INTO complete_roots(root_id) VALUES(?1)",
                        [object_id(commit.root.as_str())],
                    )
                    .map_err(database_error)?;
            }
        }
    }
    Ok(())
}

fn inherited_boundary(state: &MockState, parent: &BranchRecord, replica_only: bool) -> u16 {
    state
        .branches
        .iter()
        .filter(|child| branch_present(child))
        .filter(|child| {
            !replica_only
                || matches!(
                    child.relation,
                    BranchRelation::RemoteCurrent {
                        mode: RemotePlacement::Replica
                    } | BranchRelation::RemotePullBehind {
                        mode: RemotePlacement::Replica,
                        ..
                    }
                )
        })
        .filter_map(|child| match &child.origin {
            BranchOrigin::Commit(branch, boundary) if branch == &parent.id => parent
                .commits
                .iter()
                .find(|commit| &commit.id == boundary)
                .map(|commit| commit.number),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

fn insert_branch(
    transaction: &Transaction<'_>,
    state: &MockState,
    branch: &BranchRecord,
    head: Option<u16>,
    nullable_head: bool,
) -> CliResult<()> {
    let (from_layer, from_branch, from_commit) = match &branch.origin {
        BranchOrigin::Layer(layer) => (Some(layer_id(layer.as_str())), None, None),
        BranchOrigin::Commit(parent, commit) => (
            None,
            Some(branch_id(parent.as_str())),
            Some(commit_id_bytes(commit.as_str())),
        ),
    };
    let head = head.map(|number| commit_id_bytes(commit_id(branch.id.as_str(), number).as_str()));
    if !nullable_head && head.is_none() {
        return Ok(());
    }
    transaction
        .execute(
            "INSERT INTO branches(branch_id,layer_stack_id,name,base_layer_id,head_commit_id,forked_from_layer_id,forked_from_branch_id,forked_from_commit_id)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                branch_id(branch.id.as_str()),
                stack_id(branch.project_id.as_str()),
                branch.name.as_str(),
                layer_id(state.branch_base_layer(branch).as_str()),
                head,
                from_layer,
                from_branch,
                from_commit,
            ],
        )
        .map_err(database_error)?;
    Ok(())
}

fn insert_objects(transaction: &Transaction<'_>, objects: &[CanonicalObject]) -> CliResult<()> {
    let mut statement = transaction
        .prepare("INSERT OR IGNORE INTO objects(object_id,bytes) VALUES(?1,?2)")
        .map_err(database_error)?;
    for object in objects {
        statement
            .execute(params![object_id(object.id.as_str()), object.bytes])
            .map_err(database_error)?;
    }
    Ok(())
}

fn branch_present(branch: &BranchRecord) -> bool {
    !matches!(
        branch.relation,
        BranchRelation::AuthorityOnly | BranchRelation::Integrity
    )
}

fn branch_is_local(branch: &BranchRecord) -> bool {
    matches!(
        branch.relation,
        BranchRelation::LocalOnly
            | BranchRelation::LocalCurrent
            | BranchRelation::LocalPushAhead { .. }
            | BranchRelation::AuthorityAhead { .. }
            | BranchRelation::Diverged
    )
}

fn placement(value: RemotePlacement) -> &'static str {
    match value {
        RemotePlacement::Reference => "reference",
        RemotePlacement::Replica => "replica",
    }
}

fn stack_id(value: &str) -> Vec<u8> {
    tagged_id(0x31, value, 17)
}

fn branch_id(value: &str) -> Vec<u8> {
    tagged_id(0x11, value, 17)
}

fn layer_id(value: &str) -> Vec<u8> {
    tagged_id(0x32, value, 33)
}

fn commit_id_bytes(value: &str) -> Vec<u8> {
    tagged_id(0x12, value, 33)
}

fn object_id(value: &str) -> Vec<u8> {
    digest(value.as_bytes(), 32)
}

fn tagged_id(tag: u8, value: &str, length: usize) -> Vec<u8> {
    let mut id = digest(value.as_bytes(), length);
    id[0] = tag;
    id
}

fn digest(value: &[u8], length: usize) -> Vec<u8> {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3);
    }
    let mut output = vec![0; length];
    for (index, byte) in output.iter_mut().enumerate() {
        hash ^= (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        hash ^= hash >> 12;
        hash ^= hash << 25;
        hash ^= hash >> 27;
        *byte = (hash.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 56) as u8;
    }
    output
}

fn random_id() -> CliResult<Vec<u8>> {
    let mut bytes = vec![0; 32];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .map_err(database_error)?;
    Ok(bytes)
}

fn short_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn file_bytes(path: &Path) -> CliResult<u64> {
    std::fs::metadata(path)
        .map(|metadata| metadata.len())
        .map_err(database_error)
}

fn object_sizes(path: &Path, role: StoreRole) -> CliResult<HashMap<Vec<u8>, i64>> {
    open(path, role)?
        .prepare("SELECT object_id,length(bytes) FROM objects")
        .map_err(database_error)?
        .query_map([], |row| Ok((row.get(0)?, row.get::<_, i64>(1)?)))
        .map_err(database_error)?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(database_error)
}

fn count(connection: &Connection, table: &str, condition: Option<&str>) -> CliResult<i64> {
    connection
        .query_row(
            &format!(
                "SELECT count(*) FROM {table}{}",
                condition.map_or_else(String::new, |value| format!(" WHERE {value}"))
            ),
            [],
            |row| row.get(0),
        )
        .map_err(database_error)
}

fn load_profile(path: &Path) -> CliResult<ContextProfile> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::NotFound(format!("context {}", path.display()))
        } else {
            database_error(error)
        }
    })?;
    let mut layerstack = None;
    let mut branch = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("layerstack=") {
            layerstack = Some(PathBuf::from(value));
        } else if let Some(value) = line.strip_prefix("branch=") {
            branch = Some(PathBuf::from(value));
        } else if !line.is_empty() {
            return Err(CliError::Integrity("context format".into()));
        }
    }
    Ok(ContextProfile {
        layerstack: layerstack
            .ok_or_else(|| CliError::Integrity("context LayerStackStore".into()))?,
        branch: branch.ok_or_else(|| CliError::Integrity("context BranchStore".into()))?,
    })
}

fn save_profile(path: &Path, profile: &ContextProfile) -> CliResult<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent).map_err(database_error)?;
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut output = std::fs::File::create(&temporary).map_err(database_error)?;
    writeln!(output, "layerstack={}", profile.layerstack.display()).map_err(database_error)?;
    writeln!(output, "branch={}", profile.branch.display()).map_err(database_error)?;
    output.sync_all().map_err(database_error)?;
    std::fs::rename(temporary, path).map_err(database_error)
}

fn absolute(value: &str) -> CliResult<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        Ok(path.to_owned())
    } else {
        Ok(std::env::current_dir().map_err(database_error)?.join(path))
    }
}

fn temporary_root() -> CliResult<PathBuf> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "layerfs-tui-real-schema-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).map_err(database_error)?;
    Ok(root)
}

fn application_id(role: StoreRole) -> i64 {
    match role {
        StoreRole::LayerStack => LAYERSTACK_APPLICATION_ID,
        StoreRole::Branch => BRANCH_APPLICATION_ID,
    }
}

fn schema(role: StoreRole) -> &'static str {
    match role {
        StoreRole::LayerStack => LAYERSTACK_SCHEMA,
        StoreRole::Branch => BRANCH_SCHEMA,
    }
}

fn expected_tables(role: StoreRole) -> &'static [(&'static str, usize)] {
    match role {
        StoreRole::LayerStack => LAYERSTACK_TABLES,
        StoreRole::Branch => BRANCH_TABLES,
    }
}

fn expected_indexes(role: StoreRole) -> &'static [&'static str] {
    match role {
        StoreRole::LayerStack => LAYERSTACK_INDEXES,
        StoreRole::Branch => BRANCH_INDEXES,
    }
}

fn database_error(error: impl std::fmt::Display) -> CliError {
    CliError::Database(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_v2_schemas_and_parent_identity_back_the_fixture() {
        let databases = Databases::open(Path::new("mock"), false).unwrap();
        let profile = databases.profile().unwrap();
        let layerstack = open(&profile.layerstack, StoreRole::LayerStack).unwrap();
        let branch = open(&profile.branch, StoreRole::Branch).unwrap();
        assert_eq!(census(&layerstack), (6, 25));
        assert_eq!(census(&branch), (9, 33));
        assert_eq!(
            parent_store_id(&profile.branch).unwrap(),
            store_id(&profile.layerstack, StoreRole::LayerStack).unwrap()
        );
        assert_eq!(count(&layerstack, "layer_stacks"), 3);
        assert_eq!(count(&layerstack, "layers"), 37);
        assert!(count(&branch, "branch_scopes") > 0);
        assert!(count(&branch, "objects") > 0);
        let authority_violations: i64 = layerstack
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        let branch_violations: i64 = branch
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((authority_violations, branch_violations), (0, 0));
    }

    fn census(connection: &Connection) -> (i64, i64) {
        connection
            .query_row(
                "SELECT count(*),sum((SELECT count(*) FROM pragma_table_info(s.name)))
                 FROM sqlite_schema s WHERE type='table' AND name NOT LIKE 'sqlite_%'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap()
    }

    fn count(connection: &Connection, table: &str) -> i64 {
        connection
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap()
    }
}
