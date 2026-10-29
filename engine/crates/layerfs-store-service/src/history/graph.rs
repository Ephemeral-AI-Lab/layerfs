//! Branch-owning history, its fork point and current main publication state.
use super::branch::{item, main_item};
use super::lookup::{self as args, commit_value, hex, history, Answer};
use crate::store::Store;
use layerfs_bridge::control::SdkValue;
use layerfs_history::{
    CommitHistoryRequest, CommitId, CommitRecord, LayerHistoryRequest, LayerStackRecord, NodeId,
    MAXIMUM_PAGE_RECORDS,
};
use std::collections::HashSet;
use std::sync::Arc;

struct Page<'a> {
    branch: &'a str,
    limit: usize,
    pos: u64,
    published: &'a HashSet<CommitId>,
    all_published: bool,
    seen: u64,
    spent: usize,
    items: Vec<SdkValue>,
    more: bool,
}

impl<'a> Page<'a> {
    fn new(
        branch: &'a str,
        limit: usize,
        pos: u64,
        published: &'a HashSet<CommitId>,
        all_published: bool,
    ) -> Self {
        Self {
            branch,
            limit,
            pos,
            published,
            all_published,
            seen: 0,
            spent: 0,
            items: Vec::new(),
            more: false,
        }
    }

    fn push(&mut self, record: &CommitRecord) -> bool {
        if self.seen < self.pos {
            self.seen += 1;
            return true;
        }
        let cost = record.meta.message.as_ref().map_or(0, String::len) + 1024;
        if self.items.len() == self.limit || self.spent + cost > crate::read::reads::REPLY_BUDGET {
            self.more = true;
            return false;
        }
        self.spent += cost;
        let published = self.all_published || self.published.contains(&record.id);
        let mut value = commit_value(record, Some(self.branch));
        if let SdkValue::Map(fields) = &mut value {
            fields.push(("published".into(), SdkValue::Bool(published)));
        }
        self.items.push(value);
        true
    }

    fn finish(self) -> (Vec<SdkValue>, bool) {
        (self.items, self.more)
    }
}

fn published(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    include_commits: bool,
) -> Result<(HashSet<CommitId>, Vec<CommitRecord>), layerfs_bridge::control::SdkRefusal> {
    let mut cursor = None;
    let mut published = HashSet::new();
    let mut commits = Vec::new();
    loop {
        let page = store
            .history()
            .layer_history(&LayerHistoryRequest {
                stack: stack.id,
                start: Some(stack.head_layer),
                cursor,
                limit: MAXIMUM_PAGE_RECORDS,
            })
            .map_err(history)?;
        for layer in page.records {
            if let Some(commit) = layer.source_commit {
                published.insert(commit);
                if include_commits {
                    commits.push(args::commit(store, commit)?);
                }
            }
        }
        match page.continuation {
            Some(next) => cursor = Some(next),
            None => return Ok((published, commits)),
        }
    }
}

fn fork_point(node: Option<NodeId>) -> SdkValue {
    node.map_or(SdkValue::Null, |node| {
        let (kind, id) = match node {
            NodeId::Layer(id) => ("layer", hex(id.as_slice())),
            NodeId::Commit(id) => ("commit", hex(id.as_slice())),
        };
        SdkValue::map([("kind", SdkValue::text(kind)), ("id", SdkValue::text(id))])
    })
}

/// A Branch's own Commits, stopping before its Commit fork point, or main's
/// Layer-source Commits. Publication is read from main's current Layer stack.
pub fn graph(
    store: &Arc<Store>,
    stack: &LayerStackRecord,
    name: &str,
    limit: usize,
    pos: u64,
) -> Answer {
    let main = name == super::MAIN;
    let branch = if main {
        None
    } else {
        Some(args::branch(store, stack, name)?)
    };
    let (published, main_commits) = published(store, stack, main)?;
    let mut page = Page::new(name, limit, pos, &published, main);
    if main {
        for record in &main_commits {
            if !page.push(record) {
                break;
            }
        }
    } else if let Some(branch) = &branch {
        let fork = match branch.forked_from {
            Some(NodeId::Commit(id)) => Some(id),
            _ => None,
        };
        let mut cursor = None;
        'pages: loop {
            let found = store
                .history()
                .commit_history(&CommitHistoryRequest {
                    branch: branch.id,
                    start: branch.head_commit,
                    cursor,
                    limit: MAXIMUM_PAGE_RECORDS,
                })
                .map_err(history)?;
            for record in found.records {
                if Some(record.id) == fork {
                    break 'pages;
                }
                if !page.push(&record) {
                    break 'pages;
                }
            }
            match found.continuation {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
    }
    let (commits, more) = page.finish();
    let next = args::next_page(pos, commits.len(), more);
    let branch_value = branch.as_ref().map_or_else(|| main_item(stack), item);
    Ok(SdkValue::map([
        ("branch", branch_value),
        (
            "forkPoint",
            fork_point(branch.and_then(|branch| branch.forked_from)),
        ),
        ("commits", SdkValue::List(commits)),
        ("hasMore", SdkValue::Bool(!matches!(next, SdkValue::Null))),
        ("nextPagePos", next),
    ]))
}
