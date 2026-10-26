//! The change walk lists exactly what the full walk of both trees lists
//! (#352 range 002), and every binding of every saved tree is recorded.
//!
//! Random histories of writes, edits, removals, renames and copies of files
//! and directories, with forks and merges, are compared Commit against Commit
//! with `branch.diff_commits` and against an in-test walk of both whole trees
//! (the walk the daemon used before). Each saved tree's every binding must be
//! among the recorded hints, or the change walk could miss a path.
//!
//! #352 F-025 fix 1: a merge names its changed inodes from those hints
//! (`name_from_hints`) instead of the breadth-first `locate`. For every compared
//! pair, every inode changed on both sides gets the same name from the hints as
//! from `locate`, in both trees, so a merge's candidate paths, and with them its
//! picks, conflicts and merged root, are unchanged.
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_content::compare::tree::{same_inode, same_metadata};
use layerfs_content::compare::{
    inode_changes, list_directory, locate, name_from_hints, DifferenceWork, EntryFacts, TreeTable,
};
use layerfs_content::filesystem::FilesystemRootId;
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{AuthenticatedObjects, ObjectId};
use layerfs_history::LayerStackId;
use layerfs_store_service::store::CatalogHints;
use std::collections::BTreeSet;
use std::fs;
use support::{items, operation, write, Fixture};

/// Small deterministic generator (xorshift64*).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, set: &'a BTreeSet<String>) -> Option<&'a String> {
        (!set.is_empty()).then(|| set.iter().nth(self.below(set.len())).unwrap())
    }
}

/// The test's own view of one Branch's tree, to choose valid operations.
#[derive(Clone)]
struct Model {
    dirs: BTreeSet<String>,
    files: BTreeSet<String>,
    serial: usize,
}

impl Model {
    fn join(dir: &str, name: &str) -> String {
        if dir == "/" {
            format!("/{name}")
        } else {
            format!("{dir}/{name}")
        }
    }
    fn under(path: &str, prefix: &str) -> bool {
        path == prefix || path.starts_with(&format!("{prefix}/"))
    }
    fn fresh(&mut self, tag: &str) -> String {
        self.serial += 1;
        format!("{tag}{}", self.serial)
    }
    /// One random valid operation, applied to the model.
    fn operation(&mut self, rng: &mut Rng) -> SdkValue {
        loop {
            let dir = rng.pick(&self.dirs).unwrap().clone();
            match rng.below(9) {
                0 | 1 => {
                    let path = Self::join(&dir, &self.fresh("f"));
                    self.files.insert(path.clone());
                    return write(&path, format!("new {path}\n"));
                }
                2 | 3 => {
                    if let Some(path) = rng.pick(&self.files).cloned() {
                        let serial = self.fresh("e");
                        return write(&path, format!("edit {serial} {path}\n"));
                    }
                }
                4 => {
                    let path = Self::join(&dir, &self.fresh("d"));
                    self.dirs.insert(path.clone());
                    return operation("mkdir", &path);
                }
                5 => {
                    if let Some(path) = rng.pick(&self.files).cloned() {
                        self.files.remove(&path);
                        return operation("rm", &path);
                    }
                }
                6 => {
                    let Some(path) = rng.pick(&self.dirs).cloned() else {
                        continue;
                    };
                    if path == "/" {
                        continue;
                    }
                    self.dirs.retain(|d| !Self::under(d, &path));
                    self.files.retain(|f| !Self::under(f, &path));
                    return operation("rm", &path);
                }
                7 | 8 => {
                    let copy = rng.below(2) == 0;
                    let from = if rng.below(2) == 0 {
                        rng.pick(&self.files).cloned()
                    } else {
                        rng.pick(&self.dirs).cloned().filter(|d| d != "/")
                    };
                    let Some(from) = from else { continue };
                    if Self::under(&dir, &from) {
                        continue;
                    }
                    let to = Self::join(&dir, &self.fresh(if copy { "c" } else { "m" }));
                    let moved = |set: &BTreeSet<String>| {
                        set.iter()
                            .filter(|p| Self::under(p, &from))
                            .map(|p| format!("{to}{}", &p[from.len()..]))
                            .collect::<Vec<_>>()
                    };
                    let (dirs, files) = (moved(&self.dirs), moved(&self.files));
                    if !copy {
                        self.dirs.retain(|d| !Self::under(d, &from));
                        self.files.retain(|f| !Self::under(f, &from));
                    }
                    self.dirs.extend(dirs);
                    self.files.extend(files);
                    return SdkValue::map([
                        ("op", SdkValue::text(if copy { "cp" } else { "mv" })),
                        ("from", SdkValue::text(from.as_str())),
                        ("to", SdkValue::text(to.as_str())),
                    ]);
                }
                _ => unreachable!(),
            }
        }
    }
}

