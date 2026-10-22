//! The sorted merge reads the children a change reaches, and writes the tree
//! the whole-page merge wrote.
//!
//! `RECORDED` holds one digest per table kind, tree shape and batch family. Each
//! digest folds, for every update of the family in order, the root identity, the
//! identities the update emitted in emission order, the original/final values
//! it observed and its created/reused/change/untouched counts. They were
//! recorded from the merge that read every child of each branch page it edited
//! (commit ea7535a42) and must not move: a different digest is a different tree.

mod support;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::marker::PhantomData;

use layerfs_content::filesystem::directory::codec::{decode_directory_page, DirectoryPage};
use layerfs_content::filesystem::directory::read::list_after;
use layerfs_content::filesystem::directory::update::apply_bindings;
use layerfs_content::filesystem::inode::{
    apply_inode_values, decode_inode_page, encode_inode_page, rows_after, InodePage, InodeTable,
};
use layerfs_content::filesystem::path::PathName;
use layerfs_content::filesystem::{
    DirectoryRoot, FilesystemObjects, SortedWork, MAXIMUM_SCRATCH_BYTES,
};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::{AuthenticatedObjects, ContentError, ContentResult, ObjectId, ObjectRole};
use support::filesystem::{synthetic, TreeStore};

/// A reader that records every demand it serves.
struct Counted<'a> {
    store: &'a TreeStore,
    demands: RefCell<Vec<Vec<ObjectId>>>,
}

impl AuthenticatedObjects for Counted<'_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        self.demands.borrow_mut().push(ids.to_vec());
        self.store.read_canonical_batch(ids)
    }
}

/// One table kind under the shared engine.
trait Kind {
    type Key: Ord + Clone + std::fmt::Debug;
    type Value: PartialEq + std::fmt::Debug;
    const NAME: &'static str;
    const LEAF: ObjectRole;
    /// A key that grows with `index`.
    fn key(index: u64) -> Self::Key;
    fn value(payload: u64) -> Self::Value;
    fn apply(
        objects: &mut FilesystemObjects<'_>,
        base: Option<ObjectId>,
        changes: Vec<(u64, Option<u64>)>,
        observed: &mut Vec<u8>,
    ) -> ContentResult<(ObjectId, SortedWork)>;
    /// Every row, read back through the public reader.
    fn rows(store: &TreeStore, root: ObjectId) -> Vec<(Self::Key, Self::Value)>;
    /// The root's level and children, when it is a branch: the index of each
    /// child's last row and its identity.
    fn children(store: &TreeStore, root: ObjectId) -> Option<(u8, Vec<(u64, ObjectId)>)>;
}

struct Inodes;
struct Names;

impl Kind for Inodes {
    type Key = u64;
    type Value = InodeValue;
    const NAME: &'static str = "inode";
    const LEAF: ObjectRole = ObjectRole::InodeLeaf;
    fn key(index: u64) -> u64 {
        index + 1
    }
    fn value(payload: u64) -> InodeValue {
        InodeValue {
            kind: InodeKind::RegularFile,
            namespace_ref_count: 1 + payload % 3,
            content_root: synthetic(&format!("localized/content/{payload}")),
            metadata_root: layerfs_content::object::inode_leaf::InodeMetadata::Root(synthetic(
                &format!("localized/metadata/{}", payload % 5),
            )),
        }
    }
    fn apply(
        objects: &mut FilesystemObjects<'_>,
        base: Option<ObjectId>,
        changes: Vec<(u64, Option<u64>)>,
        _: &mut Vec<u8>,
    ) -> ContentResult<(ObjectId, SortedWork)> {
        apply_inode_values(
            objects,
            base,
            changes
                .into_iter()
                .map(|(index, payload)| Ok((Self::key(index), payload.map(Self::value)))),
            MAXIMUM_SCRATCH_BYTES,
        )
    }
    fn rows(store: &TreeStore, root: ObjectId) -> Vec<(u64, InodeValue)> {
        let table = InodeTable {
            root,
            root_serial: 1,
        };
        let mut rows = Vec::new();
        loop {
            let after = rows.last().map(|(serial, _)| *serial);
            let window =
                rows_after(store, table, after, &mut Default::default()).expect("inode rows");
            if window.is_empty() {
                return rows;
            }
            rows.extend(window);
        }
    }
    fn children(store: &TreeStore, root: ObjectId) -> Option<(u8, Vec<(u64, ObjectId)>)> {
        match decode_inode_page(store.canonical(root)?).expect("inode page") {
            InodePage::Leaf { .. } => None,
            InodePage::Branch {
                level, children, ..
            } => Some((
                level,
                children
                    .into_iter()
                    .map(|(serial, id)| (serial - 1, id))
                    .collect(),
            )),
        }
    }
}

