use super::{external_id, object_id, open, Databases};
use crate::fixture::{BranchRecord, CommitRecord, LayerRecord, MockState, ProjectRecord};
use crate::model::{BranchOrigin, BranchRelation, RemotePlacement};
use crate::workspace::{canonical_tree, Tree, TreeEntry};
use crate::{
    BranchId, CliError, CliResult, CommitId, EntityName, LayerId, LayerStackId, StoreRole,
};
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};

type RawId = Vec<u8>;

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawLayer {
    id: RawId,
    stack: RawId,
    parent: Option<RawId>,
    root: RawId,
    source_branch: Option<RawId>,
    source_commit: Option<RawId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawCommit {
    id: RawId,
    root: RawId,
    parent: Option<RawId>,
    base_layer: RawId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RawBranch {
    id: RawId,
    stack: RawId,
    name: String,
    base_layer: Option<RawId>,
    head: Option<RawId>,
    from_layer: Option<RawId>,
    from_branch: Option<RawId>,
    from_commit: Option<RawId>,
}

#[derive(Clone)]
struct RawScope {
    local: bool,
    mode: Option<RemotePlacement>,
}

pub(super) fn state(databases: &Databases) -> CliResult<MockState> {
    let profile = databases.profile()?;
    let authority = open(&profile.layerstack, StoreRole::LayerStack)?;
    let work = open(&profile.branch, StoreRole::Branch)?;
    let stacks = load_stacks(&authority)?;
    let authority_layers = load_layers(&authority)?;
    let work_layers = load_layers(&work)?;
    let authority_branches = load_branches(&authority)?;
    let work_branches = load_branches(&work)?;
    let authority_commits = load_commits(&authority)?;
    let work_commits = load_commits(&work)?;
    let scopes = load_branch_scopes(&work)?;
    let stack_scopes = load_stack_scopes(&work)?;
    let complete_roots = load_ids(&work, "SELECT root_id FROM complete_roots")?;
    let objects = merge_objects(load_objects(&authority)?, load_objects(&work)?)?;

    let stack_ids = typed_ids::<LayerStackId>(0x31, stacks.iter().map(|row| &row.0))?;
    let layer_ids = typed_ids::<LayerId>(0x32, authority_layers.iter().map(|row| &row.id))?;
    let branch_order = ordered_union(&work_branches, &authority_branches, |row| &row.id);
    let branch_ids = typed_ids::<BranchId>(0x11, branch_order.iter().map(|row| &row.id))?;
    let raw_commits = merge_rows(authority_commits, work_commits)?;
    let commit_ids = typed_ids::<CommitId>(0x12, raw_commits.keys())?;
    let layer_numbers = layer_numbers(&stacks, &authority_layers)?;
    verify_work_layers(&work_layers, &authority_layers)?;
    let commit_trees = commit_trees(&raw_commits, &objects)?;

    let layers = authority_layers
        .iter()
        .map(|raw| {
            let tree = decode_tree(&raw.root, &objects)?;
            let (root, canonical_objects) = canonical_tree(&tree);
            verify_root(&root, &raw.root)?;
            Ok(LayerRecord {
                id: get(&layer_ids, &raw.id, "Layer")?,
                project_id: get(&stack_ids, &raw.stack, "LayerStack")?,
                number: *layer_numbers
                    .get(&raw.id)
                    .ok_or_else(|| CliError::Integrity("Layer number".into()))?,
                source: match (&raw.source_branch, &raw.source_commit) {
                    (Some(branch), Some(commit)) => Some((
                        get(&branch_ids, branch, "source Branch")?,
                        get(&commit_ids, commit, "source Commit")?,
                    )),
                    (None, None) => None,
                    _ => return Err(CliError::Integrity("Layer source".into())),
                },
                root,
                tree,
                objects: canonical_objects,
            })
        })
        .collect::<CliResult<Vec<_>>>()?;

    let accepted = authority_layers
        .iter()
        .filter_map(|layer| {
            Some((
                layer.source_commit.clone()?,
                layer_ids.get(&layer.id)?.clone(),
            ))
        })
        .collect::<HashMap<_, _>>();
    let authority_commit_ids = load_ids(&authority, "SELECT commit_id FROM commits")?;
    let work_commit_ids = load_ids(&work, "SELECT commit_id FROM commits")?;
    let authority_branch_map = by_id(&authority_branches);
    let work_branch_map = by_id(&work_branches);
    let work_complete = complete_roots.iter().cloned().collect::<HashSet<_>>();
    let mut branches = Vec::with_capacity(branch_order.len());
    for raw in branch_order {
        let authority_row = authority_branch_map.get(&raw.id).copied();
        let work_row = work_branch_map.get(&raw.id).copied();
        verify_branch_copies(authority_row, work_row)?;
        let boundary = raw.from_commit.as_ref();
        let authority_chain = owned_chain(
            authority_row.and_then(|row| row.head.as_ref()),
            boundary,
            &raw_commits,
        )?;
        let work_chain = owned_chain(
            work_row.and_then(|row| row.head.as_ref()),
            boundary,
            &raw_commits,
        )?;
        let chain = compatible_chain(&authority_chain, &work_chain)?;
        let authority_set = authority_chain.iter().collect::<HashSet<_>>();
        let work_set = work_chain.iter().collect::<HashSet<_>>();
        let commits = chain
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let raw_commit = raw_commits
                    .get(id)
                    .ok_or_else(|| CliError::Integrity("Branch Commit".into()))?;
                let tree = commit_trees
                    .get(id)
                    .ok_or_else(|| CliError::Integrity("Commit tree".into()))?
                    .clone();
                let (root, objects) = canonical_tree(&tree);
                verify_root(&root, &raw_commit.root)?;
                if raw
                    .base_layer
                    .as_ref()
                    .is_some_and(|base| base != &raw_commit.base_layer)
                {
                    return Err(CliError::Integrity("Commit base Layer".into()));
                }
                Ok(CommitRecord {
                    id: get(&commit_ids, id, "Commit")?,
                    number: (index + 1) as u16,
                    authority: authority_set.contains(id) && authority_commit_ids.contains(id),
                    work: work_set.contains(id) && work_commit_ids.contains(id),
                    inherited: false,
                    owned: true,
                    accepted_layer: accepted.get(id).cloned(),
                    root,
                    tree,
                    objects,
                })
            })
            .collect::<CliResult<Vec<_>>>()?;
        let authority_head = head_number(authority_row.and_then(|row| row.head.as_ref()), &chain);
        let work_head = head_number(work_row.and_then(|row| row.head.as_ref()), &chain);
        let relation = relation(
            authority_row.is_some(),
            work_row.is_some(),
            scopes.get(&raw.id),
            authority_head,
            work_head,
        )?;
        let remote_complete_through = match relation {
            BranchRelation::RemoteCurrent {
                mode: RemotePlacement::Replica,
            }
            | BranchRelation::RemotePullBehind {
                mode: RemotePlacement::Replica,
                ..
            } => chain
                .iter()
                .enumerate()
                .filter(|(_, id)| {
                    raw_commits
                        .get(*id)
                        .is_some_and(|commit| work_complete.contains(&commit.root))
                })
                .map(|(index, _)| (index + 1) as u16)
                .max(),
            _ => None,
        };
        let origin = match (&raw.from_layer, &raw.from_branch, &raw.from_commit) {
            (Some(layer), None, None) => {
                BranchOrigin::Layer(get(&layer_ids, layer, "origin Layer")?)
            }
            (None, Some(branch), Some(commit)) => BranchOrigin::Commit(
                get(&branch_ids, branch, "origin Branch")?,
                get(&commit_ids, commit, "origin Commit")?,
            ),
            _ => return Err(CliError::Integrity("Branch origin".into())),
        };
        branches.push(BranchRecord {
            id: get(&branch_ids, &raw.id, "Branch")?,
            project_id: get(&stack_ids, &raw.stack, "LayerStack")?,
            name: EntityName::parse(raw.name.clone())
                .map_err(|error| CliError::Integrity(error.into()))?,
            origin,
            authority_head,
            work_head,
            remote_complete_through,
            boundary_commit: raw
                .from_commit
                .as_ref()
                .map(|id| get(&commit_ids, id, "boundary Commit"))
                .transpose()?,
            effective_base: raw
                .base_layer
                .as_ref()
                .map(|id| get(&layer_ids, id, "base Layer"))
                .transpose()?,
            relation,
            commits,
        });
    }

    let complete = complete_roots.iter().collect::<HashSet<_>>();
    let projects = stacks
        .into_iter()
        .map(|(raw_id, name, head)| {
            let authority_count = *layer_numbers
                .get(&head)
                .ok_or_else(|| CliError::Integrity("LayerStack head".into()))?;
            let (work_layers, mode) = stack_scopes
                .get(&raw_id)
                .map(|(layer, mode)| {
                    Ok((
                        Some(*layer_numbers.get(layer).ok_or_else(|| {
                            CliError::Integrity("LayerStack work boundary".into())
                        })?),
                        Some(*mode),
                    ))
                })
                .transpose()?
                .unwrap_or((None, None));
            let complete_roots = complete_layer_count(
                &raw_id,
                work_layers.unwrap_or(0),
                &authority_layers,
                &layer_numbers,
                &complete,
            )?;
            Ok(ProjectRecord {
                id: get(&stack_ids, &raw_id, "LayerStack")?,
                name: EntityName::parse(name).map_err(|error| CliError::Integrity(error.into()))?,
                authority_layers: authority_count,
                work_layers,
                mode,
                complete_roots,
                authority_available: true,
                observed: "persisted".into(),
            })
        })
        .collect::<CliResult<Vec<_>>>()?;

    let local_branches = scopes.values().filter(|scope| scope.local).count() as u16;
    let mut result = MockState::empty();
    result.projects = projects;
    result.layers = layers;
    result.branches = branches;
    result.next_branch = 90_u16.saturating_add(local_branches);
    result.next_workspace = 90;
    databases.decorate_storage(&mut result)?;
    Ok(result)
}

