//! Positional reads of one snapshot, with numeric paging and bounded
//! streaming text windows.
use super::{
    cached::{self, Cached},
    search,
    snapshot::{answer, existing, header, kind_name, size, Snap},
    walk, window,
};
use crate::history::lookup::{self as args, invalid, Answer, Arg};
use crate::store::StorePorts;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_content::{
    compare::{memo::Memo, EntryFacts},
    filesystem::{
        attributes::{read_portable, AttributeReadWork},
        directory::read::{list_after, DirectoryReadWork},
        sorted::DirectoryRoot,
    },
    merge::Rule,
    object::inode_leaf::InodeKind,
};

pub const REPLY_BUDGET: usize = window::BYTES;

/// One window of the file at `path`: lines `start` through `end`, as far as
/// the reply budget allows.
pub fn read(
    reader: &StorePorts,
    snap: &Snap,
    path: &str,
    start: Arg<u64>,
    end: Arg<u64>,
) -> Answer {
    // The lookup's pages are memoized over the call's caches; the file's
    // own objects are read through the caches only.
    let cached = Cached::new(reader)?;
    let entry = existing(&Memo::new(&cached), snap, path)?;
    let (start, end) = (start?, end?);
    if end < start {
        return Err(invalid("endLine must be at least startLine"));
    }
    let requested_end = end;
    let end = end.min(start.saturating_add(window::LINES as u64 - 1));
    let (mut content, mut next, mut clipped) = (String::new(), None, false);
    let mut last = None;
    window::lines(&cached, &entry, REPLY_BUDGET, start, |n, line| {
        if n > requested_end {
            return Ok(false);
        }
        if n > end || (!content.is_empty() && content.len() + line.text.len() > REPLY_BUDGET) {
            next = Some(n);
            return Ok(false);
        }
        clipped |= line.truncated;
        last = Some(n);
        content.push_str(&line.text);
        Ok(true)
    })?;
    // Return either aggregate text or per-line data, avoiding duplicate payload on the wire.
    let mut fields = header(snap, path);
    fields.extend([
        ("text", SdkValue::text(content)),
        ("startLine", SdkValue::Int(start as i64)),
        (
            "endLine",
            last.map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
        ),
        ("lineTruncated", SdkValue::Bool(clipped)),
        ("truncated", SdkValue::Bool(clipped || next.is_some())),
        (
            "nextLine",
            next.map_or(SdkValue::Null, |n| SdkValue::Int(n as i64)),
        ),
        ("size", SdkValue::Int(size(reader, &entry)? as i64)),
    ]);
    answer(fields)
}

/// The type, inode, size and permission mode of the entry at `path`. An
/// inline mode costs no object read.
pub fn stat(reader: &StorePorts, snap: &Snap, path: &str) -> Answer {
    let cached = Cached::new(reader)?;
    let entry = existing(&Memo::new(&cached), snap, path)?;
    let metadata = read_portable(
        &cached,
        entry.value.metadata_root,
        entry.value.kind,
        &mut AttributeReadWork::default(),
    )
    .map_err(args::content)?;
    let mut fields = header(snap, path);
    fields.extend([
        ("type", SdkValue::text(kind_name(entry.value.kind))),
        ("inode", SdkValue::Int(entry.serial as i64)),
        ("size", SdkValue::Int(size(reader, &entry)? as i64)),
        ("mode", SdkValue::Int(i64::from(metadata.mode))),
    ]);
    answer(fields)
}

/// One page of the directory at `path`, `paging` (limit, offset) from the
/// caller.
pub fn list(reader: &StorePorts, snap: &Snap, path: &str, paging: Arg<(usize, u64)>) -> Answer {
    let cached = Cached::new(reader)?;
    // One memo for the call (#352 011.a1): the entries' inode lookups share
    // the inode table's upper pages, read once, so an entry costs its leaf.
    let memo = Memo::new(&cached);
    let dir = existing(&memo, snap, path)?;
    if dir.value.kind != InodeKind::Directory {
        return Err(args::refuse(
            "NotDirectory",
            "expected a directory",
            SdkValue::Null,
        ));
    }
    let (limit, pos) = paging?;
    let (mut after, mut skipped, mut items, mut spent, mut more) =
        (None, 0u64, Vec::new(), 0, false);
    'pages: loop {
        let page = list_after(
            &memo,
            DirectoryRoot(dir.value.content_root),
            after.as_ref(),
            64,
            REPLY_BUDGET,
            &mut DirectoryReadWork::default(),
        )
        .map_err(args::content)?;
        let mut taken = Vec::new();
        for (name, serial) in &page.entries {
            if skipped < pos {
                skipped += 1;
                continue;
            }
            if items.len() + taken.len() == limit
                || spent + name.as_str().len() + path.len() + 160 > REPLY_BUDGET
            {
                more = true;
                break;
            }
            spent += name.as_str().len() + path.len() + 160;
            taken.push((name, *serial));
        }
        // The page's entries are looked up together, one descent per level.
        let serials: Vec<u64> = taken.iter().map(|(_, serial)| *serial).collect();
        let values = snap.tree.values(&memo, &serials).map_err(args::content)?;
        for ((name, serial), value) in taken.into_iter().zip(values) {
            let value = value.ok_or_else(|| args::failed("missing bound inode"))?;
            items.push(SdkValue::map([
                ("name", SdkValue::text(name.as_str())),
                (
                    "path",
                    SdkValue::text(layerfs_content::compare::tree::join(path, name)),
                ),
                ("type", SdkValue::text(kind_name(value.kind))),
                ("inode", SdkValue::Int(serial as i64)),
            ]));
        }
        if more {
            break 'pages;
        }
        match page.continuation {
            Some(next) => after = Some(next),
            None => break,
        }
    }
    let next = args::next_page(pos, items.len(), more);
    let mut fields = header(snap, path);
    fields.extend([
        ("items", SdkValue::List(items)),
        ("hasMore", SdkValue::Bool(!matches!(next, SdkValue::Null))),
        ("nextPagePos", next),
    ]);
    answer(fields)
}

