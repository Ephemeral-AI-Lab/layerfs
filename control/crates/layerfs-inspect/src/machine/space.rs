//! `machine.space` and its inspect kind `machine_space`: where a machine's
//! data disk goes. One bounded walk of LayerFS's own folders on the side
//! that holds the machine's data (the home on `native`, the machine's
//! worker root otherwise) counts the blocks each part takes: the machine's
//! Stores, each recorded Sandbox's run folder (its Workspace overlays, its
//! own writes, its home, its log, its daemon and runtime copies), the
//! Sandbox image cache, run folders no listed Sandbox names, LayerFS's logs
//! and its other records. Beside the same `statvfs` as `machine.stats`,
//! `outside` is what the filesystem uses that is not LayerFS's.
//!
//! The walk is a read. It takes no lock, lease or daemon connection,
//! follows no link, opens no file (each size is one `lstat`, each folder
//! one listing) and descends into no other device. A run folder's
//! `workspaces/` (its Workspace mount points) and a private root
//! `<name>.root` are never entered or `stat`ed, so a stopped daemon cannot
//! hang it. The home itself is never listed: the names an integration
//! keeps there are neither counted nor read. It stops at [`ENTRIES`]
//! entries or the request's soft budget, and then answers `complete:
//! false` with what it counted, each number a lower bound.
use super::{live, stats};
use layerfs_home::home::{self, paths};
use layerfs_home::names::{MachineName, StoreName};
use layerfs_home::outcome::Outcome;
use layerfs_protocol::sdk::{SdkRefusal, SdkValue};
use layerfs_sandbox::deploy::Side;
use layerfs_sandbox::inspect::{owner::Budget, route};
use std::collections::HashSet;
use std::fs::Metadata;
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

/// The inspect kind.
pub(crate) const KIND: &str = "machine_space";
/// The most directory entries one walk measures.
pub const ENTRIES: usize = 250_000;
/// The most files with several names whose identity a walk keeps, so a
/// file is counted once whichever of its names comes first; past it, a
/// further name is counted again.
const LINKED: usize = 1 << 20;

/// `machine.space(machine)`: `{ machine, platform, observed_at, disk,
/// layerfs, outside, parts, stores, sandboxes, unreadable, complete,
/// reason? }`, every size the blocks a part takes (`st_blocks` × 512).
///
/// - `disk`: `{ total, used, free, path, fs, scope }`, the filesystem
///   `machine.stats` measures, with its `scope`.
/// - `layerfs`: the sum of `parts`; `outside`: `disk.used` less `layerfs`
///   (never below 0), null unless the walk and the disk read are complete.
/// - `parts`: `{ stores, sandboxes, images, other_runs, logs, other }`; a
///   part the walk did not reach is null.
/// - `stores`: `[{ name, bytes }]`, each Store's folder.
/// - `sandboxes`: `[{ name, bytes, overlays, writes, home, log, runtime,
///   reason? }]`, one row per recorded Sandbox, null sizes with `reason`
///   (`no_run_folder`, `outside_run_root`, `record_unreadable`,
///   `deadline`) when its run folder was not measured; a row the bound
///   ended inside keeps what it counted, with `reason: "deadline"`.
/// - `unreadable`: folders that could not be listed (their sizes are not
///   in the numbers).
/// - `reason`: comma-joined `deadline`, `unreadable`, `disk_not_read`.
///
/// A machine that is stopped or does not answer is `Unavailable`; nothing
/// starts it. No lock, lease, Store open or write, here or there.
pub fn machine_space(home: &Path, machine: &MachineName) -> Outcome {
    let name = machine.as_str();
    let placed = stats::placed(home, name)?;
    let answer = crate::ask(home, name, KIND, placed.request())?;
    let part = |field: &str| answer.get(field).cloned().unwrap_or_default();
    let (_, holds) = stats::scopes(&placed.platform);
    let disk = match part("disk") {
        SdkValue::Map(mut fields) => {
            fields.push(("scope".to_owned(), SdkValue::text(holds)));
            SdkValue::Map(fields)
        }
        other => other,
    };
    let mut shown = vec![
        ("machine", SdkValue::text(name)),
        ("platform", SdkValue::Text(placed.platform)),
        ("observed_at", part("observed_at")),
        ("disk", disk),
        ("layerfs", part("layerfs")),
        ("outside", part("outside")),
        ("parts", part("parts")),
        ("stores", part("stores")),
        ("sandboxes", part("sandboxes")),
        ("unreadable", part("unreadable")),
        ("complete", part("complete")),
    ];
    shown.extend(answer.get("reason").cloned().map(|why| ("reason", why)));
    Ok(map(shown))
}