fn load_stacks(connection: &Connection) -> CliResult<Vec<(RawId, String, RawId)>> {
    connection
        .prepare("SELECT layer_stack_id,name,head_layer_id FROM layer_stacks ORDER BY rowid")
        .map_err(db)?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(db)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db)
}

fn load_layers(connection: &Connection) -> CliResult<Vec<RawLayer>> {
    connection
        .prepare(
            "SELECT layer_id,layer_stack_id,parent_layer_id,root_id,source_branch_id,source_commit_id
             FROM layers ORDER BY rowid",
        )
        .map_err(db)?
        .query_map([], |row| {
            Ok(RawLayer {
                id: row.get(0)?,
                stack: row.get(1)?,
                parent: row.get(2)?,
                root: row.get(3)?,
                source_branch: row.get(4)?,
                source_commit: row.get(5)?,
            })
        })
        .map_err(db)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db)
}

fn load_commits(connection: &Connection) -> CliResult<Vec<RawCommit>> {
    connection
        .prepare("SELECT commit_id,root_id,parent_commit_id,base_layer_id FROM commits")
        .map_err(db)?
        .query_map([], |row| {
            Ok(RawCommit {
                id: row.get(0)?,
                root: row.get(1)?,
                parent: row.get(2)?,
                base_layer: row.get(3)?,
            })
        })
        .map_err(db)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db)
}