impl Kind for Names {
    type Key = PathName;
    type Value = u64;
    const NAME: &'static str = "directory";
    const LEAF: ObjectRole = ObjectRole::DirectoryLeaf;
    fn key(index: u64) -> PathName {
        // Rows of very different widths, so that pages fill by bytes.
        let tail = "n".repeat((index.wrapping_mul(0x9E37_79B9) >> 7) as usize % 90);
        PathName::new(&format!("{index:08}{tail}")).expect("name")
    }
    fn value(payload: u64) -> u64 {
        payload + 2
    }
    fn apply(
        objects: &mut FilesystemObjects<'_>,
        base: Option<ObjectId>,
        changes: Vec<(u64, Option<u64>)>,
        observed: &mut Vec<u8>,
    ) -> ContentResult<(ObjectId, SortedWork)> {
        let mut observe = |before: Option<u64>, after: Option<u64>| {
            for side in [before, after] {
                observed.push(u8::from(side.is_some()));
                observed.extend_from_slice(&side.unwrap_or(0).to_be_bytes());
            }
            Ok(())
        };
        let (root, work) = apply_bindings(
            objects,
            base.map(DirectoryRoot),
            changes
                .into_iter()
                .map(|(index, payload)| Ok((Self::key(index), payload.map(Self::value)))),
            MAXIMUM_SCRATCH_BYTES,
            &mut observe,
        )?;
        Ok((root.0, work))
    }
    fn rows(store: &TreeStore, root: ObjectId) -> Vec<(PathName, u64)> {
        let mut rows: Vec<(PathName, u64)> = Vec::new();
        loop {
            let after = rows.last().map(|(name, _)| name.clone());
            let page = list_after(
                store,
                DirectoryRoot(root),
                after.as_ref(),
                4096,
                1 << 20,
                &mut Default::default(),
            )
            .expect("listing");
            rows.extend(page.entries);
            if page.continuation.is_none() {
                return rows;
            }
        }
    }
    fn children(store: &TreeStore, root: ObjectId) -> Option<(u8, Vec<(u64, ObjectId)>)> {
        match decode_directory_page(store.canonical(root)?).expect("directory page") {
            DirectoryPage::Leaf { .. } => None,
            DirectoryPage::Branch {
                level, children, ..
            } => Some((
                level,
                children
                    .into_iter()
                    .map(|(name, id)| (name.as_str()[..8].parse().expect("index"), id))
                    .collect(),
            )),
        }
    }
}

/// What one update demanded.
struct Applied {
    work: SortedWork,
    demands: Vec<Vec<ObjectId>>,
    leaves: Vec<ObjectId>,
}

/// One evolving table, its model and the digest of everything it produced.
struct Tree<K: Kind> {
    store: TreeStore,
    root: ObjectId,
    model: BTreeMap<u64, u64>,
    digest: ObjectId,
    kind: PhantomData<K>,
}