/// The request on the side that holds the machine's data, rooted at the
/// home or the worker's root: `{ observed_at, disk: { total, used, free,
/// path, fs }, layerfs, outside, parts, stores, sandboxes, unreadable,
/// complete, reason? }`.
pub(crate) fn serve(side: &Side<'_>, request: &SdkValue, budget: &Budget) -> Outcome {
    let (Side::Home(root) | Side::Worker(root)) = side;
    let root: &Path = root;
    let stores = live::listed(request, "stores")?;
    for store in &stores {
        StoreName::parse(store)?;
    }
    let sandboxes = live::names(request)?;
    let disk = home::filesystem(root).ok();
    let mut walk = Walk {
        budget,
        left: ENTRIES,
        complete: true,
        unreadable: 0,
        linked: HashSet::new(),
    };

    let mut store_rows = Vec::new();
    let stores_part = walk.part(|walk| {
        let mut total = 0;
        for store in &stores {
            let bytes = walk.measure(&paths::store_dir(root, store), &[]);
            total += bytes;
            store_rows.push(map([
                ("name", SdkValue::text(store)),
                ("bytes", size(bytes)),
            ]));
        }
        total
    });
    let mut rows = Vec::new();
    let mut named = HashSet::new();
    let sandboxes_part = walk.part(|walk| {
        let mut total = 0;
        for sandbox in &sandboxes {
            let (row, bytes) = walk.sandbox(side, sandbox, &mut named);
            total += bytes;
            rows.push(row);
        }
        total
    });
    let run = paths::run_root(root);
    let images = walk.part(|walk| walk.measure(&run.join("images"), &[]));
    let other_runs = walk.part(|walk| walk.other_runs(&run, &named));
    let logs = walk.part(|walk| {
        walk.measure(&paths::log_dir(root), &[]) + walk.measure(&paths::trace_dir(root), &[])
    });
    let other = walk.part(|walk| {
        ["incoming", "backup", "sandbox"]
            .into_iter()
            .map(|folder| walk.measure(&root.join(folder), &[]))
            .sum()
    });

    let parts = [
        ("stores", stores_part),
        ("sandboxes", sandboxes_part),
        ("images", images),
        ("other_runs", other_runs),
        ("logs", logs),
        ("other", other),
    ];
    let layerfs: u64 = parts.iter().filter_map(|(_, bytes)| *bytes).sum();
    let outside = (disk.as_ref())
        .filter(|_| walk.complete)
        .map(|disk| disk.used.saturating_sub(layerfs));
    let mut reasons = Vec::new();
    if !walk.complete {
        reasons.push(live::DEADLINE);
    }
    if walk.unreadable > 0 {
        reasons.push("unreadable");
    }
    if disk.is_none() {
        reasons.push("disk_not_read");
    }
    let disk = map([
        ("total", opt(disk.as_ref().map(|disk| disk.total))),
        ("used", opt(disk.as_ref().map(|disk| disk.used))),
        ("free", opt(disk.as_ref().map(|disk| disk.free))),
        ("path", SdkValue::Text(root.display().to_string())),
        (
            "fs",
            SdkValue::opt_text(disk.as_ref().map(|disk| disk.kind.clone())),
        ),
    ]);
    let mut answer = vec![
        ("observed_at", SdkValue::Int(home::now_ms())),
        ("disk", disk),
        ("layerfs", size(layerfs)),
        ("outside", opt(outside)),
        (
            "parts",
            map(parts.map(|(field, bytes)| (field, opt(bytes)))),
        ),
        ("stores", SdkValue::List(store_rows)),
        ("sandboxes", SdkValue::List(rows)),
        ("unreadable", SdkValue::Int(walk.unreadable)),
        ("complete", SdkValue::Bool(walk.complete)),
    ];
    if !reasons.is_empty() {
        answer.push(("reason", SdkValue::Text(reasons.join(", "))));
    }
    Ok(map(answer))
}