fn load_branches(connection: &Connection) -> CliResult<Vec<RawBranch>> {
    connection
        .prepare(
            "SELECT branch_id,layer_stack_id,name,base_layer_id,head_commit_id,
                    forked_from_layer_id,forked_from_branch_id,forked_from_commit_id
             FROM branches ORDER BY rowid",
        )
        .map_err(db)?
        .query_map([], |row| {
            Ok(RawBranch {
                id: row.get(0)?,
                stack: row.get(1)?,
                name: row.get(2)?,
                base_layer: row.get(3)?,
                head: row.get(4)?,
                from_layer: row.get(5)?,
                from_branch: row.get(6)?,
                from_commit: row.get(7)?,
            })
        })
        .map_err(db)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db)
}

fn load_objects(connection: &Connection) -> CliResult<HashMap<RawId, Vec<u8>>> {
    connection
        .prepare("SELECT object_id,bytes FROM objects")
        .map_err(db)?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(db)?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(db)
}

fn load_ids(connection: &Connection, query: &str) -> CliResult<HashSet<RawId>> {
    connection
        .prepare(query)
        .map_err(db)?
        .query_map([], |row| row.get(0))
        .map_err(db)?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(db)
}

fn load_branch_scopes(connection: &Connection) -> CliResult<HashMap<RawId, RawScope>> {
    connection
        .prepare("SELECT branch_id,scope_kind,serving_mode FROM branch_scopes")
        .map_err(db)?
        .query_map([], |row| {
            let id = row.get(0)?;
            let kind: String = row.get(1)?;
            let mode: Option<String> = row.get(2)?;
            Ok((id, (kind, mode)))
        })
        .map_err(db)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db)?
        .into_iter()
        .map(|(id, (kind, mode))| {
            Ok((
                id,
                RawScope {
                    local: kind == "local",
                    mode: mode.as_deref().map(parse_mode).transpose()?,
                },
            ))
        })
        .collect()
}