impl<K: Kind> Tree<K> {
    /// A table of `rows` rows at indices `16, 32, ...`, built in one update.
    fn build(rows: u64) -> Self {
        let mut tree = Self {
            store: TreeStore::new(),
            root: ObjectId::for_bytes(b"no root"),
            model: BTreeMap::new(),
            digest: ObjectId::for_bytes(K::NAME.as_bytes()),
            kind: PhantomData,
        };
        let batch = (1..=rows)
            .map(|row| (row * 16, Some(row)))
            .collect::<BTreeMap<_, _>>();
        tree.update(None, batch).expect("build");
        tree
    }

    fn fold(&mut self, bytes: &[u8]) {
        let mut joined = self.digest.as_bytes().to_vec();
        joined.extend_from_slice(bytes);
        self.digest = ObjectId::for_bytes(&joined);
    }

    fn apply(&mut self, batch: BTreeMap<u64, Option<u64>>) -> Applied {
        self.update(Some(self.root), batch).expect("update")
    }

    fn update(
        &mut self,
        base: Option<ObjectId>,
        batch: BTreeMap<u64, Option<u64>>,
    ) -> ContentResult<Applied> {
        let mut emitted = TreeStore::new();
        let mut observed = Vec::new();
        let reader = Counted {
            store: &self.store,
            demands: RefCell::new(Vec::new()),
        };
        let (root, work) = {
            let mut objects = FilesystemObjects::new(&reader, &mut emitted);
            K::apply(
                &mut objects,
                base,
                batch
                    .iter()
                    .map(|(index, value)| (*index, *value))
                    .collect(),
                &mut observed,
            )?
        };
        let demands = reader.demands.into_inner();
        let leaves = demands
            .iter()
            .flatten()
            .copied()
            .filter(|id| self.store.role(*id) == Some(K::LEAF))
            .collect();
        self.store.absorb(&emitted);
        self.root = root;
        for (index, value) in batch {
            match value {
                Some(value) => self.model.insert(index, value),
                None => self.model.remove(&index),
            };
        }
        self.fold(root.as_bytes());
        for (id, _) in emitted.order() {
            self.fold(id.as_bytes());
        }
        self.fold(&observed);
        for count in [
            work.pages_created,
            work.pages_reused,
            work.change_keys,
            work.untouched_subtrees,
        ] {
            self.fold(&count.to_be_bytes());
        }
        Ok(Applied {
            work,
            demands,
            leaves,
        })
    }

    /// The stored rows are exactly the model's.
    fn check(&self) {
        let stored = K::rows(&self.store, self.root);
        let model = self
            .model
            .iter()
            .map(|(index, payload)| (K::key(*index), K::value(*payload)))
            .collect::<Vec<_>>();
        assert_eq!(stored.len(), model.len(), "{} row count", K::NAME);
        assert!(stored == model, "{} rows differ from the model", K::NAME);
    }

    fn indices(&self) -> Vec<u64> {
        self.model.keys().copied().collect()
    }

    fn depth(&self) -> u8 {
        K::children(&self.store, self.root).map_or(0, |(level, _)| level)
    }
}

/// A fixed xorshift stream: the batches are the same on every run.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
    fn payload(&mut self) -> u64 {
        self.next() % 1_000_000
    }
}

/// A few rows changed at unrelated places.
fn scattered<K: Kind>(tree: &mut Tree<K>, stream: &mut Stream) {
    for round in 0..36 {
        let indices = tree.indices();
        let mut batch = BTreeMap::new();
        for _ in 0..3 + stream.below(4) {
            batch.insert(indices[stream.below(indices.len())], Some(stream.payload()));
        }
        if round % 3 == 1 {
            let top = *indices.last().expect("rows") + 64;
            for _ in 0..2 {
                batch.insert(stream.next() % top, Some(stream.payload()));
            }
        }
        if round % 4 == 2 {
            batch.insert(indices[stream.below(indices.len())], None);
        }
        if round % 9 == 8 {
            // The last and the first child alone.
            batch.clear();
            batch.insert(*indices.last().expect("rows"), Some(stream.payload()));
            batch.insert(indices[0], Some(stream.payload()));
        }
        tree.apply(batch);
        if round % 6 == 5 {
            tree.check();
        }
    }
    tree.check();
}

