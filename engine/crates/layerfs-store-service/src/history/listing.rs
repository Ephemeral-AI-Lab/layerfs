//! The paged lists of history: a project's Branches in name order, a
//! Branch's Commits newest first, and one Commit's record.
//!
//! A page is at most `limit` rows and ends early when its rows reach
//! [`PAGE_BYTES`]; it then answers `next`, the place the page after it
//! starts from: the last name, or the last Commit, it listed. A list never
//! refuses `Capacity` for its size. Branch names are read through the name
//! index from that place on, so a page reads its own rows however many
//! Branches come before them; a Commit list follows first parents from its
//! place. Rows carry the keys the call surface answers (camelCase here); a
//! Commit's time is `createdMs`, which the caller writes as a date.
use super::lookup::{self as args, hex, invalid, node_text, refuse, Answer};
use super::resolve::{self, Saved};
use super::{saved::reply_cost, MAIN};
use crate::store::Store;
use layerfs_history::{
    BranchId, BranchRecord, CommitRecord, CommitVia, LayerRecord, LayerStackRecord, NodeId,
    MAXIMUM_PAGE_RECORDS,
};
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use std::collections::BTreeMap;
use std::sync::Arc;

type Result<T> = std::result::Result<T, SdkRefusal>;

/// The rows of one page when no limit is given.
pub const DEFAULT_LIMIT: usize = 100;
/// The most rows one page may ask for.
pub const MAX_LIMIT: usize = 1000;
/// The size past which a page ends early, with a place to go on from.
pub const PAGE_BYTES: usize = 64 << 10;
/// The characters of a Commit message a list row shows.
const MESSAGE: usize = 128;
/// The bytes of overwritten paths one Commit's record shows.
const OVERWRITTEN_BYTES: usize = 4096;
/// A character above every character of a Branch name: `prefix` followed
/// by it sorts after every name that starts with `prefix`.
const ABOVE: char = '\u{7f}';

/// The `limit` argument: 1 to [`MAX_LIMIT`], [`DEFAULT_LIMIT`] when absent.
pub fn limit(args: &SdkValue) -> Result<usize> {
    match args.get("limit") {
        None | Some(SdkValue::Null) => Ok(DEFAULT_LIMIT),
        Some(SdkValue::Int(n)) if (1..=MAX_LIMIT as i64).contains(n) => Ok(*n as usize),
        Some(_) => Err(invalid(format!("limit must be 1 to {MAX_LIMIT}"))),
    }
}

/// Which Branches a page lists.
#[derive(Clone, Copy, Debug, Default)]
pub struct Names<'a> {
    /// Only names that start with this.
    pub prefix: Option<&'a str>,
    /// No name that starts with this.
    pub exclude: Option<&'a str>,
    /// Only names after this one: the last name of the page before.
    pub after: Option<&'a str>,
}

impl Names<'_> {
    fn lists(&self, name: &str) -> bool {
        self.prefix.is_none_or(|prefix| name.starts_with(prefix))
            && !self
                .exclude
                .is_some_and(|exclude| name.starts_with(exclude))
            && self.after.is_none_or(|after| name > after)
    }
}

/// One page in the making: its rows, their cost and whether more follow.
struct Page {
    limit: usize,
    spent: usize,
    items: Vec<SdkValue>,
    more: bool,
}

impl Page {
    fn new(limit: usize) -> Self {
        Page {
            limit,
            spent: 0,
            items: Vec::new(),
            more: false,
        }
    }

    /// Adds `row`, or ends the page before it: at its limit, or when the
    /// row does not fit [`PAGE_BYTES`] beside the rows before it.
    fn add(&mut self, row: SdkValue) -> bool {
        let cost = reply_cost(&row);
        let full = self.items.len() == self.limit
            || (!self.items.is_empty() && self.spent + cost > PAGE_BYTES);
        if full {
            self.more = true;
            return false;
        }
        self.spent += cost;
        self.items.push(row);
        true
    }