fn load_stack_scopes(
    connection: &Connection,
) -> CliResult<HashMap<RawId, (RawId, RemotePlacement)>> {
    connection
        .prepare("SELECT layer_stack_id,through_layer_id,serving_mode FROM layer_stack_scopes")
        .map_err(db)?
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get::<_, String>(2)?))
        })
        .map_err(db)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db)?
        .into_iter()
        .map(|(stack, layer, mode)| Ok((stack, (layer, parse_mode(&mode)?))))
        .collect()
}

fn parse_mode(value: &str) -> CliResult<RemotePlacement> {
    match value {
        "reference" => Ok(RemotePlacement::Reference),
        "replica" => Ok(RemotePlacement::Replica),
        _ => Err(CliError::Integrity("serving mode".into())),
    }
}

fn layer_numbers(
    stacks: &[(RawId, String, RawId)],
    layers: &[RawLayer],
) -> CliResult<HashMap<RawId, u16>> {
    let mut numbers = HashMap::new();
    for (stack, _, head) in stacks {
        let mut current = layers
            .iter()
            .find(|layer| &layer.stack == stack && layer.parent.is_none())
            .ok_or_else(|| CliError::Integrity("genesis Layer".into()))?;
        for number in 1..=u16::MAX {
            if numbers.insert(current.id.clone(), number).is_some() {
                return Err(CliError::Integrity("Layer cycle".into()));
            }
            if &current.id == head {
                break;
            }
            current = layers
                .iter()
                .find(|layer| &layer.stack == stack && layer.parent.as_ref() == Some(&current.id))
                .ok_or_else(|| CliError::Integrity("Layer chain".into()))?;
        }
    }
    if numbers.len() != layers.len() {
        return Err(CliError::Integrity("unreachable Layer".into()));
    }
    Ok(numbers)
}

fn commit_trees(
    commits: &HashMap<RawId, RawCommit>,
    objects: &HashMap<RawId, Vec<u8>>,
) -> CliResult<HashMap<RawId, Tree>> {
    commits
        .iter()
        .map(|(id, commit)| Ok((id.clone(), decode_tree(&commit.root, objects)?)))
        .collect()
}