/// One page of the lines under `path` that contain `pattern`, a nonempty
/// literal the caller checked; binary files are skipped and counted.
pub fn grep(
    reader: &StorePorts,
    snap: &Snap,
    pattern: &str,
    path: &str,
    limit: usize,
    pos: u64,
) -> Answer {
    let mut page = Page {
        pattern,
        limit,
        pos,
        items: Vec::new(),
        found: 0,
        spent: 0,
        more: false,
        skipped: 0,
    };
    let mut reuse = search::Reuse::default();
    // The walk and the file scans share one call's read caches; files are
    // scanned in walk order, a window of them at a time.
    let cached = Cached::new(reader)?;
    let mut window = Vec::new();
    walk::walk_within(&cached, snap, path, None, |file, entry| {
        if entry.value.kind == InodeKind::RegularFile {
            window.push((file.to_owned(), entry));
        }
        if window.len() == cached::ROOTS {
            page.scan(&cached, &mut reuse, &mut window)?;
        }
        Ok(!page.more)
    })?;
    if !page.more {
        page.scan(&cached, &mut reuse, &mut window)?;
    }
    let Page {
        items,
        more,
        skipped,
        ..
    } = page;
    let next = args::next_page(pos, items.len(), more);
    let mut fields = header(snap, path);
    fields.extend([
        ("items", SdkValue::List(items)),
        ("skipped", SdkValue::Int(skipped)),
        ("hasMore", SdkValue::Bool(!matches!(next, SdkValue::Null))),
        ("nextPagePos", next),
    ]);
    answer(fields)
}

/// One grep page's results so far.
struct Page<'p> {
    pattern: &'p str,
    limit: usize,
    pos: u64,
    items: Vec<SdkValue>,
    found: u64,
    spent: usize,
    more: bool,
    skipped: i64,
}

impl Page<'_> {
    /// Scans `window`'s files in order until the page is full, their roots
    /// read together; contents already scanned in this call are not read.
    fn scan(
        &mut self,
        cached: &Cached,
        reuse: &mut search::Reuse,
        window: &mut Vec<(String, EntryFacts)>,
    ) -> Result<(), SdkRefusal> {
        let mut ids: Vec<_> = window
            .iter()
            .map(|(_, entry)| entry.value.content_root)
            .filter(|id| !reuse.kept(id))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let mut views = cached.roots(&ids)?;
        let pattern = self.pattern;
        for (file, entry) in window.drain(..) {
            if self.more {
                continue;
            }
            let view = views.remove(&entry.value.content_root);
            let scan = reuse.matches(cached, &entry, view, 1024, pattern, |n, line| {
                Ok(self.line(&file, n, line))
            });
            match scan {
                Err(e) if e.code == "Binary" => self.skipped += 1,
                other => other?,
            }
        }
        Ok(())
    }

    /// Takes one matching line; false when the page is full.
    fn line(&mut self, file: &str, n: u64, line: window::Line) -> bool {
        if self.found < self.pos {
            self.found += 1;
            return true;
        }
        let cost = file.len() + line.text.len() + 160;
        if self.items.len() == self.limit || self.spent + cost > REPLY_BUDGET {
            self.more = true;
            return false;
        }
        self.spent += cost;
        self.items.push(SdkValue::map([
            ("path", SdkValue::text(file)),
            ("line", SdkValue::Int(n as i64)),
            ("text", SdkValue::text(line.text)),
            ("lineTruncated", SdkValue::Bool(line.truncated)),
        ]));
        true
    }
}

/// One page of the paths under `path` that match the glob `pattern`.
pub fn glob(
    reader: &StorePorts,
    snap: &Snap,
    pattern: &str,
    path: &str,
    limit: usize,
    pos: u64,
) -> Answer {
    let rule = Rule::glob(pattern);
    let (mut items, mut found, mut spent, mut more) = (Vec::new(), 0u64, 0, false);
    // Only the folders the pattern's literal leading folders allow.
    let cached = Cached::new(reader)?;
    let within = walk::leading(pattern);
    walk::walk_within(&cached, snap, path, within, |file, _| {
        if !rule.matches_path(file) {
            return Ok(true);
        }
        if found < pos {
            found += 1;
            return Ok(true);
        }
        if items.len() == limit || spent + file.len() + 16 > REPLY_BUDGET {
            more = true;
            return Ok(false);
        }
        spent += file.len() + 16;
        items.push(SdkValue::text(file));
        Ok(true)
    })?;
    let next = args::next_page(pos, items.len(), more);
    let mut fields = header(snap, path);
    fields.extend([
        ("items", SdkValue::List(items)),
        ("hasMore", SdkValue::Bool(!matches!(next, SdkValue::Null))),
        ("nextPagePos", next),
    ]);
    answer(fields)
}