/// Changes that reach every leaf, or every row.
fn dense<K: Kind>(tree: &mut Tree<K>, stream: &mut Stream) {
    for round in 0..9 {
        let indices = tree.indices();
        let mut batch = BTreeMap::new();
        match round {
            0 | 8 => {
                for index in &indices {
                    batch.insert(*index, Some(stream.payload()));
                }
            }
            1 => {
                for index in indices.iter().step_by(7) {
                    batch.insert(*index, Some(stream.payload()));
                }
            }
            2 => {
                for index in &indices[indices.len() / 4..indices.len() * 3 / 4] {
                    batch.insert(*index, Some(stream.payload()));
                }
            }
            3 => {
                for index in indices.iter().step_by(50) {
                    batch.insert(*index, Some(stream.payload()));
                }
            }
            4 => {
                for index in indices.iter().step_by(30) {
                    batch.insert(*index + 1, Some(stream.payload()));
                }
            }
            5 => {
                for index in indices.iter().step_by(9) {
                    batch.insert(*index, None);
                }
            }
            6 => {
                for index in indices.iter().step_by(2) {
                    batch.insert(*index, Some(stream.payload()));
                    batch.insert(*index + 2, Some(stream.payload()));
                }
            }
            _ => {
                for index in indices.iter().step_by(3) {
                    batch.insert(*index, None);
                    batch.insert(*index + 5, Some(stream.payload()));
                }
            }
        }
        tree.apply(batch);
        tree.check();
    }
}

/// Many new rows: spread out, in one burst and past the end.
fn insert_heavy<K: Kind>(tree: &mut Tree<K>, stream: &mut Stream) {
    for round in 0..7 {
        let indices = tree.indices();
        let top = *indices.last().expect("rows");
        let mut batch = BTreeMap::new();
        for _ in 0..400 {
            batch.insert(stream.next() % top, Some(stream.payload()));
        }
        let start = indices[stream.below(indices.len())];
        for step in 0..250 {
            batch.insert(start + step, Some(stream.payload()));
        }
        for step in 0..120 {
            batch.insert(top + 1 + step * 3, Some(stream.payload()));
        }
        if round % 2 == 1 {
            // Only new rows inside one child, and only past the end.
            tree.apply(batch);
            batch = BTreeMap::new();
            let top = *tree.indices().last().expect("rows");
            for step in 0..300 {
                batch.insert(top + 7 + step, Some(stream.payload()));
            }
        }
        tree.apply(batch);
        tree.check();
    }
}

/// Rows removed in runs, in stripes and at both ends, down to one row and back.
fn delete_heavy<K: Kind>(tree: &mut Tree<K>, stream: &mut Stream) {
    let mut round = 0;
    while tree.model.len() > 60 {
        let indices = tree.indices();
        let mut batch = BTreeMap::new();
        let at = stream.below(indices.len());
        match round % 6 {
            0 => {
                for index in &indices {
                    if stream.below(100) < 15 {
                        batch.insert(*index, None);
                    }
                }
            }
            1 => {
                for index in indices.iter().skip(at).take(400) {
                    batch.insert(*index, None);
                }
            }
            2 => {
                for index in indices.iter().skip(at).take(1000).step_by(2) {
                    batch.insert(*index, None);
                }
            }
            3 => {
                for index in indices.iter().take(150) {
                    batch.insert(*index, None);
                }
            }
            4 => {
                for index in indices.iter().rev().take(150) {
                    batch.insert(*index, None);
                }
            }
            _ => {
                // One row out of each of a few scattered leaves.
                for _ in 0..5 {
                    batch.insert(indices[stream.below(indices.len())], None);
                }
            }
        }
        for _ in 0..2 {
            batch
                .entry(indices[stream.below(indices.len())])
                .or_insert_with(|| Some(stream.payload()));
        }
        if batch.values().filter(|value| value.is_none()).count() >= tree.model.len() {
            batch.remove(&indices[indices.len() / 2]);
        }
        tree.apply(batch);
        if round % 4 == 3 {
            tree.check();
        }
        round += 1;
    }
    tree.check();
    let indices = tree.indices();
    let keep = indices[indices.len() / 2];
    tree.apply(
        indices
            .iter()
            .filter(|index| **index != keep)
            .map(|index| (*index, None))
            .collect(),
    );
    tree.check();
    assert_eq!(tree.model.len(), 1);
    tree.apply((0..300).map(|step| (step * 5, Some(step))).collect());
    tree.check();
}