/// One walk in progress.
struct Walk<'a> {
    budget: &'a Budget,
    left: usize,
    complete: bool,
    unreadable: i64,
    /// `(device, inode)` of the files with several names counted so far.
    linked: HashSet<(u64, u64)>,
}

impl Walk<'_> {
    /// Counts one more entry; `false` once a bound has ended the walk.
    fn more(&mut self) -> bool {
        if self.left == 0 || self.budget.spent() {
            self.complete = false;
            return false;
        }
        self.left -= 1;
        true
    }

    /// `count`'s total, or `None` when a bound ended the walk before this
    /// part began.
    fn part(&mut self, count: impl FnOnce(&mut Self) -> u64) -> Option<u64> {
        if self.left == 0 || self.budget.spent() {
            self.complete = false;
            return None;
        }
        Some(count(self))
    }

    /// The blocks `found` takes; 0 for a further name of a file already
    /// counted.
    fn blocks(&mut self, found: &Metadata) -> u64 {
        if !found.is_dir() && found.nlink() > 1 {
            let key = (found.dev(), found.ino());
            if self.linked.contains(&key) {
                return 0;
            }
            if self.linked.len() < LINKED {
                self.linked.insert(key);
            }
        }
        found.blocks().saturating_mul(512)
    }

    /// The blocks of `path`: the file, or the folder and everything under
    /// it, through no link and into no other device. `skip` names children
    /// of `path` itself that are neither entered nor `stat`ed. An absent
    /// path is 0; a folder that cannot be listed counts once in
    /// `unreadable`. A bound ends it early with what it counted.
    fn measure(&mut self, path: &Path, skip: &[&str]) -> u64 {
        if !self.more() {
            return 0;
        }
        let top = match std::fs::symlink_metadata(path) {
            Ok(top) => top,
            Err(error) if error.kind() == ErrorKind::NotFound => return 0,
            Err(_) => {
                self.unreadable += 1;
                return 0;
            }
        };
        let mut bytes = self.blocks(&top);
        if !top.is_dir() {
            return bytes;
        }
        let device = top.dev();
        let mut pending = vec![(path.to_owned(), true)];
        while let Some((dir, first)) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                self.unreadable += 1;
                continue;
            };
            for entry in entries.flatten() {
                if first && skip.iter().any(|skip| entry.file_name() == *skip) {
                    continue;
                }
                if !self.more() {
                    return bytes;
                }
                // gone since the listing: nothing to count
                let Ok(found) = std::fs::symlink_metadata(entry.path()) else {
                    continue;
                };
                if !found.is_dir() {
                    bytes += self.blocks(&found);
                } else if found.dev() == device {
                    bytes += self.blocks(&found);
                    pending.push((entry.path(), false));
                }
            }
        }
        bytes
    }

    /// Sandbox `name`'s row and the blocks its run folder takes: the run
    /// folder its daemon record names on this side ([`route::run_folder`]),
    /// whose name goes into `named`. Each top-level entry counts to one
    /// part: `overlay/` to `overlays`, but its `upper/` and `work/` (the
    /// Sandbox's own writable layer) to `writes`; `tmp/` to `home`;
    /// `daemon.log*` to `log`; the rest (`bin/`, `runtime/`, `config/`,
    /// the folder itself) to `runtime`. `workspaces/` is not entered.
    fn sandbox(
        &mut self,
        side: &Side<'_>,
        name: &str,
        named: &mut HashSet<String>,
    ) -> (SdkValue, u64) {
        let run = match route::run_folder(side, name) {
            Ok(run) => run,
            Err(refusal) => return (row(name, None, Some(why(&refusal))), 0),
        };
        if let Some(folder) = run.file_name().and_then(|folder| folder.to_str()) {
            named.insert(folder.to_owned());
        }
        if !self.more() {
            return (row(name, None, Some(live::DEADLINE)), 0);
        }
        // overlays, writes, home, log, runtime
        let mut parts = [0u64; 5];
        if let Ok(top) = std::fs::symlink_metadata(&run) {
            parts[4] += self.blocks(&top);
        }
        let Ok(entries) = std::fs::read_dir(&run) else {
            self.unreadable += 1;
            return (row(name, Some(parts), None), parts.iter().sum());
        };
        for entry in entries.flatten() {
            let file = entry.file_name();
            let Some(file) = file.to_str() else {
                parts[4] += self.measure(&entry.path(), &[]);
                continue;
            };
            match file {
                "workspaces" => {}
                "overlay" => {
                    parts[0] += self.measure(&entry.path(), &["upper", "work"]);
                    parts[1] += self.measure(&entry.path().join("upper"), &[]);
                    parts[1] += self.measure(&entry.path().join("work"), &[]);
                }
                "tmp" => parts[2] += self.measure(&entry.path(), &[]),
                log if log.starts_with("daemon.log") => {
                    parts[3] += self.measure(&entry.path(), &[]);
                }
                _ => parts[4] += self.measure(&entry.path(), &[]),
            }
        }
        // a bound that ended the walk inside this run folder: its sizes
        // are what was counted, and `deadline` says so
        let cut = (!self.complete).then_some(live::DEADLINE);
        (row(name, Some(parts), cut), parts.iter().sum())
    }

    /// The run root's entries no listed Sandbox's run folder is (`named`):
    /// run folders no record here names, and files. The image cache and
    /// the private roots `<name>.root` are not entered, nor any run
    /// folder's `workspaces/`.
    fn other_runs(&mut self, run: &Path, named: &HashSet<String>) -> u64 {
        let Ok(entries) = std::fs::read_dir(run) else {
            return 0;
        };
        let mut bytes = 0;
        for entry in entries.flatten() {
            let file = entry.file_name();
            let file = file.to_string_lossy();
            if file == "images" || file.ends_with(".root") || named.contains(file.as_ref()) {
                continue;
            }
            bytes += self.measure(&entry.path(), &["workspaces"]);
        }
        bytes
    }
}