    /// The page's answer; `next` is where the page after it starts.
    fn answer(self, key: &str) -> Vec<(&'static str, SdkValue)> {
        let next = match (self.more, self.items.last()) {
            (true, Some(last)) => last.get(key).cloned().unwrap_or_default(),
            _ => SdkValue::Null,
        };
        vec![("items", SdkValue::List(self.items)), ("next", next)]
    }
}

/// A Branch as a list shows it: `commit` is its head, else the Layer it
/// was forked at, and `root` that node's tree; `head` is its head Commit
/// alone, for callers that need the Branch's own Commit.
fn branch_item(store: &Arc<Store>, branch: &BranchRecord) -> Result<SdkValue> {
    let node = args::branch_node(branch);
    Ok(SdkValue::map([
        ("branch", SdkValue::text(branch.name.as_str())),
        ("commit", SdkValue::text(node_text(node))),
        (
            "forkedFrom",
            SdkValue::opt_text(branch.forked_from.map(node_text)),
        ),
        ("id", SdkValue::text(hex(branch.id.as_slice()))),
        ("base", SdkValue::text(hex(branch.base_layer.as_slice()))),
        (
            "root",
            SdkValue::text(hex(args::root(store, node)?.as_bytes())),
        ),
        (
            "head",
            SdkValue::opt_text(branch.head_commit.map(|id| hex(id.as_slice()))),
        ),
    ]))
}

/// The project's main line as a list shows it: its head Layer. `main` is the
/// stack's head Layer, but `init` and `add_project` also fork a Branch named
/// `main` at that head; when it exists, the row carries its id so
/// `service::branch_named` resolves the main mount scope. The name index is
/// read directly: `args::branch` names `main` as the line and never a Branch,
/// refusing it (spec 4.4).
fn main_item(store: &Arc<Store>, stack: &LayerStackRecord) -> Result<SdkValue> {
    let head = NodeId::Layer(stack.head_layer);
    let id = recorded_main(store, stack).map_or(SdkValue::Null, |branch| {
        SdkValue::text(hex(branch.id.as_slice()))
    });
    Ok(SdkValue::map([
        ("branch", SdkValue::text(MAIN)),
        ("commit", SdkValue::text(node_text(head))),
        ("forkedFrom", SdkValue::Null),
        ("id", id),
        ("base", SdkValue::text(node_text(head))),
        (
            "root",
            SdkValue::text(hex(args::root(store, head)?.as_bytes())),
        ),
        ("head", SdkValue::Null),
    ]))
}

/// The Branch `stack` records for its main line, when it records one:
/// `init` and `add_project` fork a Branch named `main` at the stack's head.
fn recorded_main(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
) -> Option<layerfs_history::BranchRecord> {
    let name = layerfs_history::HistoryName::new(MAIN).ok()?;
    store.history().branch_named(stack.id, &name).ok().flatten()
}

/// The one Branch `name`, or main, from the name index: a list of one, or
/// of none when no Branch has the name.
pub fn branch(store: &Arc<Store>, stack: &LayerStackRecord, name: &str) -> Answer {
    let found = if name == MAIN {
        Some(main_item(store, stack)?)
    } else {
        match args::branch(store, stack, name) {
            Ok(branch) => Some(branch_item(store, &branch)?),
            Err(refusal) if refusal.code == "NoSuchBranch" => None,
            Err(refusal) => return Err(refusal),
        }
    };
    Ok(SdkValue::map([
        ("items", SdkValue::List(found.into_iter().collect())),
        ("next", SdkValue::Null),
    ]))
}