/// The rows the model holds beneath each child of the root, in child order.
fn beneath<K: Kind>(tree: &Tree<K>) -> Vec<Vec<u64>> {
    let (_, children) = K::children(&tree.store, tree.root).expect("branch root");
    let mut rows = tree.indices().into_iter().peekable();
    children
        .iter()
        .map(|(last, _)| {
            let mut child = Vec::new();
            while let Some(index) = rows.next_if(|index| index <= last) {
                child.push(index);
            }
            child
        })
        .collect()
}

/// Whole children of the root removed around children no change reaches, and
/// children of the root left too small to stand alone. Every case starts from
/// the built tree again; the cases share one digest.
fn edges<K: Kind>(tree: &mut Tree<K>, rows: u64) {
    let children = beneath(tree).len();
    let middle = children / 2;
    // Which children keep every row, and how many rows each other child keeps.
    let cases: [(&[usize], usize); 9] = [
        (&[0], 0),
        (&[children - 1], 0),
        (&[middle], 0),
        (&[middle, middle + 1], 0),
        (&[0, children - 1], 0),
        (&[middle], 3),
        (&[0, middle, children - 1], 1),
        (&[], 2),
        (&[1], 40),
    ];
    for (untouched, kept) in cases {
        let mut case = Tree::<K>::build(rows);
        let mut batch = BTreeMap::new();
        for (child, indices) in beneath(&case).into_iter().enumerate() {
            if untouched.contains(&child) {
                continue;
            }
            // The kept rows are the child's last ones: its key stays the same.
            for index in &indices[..indices.len() - kept.min(indices.len())] {
                batch.insert(*index, None);
            }
        }
        case.apply(batch);
        case.check();
        // One more update over what the first one left behind.
        let indices = case.indices();
        case.apply(
            indices
                .iter()
                .step_by(indices.len() / 4 + 1)
                .map(|index| (*index, Some(*index % 97)))
                .collect(),
        );
        case.check();
        tree.fold(case.digest.as_bytes());
    }
}

/// Rows of each shape: one branch page over leaves, three levels with a few
/// branch pages, and three levels with many.
const SHAPES: [(&str, u64, u8); 3] = [("page", 4_040, 1), ("deep", 9_000, 2), ("wide", 40_000, 2)];
const FAMILIES: [&str; 5] = [
    "scattered",
    "dense",
    "insert-heavy",
    "delete-heavy",
    "edges",
];

