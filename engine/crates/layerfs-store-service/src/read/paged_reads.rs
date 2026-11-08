//! Opt-in matching-result pages, with explicit replay, payload and reply admission.
use super::{
    file_facts,
    query::{self, ResultPage, Work},
    reads, search,
    snapshot::{self, Snap},
    walk,
};
use crate::{
    history::{lookup as args, saved},
    store::{Store, StorePorts},
};
use layerfs_bridge::control::SdkValue;
use layerfs_content::{merge::Rule, object::inode_leaf::InodeKind};
use std::sync::Arc;

/// Every saved read publishes its exact immutable anchor, including legacy calls.
pub fn pin(store: &Arc<Store>, snap: &Snap, answer: args::Answer) -> args::Answer {
    let SdkValue::Map(mut fields) = answer? else {
        return Err(args::failed("saved read response must be a map"));
    };
    fields.push(("at".into(), saved::reference(store, snap.node)?));
    super::read_reply::finish(fields)
}

fn finish(
    snap: &Snap,
    path: &str,
    page: ResultPage,
    items: Vec<SdkValue>,
    more: bool,
    skipped: i64,
) -> args::Answer {
    let mut fields = snapshot::header(snap, path);
    fields.extend([
        ("items", SdkValue::List(items)),
        ("hasMore", SdkValue::Bool(more)),
        ("page", SdkValue::Int(page.number as i64)),
        ("pageSize", SdkValue::Int(page.limit as i64)),
        ("skipped", SdkValue::Int(skipped)),
    ]);
    let answer = snapshot::answer(fields)?;
    if file_facts::value_bytes(&answer) + 512 > super::window::BYTES {
        return Err(query::capacity(
            "matching page exceeds reply budget; reduce page_size",
        ));
    }
    Ok(answer)
}

pub fn list(reader: &StorePorts, snap: &Snap, path: &str, page: ResultPage) -> args::Answer {
    if page.skip + page.limit as u64 + 1 > query::MAXIMUM_QUERY_WORK {
        return Err(query::capacity("directory page replay bound reached"));
    }
    let answer = reads::list(reader, snap, path, Ok((page.limit, page.skip)))?;
    let items = match answer.get("items") {
        Some(SdkValue::List(items)) => items.clone(),
        _ => return Err(args::failed("directory page missing items")),
    };
    let more = answer.get_bool("hasMore");
    if more && items.len() < page.limit {
        return Err(query::capacity(
            "directory page exceeds reply budget; reduce page_size",
        ));
    }
    finish(snap, path, page, items, more, 0)
}

pub fn glob(
    reader: &StorePorts,
    snap: &Snap,
    pattern: &str,
    path: &str,
    page: ResultPage,
) -> args::Answer {
    let rule = Rule::glob(pattern);
    let mut walker = walk::Walker::within(reader, snap, path, walk::leading(pattern))?;
    let (mut work, mut found, mut items, mut more) = (Work::default(), 0u64, Vec::new(), false);
    while let Some((file, _)) = walker.next()? {
        work.step()?;
        if !rule.matches_path(&file) {
            continue;
        }
        if found < page.skip {
            found += 1;
            continue;
        }
        if items.len() == page.limit {
            more = true;
            break;
        }
        items.push(SdkValue::text(file));
    }
    finish(snap, path, page, items, more, 0)
}

pub fn grep(
    reader: &StorePorts,
    snap: &Snap,
    pattern: &str,
    path: &str,
    page: ResultPage,
) -> args::Answer {
    let mut walker = walk::Walker::new(reader, snap, path)?;
    let (mut work, mut found, mut items, mut more, mut skipped, mut bytes) =
        (Work::default(), 0u64, Vec::new(), false, 0i64, 0u64);
    while let Some((file, entry)) = walker.next()? {
        work.step()?;
        if entry.value.kind != InodeKind::RegularFile {
            continue;
        }
        bytes = bytes.saturating_add(snapshot::size(reader, &entry)?);
        if bytes > 16 * 1024 * 1024 {
            return Err(query::capacity(
                "grep payload replay budget reached; narrow the path",
            ));
        }
        let result = search::matches(reader, &entry, None, 1024, pattern, |n, line| {
            work.step()?;
            if found < page.skip {
                found += 1;
                return Ok(true);
            }
            if items.len() == page.limit {
                more = true;
                return Ok(false);
            }
            items.push(SdkValue::map([
                ("path", SdkValue::text(&file)),
                ("line", SdkValue::Int(n as i64)),
                ("text", SdkValue::text(line.text)),
                ("lineTruncated", SdkValue::Bool(line.truncated)),
            ]));
            Ok(true)
        });
        match result {
            Err(e) if e.code == "Binary" => skipped += 1,
            other => other?,
        }
        if more {
            break;
        }
    }
    finish(snap, path, page, items, more, skipped)
}