fn decode_tree(root: &[u8], objects: &HashMap<RawId, Vec<u8>>) -> CliResult<Tree> {
    let manifest = objects
        .get(root)
        .ok_or_else(|| CliError::Integrity("root object payload".into()))?;
    let mut tree = Tree::new();
    for line in manifest
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let separator = line
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| CliError::Integrity("root manifest path".into()))?;
        let path = std::str::from_utf8(&line[..separator])
            .map_err(|_| CliError::Integrity("root manifest UTF-8".into()))?
            .to_owned();
        let tag = *line
            .get(separator + 1)
            .ok_or_else(|| CliError::Integrity("root manifest tag".into()))?;
        let child = std::str::from_utf8(
            line.get(separator + 2..)
                .ok_or_else(|| CliError::Integrity("root manifest object".into()))?,
        )
        .map_err(|_| CliError::Integrity("root manifest object ID".into()))?;
        let entry = match tag {
            b'd' => TreeEntry::Directory,
            b'f' => TreeEntry::File(
                objects
                    .get(&object_id(child))
                    .ok_or_else(|| CliError::Integrity("file object payload".into()))?
                    .clone(),
            ),
            _ => return Err(CliError::Integrity("root manifest entry".into())),
        };
        if tree.insert(path, entry).is_some() {
            return Err(CliError::Integrity("duplicate root path".into()));
        }
    }
    Ok(tree)
}

fn verify_root(root: &crate::ObjectId, raw: &[u8]) -> CliResult<()> {
    if object_id(root.as_str()) == raw {
        Ok(())
    } else {
        Err(CliError::Integrity("canonical root identity".into()))
    }
}

fn owned_chain(
    head: Option<&RawId>,
    boundary: Option<&RawId>,
    commits: &HashMap<RawId, RawCommit>,
) -> CliResult<Vec<RawId>> {
    let Some(mut current) = head.cloned() else {
        return Ok(Vec::new());
    };
    let mut reversed = Vec::new();
    let mut seen = HashSet::new();
    while Some(&current) != boundary {
        if !seen.insert(current.clone()) {
            return Err(CliError::Integrity("Commit cycle".into()));
        }
        let commit = commits
            .get(&current)
            .ok_or_else(|| CliError::Integrity("Commit ancestry".into()))?;
        reversed.push(current);
        let Some(parent) = commit.parent.clone() else {
            if boundary.is_some() {
                return Err(CliError::Integrity("Commit boundary".into()));
            }
            break;
        };
        current = parent;
    }
    reversed.reverse();
    Ok(reversed)
}

fn compatible_chain(authority: &[RawId], work: &[RawId]) -> CliResult<Vec<RawId>> {
    if authority.starts_with(work) {
        Ok(authority.to_vec())
    } else if work.starts_with(authority) {
        Ok(work.to_vec())
    } else {
        Err(CliError::Integrity("divergent persisted Branch".into()))
    }
}

fn relation(
    authority: bool,
    work: bool,
    scope: Option<&RawScope>,
    authority_head: Option<u16>,
    work_head: Option<u16>,
) -> CliResult<BranchRelation> {
    if authority && !work {
        return Ok(BranchRelation::AuthorityOnly);
    }
    let scope = scope.ok_or_else(|| CliError::Integrity("Branch scope".into()))?;
    if scope.local {
        return Ok(match (authority_head, work_head) {
            (None, _) => BranchRelation::LocalOnly,
            (Some(authority), Some(work)) if authority == work => BranchRelation::LocalCurrent,
            (Some(authority), Some(work)) if authority < work => BranchRelation::LocalPushAhead {
                commits: work - authority,
            },
            (Some(authority), Some(work)) => BranchRelation::AuthorityAhead {
                commits: authority - work,
            },
            _ => BranchRelation::Integrity,
        });
    }
    let mode = scope
        .mode
        .ok_or_else(|| CliError::Integrity("remote serving mode".into()))?;
    Ok(match (authority_head, work_head) {
        (Some(authority), Some(work)) if authority == work => {
            BranchRelation::RemoteCurrent { mode }
        }
        (Some(authority), Some(work)) if authority > work => BranchRelation::RemotePullBehind {
            mode,
            commits: authority - work,
        },
        _ => BranchRelation::Integrity,
    })
}

fn head_number(head: Option<&RawId>, chain: &[RawId]) -> Option<u16> {
    head.and_then(|head| {
        chain
            .iter()
            .position(|commit| commit == head)
            .map(|index| (index + 1) as u16)
    })
}