/// One digest per kind, shape and family, recorded at commit ea7535a42.
const RECORDED: &[(&str, &str)] = &[
    (
        "inode/page/scattered",
        "9c05ce89e5bc70443a22d93717bb869f4d0049422e23b2004e7b16c810c7a251",
    ),
    (
        "inode/page/dense",
        "1efabe971add876cd36da312fd6ab63744bb9c6acb2ccb3ad9ecc9d9fd7e03d2",
    ),
    (
        "inode/page/insert-heavy",
        "c96f5417d35c781a8d1f02da5108c1a9c8820bb0c20e696649b1f62547f068d8",
    ),
    (
        "inode/page/delete-heavy",
        "899d5384a44e738e77b16aa2d54a4eed3f145092455d324acbd3902e46e47d44",
    ),
    (
        "inode/page/edges",
        "b04ede61e2ea26444a516d07715026fe1a0d36ffdf45451fcd5f79f0293fb4ab",
    ),
    (
        "inode/deep/scattered",
        "574aa9d65ce0f1664470086dcfb170ee1f28e84daaaa11aab2853873c5276ee5",
    ),
    (
        "inode/deep/dense",
        "10aefcf7eaf01ad06356265ce9f17e74788ec0870234b5760071cbc195fbad64",
    ),
    (
        "inode/deep/insert-heavy",
        "8bc5ff2b82bdbef38037bf2439018a2bc20e1726503aa0f102b0e5646781dbea",
    ),
    (
        "inode/deep/delete-heavy",
        "32c42c57a9c2f00f2bcc570ca1f136b7d57595dd1390b9c7d6342d13963c828a",
    ),
    (
        "inode/deep/edges",
        "d27dd2d5efcb1c5565a18e36b0010f3afaa7ba956569a766531ed669be3356f5",
    ),
    (
        "inode/wide/scattered",
        "cfca1adc4efde33aa929f53014b77a45ee43ff4b56c5ef79d6d73d3eeb5e449d",
    ),
    (
        "inode/wide/delete-heavy",
        "c47647da1ff0225d9a5b5fb09454c06d43a8b44ed359b0130798f43a0cdecfba",
    ),
    (
        "inode/wide/edges",
        "a09604b9504bcdf12d2d4d4df88c00b7ae1ced963fb19064f5883ec3fffad3e7",
    ),
    (
        "directory/page/scattered",
        "b33be12c00e0aa8d3ad156e7a2302e0d4f125e16c0eed64fc88d46dd40f70a11",
    ),
    (
        "directory/page/dense",
        "59ff8ee6f503bf96431950ff7ff34a719300ddb25e6d962b2625780ed5409549",
    ),
    (
        "directory/page/insert-heavy",
        "ca07fd020abbc9c7380fb06c63c139ed60ce0ce23aa37af4f4ba547c741848d6",
    ),
    (
        "directory/page/delete-heavy",
        "c52991941e5ef1ea3f18fe84e881253cc8ae76df4147e4f75f83d7a0eeb0d92e",
    ),
    (
        "directory/page/edges",
        "7b77d9c763a17e3eee00f052e82ebfeeb8f00764a1f07fc024af083039a86b60",
    ),
    (
        "directory/deep/scattered",
        "cd3c8ec4bbc92287288de048d55e56ab83a1178aefb6ddcc0d459abe6080ec57",
    ),
    (
        "directory/deep/dense",
        "9a2ca3083a3b663431288b785ca09edac30a27a8bc8ec752749b505b02fde8c7",
    ),
    (
        "directory/deep/insert-heavy",
        "c0fffd1cf5cde93f8affa2059f360310941e78360825fdef30f42e32b41e8458",
    ),
    (
        "directory/deep/delete-heavy",
        "a3890f58aea0b0877ef7bafba81df792c0e1a895aaefdb357c8692e80bbcdb8c",
    ),
    (
        "directory/deep/edges",
        "1e39ba42eb24f35da3d678a18dba081835fa5a18fa635fe0315baa9526ffc6f2",
    ),
    (
        "directory/wide/scattered",
        "3e8a5fe98ddc978d5792f1f536c6c496a8dfe99024cca0073774d14c9212dfeb",
    ),
    (
        "directory/wide/delete-heavy",
        "81dde37ba06a10d2e7695da124518fb67d543473b11a490fe9413973a9fa4eb0",
    ),
    (
        "directory/wide/edges",
        "0d5de93340948c9df30e0f786476af59f783a6604658ac544d3e06e6d96da052",
    ),
];

