//! One path resolved across many versions' roots (#818 H5): the batched
//! window gives each version exactly the answer of walking it alone, and a
//! window costs one roots demand, one demand per inode level and one per
//! directory page level of its memo misses, whatever its width.
mod support;

use std::cell::Cell;
use std::collections::BTreeMap;

use layerfs_content::compare::{
    AcrossWork, DirectoryMemo, EntryFacts, PathAcross, PathAnswer, TreeTable,
};
use layerfs_content::filesystem::directory::read::{lookup_counted, DirectoryReadWork};
use layerfs_content::filesystem::inode::read::{lookup_many, lookup_tables, InodeReadWork};
use layerfs_content::filesystem::{DirectoryRoot, FilesystemResources, FilesystemRootId, PathName};
use layerfs_content::object::inode_leaf::InodeKind;
use layerfs_content::{AuthenticatedObjects, ContentResult, ObjectId};
use support::filesystem::{name_of, synthetic, value, TreeStore};
use support::topology::{update_resident, Rowset, Shape, Tree, ROOT};

/// The directory-lookup memo bound the store service's history calls use (H3).
const MEMO: usize = 4_096;

/// Counts demands and the widest one.
struct Counting<'s> {
    store: &'s TreeStore,
    demands: Cell<u64>,
    largest: Cell<usize>,
}
impl<'s> Counting<'s> {
    fn new(store: &'s TreeStore) -> Self {
        Self {
            store,
            demands: Cell::new(0),
            largest: Cell::new(0),
        }
    }
}
impl AuthenticatedObjects for Counting<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demands.set(self.demands.get() + 1);
        self.largest.set(self.largest.get().max(ids.len()));
        self.store.read_canonical_batch(ids)
    }
}

/// Versions of one tree, oldest first.
struct History {
    tree: Tree,
    roots: Vec<FilesystemRootId>,
}
impl History {
    fn new(shape: Shape) -> Self {
        let tree = shape.build();
        let roots = vec![tree.root];
        Self { tree, roots }
    }
    fn allocate(&mut self) -> u64 {
        self.tree.next += 1;
        self.tree.next - 1
    }
    /// Applies one update as the next version; false when it is refused.
    fn try_push(&mut self, rows: &Rowset) -> bool {
        let (result, emitted) = update_resident(&self.tree, rows, FilesystemResources::default());
        let Ok(result) = result else { return false };
        self.tree.store.absorb(&emitted);
        self.tree.root = result.root;
        self.roots.push(result.root);
        true
    }
    fn push(&mut self, rows: Rowset) {
        assert!(self.try_push(&rows), "version update");
    }
    /// One version that changes only the content of `file`.
    fn edit(&mut self, file: u64) {
        let label = format!("edit/{}", self.roots.len());
        self.push(Rowset::new().value(
            file,
            value(InodeKind::RegularFile, synthetic(&label), synthetic("meta")),
        ));
    }
    /// Newest first, as a history walk takes them.
    fn walk_order(&self) -> Vec<FilesystemRootId> {
        self.roots.iter().rev().copied().collect()
    }
}

fn names(path: &str) -> Vec<PathName> {
    path.split('/')
        .filter(|name| !name.is_empty())
        .map(name_of)
        .collect()
}

/// The store service's per-version walk (`read/snapshot.rs` `find_with`),
/// through the public tree reads.
fn reference(
    reader: &dyn AuthenticatedObjects,
    root: FilesystemRootId,
    names: &[PathName],
    strict: bool,
) -> PathAnswer {
    let tree = TreeTable::open(reader, root).unwrap();
    let mut serial = tree.root_serial();
    for (index, name) in names.iter().enumerate() {
        match tree.lookup(reader, serial, name).unwrap() {
            Some(child) => serial = child,
            None => return PathAnswer::Absent,
        }
        if strict && index + 1 < names.len() {
            match tree.value(reader, serial).unwrap() {
                None => return PathAnswer::Unbound { component: index },
                Some(value) if value.kind == InodeKind::Symlink => {
                    return PathAnswer::Symlink { component: index }
                }
                Some(_) => {}
            }
        }
    }
    tree.value(reader, serial)
        .unwrap()
        .map_or(PathAnswer::Absent, |value| {
            PathAnswer::Entry(EntryFacts { serial, value })
        })
}