/// Every entry of a tree in component order: (path, entry).
fn entries(reader: &dyn AuthenticatedObjects, root: ObjectId) -> Vec<(String, EntryFacts)> {
    let tree = TreeTable::open(reader, FilesystemRootId(root)).unwrap();
    let mut out = Vec::new();
    let root_value = tree.value(reader, tree.root_serial()).unwrap().unwrap();
    let mut stack = vec![("/".to_owned(), root_value.content_root)];
    while let Some((path, listing)) = stack.pop() {
        let mut children = Vec::new();
        list_directory(reader, listing, |name, serial| {
            children.push((layerfs_content::compare::tree::join(&path, name), serial));
            Ok(true)
        })
        .unwrap();
        let serials: Vec<u64> = children.iter().map(|(_, s)| *s).collect();
        let values = tree.values(reader, &serials).unwrap();
        // Depth first, children in name order: push in reverse, emit on pop.
        let mut level = Vec::new();
        for ((child, serial), value) in children.into_iter().zip(values) {
            level.push((
                child,
                EntryFacts {
                    serial,
                    value: value.unwrap(),
                },
            ));
        }
        for (child, entry) in level.into_iter().rev() {
            if entry.value.kind == InodeKind::Directory {
                stack.push((child.clone(), entry.value.content_root));
            }
            out.push((child, entry));
        }
    }
    // The stack visited depth first in reverse; order by components.
    out.sort_by(|a, b| a.0.split('/').cmp(b.0.split('/')));
    out
}

/// The full walk's listing: every path whose entries differ, except a
/// directory whose only change is its children.
fn walked(reader: &dyn AuthenticatedObjects, from: ObjectId, to: ObjectId) -> Vec<String> {
    let mut work = layerfs_content::filesystem::attributes::AttributeReadWork::default();
    let (left, right) = (entries(reader, from), entries(reader, to));
    let (mut l, mut r) = (left.into_iter().peekable(), right.into_iter().peekable());
    let mut out = Vec::new();
    loop {
        let order = match (l.peek(), r.peek()) {
            (None, None) => break,
            (Some(a), Some(b)) => a.0.split('/').cmp(b.0.split('/')),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
        };
        let (path, before, after) = match order {
            std::cmp::Ordering::Less => {
                let (p, e) = l.next().unwrap();
                (p, Some(e), None)
            }
            std::cmp::Ordering::Greater => {
                let (p, e) = r.next().unwrap();
                (p, None, Some(e))
            }
            std::cmp::Ordering::Equal => {
                let (p, a) = l.next().unwrap();
                let (_, b) = r.next().unwrap();
                (p, Some(a), Some(b))
            }
        };
        // The SDK diff compares same-serial inodes semantically: a form-only
        // metadata difference is no change, and a directory whose decoded
        // portable metadata matches lists only its children.
        if match (before, after) {
            (Some(x), Some(y)) if x.serial == y.serial => {
                same_inode(reader, &x.value, &y.value, &mut work).unwrap()
            }
            _ => before == after,
        } {
            continue;
        }
        if let (Some(x), Some(y)) = (before, after) {
            if x.serial == y.serial
                && x.value.kind == InodeKind::Directory
                && y.value.kind == InodeKind::Directory
                && same_metadata(
                    reader,
                    x.value.kind,
                    x.value.metadata_root,
                    y.value.metadata_root,
                    &mut work,
                )
                .unwrap()
            {
                continue;
            }
        }
        let side = |e: Option<EntryFacts>| e.map_or("-".to_owned(), |e| e.serial.to_string());
        out.push(format!("{path} {} {}", side(before), side(after)));
    }
    out
}

/// The daemon's listing, all pages of `page` items.
fn listed(fixture: &Fixture, from: &str, to: &str, page: i64) -> (Vec<String>, ObjectId, ObjectId) {
    let (mut out, mut pos) = (Vec::new(), 0i64);
    loop {
        let reply = fixture.call(
            "branch.diff_commits",
            vec![
                ("from", SdkValue::text(from)),
                ("to", SdkValue::text(to)),
                ("pageSize", SdkValue::Int(page)),
                ("pagePos", SdkValue::Int(pos)),
            ],
        );
        let side = |item: &SdkValue, name: &str| match item.get(name) {
            Some(SdkValue::Null) | None => "-".to_owned(),
            Some(facts) => facts.get_int("inode").unwrap().to_string(),
        };
        for item in items(&reply) {
            out.push(format!(
                "{} {} {}",
                item.get_text("path").unwrap(),
                side(item, "before"),
                side(item, "after")
            ));
        }
        let root = |name: &str| {
            let hex = reply.get(name).unwrap().get_text("root").unwrap();
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            ObjectId::from_bytes(&bytes).unwrap()
        };
        match reply.get_int("nextPagePos") {
            Some(next) => pos = next,
            None => return (out, root("from"), root("to")),
        }
    }
}