fn family<K: Kind>(shape: &str, rows: u64, name: &str, seed: u64) -> (String, String) {
    let mut tree = Tree::<K>::build(rows);
    tree.check();
    let mut stream = Stream(seed);
    match name {
        "scattered" => scattered(&mut tree, &mut stream),
        "dense" => dense(&mut tree, &mut stream),
        "insert-heavy" => insert_heavy(&mut tree, &mut stream),
        "delete-heavy" => delete_heavy(&mut tree, &mut stream),
        _ => edges(&mut tree, rows),
    }
    (
        format!("{}/{shape}/{name}", K::NAME),
        tree.digest.to_string(),
    )
}

fn families<K: Kind>() {
    let mut produced = Vec::new();
    for (shape, rows, depth) in SHAPES {
        assert_eq!(Tree::<K>::build(rows).depth(), depth, "{shape}");
        for (number, name) in FAMILIES.into_iter().enumerate() {
            // The largest shape runs the families that reach its branch pages.
            if shape == "wide" && matches!(name, "dense" | "insert-heavy") {
                continue;
            }
            produced.push(family::<K>(shape, rows, name, 0x5EED_0000 + number as u64));
        }
    }
    for (label, digest) in &produced {
        println!("SORTED_LOCALIZED_DIGEST (\"{label}\", \"{digest}\"),");
    }
    assert_eq!(produced.len() * 2, RECORDED.len());
    for (label, digest) in produced {
        let recorded = RECORDED
            .iter()
            .find(|(name, _)| *name == label)
            .unwrap_or(&("", "not recorded"));
        assert_eq!(digest, recorded.1, "{label}");
    }
}

#[test]
fn inode_tables_are_the_trees_the_whole_page_merge_wrote() {
    families::<Inodes>();
}

#[test]
fn directories_are_the_trees_the_whole_page_merge_wrote() {
    families::<Names>();
}

/// The children of an 80-leaf inode page and the first index of each leaf.
fn page_of_eighty() -> (Tree<Inodes>, Vec<ObjectId>, Vec<u64>) {
    let tree = Tree::<Inodes>::build(SHAPES[0].1);
    let (level, children) = Inodes::children(&tree.store, tree.root).expect("branch root");
    assert_eq!((level, children.len()), (1, 80));
    let children = children.into_iter().map(|(_, id)| id).collect::<Vec<_>>();
    let firsts = children
        .iter()
        .map(|id| {
            match decode_inode_page(tree.store.canonical(*id).expect("leaf")).expect("leaf") {
                InodePage::Leaf { entries } => entries[0].0 - 1,
                InodePage::Branch { .. } => panic!("a leaf was expected"),
            }
        })
        .collect();
    (tree, children, firsts)
}

#[test]
fn a_three_leaf_change_in_an_eighty_leaf_page_demands_three_leaves() {
    let (mut tree, children, firsts) = page_of_eighty();
    let root = tree.root;
    // One row replaced in each of three leaves.
    let applied = tree.apply(
        [10, 40, 70]
            .into_iter()
            .map(|child| (firsts[child], Some(7)))
            .collect(),
    );
    println!(
        "SORTED_LOCALIZED_COUNT replace: leaves {} pages_read {} read_waves {} untouched {} created {}",
        applied.leaves.len(),
        applied.work.pages_read,
        applied.work.read_waves,
        applied.work.untouched_subtrees,
        applied.work.pages_created
    );
    assert_eq!(applied.demands[0], [root]);
    assert_eq!(
        applied.leaves,
        [children[10], children[40], children[70]],
        "the leaves that receive a change, in key order, and no other"
    );
    assert_eq!(
        (
            applied.work.pages_read,
            applied.work.read_waves,
            applied.work.untouched_subtrees,
            applied.work.pages_created
        ),
        (4, 4, 77, 4)
    );
    tree.check();

    // One row added to each of three leaves: no leaf overflows.
    let (mut tree, children, firsts) = page_of_eighty();
    let applied = tree.apply(
        [0, 41, 79]
            .into_iter()
            .map(|child| (firsts[child] + 1, Some(9)))
            .collect(),
    );
    assert_eq!(applied.leaves, [children[0], children[41], children[79]]);
    assert_eq!((applied.work.pages_read, applied.work.read_waves), (4, 4));
    tree.check();

    // Three rows of one leaf: one leaf.
    let (mut tree, children, firsts) = page_of_eighty();
    let applied = tree.apply(
        (0..3)
            .map(|row| (firsts[33] + 16 * row, Some(11)))
            .collect(),
    );
    assert_eq!(applied.leaves, [children[33]]);
    assert_eq!((applied.work.pages_read, applied.work.read_waves), (2, 2));
    tree.check();
}