/// Windows double from 16 to 128, as the history calls take them.
fn widths(total: usize) -> Vec<usize> {
    let (mut widths, mut width, mut left) = (Vec::new(), 16, total);
    while left > 0 {
        widths.push(width.min(left));
        left -= width.min(left);
        width = (width * 2).min(128);
    }
    widths
}

/// Resolves `path` over the history in windows and checks every version's
/// answer against the per-version walk; returns each window's demands.
fn resolve_windows(
    history: &History,
    path: &str,
    strict: bool,
    memo: &mut DirectoryMemo,
) -> Vec<u64> {
    let reader = Counting::new(&history.tree.store);
    let roots = history.walk_order();
    let names = names(path);
    let mut across = PathAcross::new(names.clone(), strict);
    let mut demands = Vec::new();
    let mut at = 0;
    for width in widths(roots.len()) {
        let window = &roots[at..at + width];
        let before = reader.demands.get();
        let mut work = AcrossWork::default();
        let answers = across
            .resolve(&reader, window, memo, true, &mut work)
            .unwrap();
        let spent = reader.demands.get() - before;
        assert_eq!(spent, work.demands(), "{path}: work counts every demand");
        demands.push(spent);
        assert_eq!(answers.len(), window.len());
        for (root, answer) in window.iter().zip(&answers) {
            let opened = TreeTable::open(&history.tree.store, *root).unwrap();
            assert_eq!(answer.tree.root, opened.root);
            assert_eq!(answer.tree.table, opened.table);
            assert_eq!(
                answer.answer,
                reference(&history.tree.store, *root, &names, strict),
                "{path} strict={strict} at {root:?}"
            );
        }
        at += width;
    }
    assert!(reader.largest.get() <= 128, "a demand above 128 objects");
    demands
}

/// The per-version fallback reads exactly what the per-version walk reads,
/// and answers the same.
fn fallback_matches(history: &History, path: &str, strict: bool) {
    let names = names(path);
    let roots = history.walk_order();
    let reference_reader = Counting::new(&history.tree.store);
    let expected = roots
        .iter()
        .map(|root| reference(&reference_reader, *root, &names, strict))
        .collect::<Vec<_>>();
    let reader = Counting::new(&history.tree.store);
    let mut work = AcrossWork::default();
    let answers = PathAcross::new(names.clone(), strict)
        .resolve(
            &reader,
            &roots,
            &mut DirectoryMemo::new(0),
            false,
            &mut work,
        )
        .unwrap();
    let answers = answers
        .iter()
        .map(|version| version.answer)
        .collect::<Vec<_>>();
    assert_eq!(answers, expected, "{path} strict={strict}");
    assert_eq!(
        reader.demands.get(),
        reference_reader.demands.get(),
        "{path} strict={strict}: the fallback reads what the walk reads"
    );
    assert_eq!(reader.demands.get(), work.demands());
    // With a memo the fallback skips directory reads it already holds.
    let memoized = Counting::new(&history.tree.store);
    let answers = PathAcross::new(names, strict)
        .resolve(
            &memoized,
            &roots,
            &mut DirectoryMemo::new(MEMO),
            false,
            &mut AcrossWork::default(),
        )
        .unwrap();
    assert!(answers
        .iter()
        .map(|version| version.answer)
        .eq(expected.iter().copied()));
    assert!(memoized.demands.get() <= reader.demands.get());
}

/// Inode table height, and the directory pages a lookup of `name` in
/// `directory` reads, in `root`.
fn heights(history: &History, root: FilesystemRootId, directory: u64, name: &str) -> (u64, u64) {
    let store = &history.tree.store;
    let tree = TreeTable::open(store, root).unwrap();
    let mut inodes = InodeReadWork::default();
    let value = lookup_many(store, tree.table, &[directory], &mut inodes).unwrap()[0].unwrap();
    let mut pages = DirectoryReadWork::default();
    lookup_counted(
        store,
        DirectoryRoot(value.content_root),
        &name_of(name),
        &mut pages,
    )
    .unwrap()
    .expect("a bound name");
    (inodes.read_waves, pages.read_waves)
}