/// One page of the project's Branches, main among them, in name order.
pub fn branches(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    names: Names<'_>,
    limit: usize,
) -> Answer {
    let mut page = Page::new(limit);
    // main is no Branch row: it takes its place in name order
    let mut main = names.lists(MAIN);
    // where the name index is read from: after the page before, and never
    // before the prefix
    let (mut from, mut inclusive) = match (names.after, names.prefix) {
        (Some(after), Some(prefix)) if prefix > after => (prefix.to_owned(), true),
        (Some(after), _) => (after.to_owned(), false),
        (None, prefix) => (prefix.unwrap_or("").to_owned(), true),
    };
    'read: loop {
        let found = store
            .published(|history| {
                history.branches_from(stack.id, &from, inclusive, MAXIMUM_PAGE_RECORDS)
            })
            .map_err(args::published)?;
        let last = found.len() < usize::from(MAXIMUM_PAGE_RECORDS);
        for record in found {
            let name = record.name.as_str();
            (from, inclusive) = (name.to_owned(), false);
            if name == MAIN {
                continue;
            }
            if names.prefix.is_some_and(|prefix| !name.starts_with(prefix)) {
                // past the last name the prefix starts
                break 'read;
            }
            if main && name > MAIN {
                main = false;
                if !page.add(main_item(store, stack)?) {
                    break 'read;
                }
            }
            if let Some(exclude) = names.exclude.filter(|exclude| name.starts_with(exclude)) {
                // read on after every name the excluded prefix starts
                (from, inclusive) = (format!("{exclude}{ABOVE}"), true);
                continue 'read;
            }
            if !page.add(branch_item(store, &record)?) {
                break 'read;
            }
        }
        if last {
            break;
        }
    }
    if main && !page.more {
        page.add(main_item(store, stack)?);
    }
    crate::read::snapshot::answer(page.answer("branch"))
}

/// The names of the Branches rows name, read once each.
#[derive(Default)]
struct BranchNames(BTreeMap<BranchId, Option<String>>);

impl BranchNames {
    /// The name of Branch `id`, a removed one included.
    fn of(&mut self, store: &Arc<Store>, id: Option<BranchId>) -> Result<Option<String>> {
        let Some(id) = id else {
            return Ok(None);
        };
        if let Some(name) = self.0.get(&id) {
            return Ok(name.clone());
        }
        let name = store
            .published(|history| history.branch_any(id))
            .map_err(args::published)?
            .map(|branch| branch.name.as_str().to_owned());
        self.0.insert(id, name.clone());
        Ok(name)
    }
}

/// What a row is for: a list's (its message cut) or one Commit's record.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Use {
    List,
    Get,
}

/// How a Commit was made, as rows name it.
fn via(via: CommitVia) -> &'static str {
    match via {
        CommitVia::Workspace => "workspace",
        CommitVia::Files => "file",
        CommitVia::Merge => "merge",
    }
}

fn opt_int(value: Option<u64>) -> SdkValue {
    value.map_or(SdkValue::Null, |n| {
        SdkValue::Int(i64::try_from(n).unwrap_or(i64::MAX))
    })
}

/// A message as a row shows it, and whether it was cut.
fn message(text: Option<&str>, used: Use) -> (SdkValue, bool) {
    match text {
        Some(text) if used == Use::List && text.chars().count() > MESSAGE => {
            (SdkValue::Text(text.chars().take(MESSAGE).collect()), true)
        }
        Some(text) => (SdkValue::text(text), false),
        None => (SdkValue::Null, false),
    }
}

/// The paths a newer-write-wins rule decided in `record`, as many as fit
/// [`OVERWRITTEN_BYTES`].
fn overwritten(record: &CommitRecord) -> SdkValue {
    let mut bytes = 0usize;
    let paths = (record.meta.lww.iter())
        .take_while(|fact| {
            bytes += fact.path.len() + 8;
            bytes <= OVERWRITTEN_BYTES
        })
        .map(|fact| SdkValue::text(&fact.path));
    SdkValue::List(paths.collect())
}

fn units(record: &CommitRecord) -> SdkValue {
    let unit = |unit: &layerfs_content::merge::MergeUnit| {
        SdkValue::map([
            ("name", SdkValue::text(&unit.name)),
            (
                "patterns",
                SdkValue::List(unit.patterns.iter().map(SdkValue::text).collect()),
            ),
        ])
    };
    SdkValue::List(record.meta.units.iter().map(unit).collect())
}