#[test]
fn a_leaf_that_underflows_demands_one_neighbour_and_no_other() {
    // Every leaf of the built page holds the minimum, so one removal underflows.
    let (mut tree, children, firsts) = page_of_eighty();
    let applied = tree.apply(
        [10, 40, 70]
            .into_iter()
            .map(|child| (firsts[child], None))
            .collect(),
    );
    println!(
        "SORTED_LOCALIZED_COUNT remove: leaves {} pages_read {} read_waves {}",
        applied.leaves.len(),
        applied.work.pages_read,
        applied.work.read_waves
    );
    assert_eq!(
        applied.leaves,
        [
            children[10],
            children[9],
            children[40],
            children[39],
            children[70],
            children[69]
        ],
        "each underfull leaf joins the neighbour before it"
    );
    assert_eq!((applied.work.pages_read, applied.work.read_waves), (7, 7));
    tree.check();

    // The first leaf has no neighbour before it and joins the one after.
    let (mut tree, children, firsts) = page_of_eighty();
    let applied = tree.apply([(firsts[0], None)].into_iter().collect());
    assert_eq!(applied.leaves, [children[0], children[1]]);
    tree.check();
}

/// A branch page over `children`, recording `count` rows beneath it.
fn branch(tree: &mut Tree<Inodes>, children: &[ObjectId], count: u64) -> ObjectId {
    let children = children
        .iter()
        .map(|id| {
            match decode_inode_page(tree.store.canonical(*id).expect("leaf")).expect("leaf") {
                InodePage::Leaf { entries } => (entries.last().expect("rows").0, *id),
                InodePage::Branch { .. } => panic!("a leaf was expected"),
            }
        })
        .collect();
    let canonical = encode_inode_page(&InodePage::Branch {
        level: 1,
        subtree_count: count,
        children,
    })
    .expect("branch");
    tree.store.insert(ObjectRole::InodeBranch, canonical)
}

#[test]
fn a_recorded_total_its_children_cannot_hold_is_refused() {
    let (mut tree, children, firsts) = page_of_eighty();
    let rows = tree.model.len() as u64;
    let one = |child: usize| -> BTreeMap<u64, Option<u64>> {
        [(firsts[child], Some(3))].into_iter().collect()
    };
    let every =
        || -> BTreeMap<u64, Option<u64>> { firsts.iter().map(|first| (*first, Some(3))).collect() };
    let refused = |result: ContentResult<Applied>| {
        assert!(
            matches!(
                result,
                Err(ContentError::InvalidRecord("tree subtree summary"))
            ),
            "{:?}",
            result.map(|applied| applied.work)
        );
    };
    let three = || -> BTreeMap<u64, Option<u64>> {
        [4, 5, 6]
            .into_iter()
            .map(|child| (firsts[child], Some(3)))
            .collect()
    };
    // The page the build wrote records every row.
    let honest = branch(&mut tree, &children, rows);
    assert_eq!(honest, tree.root);
    // Fewer rows than the children that were read hold.
    let short = branch(&mut tree, &children, 100);
    refused(tree.update(Some(short), three()));
    // Fewer rows than one for every child that was not read.
    let thin = branch(&mut tree, &children, 100);
    refused(tree.update(Some(thin), one(5)));
    // A wrong total is refused whenever every child was read.
    for wrong in [rows - 1, rows + 1] {
        let page = branch(&mut tree, &children, wrong);
        refused(tree.update(Some(page), every()));
    }
}