#[test]
fn deep_narrow_content_edits_cost_one_inode_wave_per_window() {
    let mut shape = Shape::new();
    let mut parent = ROOT;
    for level in 0..12 {
        parent = shape.dir(parent, &format!("d{level}"));
    }
    let file = shape.file(parent, "f");
    let mut history = History::new(shape);
    for _ in 0..239 {
        history.edit(file);
    }
    let mut path = (0..12).fold(String::new(), |path, level| {
        path + "/d" + &level.to_string()
    });
    path.push_str("/f");
    let (h, hd) = heights(&history, *history.roots.last().unwrap(), ROOT, "d0");
    assert_eq!((h, hd), (1, 1));
    for strict in [true, false] {
        let demands = resolve_windows(&history, &path, strict, &mut DirectoryMemo::new(MEMO));
        // Cold window: roots, the root's value, then a binding and a value per
        // component. Later windows: roots and one inode wave.
        assert_eq!(demands, vec![28, 2, 2, 2], "strict={strict}");
        println!("ACROSS deep-narrow strict={strict} windows 16/32/64/128: {demands:?}");
        fallback_matches(&history, &path, strict);
    }
}

#[test]
fn a_changing_wide_directory_costs_its_page_height_per_window() {
    let mut shape = Shape::new();
    let wide = shape.dir(ROOT, "wide");
    for index in 0..10_000 {
        shape.file(wide, &format!("f{index:05}"));
    }
    let mut history = History::new(shape);
    for version in 0..47 {
        let serial = history.allocate();
        history.push(Rowset::new().fresh(serial, InodeKind::RegularFile).bind(
            wide,
            &format!("n{version:05}"),
            serial,
        ));
    }
    let last = *history.roots.last().unwrap();
    let (h, hd_root) = heights(&history, last, ROOT, "wide");
    let (_, hd_wide) = heights(&history, last, wide, "f05000");
    println!("ACROSS wide-flat heights: inode {h}, root dir {hd_root}, wide dir {hd_wide}");
    for strict in [true, false] {
        let demands = resolve_windows(
            &history,
            "/wide/f05000",
            strict,
            &mut DirectoryMemo::new(MEMO),
        );
        let cold = 1 + h + (hd_root + h) + (hd_wide + h);
        assert_eq!(demands, vec![cold, 1 + h + hd_wide], "strict={strict}");
        println!("ACROSS wide-flat strict={strict} windows 16/32: {demands:?}");
    }
    fallback_matches(&history, "/wide/f05000", true);
}

/// `/a/l` a symlink, `/a/b/f` a file, `/e` edited in every version.
fn symlink_history() -> (History, u64) {
    let mut shape = Shape::new();
    let a = shape.dir(ROOT, "a");
    shape.symlink(a, "l");
    let b = shape.dir(a, "b");
    shape.file(b, "f");
    let e = shape.file(ROOT, "e");
    let mut history = History::new(shape);
    for _ in 0..47 {
        history.edit(e);
    }
    (history, e)
}

#[test]
fn an_intermediate_symlink_is_refused_where_the_walk_refuses_it() {
    let (history, _) = symlink_history();
    let demands = resolve_windows(&history, "/a/l/f", true, &mut DirectoryMemo::new(MEMO));
    assert_eq!(demands, vec![6, 2]);
    let demands = resolve_windows(&history, "/a/l/f", false, &mut DirectoryMemo::new(MEMO));
    assert_eq!(demands, vec![6, 2]);
    let reader = Counting::new(&history.tree.store);
    let answers = PathAcross::new(names("/a/l/f"), true)
        .resolve(
            &reader,
            &history.walk_order(),
            &mut DirectoryMemo::new(MEMO),
            true,
            &mut AcrossWork::default(),
        )
        .unwrap();
    assert!(answers
        .iter()
        .all(|version| version.answer == PathAnswer::Symlink { component: 1 }));
    fallback_matches(&history, "/a/l/f", true);
}

/// `/a/x` and `/e`; at version 30 `/a/k` appears, or with `kind_change`
/// the file `/a/k` becomes a new directory; `/e` is edited otherwise.
fn appearing(kind_change: bool) -> History {
    let mut shape = Shape::new();
    let a = shape.dir(ROOT, "a");
    shape.file(a, "x");
    if kind_change {
        shape.file(a, "k");
    }
    let e = shape.file(ROOT, "e");
    let mut history = History::new(shape);
    for version in 1..48 {
        if version == 30 {
            let serial = history.allocate();
            let rows = if kind_change {
                Rowset::new().unbind(a, "k").mkdir(a, "k", serial)
            } else {
                Rowset::new()
                    .fresh(serial, InodeKind::RegularFile)
                    .bind(a, "k", serial)
            };
            history.push(rows);
        } else {
            history.edit(e);
        }
    }
    history
}

