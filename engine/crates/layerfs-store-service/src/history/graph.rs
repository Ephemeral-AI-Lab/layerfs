//! Branch-owning history, its fork point and current main publication state.
use super::branch::{item, main_item};
use super::lookup::{self as args, hex, history, Answer};
use crate::store::Store;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::{commit_value_with_budget, value_cost};
use layerfs_history::{
    CommitHistoryRequest, CommitRecord, LayerHistoryRequest, LayerStackRecord, NodeId,
    MAXIMUM_PAGE_RECORDS,
};
use std::sync::Arc;

struct Page<'a> {
    branch: &'a str,
    limit: usize,
    pos: u64,
    seen: u64,
    spent: usize,
    items: Vec<SdkValue>,
    more: bool,
}

impl<'a> Page<'a> {
    fn new(branch: &'a str, limit: usize, pos: u64) -> Self {
        Self {
            branch,
            limit,
            pos,
            seen: 0,
            spent: 0,
            items: Vec::new(),
            more: false,
        }
    }

    fn skip(&mut self) -> bool {
        if self.seen < self.pos {
            self.seen += 1;
            return true;
        }
        false
    }

    fn push(
        &mut self,
        record: &CommitRecord,
        published: impl FnOnce() -> Result<bool, SdkRefusal>,
    ) -> Result<bool, SdkRefusal> {
        if self.skip() {
            return Ok(true);
        }
        let published = published()?;
        let mut value =
            commit_value_with_budget(record, Some(self.branch), crate::read::reads::REPLY_BUDGET);
        if let SdkValue::Map(fields) = &mut value {
            fields.push(("published".into(), SdkValue::Bool(published)));
        }
        let cost = value_cost(&value);
        if !self.items.is_empty()
            && (self.items.len() == self.limit
                || self.spent + cost > crate::read::reads::REPLY_BUDGET)
        {
            self.more = true;
            return Ok(false);
        }
        self.spent += cost;
        self.items.push(value);
        Ok(true)
    }

    fn finish(self) -> (Vec<SdkValue>, bool) {
        (self.items, self.more)
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
    let mut page = Page::new(name, limit, pos);
    if main {
        let mut cursor = None;
        'pages: loop {
            let found = store
                .history()
                .layer_history(&LayerHistoryRequest {
                    stack: stack.id,
                    start: Some(stack.head_layer),
                    cursor,
                    limit: MAXIMUM_PAGE_RECORDS,
                })
                .map_err(history)?;
            for layer in found.records {
                let Some(commit) = layer.source_commit else {
                    continue;
                };
                if page.skip() {
                    continue;
                }
                let record = args::commit(store, commit)?;
                if !page.push(&record, || Ok(true))? {
                    break 'pages;
                }
            }
            match found.continuation {
                Some(next) => cursor = Some(next),
                None => break,
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
                if !page.push(&record, || {
                    store
                        .history()
                        .is_published(stack.head_layer, &record)
                        .map_err(history)
                })? {
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
