//! Opt-in matching-result pages, with explicit replay, payload and reply admission.
use super::{
    file_facts,
    paged_boundaries::{self, Boundary, Key},
    query::{self, ResultPage, Work},
    search,
    snapshot::{self, Snap},
    walk,
};
use crate::{
    history::{lookup as args, saved},
    store::{Store, StorePorts},
};
use layerfs_bridge::control::SdkValue;
use layerfs_content::{
    compare::{tree::join, Memo},
    filesystem::{
        directory::read::{list_after, DirectoryReadWork},
        path::PathName,
        sorted::DirectoryRoot,
    },
    merge::Rule,
    object::inode_leaf::InodeKind,
};
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

fn exhausted(cached: &Option<Option<Boundary>>) -> bool {
    cached
        .as_ref()
        .is_some_and(|value| value.as_ref().is_none_or(|boundary| boundary.exhausted))
}

fn key(snap: &Snap, mode: &'static str, path: &str, pattern: &str, page: ResultPage) -> Key {
    Key {
        node: snap.node,
        mode,
        path: path.to_owned(),
        pattern: pattern.to_owned(),
        size: page.limit,
        page: page.number,
    }
}
struct Output {
    items: Vec<SdkValue>,
    more: bool,
    skipped: i64,
    last: Option<Boundary>,
}
impl Output {
    fn empty(skipped: i64) -> Self {
        Self {
            items: Vec::new(),
            more: false,
            skipped,
            last: None,
        }
    }
}
fn complete(
    store: &Arc<Store>,
    mut key: Key,
    snap: &Snap,
    path: &str,
    page: ResultPage,
    output: Output,
) -> args::Answer {
    let Output {
        items,
        more,
        skipped,
        last,
    } = output;
    let answer = finish(snap, path, page, items, more, skipped)?;
    key.page += 1;
    paged_boundaries::keep(
        store,
        key,
        if more {
            last
        } else {
            Some(Boundary {
                path: String::new(),
                line: 0,
                skipped,
                exhausted: true,
            })
        },
    );
    Ok(answer)
}