#[test]
fn a_path_created_inside_a_window_costs_one_directory_wave() {
    let history = appearing(false);
    for strict in [true, false] {
        let demands = resolve_windows(&history, "/a/k", strict, &mut DirectoryMemo::new(MEMO));
        assert_eq!(demands, vec![6, 3], "strict={strict}");
    }
    fallback_matches(&history, "/a/k", true);
}

#[test]
fn a_kind_change_with_a_new_serial_reads_the_old_serial_once_more() {
    let history = appearing(true);
    for strict in [true, false] {
        let demands = resolve_windows(&history, "/a/k", strict, &mut DirectoryMemo::new(MEMO));
        assert_eq!(demands, vec![6, 4], "strict={strict}");
    }
    fallback_matches(&history, "/a/k", false);
}

#[test]
fn a_kind_change_on_the_same_serial_costs_nothing_extra() {
    let mut shape = Shape::new();
    let a = shape.dir(ROOT, "a");
    let k = shape.file(a, "k");
    let e = shape.file(ROOT, "e");
    let mut history = History::new(shape);
    for version in 1..48 {
        if version == 30 {
            history.push(Rowset::new().value(
                k,
                value(InodeKind::Symlink, synthetic("link"), synthetic("meta")),
            ));
        } else {
            history.edit(e);
        }
    }
    let mut memo = DirectoryMemo::new(MEMO);
    assert_eq!(
        resolve_windows(&history, "/a/k", true, &mut memo),
        vec![6, 2]
    );
    // The memo already holds `/` and `/a`: one binding wave fewer.
    let demands = resolve_windows(&history, "/a/k/x", true, &mut memo);
    assert_eq!(demands, vec![4, 2]);
    fallback_matches(&history, "/a/k/x", true);
}

#[test]
fn tables_of_several_versions_answer_as_each_table_alone() {
    let mut shape = Shape::new();
    let wide = shape.dir(ROOT, "wide");
    for index in 0..10_000 {
        shape.file(wide, &format!("f{index:05}"));
    }
    let mut history = History::new(shape);
    for version in 0..20 {
        let serial = history.allocate();
        history.push(Rowset::new().fresh(serial, InodeKind::RegularFile).bind(
            wide,
            &format!("n{version:05}"),
            serial,
        ));
    }
    let mut small = Shape::new();
    small.file(ROOT, "only");
    let small = small.build();
    let mut store = history.tree.store.clone();
    store.absorb(&small.store);
    let mut tables = history
        .roots
        .iter()
        .map(|root| TreeTable::open(&store, *root).unwrap().table)
        .collect::<Vec<_>>();
    tables.push(TreeTable::open(&store, small.root).unwrap().table);
    let mut random = Random(0x5eed);
    let demands = (0..2_000)
        .map(|_| {
            let table = tables[random.below(tables.len())];
            (table, 1 + random.below(10_030) as u64)
        })
        .collect::<Vec<_>>();
    let reader = Counting::new(&store);
    let mut work = InodeReadWork::default();
    let found = lookup_tables(&reader, &demands, 128, &mut work).unwrap();
    for ((table, serial), value) in demands.iter().zip(&found) {
        let alone =
            lookup_many(&store, *table, &[*serial], &mut InodeReadWork::default()).unwrap()[0];
        assert_eq!(*value, alone, "serial {serial}");
    }
    assert!(reader.largest.get() <= 128);
    assert_eq!(reader.demands.get(), work.read_waves);
    println!(
        "ACROSS lookup_tables: {} demands over {} tables, {} waves, {} pages",
        demands.len(),
        tables.len(),
        work.read_waves,
        work.pages_read
    );
    assert_eq!(
        lookup_tables(&reader, &[(tables[0], 0)], 128, &mut work),
        Err(layerfs_content::ContentError::InvalidRecord("inode serial"))
    );
}

#[test]
fn hardlinked_names_share_their_inode_reads() {
    let mut shape = Shape::new();
    let x = shape.dir(ROOT, "x");
    let y = shape.dir(ROOT, "y");
    let h = shape.file(x, "h");
    shape.link(y, "h2", h);
    shape.link(y, "h3", h);
    let mut history = History::new(shape);
    for _ in 0..47 {
        history.edit(h);
    }
    let mut memo = DirectoryMemo::new(MEMO);
    for path in ["/x/h", "/y/h2", "/y/h3"] {
        let demands = resolve_windows(&history, path, true, &mut memo);
        // The shared memo already holds `/y` for `/y/h3`.
        let cold = if path == "/y/h3" { 5 } else { 6 };
        assert_eq!(demands, vec![cold, 2], "{path}");
    }
}

