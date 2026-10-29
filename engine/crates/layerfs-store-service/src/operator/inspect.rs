//! The read model behind `sandbox.list_workspaces({ project, since? })`.
//!
//! One answer holds the project's Branches, its newest Commits and Layers, and
//! this daemon's Workspaces on the project. `backend.store.project.get_history` is the
//! same answer with no Workspaces, read from the Store with no daemon, so a
//! reader starts no Sandbox to show history. It is not agent-facing. Commits
//! and Layers are newest first within one control record; `more` gives the
//! cursor `branch.list_commits` continues each Branch from, and `moreLayers`
//! says older Layers were left out. `version` is a digest of the answer, so a
//! caller polling with `since` gets `unchanged` until something it shows
//! changes: a Commit, merge, publish, fork, mount, unmount or Workspace write.
use crate::history::lookup::{self as args, hex, layer_value, Answer};
use crate::history::MAIN;
use crate::store::Store;
use layerfs_bridge::control::SdkValue;
use layerfs_bridge::sdk_values::{commit_value_with_budget, value_cost};
use layerfs_history::{
    BranchId, BranchSnapshot, CommitHistoryRequest, CommitId, LayerId, MAXIMUM_PAGE_RECORDS,
};
use std::collections::{BTreeMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

/// Encoded bytes the answer may use, below one control record.
const BUDGET: usize = 56 * 1024;
/// Bytes the Layers may use.
const LAYER_BUDGET: usize = 12 * 1024;
/// Estimated bytes of one Commit value, for page sizes.
const COMMIT_COST: usize = 5400;

/// One Workspace this daemon holds, copied from its control registry.
pub struct WorkspaceView {
    pub workspace: String,
    pub namespace: i64,
    pub binding: BranchSnapshot,
    pub activity: &'static str,
    pub mounted: bool,
    pub directory: Option<String>,
    /// Changed inodes and names not yet captured, when observed.
    pub dirty: Option<(u64, u64)>,
    pub pending_merge: bool,
    pub revision: Option<i64>,
}

fn workspace_value(
    view: &WorkspaceView,
    names: &BTreeMap<BranchId, (String, Option<CommitId>)>,
) -> SdkValue {
    let binding = &view.binding;
    let (name, head) = names
        .get(&binding.branch.id)
        .map_or((None, None), |(name, head)| {
            (Some(name.clone()), Some(*head))
        });
    SdkValue::map([
        ("workspace", SdkValue::text(view.workspace.as_str())),
        ("namespace", SdkValue::Int(view.namespace)),
        ("branch", SdkValue::opt_text(name)),
        (
            "base",
            SdkValue::opt_text(binding.branch.head_commit.map(|id| hex(id.as_slice()))),
        ),
        (
            "baseLayer",
            SdkValue::text(hex(binding.branch.base_layer.as_slice())),
        ),
        (
            "root",
            SdkValue::text(hex(binding.effective_root.as_bytes())),
        ),
        ("activity", SdkValue::text(view.activity)),
        ("mounted", SdkValue::Bool(view.mounted)),
        ("directory", SdkValue::opt_text(view.directory.clone())),
        (
            "headMovedSinceMount",
            head.map_or(SdkValue::Null, |head| {
                SdkValue::Bool(head != binding.branch.head_commit)
            }),
        ),
        (
            "dirty",
            view.dirty.map_or(SdkValue::Null, |(inodes, entries)| {
                SdkValue::map([
                    ("inodes", SdkValue::Int(inodes as i64)),
                    ("entries", SdkValue::Int(entries as i64)),
                ])
            }),
        ),
        ("pendingMerge", SdkValue::Bool(view.pending_merge)),
        (
            "revision",
            view.revision.map_or(SdkValue::Null, SdkValue::Int),
        ),
    ])
}

/// One Branch's name and head, from the per-call memo or, on a miss, from a
/// single point read. Snapshot callers use this for names outside the
/// displayed page: mounted Workspaces, budgeted Layer sources, and returned
/// Commits. The memo lives for one call only, so nothing persists and
/// nothing needs invalidation.
fn branch_name(
    store: &Arc<Store>,
    names: &mut BTreeMap<BranchId, (String, Option<CommitId>)>,
    id: BranchId,
) -> Result<Option<String>, layerfs_bridge::control::SdkRefusal> {
    if let Some((name, _)) = names.get(&id) {
        return Ok(Some(name.clone()));
    }
    match store.history().branch(id).map_err(args::history)? {
        Some(branch) => {
            let name = branch.name.as_str().to_owned();
            names.insert(id, (name.clone(), branch.head_commit));
            Ok(Some(name))
        }
        None => Ok(None),
    }
}

/// The snapshot answer, or `unchanged` when `since` names its version.
pub fn snapshot(store: &Arc<Store>, args: &SdkValue, views: &[WorkspaceView]) -> Answer {
    let stack = super::args::project(store, args)?;
    let page = store
        .history()
        .branches(
            stack.id,
            &layerfs_history::Page {
                cursor: None,
                limit: 64,
            },
        )
        .map_err(args::history)?;
    let more_branches = page.continuation.is_some();
    let branches = page
        .records
        .iter()
        .filter(|branch| branch.name.as_str() != MAIN)
        .cloned()
        .collect::<Vec<_>>();
    // Names are seeded from the displayed page only. The snapshot used to
    // follow every Branch continuation here, so a cheap status call paid
    // one provider page per ~81 Branches before answering anything. Names
    // outside the page resolve lazily, at the single site that needs them.
    let mut names = BTreeMap::new();
    names.extend(page.records.iter().map(|branch| {
        (
            branch.id,
            (branch.name.as_str().to_owned(), branch.head_commit),
        )
    }));
    // The page is bounded, but a Workspace may be mounted on a Branch outside
    // it. Resolve just those mounted identities, so the listing does not
    // depend on how many Branches the project has. The page still reports
    // `moreBranches`; the Workspace answer needs the name and current head.
    for view in views {
        if view.binding.branch.stack != stack.id {
            continue;
        }
        branch_name(store, &mut names, view.binding.branch.id)?;
    }
    let mut spent = 0usize;
    let mut take = |value: SdkValue| {
        spent += value_cost(&value);
        value
    };
    let mut branch_items = vec![take(SdkValue::map([
        ("name", SdkValue::text(MAIN)),
        ("base", SdkValue::text(hex(stack.head_layer.as_slice()))),
        ("head", SdkValue::Null),
    ]))];
    for branch in &branches {
        branch_items.push(take(SdkValue::map([
            ("name", SdkValue::text(branch.name.as_str())),
            (
                "forkedFrom",
                branch
                    .forked_from
                    .map_or(SdkValue::Null, |node| SdkValue::text(args::node_text(node))),
            ),
            ("base", SdkValue::text(hex(branch.base_layer.as_slice()))),
            (
                "head",
                SdkValue::opt_text(branch.head_commit.map(|id| hex(id.as_slice()))),
            ),
        ])));
    }
    let workspaces = views
        .iter()
        .filter(|view| view.binding.branch.stack == stack.id)
        .map(|view| take(workspace_value(view, &names)))
        .collect::<Vec<_>>();
    let mut layers = Vec::new();
    let mut layer_spent = 0usize;
    let mut next: Option<LayerId> = Some(stack.head_layer);
    while let Some(id) = next {
        let record = args::layer(store, id)?;
        let source = match record.source_branch {
            Some(branch) => branch_name(store, &mut names, branch)?,
            None => None,
        };
        let value = layer_value(&record, source.as_deref());
        if layer_spent + value_cost(&value) > LAYER_BUDGET {
            break;
        }
        layer_spent += value_cost(&value);
        layers.push(value);
        next = record.parent;
    }
    let more_layers = next.is_some();
    spent += layer_spent;
    let mut commits = Vec::new();
    let mut seen = HashSet::new();
    let mut more = Vec::new();
    for branch in &branches {
        if branch.head_commit.is_none() {
            continue;
        }
        let room = BUDGET.saturating_sub(spent) / COMMIT_COST;
        if room == 0 {
            more.push((branch.name.as_str().to_owned(), SdkValue::Null));
            continue;
        }
        let page = store
            .history()
            .commit_history(&CommitHistoryRequest {
                branch: branch.id,
                start: None,
                cursor: None,
                limit: room.min(usize::from(MAXIMUM_PAGE_RECORDS)) as u16,
            })
            .map_err(args::history)?;
        // A rejected Commit yields exactly one continuation. Provider pages
        // hold one Commit each (capacity is the 16 KiB result budget over the
        // 12,608-byte maximal Commit width), so a rejection means nothing was
        // emitted for this Branch: restart (Null) replays nothing for it and
        // yields the rejected Commit first. The provider cursor resumes after
        // the page's record, so it would skip the rejected Commit and must
        // not also be pushed; pushing both produced a duplicate map key,
        // which the control decoder rejects as "SDK field repeated".
        // NOTE: when finding 4 packs provider pages by actual encoded size,
        // a page can hold several Commits and a mid-page rejection needs a
        // resume-at-rejected-Commit cursor instead of restart; revisit then.
        let mut rejected = false;
        for record in &page.records {
            if seen.insert(record.id) {
                let name = match record.meta.branch {
                    Some(id) => branch_name(store, &mut names, id)?,
                    None => None,
                };
                let value = commit_value_with_budget(record, name.as_deref(), BUDGET);
                if spent + value_cost(&value) > BUDGET {
                    rejected = true;
                    break;
                }
                spent += value_cost(&value);
                commits.push(value);
            }
        }
        if rejected {
            more.push((branch.name.as_str().to_owned(), SdkValue::Null));
        } else if let Some(cursor) = page.continuation.as_deref() {
            more.push((branch.name.as_str().to_owned(), SdkValue::text(hex(cursor))));
        }
    }
    let body = vec![
        (
            "store".to_owned(),
            SdkValue::map([("stats", super::store::stats(store)?)]),
        ),
        ("project".to_owned(), SdkValue::text(stack.name.as_str())),
        ("branches".to_owned(), SdkValue::List(branch_items)),
        ("moreBranches".to_owned(), SdkValue::Bool(more_branches)),
        ("commits".to_owned(), SdkValue::List(commits)),
        ("more".to_owned(), SdkValue::Map(more)),
        ("layers".to_owned(), SdkValue::List(layers)),
        ("moreLayers".to_owned(), SdkValue::Bool(more_layers)),
        ("workspaces".to_owned(), SdkValue::List(workspaces)),
    ];
    let mut hasher = DefaultHasher::new();
    body.hash(&mut hasher);
    let version = format!("{:016x}", hasher.finish());
    if args.get_text("since") == Some(version.as_str()) {
        return Ok(SdkValue::map([
            ("kind", SdkValue::text("unchanged")),
            ("version", SdkValue::Text(version)),
        ]));
    }
    let mut fields = vec![
        ("kind".to_owned(), SdkValue::text("ok")),
        ("version".to_owned(), SdkValue::Text(version)),
    ];
    fields.extend(body);
    Ok(SdkValue::Map(fields))
}