fn typed_ids<'a, T: for<'b> From<&'b str> + Clone>(
    tag: u8,
    ids: impl IntoIterator<Item = &'a RawId>,
) -> CliResult<HashMap<RawId, T>> {
    ids.into_iter()
        .map(|raw| {
            let value = external_id(tag, raw)?;
            Ok((raw.clone(), T::from(value.as_str())))
        })
        .collect()
}

fn get<T: Clone>(values: &HashMap<RawId, T>, id: &RawId, label: &str) -> CliResult<T> {
    values
        .get(id)
        .cloned()
        .ok_or_else(|| CliError::Integrity(label.into()))
}

fn merge_rows(
    authority: Vec<RawCommit>,
    work: Vec<RawCommit>,
) -> CliResult<HashMap<RawId, RawCommit>> {
    let mut rows = authority
        .into_iter()
        .map(|row| (row.id.clone(), row))
        .collect::<HashMap<_, _>>();
    for row in work {
        if let Some(existing) = rows.get(&row.id) {
            if existing != &row {
                return Err(CliError::Integrity("Commit copies disagree".into()));
            }
        } else {
            rows.insert(row.id.clone(), row);
        }
    }
    Ok(rows)
}

fn merge_objects(
    mut authority: HashMap<RawId, Vec<u8>>,
    work: HashMap<RawId, Vec<u8>>,
) -> CliResult<HashMap<RawId, Vec<u8>>> {
    for (id, bytes) in work {
        if authority
            .get(&id)
            .is_some_and(|existing| existing != &bytes)
        {
            return Err(CliError::Integrity("object copies disagree".into()));
        }
        authority.entry(id).or_insert(bytes);
    }
    Ok(authority)
}

fn ordered_union<'a, T>(first: &'a [T], second: &'a [T], id: impl Fn(&T) -> &RawId) -> Vec<&'a T> {
    let mut seen = HashSet::new();
    first
        .iter()
        .chain(second)
        .filter(|row| seen.insert(id(row).clone()))
        .collect()
}

fn by_id<T>(rows: &[T]) -> HashMap<RawId, &T>
where
    T: HasId,
{
    rows.iter().map(|row| (row.id().clone(), row)).collect()
}

trait HasId {
    fn id(&self) -> &RawId;
}

impl HasId for RawBranch {
    fn id(&self) -> &RawId {
        &self.id
    }
}

fn verify_branch_copies(authority: Option<&RawBranch>, work: Option<&RawBranch>) -> CliResult<()> {
    if let (Some(authority), Some(work)) = (authority, work) {
        let same = authority.id == work.id
            && authority.stack == work.stack
            && authority.name == work.name
            && authority.base_layer == work.base_layer
            && authority.from_layer == work.from_layer
            && authority.from_branch == work.from_branch
            && authority.from_commit == work.from_commit;
        if !same {
            return Err(CliError::Integrity("Branch copies disagree".into()));
        }
    }
    Ok(())
}

fn complete_layer_count(
    stack: &RawId,
    count: u16,
    layers: &[RawLayer],
    numbers: &HashMap<RawId, u16>,
    complete: &HashSet<&RawId>,
) -> CliResult<u16> {
    let mut result = 0;
    for number in 1..=count {
        let layer = layers
            .iter()
            .find(|layer| &layer.stack == stack && numbers.get(&layer.id) == Some(&number))
            .ok_or_else(|| CliError::Integrity("complete Layer chain".into()))?;
        if !complete.contains(&layer.root) {
            break;
        }
        result = number;
    }
    Ok(result)
}

fn verify_work_layers(work: &[RawLayer], authority: &[RawLayer]) -> CliResult<()> {
    for layer in work {
        let source = authority
            .iter()
            .find(|candidate| candidate.id == layer.id)
            .ok_or_else(|| CliError::Integrity("work Layer not in authority".into()))?;
        if source != layer {
            return Err(CliError::Integrity("Layer copies disagree".into()));
        }
    }
    Ok(())
}

fn db(error: impl std::fmt::Display) -> CliError {
    CliError::Database(error.to_string())
}