/// A small namespace model to generate valid random versions.
#[derive(Clone)]
struct Model {
    dirs: BTreeMap<u64, BTreeMap<String, u64>>,
    kinds: BTreeMap<u64, InodeKind>,
    a: u64,
}

impl Model {
    fn names_of(&self, serial: u64) -> usize {
        self.dirs
            .values()
            .flat_map(BTreeMap::values)
            .filter(|child| **child == serial)
            .count()
    }
    fn unbind(&mut self, parent: u64, name: &str) {
        if let Some(serial) = self.dirs.get_mut(&parent).unwrap().remove(name) {
            if self.names_of(serial) == 0 {
                self.kinds.remove(&serial);
                self.dirs.remove(&serial);
            }
        }
    }
}

/// xorshift64*, seeded.
struct Random(u64);
impl Random {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 33) as usize % bound.max(1)
    }
    fn pick<T: Copy>(&mut self, items: &[T]) -> Option<T> {
        (!items.is_empty()).then(|| items[self.below(items.len())])
    }
}

const POOL: [&str; 8] = ["f", "g", "new", "x", "n5", "h2", "l", "b"];

/// One random version: an edit, create, remove, replace, link, rename or
/// kind change, kept only when the update accepts it.
fn random_version(history: &mut History, model: &mut Model, random: &mut Random, label: usize) {
    let before = model.clone();
    let dirs = model.dirs.keys().copied().collect::<Vec<_>>();
    let leaves = model
        .kinds
        .iter()
        .filter(|(_, kind)| **kind != InodeKind::Directory)
        .map(|(serial, kind)| (*serial, *kind))
        .collect::<Vec<_>>();
    let mut rows = Rowset::new();
    match random.below(7) {
        0 | 6 => {
            let Some((serial, kind)) = random.pick(&leaves) else {
                return;
            };
            let kind = match (random.below(7), kind) {
                (0, InodeKind::RegularFile) => InodeKind::Symlink,
                _ => kind,
            };
            model.kinds.insert(serial, kind);
            rows = rows.value(
                serial,
                value(kind, synthetic(&format!("r/{label}")), synthetic("meta")),
            );
        }
        1 => {
            let parent = random.pick(&dirs).unwrap();
            let name = POOL[random.below(POOL.len())];
            if model.dirs[&parent].contains_key(name) {
                return;
            }
            let serial = history.allocate();
            model.kinds.insert(serial, InodeKind::RegularFile);
            model
                .dirs
                .get_mut(&parent)
                .unwrap()
                .insert(name.into(), serial);
            rows = rows
                .fresh(serial, InodeKind::RegularFile)
                .bind(parent, name, serial);
        }
        2 | 5 => {
            let parent = random.pick(&dirs).unwrap();
            let names = model.dirs[&parent]
                .iter()
                .filter(|(_, child)| model.kinds[child] != InodeKind::Directory)
                .map(|(name, child)| (name.clone(), *child))
                .collect::<Vec<_>>();
            let Some(index) = (!names.is_empty()).then(|| random.below(names.len())) else {
                return;
            };
            let (name, serial) = names[index].clone();
            rows = rows.unbind(parent, &name);
            if random.below(2) == 0 {
                let target = random.pick(&dirs).unwrap();
                let new = POOL[random.below(POOL.len())];
                if model.dirs[&target].contains_key(new) {
                    return;
                }
                model
                    .dirs
                    .get_mut(&target)
                    .unwrap()
                    .insert(new.into(), serial);
                rows = rows.bind(target, new, serial);
            }
            model.unbind(parent, &name);
        }
        3 => {
            let a = model.a;
            match model.dirs[&a].get("b").copied() {
                Some(b) if model.kinds[&b] == InodeKind::Directory => {
                    for name in model.dirs[&b].keys().cloned().collect::<Vec<_>>() {
                        if model.kinds[&model.dirs[&b][&name]] == InodeKind::Directory {
                            *model = before;
                            return;
                        }
                        rows = rows.unbind(b, &name);
                        model.unbind(b, &name);
                    }
                    model.unbind(a, "b");
                    let serial = history.allocate();
                    model.kinds.insert(serial, InodeKind::Symlink);
                    model.dirs.get_mut(&a).unwrap().insert("b".into(), serial);
                    rows = rows
                        .unbind(a, "b")
                        .fresh(serial, InodeKind::Symlink)
                        .bind(a, "b", serial);
                }
                other => {
                    if other.is_some() {
                        model.unbind(a, "b");
                        rows = rows.unbind(a, "b");
                    }
                    let (b, f) = (history.allocate(), history.allocate());
                    model.kinds.insert(b, InodeKind::Directory);
                    model.kinds.insert(f, InodeKind::RegularFile);
                    model.dirs.insert(b, BTreeMap::from([("f".into(), f)]));
                    model.dirs.get_mut(&a).unwrap().insert("b".into(), b);
                    rows = rows
                        .mkdir(a, "b", b)
                        .fresh(f, InodeKind::RegularFile)
                        .bind(b, "f", f);
                }
            }
        }
        _ => {
            let Some((serial, InodeKind::RegularFile)) = random.pick(&leaves) else {
                return;
            };
            let target = random.pick(&dirs).unwrap();
            let name = POOL[random.below(POOL.len())];
            if model.dirs[&target].contains_key(name) {
                return;
            }
            model
                .dirs
                .get_mut(&target)
                .unwrap()
                .insert(name.into(), serial);
            rows = rows.bind(target, name, serial);
        }
    }
    if !history.try_push(&rows) {
        *model = before;
    }
}