/// Indexed name boundaries make sequential pages independent of preceding names.
pub fn list(
    store: &Arc<Store>,
    reader: &StorePorts,
    snap: &Snap,
    path: &str,
    page: ResultPage,
) -> args::Answer {
    let key = key(snap, "list", path, "", page);
    let cached = paged_boundaries::find(store, &key);
    let memo = Memo::new(reader);
    let dir = snapshot::existing(&memo, snap, path)?;
    if dir.value.kind != InodeKind::Directory {
        return Err(args::refuse(
            "NotDirectory",
            "expected a directory",
            SdkValue::Null,
        ));
    }
    if exhausted(&cached) {
        return complete(store, key, snap, path, page, Output::empty(0));
    }
    let skip = if cached.is_some() { 0 } else { page.skip };
    let mut after = cached
        .as_ref()
        .and_then(|v| v.as_ref())
        .map(|b| PathName::new(&b.path).map_err(|_| args::invalid("cached directory boundary")))
        .transpose()?;
    let (mut work, mut seen, mut items, mut more, mut last) =
        (Work::default(), 0u64, Vec::new(), false, None);
    'windows: loop {
        let batch = list_after(
            &memo,
            DirectoryRoot(dir.value.content_root),
            after.as_ref(),
            64,
            8192,
            &mut DirectoryReadWork::default(),
        )
        .map_err(args::content)?;
        let mut selected = Vec::new();
        for (name, serial) in &batch.entries {
            work.step()?;
            if seen < skip {
                seen += 1;
                continue;
            }
            if items.len() + selected.len() == page.limit {
                more = true;
                break;
            }
            selected.push((name, *serial));
        }
        let serials = selected
            .iter()
            .map(|(_, serial)| *serial)
            .collect::<Vec<_>>();
        let values = snap.tree.values(&memo, &serials).map_err(args::content)?;
        for ((name, serial), value) in selected.into_iter().zip(values) {
            let value = value.ok_or_else(|| args::failed("missing bound inode"))?;
            last = Some(Boundary {
                path: name.as_str().to_owned(),
                line: 0,
                skipped: 0,
                exhausted: false,
            });
            items.push(SdkValue::map([
                ("name", SdkValue::text(name.as_str())),
                ("path", SdkValue::text(join(path, name))),
                ("type", SdkValue::text(snapshot::kind_name(value.kind))),
                ("inode", SdkValue::Int(serial as i64)),
            ]));
        }
        if more {
            break 'windows;
        }
        match batch.continuation {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    complete(
        store,
        key,
        snap,
        path,
        page,
        Output {
            items,
            more,
            skipped: 0,
            last,
        },
    )
}

pub fn glob(
    store: &Arc<Store>,
    reader: &StorePorts,
    snap: &Snap,
    pattern: &str,
    path: &str,
    page: ResultPage,
) -> args::Answer {
    let key = key(snap, "glob", path, pattern, page);
    let cached = paged_boundaries::find(store, &key);
    if exhausted(&cached) {
        return complete(store, key, snap, path, page, Output::empty(0));
    }
    let boundary = cached.as_ref().and_then(|v| v.as_ref());
    let narrowed =
        walk::leading(pattern).map(|folders| format!("/{}", folders.trim_end_matches('/')));
    let resume_path = narrowed
        .as_deref()
        .filter(|deeper| deeper.len() > path.len())
        .unwrap_or(path);
    let mut walker = if let Some(b) = boundary {
        walk::Walker::after(reader, snap, resume_path, Some(&b.path))?
    } else {
        walk::Walker::within(reader, snap, path, walk::leading(pattern))?
    };
    let rule = Rule::glob(pattern);
    let skip = if cached.is_some() { 0 } else { page.skip };
    let (mut work, mut seen, mut items, mut more, mut last) =
        (Work::default(), 0u64, Vec::new(), false, None);
    while let Some((file, _)) = walker.next()? {
        work.step()?;
        if !rule.matches_path(&file) {
            continue;
        }
        if seen < skip {
            seen += 1;
            continue;
        }
        if items.len() == page.limit {
            more = true;
            break;
        }
        last = Some(Boundary {
            path: file.clone(),
            line: 0,
            skipped: 0,
            exhausted: false,
        });
        items.push(SdkValue::text(file));
    }
    complete(
        store,
        key,
        snap,
        path,
        page,
        Output {
            items,
            more,
            skipped: 0,
            last,
        },
    )
}

pub fn grep(
    store: &Arc<Store>,
    reader: &StorePorts,
    snap: &Snap,
    pattern: &str,
    path: &str,
    page: ResultPage,
) -> args::Answer {
    let key = key(snap, "grep", path, pattern, page);
    let cached = paged_boundaries::find(store, &key);
    if exhausted(&cached) {
        let skipped = cached
            .as_ref()
            .and_then(|v| v.as_ref())
            .map_or(0, |b| b.skipped);
        return complete(store, key, snap, path, page, Output::empty(skipped));
    }
    let boundary = cached.as_ref().and_then(|v| v.as_ref());
    let mut walker = walk::Walker::after(reader, snap, path, boundary.map(|b| b.path.as_str()))?;
    let mut resume = boundary
        .map(|b| {
            snapshot::entry(reader, &snap.tree, &b.path).map(|e| e.map(|e| (b.path.clone(), e)))
        })
        .transpose()?
        .flatten();
    let skip = if cached.is_some() { 0 } else { page.skip };
    let (mut work, mut found, mut items, mut more, mut skipped, mut bytes, mut last) = (
        Work::default(),
        0u64,
        Vec::new(),
        false,
        boundary.map_or(0, |b| b.skipped),
        16 * 1024 * 1024u64,
        None,
    );
    loop {
        let next = match resume.take() {
            Some(entry) => Some(entry),
            None => walker.next()?,
        };
        let Some((file, entry)) = next else { break };
        work.step()?;
        if entry.value.kind != InodeKind::RegularFile {
            continue;
        }
        let result =
            search::matches_bounded(reader, &entry, None, 1024, pattern, bytes, |n, line| {
                if boundary.is_some_and(|b| b.path == file && n <= b.line) {
                    return Ok(true);
                }
                work.step()?;
                if found < skip {
                    found += 1;
                    return Ok(true);
                }
                if items.len() == page.limit {
                    more = true;
                    return Ok(false);
                }
                last = Some(Boundary {
                    path: file.clone(),
                    line: n,
                    skipped,
                    exhausted: false,
                });
                items.push(SdkValue::map([
                    ("path", SdkValue::text(&file)),
                    ("line", SdkValue::Int(n as i64)),
                    ("text", SdkValue::text(line.text)),
                    ("lineTruncated", SdkValue::Bool(line.truncated)),
                ]));
                Ok(true)
            });
        match result {
            Err(e) if e.code == "Binary" => {
                // Binary scans disclose actual delivered bytes, including the failing window.
                bytes =
                    bytes.saturating_sub(
                        e.detail.get_int("bytesScanned").unwrap_or(bytes as i64) as u64
                    );
                skipped += 1;
            }
            other => {
                let scan = other?;
                bytes = bytes.saturating_sub(scan.bytes);
                if scan.exhausted {
                    return Err(query::capacity(
                        "grep payload query budget reached; narrow the path",
                    ));
                }
            }
        }
        if more {
            break;
        }
    }
    complete(
        store,
        key,
        snap,
        path,
        page,
        Output {
            items,
            more,
            skipped,
            last,
        },
    )
}