/// Every inode changed on both sides of `from` -> `to` is named alike by the
/// hints and by the breadth-first walk, in each tree; returns how many.
fn named_alike(
    fixture: &Fixture,
    reader: &dyn AuthenticatedObjects,
    from: ObjectId,
    to: ObjectId,
) -> usize {
    let trees = [from, to].map(|root| TreeTable::open(reader, FilesystemRootId(root)).unwrap());
    let mut wanted = BTreeSet::new();
    inode_changes(
        reader,
        &trees[0],
        &trees[1],
        &mut DifferenceWork::default(),
        |serial, b, a| {
            if b.is_some() && a.is_some() {
                wanted.insert(serial);
            }
            Ok(())
        },
    )
    .unwrap();
    let hints = CatalogHints::new(fixture.service.store.history());
    for tree in &trees {
        let walked = locate(reader, tree, &wanted, usize::MAX).unwrap();
        let hinted =
            name_from_hints(reader, &hints, tree, &wanted, &mut Default::default()).unwrap();
        assert_eq!(
            hinted.as_ref(),
            Some(&walked),
            "hints name {wanted:?} unlike the walk"
        );
    }
    wanted.len()
}

/// Every binding of `root` must be a recorded hint.
fn recorded(fixture: &Fixture, reader: &dyn AuthenticatedObjects, root: ObjectId) {
    let tree = TreeTable::open(reader, FilesystemRootId(root)).unwrap();
    let mut stack = vec![tree.root_serial()];
    let history = fixture.service.store.history();
    while let Some(directory) = stack.pop() {
        let value = tree.value(reader, directory).unwrap().unwrap();
        let mut children = Vec::new();
        list_directory(reader, value.content_root, |name, serial| {
            children.push((name.as_bytes().to_vec(), serial));
            Ok(true)
        })
        .unwrap();
        for (name, serial) in children {
            let hints = history.bindings_of(serial).unwrap();
            assert!(
                hints
                    .iter()
                    .any(|h| h.parent == directory && h.name == name),
                "binding {directory}/{} -> {serial} is not recorded",
                String::from_utf8_lossy(&name)
            );
            if tree.value(reader, serial).unwrap().unwrap().kind == InodeKind::Directory {
                stack.push(serial);
            }
        }
    }
}

#[test]
fn change_walk_equals_the_full_walk() {
    for seed in [352u64, 7, 2026] {
        let fixture = Fixture::new(&format!("diff-oracle-{seed}"), |source| {
            for dir in ["a", "a/b", "c"] {
                fs::create_dir(source.join(dir)).unwrap();
            }
            for file in ["a/one", "a/b/two", "c/three", "four"] {
                fs::write(source.join(file), format!("{file}\n")).unwrap();
            }
            7
        });
        let stack = fixture
            .service
            .store
            .history()
            .layer_stack(LayerStackId::from_authority([23; 16]))
            .unwrap()
            .unwrap();
        let reader = fixture.service.store.ports(stack.scope);
        let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        let mut model = Model {
            dirs: ["/", "/a", "/a/b", "/c"].map(str::to_owned).into(),
            files: ["/a/one", "/a/b/two", "/c/three", "/four"]
                .map(str::to_owned)
                .into(),
            serial: 0,
        };
        let mut branch = "work".to_owned();
        fixture.fork(&branch);
        let mut heads = Vec::new();
        for step in 0..120 {
            if step % 20 == 19 {
                // A merge: a side Branch from the current head, one Commit on
                // each side, then continue on the merge.
                let side = format!("side-{step}");
                fixture.call(
                    "branch.fork",
                    vec![
                        ("name", SdkValue::text(side.as_str())),
                        ("from", SdkValue::text(branch.as_str())),
                    ],
                );
                let tag = model.fresh("s");
                fixture.apply(&side, vec![write(&format!("/{tag}"), "side\n")]);
                model.files.insert(format!("/{tag}"));
                let op = model.operation(&mut rng);
                fixture.apply(&branch, vec![op]);
                let merged = format!("merged-{step}");
                let reply = fixture.call(
                    "branch.merge",
                    vec![
                        ("branch", SdkValue::text(branch.as_str())),
                        ("with", SdkValue::text(side.as_str())),
                        ("name", SdkValue::text(merged.as_str())),
                    ],
                );
                assert_eq!(reply.get_text("kind"), Some("Merged"), "{reply:?}");
                heads.push(reply.get_text("head").unwrap().to_owned());
                branch = merged;
                continue;
            }
            let count = 1 + rng.below(3);
            let ops = (0..count).map(|_| model.operation(&mut rng)).collect();
            let reply = fixture.apply(&branch, ops);
            heads.push(reply.get_text("head").unwrap().to_owned());
        }
        let mut compared = (0, 0, 0);
        for i in 0..heads.len() {
            let j = if i % 3 == 0 {
                rng.below(heads.len())
            } else {
                (i + 1).min(heads.len() - 1)
            };
            let page = [1000, 3][i % 2];
            let (daemon, from, to) = listed(&fixture, &heads[i], &heads[j], page);
            let oracle = walked(&*reader, from, to);
            assert_eq!(daemon, oracle, "seed {seed}: diff {i} -> {j}");
            compared.2 += named_alike(&fixture, &*reader, from, to);
            compared.0 += 1;
            compared.1 += daemon.len();
            if i % 10 == 0 {
                recorded(&fixture, &*reader, to);
            }
        }
        println!(
            "DIFF_ORACLE seed={seed} diffs={} entries={} named_alike={} profile=Disposable",
            compared.0, compared.1, compared.2
        );
        drop(reader);
        fixture.cleanup();
    }
}