#[test]
fn random_histories_answer_as_each_version_walked_alone() {
    const PATHS: [&str; 16] = [
        "/",
        "/a",
        "/a/b",
        "/a/b/f",
        "/a/b/g",
        "/a/b/new",
        "/a/l",
        "/a/l/f",
        "/a/b/f/x",
        "/a/h",
        "/c/h2",
        "/c/x",
        "/d/n5",
        "/d/n39/x",
        "/missing/x",
        "/a/b/b/f",
    ];
    for seed in 1..=6_u64 {
        let mut shape = Shape::new();
        let a = shape.dir(ROOT, "a");
        let b = shape.dir(a, "b");
        let f = shape.file(b, "f");
        let g = shape.file(b, "g");
        let l = shape.symlink(a, "l");
        let h = shape.file(a, "h");
        let c = shape.dir(ROOT, "c");
        shape.link(c, "h2", h);
        let d = shape.dir(ROOT, "d");
        let mut model = Model {
            dirs: BTreeMap::from([
                (
                    ROOT,
                    BTreeMap::from([("a".into(), a), ("c".into(), c), ("d".into(), d)]),
                ),
                (
                    a,
                    BTreeMap::from([("b".into(), b), ("l".into(), l), ("h".into(), h)]),
                ),
                (b, BTreeMap::from([("f".into(), f), ("g".into(), g)])),
                (c, BTreeMap::from([("h2".into(), h)])),
                (d, BTreeMap::new()),
            ]),
            kinds: BTreeMap::from([
                (ROOT, InodeKind::Directory),
                (a, InodeKind::Directory),
                (b, InodeKind::Directory),
                (c, InodeKind::Directory),
                (d, InodeKind::Directory),
                (f, InodeKind::RegularFile),
                (g, InodeKind::RegularFile),
                (h, InodeKind::RegularFile),
                (l, InodeKind::Symlink),
            ]),
            a,
        };
        for index in 0..40 {
            let serial = shape.file(d, &format!("n{index}"));
            model
                .dirs
                .get_mut(&d)
                .unwrap()
                .insert(format!("n{index}"), serial);
            model.kinds.insert(serial, InodeKind::RegularFile);
        }
        let mut history = History::new(shape);
        let mut random = Random(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
        for label in 0..60 {
            random_version(&mut history, &mut model, &mut random, label);
        }
        let mut memo = DirectoryMemo::new(MEMO);
        let mut total = 0;
        for path in PATHS {
            for strict in [true, false] {
                total += resolve_windows(&history, path, strict, &mut memo)
                    .iter()
                    .sum::<u64>();
                fallback_matches(&history, path, strict);
            }
        }
        println!(
            "ACROSS random seed={seed} versions={} memo={} demands={total}",
            history.roots.len(),
            memo.len()
        );
        assert!(history.roots.len() > 30, "too many refused versions");
    }
}