/// A Sandbox's row: its sizes, or null sizes with `reason`.
fn row(name: &str, parts: Option<[u64; 5]>, reason: Option<&str>) -> SdkValue {
    let part = |at: usize| parts.map_or(SdkValue::Null, |parts| size(parts[at]));
    let mut fields = vec![
        ("name", SdkValue::text(name)),
        (
            "bytes",
            parts.map_or(SdkValue::Null, |parts| size(parts.iter().sum())),
        ),
        ("overlays", part(0)),
        ("writes", part(1)),
        ("home", part(2)),
        ("log", part(3)),
        ("runtime", part(4)),
    ];
    fields.extend(reason.map(|why| ("reason", SdkValue::text(why))));
    map(fields)
}

/// Why a Sandbox's run folder was not measured.
fn why(refusal: &SdkRefusal) -> &'static str {
    match refusal.code.as_str() {
        "NoSuchSandbox" => "no_run_folder",
        "InvalidInput" => "outside_run_root",
        _ => "record_unreadable",
    }
}

fn size(bytes: u64) -> SdkValue {
    i64::try_from(bytes).map_or(SdkValue::Null, SdkValue::Int)
}

fn opt(bytes: Option<u64>) -> SdkValue {
    bytes.map_or(SdkValue::Null, size)
}

fn map<'a>(fields: impl IntoIterator<Item = (&'a str, SdkValue)>) -> SdkValue {
    SdkValue::Map(
        (fields.into_iter())
            .map(|(field, value)| (field.to_owned(), value))
            .collect(),
    )
}