fn map(fields: Vec<(&str, SdkValue)>) -> SdkValue {
    SdkValue::Map(
        (fields.into_iter())
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

/// A Commit as a row: a list's, or one Commit's record.
fn commit_row(record: &CommitRecord, branch: Option<&str>, used: Use) -> SdkValue {
    let meta = &record.meta;
    let (text, cut) = message(meta.message.as_deref(), used);
    let mut fields: Vec<(&str, SdkValue)> = vec![
        ("commit", SdkValue::text(hex(record.id.as_slice()))),
        ("message", text),
    ];
    if used == Use::List {
        fields.push(("messageTruncated", SdkValue::Bool(cut)));
    }
    fields.extend([
        ("branch", SdkValue::opt_text(branch)),
        (
            "parent",
            SdkValue::opt_text(record.parent.map(|id| hex(id.as_slice()))),
        ),
        (
            "secondParent",
            SdkValue::opt_text(record.second_parent.map(node_text)),
        ),
        ("base", SdkValue::text(hex(record.base_layer.as_slice()))),
        ("root", SdkValue::text(hex(record.root.as_bytes()))),
        ("via", SdkValue::text(via(meta.via))),
        ("createdMs", SdkValue::Int(meta.time_ms)),
        (
            "mergedFrom",
            SdkValue::opt_text(meta.merged_from.map(|id| hex(id.as_slice()))),
        ),
        ("sourceBranch", SdkValue::Null),
        ("sourceCommit", SdkValue::Null),
        ("size", opt_int(Some(meta.statistics.logical))),
        (
            "newCanonicalBytes",
            opt_int(Some(meta.statistics.new_canonical)),
        ),
        ("newStoredBytes", opt_int(Some(meta.statistics.new_stored))),
        (
            "newMetadataBytes",
            opt_int(Some(meta.statistics.new_metadata)),
        ),
    ]);
    if used == Use::Get {
        fields.extend([
            (
                "displaced",
                SdkValue::opt_text(meta.displaced.map(|id| hex(id.as_slice()))),
            ),
            ("overwritten", overwritten(record)),
            ("overwrittenCount", opt_int(Some(meta.lww.len() as u64))),
            ("units", units(record)),
        ]);
    }
    map(fields)
}

/// A publication of main as a row: the Branch and Commit it came from, no
/// message and no Save of its own.
fn layer_row(record: &LayerRecord, source: Option<&str>, used: Use) -> SdkValue {
    let via = match record.source_commit {
        Some(_) => "publish",
        None => "init",
    };
    let mut fields: Vec<(&str, SdkValue)> = vec![
        ("commit", SdkValue::text(hex(record.id.as_slice()))),
        ("message", SdkValue::Null),
    ];
    if used == Use::List {
        fields.push(("messageTruncated", SdkValue::Bool(false)));
    }
    fields.extend([
        ("branch", SdkValue::text(MAIN)),
        (
            "parent",
            SdkValue::opt_text(record.parent.map(|id| hex(id.as_slice()))),
        ),
        ("secondParent", SdkValue::Null),
        ("base", SdkValue::Null),
        ("root", SdkValue::text(hex(record.root.as_bytes()))),
        ("via", SdkValue::text(via)),
        ("createdMs", SdkValue::Null),
        ("mergedFrom", SdkValue::Null),
        ("sourceBranch", SdkValue::opt_text(source)),
        (
            "sourceCommit",
            SdkValue::opt_text(record.source_commit.map(|id| hex(id.as_slice()))),
        ),
        ("size", opt_int(Some(record.logical))),
        ("newCanonicalBytes", SdkValue::Null),
        ("newStoredBytes", SdkValue::Null),
        ("newMetadataBytes", SdkValue::Null),
    ]);
    if used == Use::Get {
        fields.extend([
            ("displaced", SdkValue::Null),
            ("overwritten", SdkValue::List(Vec::new())),
            ("overwrittenCount", SdkValue::Int(0)),
            ("units", SdkValue::List(Vec::new())),
        ]);
    }
    map(fields)
}

/// One saved node as a row: a Commit with the Branch it was made on, or a
/// publication of main with the Branch it came from.
fn row(store: &Arc<Store>, names: &mut BranchNames, saved: &Saved, used: Use) -> Result<SdkValue> {
    Ok(match saved {
        Saved::Commit(record) => {
            let branch = names.of(store, record.meta.branch)?;
            commit_row(record, branch.as_deref(), used)
        }
        Saved::Layer(record) => {
            let source = names.of(store, record.source_branch)?;
            layer_row(record, source.as_deref(), used)
        }
    })
}

fn not_in_history(what: &str, branch: &str) -> SdkRefusal {
    refuse(
        "NotInHistory",
        format!("{what} is not in the history of {branch}"),
        SdkValue::Null,
    )
}

/// The newest node of `branch`: main's head Layer, else the Branch's head
/// Commit or the Layer it was forked at.
fn head(store: &Arc<Store>, stack: &LayerStackRecord, branch: &str) -> Result<NodeId> {
    if branch == MAIN {
        return Ok(NodeId::Layer(stack.head_layer));
    }
    Ok(args::branch_node(&args::branch(store, stack, branch)?))
}

/// `node`, when the history of `branch` (whose newest node is `head`) holds
/// it; on main only a publication is in history.
pub fn held(store: &Arc<Store>, branch: &str, head: NodeId, node: NodeId) -> Result<NodeId> {
    let kind_fits = branch != MAIN || matches!(node, NodeId::Layer(_));
    let within = kind_fits
        && store
            .published(|history| history.is_ancestor(node, head))
            .map_err(args::published)?;
    match within {
        true => Ok(node),
        false => Err(not_in_history(&node_text(node), branch)),
    }
}

/// Where a Commit list starts and goes on.
#[derive(Clone, Copy, Debug, Default)]
pub struct From<'a> {
    /// The Commit the first page starts at instead of the head: its id or
    /// a prefix. A later page names the Commit the first page started at,
    /// by its id: the pin.
    pub at: Option<&'a str>,
    /// The last Commit of the page before, by its id.
    pub after: Option<&'a str>,
}

/// The node after `saved` along first parents.
fn parent(saved: &Saved) -> Option<NodeId> {
    match saved {
        Saved::Commit(record) => record.parent.map(NodeId::Commit),
        Saved::Layer(record) => record.parent.map(NodeId::Layer),
    }
}

/// One page of the Commits of `branch`, newest first, along first parents;
/// on `main`, of its publications. The answer's `at` is the Commit the
/// first page started at, which a later page names as `at` beside `after`:
/// it then lists what that Commit's history holds even if the Branch has
/// moved, and is refused `NoSuchCommit` when that Commit is gone.
pub fn commits(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    branch: &str,
    from: From<'_>,
    limit: args::Arg<usize>,
) -> Answer {
    let head = head(store, stack, branch)?;
    let limit = limit?;
    let chain = |node: NodeId| match (node, branch == MAIN) {
        (NodeId::Layer(_), true) | (NodeId::Commit(_), false) => Some(node),
        _ => None,
    };
    let (start, mut current) = match (from.at, from.after) {
        (at, Some(after)) => {
            // a later page: its pin must still exist, and so must its place
            let pin = at.map(|at| resolve::commit(store, stack, at)).transpose()?;
            let last = resolve::commit(store, stack, after)?;
            (pin.map(|pin| pin.node()), parent(&last).and_then(chain))
        }
        (Some(at), None) => {
            let start = resolve::commit(store, stack, at)?.node();
            let start = held(store, branch, head, start)?;
            (Some(start), chain(start))
        }
        (None, None) => (chain(head), chain(head)),
    };
    let mut page = Page::new(limit);
    let mut names = BranchNames::default();
    while let Some(node) = current {
        let saved = resolve::saved(store, stack, node)?;
        current = parent(&saved).and_then(chain);
        if !page.add(row(store, &mut names, &saved, Use::List)?) {
            break;
        }
    }
    let mut fields = page.answer("commit");
    fields.push(("at", SdkValue::opt_text(start.map(node_text))));
    crate::read::snapshot::answer(fields)
}

/// One Commit of the project by its id or a prefix, with its whole message;
/// with `branch`, refused `NotInHistory` unless that Branch's history holds
/// it. On main a publication's id is read the same way.
pub fn commit(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    text: &str,
    branch: Option<&str>,
) -> Answer {
    let saved = resolve::commit(store, stack, text)?;
    if let Some(branch) = branch {
        let head = head(store, stack, branch)?;
        held(store, branch, head, saved.node())?;
    }
    row(store, &mut BranchNames::default(), &saved, Use::Get)
}
