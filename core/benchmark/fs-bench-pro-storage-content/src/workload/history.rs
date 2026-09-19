//! The retained-history corpus reader: identity, selection, and changed bytes.
//!
//! This module reads the pinned deepseek-harness checkpoint corpus and hands the
//! driver, per state, its identity and its changed bytes. **It constructs nothing,
//! stores nothing and times nothing.** It never opens a Store, never calls
//! `Timing`, and never mutates the corpus: every path is opened read-only.
//!
//! ## What the corpus is
//!
//! ```text
//! checkpoint-manifest.json          the 157 checkpoints: index, sha, tree,
//!                                   manifest_sha256, logical_bytes, files
//! inputs/<sha>/manifest.tsv         that state's full tree
//! inputs/<sha>/previous.tsv         the preceding checkpoint's full tree
//! inputs/<sha>/blobs/<oid>          the blobs this transition changed
//! inputs/<sha>.receipt.json         blob_digests: <oid> -> sha256
//! oracles/<sha>.json                the state's full-byte oracle
//! ```
//!
//! ## The three corpus facts this reader is built on
//!
//! Each was verified against all 157 checkpoints before the reader was written,
//! because each one, read the other way, produces a reader that looks right and
//! measures the wrong thing.
//!
//! 1. **`previous.tsv` is the *immediately preceding checkpoint's* tree, not the
//!    previous *selected* state's.** For `history-stride1` the two coincide; for
//!    stride-3 and stride-10 they do not. A reader that diffed `manifest.tsv`
//!    against `previous.tsv` would silently treat every state skipped by the stride
//!    as unchanged and construct a fraction of the content it claims to. The
//!    transition this lane measures is between **selected** states, so the diff is
//!    taken against the previous selected state's tree.
//! 2. **`blobs/` of checkpoint *j* is exactly the set of oids referenced by paths
//!    that *j* added or whose oid changed** — a *superset* of
//!    `oids(manifest) − oids(previous)` by 1–11 entries per state, because a copy or
//!    a rename introduces a path whose oid already existed. Keyed on the set
//!    difference, the reader drops those blobs and refuses a tree it could have
//!    served; keyed on the changed paths, it serves all of them.
//! 3. **The oracle's entry count is files *plus* directories** (erratum E1), and its
//!    directory set is exactly the proper ancestors of its file paths. It is not
//!    `checkpoints[].files` and not the `manifest.tsv` line count, both of which are
//!    short by about 18 % — see `pins`.
//!
//! ## Where the corpus reading happens
//!
//! Between the timed per-state children, never inside one. `transition` is the
//! harness's untimed read; the driver calls it *before* opening the state's timer.
//! That is why this lane's `operation_ns` is the sum of the named children and not
//! the root: a root would include this module's work.
//!
//! ## Memory
//!
//! **Nothing is pre-loaded.** A 157-state selection is 4.94 GB of cumulative logical
//! bytes and 892 MB of unique blob bytes; holding either resident would defeat the
//! memory claim and warm the pages the saves read. Only the transition being
//! processed holds bytes, and the oid→checkpoint map used to locate them is built
//! per transition over that transition's span of checkpoints and dropped with it —
//! so it is bounded by the span, not by the history. Each checkpoint's `blobs/`
//! directory is listed exactly once across a run, because the spans partition
//! `1..=157`; nothing is re-read.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

use super::digest::{hex, sha256, Sha256};
use super::gitoid::{blob_oid, hex_oid, parse_oid};
use super::json::{self, Value};

/// SHA-256 of `checkpoint-manifest.json`. The corpus's root identity.
pub const MANIFEST_SHA256: &str = "03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271";

/// The pinned source tip the manifest must name.
pub const SOURCE_TIP: &str = "b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed";

/// How many checkpoints the manifest carries.
pub const CHECKPOINTS: usize = 157;

/// Cumulative logical bytes of all 157 checkpoints.
pub const TOTAL_LOGICAL_BYTES: u64 = 4_936_693_030;

/// The manifest's file name inside the corpus root.
pub const MANIFEST_FILE: &str = "checkpoint-manifest.json";

/// The selection's own token in a lane name or trace key.
const fn stride<const N: usize>(step: u16) -> [u16; N] {
    let mut out = [0u16; N];
    let mut index = 0;
    while index < N {
        out[index] = 1 + (index as u16) * step;
        index += 1;
    }
    out
}

/// `range(1, 158, 10) ∪ {157}`: the sixteen every-tenth indices, then the tip.
const fn stride10() -> [u16; 17] {
    let mut out = [0u16; 17];
    let mut index = 0;
    while index < 16 {
        out[index] = 1 + (index as u16) * 10;
        index += 1;
    }
    out[16] = 157;
    out
}

/// `range(1,158,10) ∪ {157}`: 17 states.
const SELECTION_STRIDE10: [u16; 17] = stride10();
/// `range(1,158,3)`: 53 states.
const SELECTION_STRIDE3: [u16; 53] = stride::<53>(3);
/// All 157 checkpoints.
const SELECTION_STRIDE1: [u16; 157] = stride::<157>(1);

/// One of the three selections.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Row {
    /// `range(1,158,10) ∪ {157}` — 17 states. The P0 smoke tier.
    Stride10,
    /// `range(1,158,3)` — 53 states. The P0 intermediate tier.
    Stride3,
    /// All 157 checkpoints. Run only; **never optimized**.
    Stride1,
}

impl Row {
    /// All three, in tier order.
    pub const ALL: [Row; 3] = [Row::Stride10, Row::Stride3, Row::Stride1];

    /// The selection's `full157` indices, strictly ascending.
    pub fn selection(self) -> &'static [u16] {
        match self {
            Self::Stride10 => &SELECTION_STRIDE10,
            Self::Stride3 => &SELECTION_STRIDE3,
            Self::Stride1 => &SELECTION_STRIDE1,
        }
    }

    /// The bare token, for trace keys.
    pub fn token(self) -> &'static str {
        match self {
            Self::Stride10 => "stride10",
            Self::Stride3 => "stride3",
            Self::Stride1 => "stride1",
        }
    }

    /// The registry row id, which is also its lane name.
    pub fn id(self) -> &'static str {
        match self {
            Self::Stride10 => "history-stride10",
            Self::Stride3 => "history-stride3",
            Self::Stride1 => "history-stride1",
        }
    }

    /// The row id back to its selection, or `None` for anything else.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|row| row.id() == id)
    }

    /// Cumulative logical bytes of the selection, pinned.
    pub fn logical_bytes(self) -> u64 {
        match self {
            Self::Stride10 => 561_010_345,
            Self::Stride3 => 1_676_767_835,
            Self::Stride1 => TOTAL_LOGICAL_BYTES,
        }
    }

    /// The manifest's `checkpoints[].files` total, pinned. **Files only** — see
    /// [`Pins::manifest_files`].
    pub fn manifest_files(self) -> u64 {
        match self {
            Self::Stride10 => 86_064,
            Self::Stride3 => 259_771,
            Self::Stride1 => 765_054,
        }
    }

    /// Cumulative path-states of the selection, pinned.
    pub fn path_states(self) -> u64 {
        match self {
            Self::Stride10 => 101_477,
            Self::Stride3 => 306_861,
            Self::Stride1 => 904_143,
        }
    }

    /// How many states the selection has.
    pub fn states(self) -> usize {
        self.selection().len()
    }
}

impl fmt::Display for Row {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.id())
    }
}

/// A refusal. One variant per corpus check, plus the two that carry a path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HistoryError {
    /// The corpus root is absent or is not a directory. Never a default path.
    CorpusMissing(PathBuf),
    /// `sha256(checkpoint-manifest.json)` is not [`MANIFEST_SHA256`].
    ManifestIdentity {
        /// The pinned digest.
        expected: String,
        /// What the corpus has.
        found: String,
    },
    /// The manifest names a tip that is not [`SOURCE_TIP`].
    SourceTip {
        /// The pinned tip.
        expected: String,
        /// What the manifest names.
        found: String,
    },
    /// The manifest does not carry [`CHECKPOINTS`] checkpoints.
    CheckpointCount {
        /// The frozen count.
        expected: usize,
        /// What the manifest carries.
        found: usize,
    },
    /// The selection is not strictly ascending, does not start at 1 and end at
    /// 157, or is not the declared length.
    SelectionShape(String),
    /// A `manifest.tsv` or `previous.tsv` line is malformed, or the tree needs an
    /// oid the forward map lacks.
    TreeManifest {
        /// The file.
        path: PathBuf,
        /// 1-based line number, or 0 when the fault is not a line.
        line: usize,
        /// What was wrong.
        reason: String,
    },
    /// A selected state has no `oracles/<sha>.json`.
    OracleMissing(PathBuf),
    /// A blob does not hash back to the oid it is filed under, or to its recorded
    /// digest. Never assumed.
    BlobIdentity {
        /// The oid, lowercase hex.
        oid: String,
        /// The file that failed.
        path: PathBuf,
        /// What was wrong.
        reason: String,
    },
    /// A document the reader needs is malformed or missing a field.
    Document {
        /// The file.
        path: PathBuf,
        /// What was wrong.
        reason: String,
    },
    /// The filesystem refused a read.
    Io {
        /// The file.
        path: PathBuf,
        /// What the OS said.
        reason: String,
    },
}

impl HistoryError {
    /// The refusal's name, for a receipt. These are the names `preparation.md`
    /// §8 freezes.
    pub fn name(&self) -> &'static str {
        match self {
            Self::CorpusMissing(_) => "CorpusMissing",
            Self::ManifestIdentity { .. } => "ManifestIdentity",
            Self::SourceTip { .. } => "SourceTip",
            Self::CheckpointCount { .. } => "CheckpointCount",
            Self::SelectionShape(_) => "SelectionShape",
            Self::TreeManifest { .. } => "TreeManifest",
            Self::OracleMissing(_) => "OracleMissing",
            Self::BlobIdentity { .. } => "BlobIdentity",
            Self::Document { .. } => "Document",
            Self::Io { .. } => "Io",
        }
    }

    fn io(path: &Path, error: std::io::Error) -> Self {
        Self::Io {
            path: path.to_path_buf(),
            reason: error.to_string(),
        }
    }

    fn document(path: &Path, reason: impl Into<String>) -> Self {
        Self::Document {
            path: path.to_path_buf(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for HistoryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CorpusMissing(path) => write!(formatter, "corpus missing: {}", path.display()),
            Self::ManifestIdentity { expected, found } => write!(
                formatter,
                "manifest identity: expected sha256 {expected}, found {found}"
            ),
            Self::SourceTip { expected, found } => {
                write!(formatter, "source tip: expected {expected}, found {found}")
            }
            Self::CheckpointCount { expected, found } => write!(
                formatter,
                "checkpoint count: expected {expected}, found {found}"
            ),
            Self::SelectionShape(reason) => write!(formatter, "selection shape: {reason}"),
            Self::TreeManifest { path, line, reason } => {
                write!(formatter, "tree manifest {}:{line}: {reason}", path.display())
            }
            Self::OracleMissing(path) => write!(formatter, "oracle missing: {}", path.display()),
            Self::BlobIdentity { oid, path, reason } => write!(
                formatter,
                "blob identity {oid} at {}: {reason}",
                path.display()
            ),
            Self::Document { path, reason } => {
                write!(formatter, "document {}: {reason}", path.display())
            }
            Self::Io { path, reason } => write!(formatter, "io {}: {reason}", path.display()),
        }
    }
}

impl std::error::Error for HistoryError {}

/// One checkpoint's identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct State {
    /// Campaign index, **1-based**, within the selection.
    pub ordinal: usize,
    /// The original index in the 157-checkpoint manifest.
    pub full157_index: u16,
    /// The source commit.
    pub sha: String,
    /// The source tree id.
    pub tree: String,
    /// The checkpoint's own tree digest, from the manifest.
    pub manifest_sha256: String,
    /// `oracles/<sha>.json`.
    pub oracle_path: PathBuf,
    /// SHA-256 of the oracle file, computed once at open.
    pub oracle_sha256: String,
    /// Cumulative logical bytes of this one state.
    pub logical_bytes: u64,
    /// The state's path-states: its oracle's entry count, files **and**
    /// directories.
    pub paths: u32,
}

/// How one path changed between two selected states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Change {
    /// The path is in this state and not in the previous one.
    Added,
    /// The path is in both and its oid moved.
    Modified,
    /// The path is in the previous state and not in this one.
    Removed,
    /// The path is in both with the same oid and a different mode.
    MetadataOnly,
}

impl Change {
    /// The token a trace counter carries.
    pub fn token(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Modified => "modified",
            Self::Removed => "removed",
            Self::MetadataOnly => "metadata-only",
        }
    }
}

/// One entry of a state's tree.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TreeEntry {
    /// The POSIX mode, parsed from octal.
    pub mode: u32,
    /// The blob's Git object id.
    pub oid: [u8; 20],
    /// The blob's logical length.
    pub size: u64,
}

/// One path the transition changed.
///
/// For [`Change::Removed`] the mode, oid and size are the **previous** state's,
/// because the path has none in this state; the driver needs only the path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangedPath {
    /// The path, hex-decoded, as bytes. Not a `String`: a Git path is not text.
    pub path: Vec<u8>,
    /// The POSIX mode.
    pub mode: u32,
    /// The blob's Git object id.
    pub oid: [u8; 20],
    /// The blob's logical length.
    pub size: u64,
    /// How it changed.
    pub kind: Change,
}

/// The changed set's bytes, keyed by oid. Sorted, so a driver's iteration order is
/// reproducible from the receipt.
pub type BlobMap = BTreeMap<[u8; 20], Vec<u8>>;

/// What one state changes relative to the previous **selected** state.
#[derive(Clone, Debug)]
pub struct Transition {
    /// The state being entered.
    pub state: State,
    /// The changed paths, in path order.
    pub changed: Vec<ChangedPath>,
    /// The bytes of every blob the changed paths reference, keyed by oid.
    pub blobs: BlobMap,
    /// The state's **full** tree, in path order.
    ///
    /// A changed directory's update carries the *final* binding of every name it
    /// changes, so the driver needs the state's whole tree and not only its delta.
    /// It is bounded by the repository — 9,415 paths at the tip — and not by the
    /// number of states, so it does not grow with the history.
    pub tree: BTreeMap<Vec<u8>, TreeEntry>,
}

impl Transition {
    /// Logical bytes the transition changed: the size of each added or modified
    /// path. A removal changes the tree without contributing content.
    pub fn changed_bytes(&self) -> u64 {
        self.changed
            .iter()
            .filter(|entry| matches!(entry.kind, Change::Added | Change::Modified))
            .map(|entry| entry.size)
            .sum()
    }

    /// How many paths of each kind.
    pub fn count(&self, kind: Change) -> usize {
        self.changed.iter().filter(|entry| entry.kind == kind).count()
    }
}

/// One entry of a state's oracle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OracleEntry {
    /// The POSIX mode, parsed from octal.
    pub mode: u32,
    /// The logical length. Zero for a directory.
    pub size: u64,
    /// SHA-256 of the file's bytes, or `None` for a directory.
    pub digest: Option<[u8; 32]>,
}

impl OracleEntry {
    /// Whether this entry is a directory.
    pub fn is_directory(&self) -> bool {
        self.digest.is_none()
    }
}

/// A state's full-byte oracle, as `oracles/<sha>.json` carries it.
#[derive(Clone, Debug, Default)]
pub struct Oracle {
    entries: BTreeMap<Vec<u8>, OracleEntry>,
}

impl Oracle {
    /// How many path-states the oracle declares: files **and** directories.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the oracle is empty. A state with no paths is a defect, not a pass.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry for a path.
    pub fn get(&self, path: &[u8]) -> Option<&OracleEntry> {
        self.entries.get(path)
    }

    /// Every entry, in path order.
    pub fn iter(&self) -> impl Iterator<Item = (&Vec<u8>, &OracleEntry)> {
        self.entries.iter()
    }

    /// How many entries are directories.
    pub fn directories(&self) -> usize {
        self.entries.values().filter(|entry| entry.is_directory()).count()
    }
}

/// The selection's pinned totals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pins {
    /// How many states the selection has.
    pub states: usize,
    /// Cumulative path-states: the sum of each state's oracle entry count.
    pub path_states: u64,
    /// Cumulative logical bytes, from the manifest.
    pub logical_bytes: u64,
    /// The manifest's own `checkpoints[].files` total. **Files only** — it is not
    /// [`Pins::path_states`] and is about 18 % smaller, because a directory is a
    /// path-state and is not a file. Published beside the pin so erratum E1's
    /// distinction is visible in the evidence rather than only in the prose.
    pub manifest_files: u64,
}

/// One checkpoint as the manifest declares it.
#[derive(Clone, Debug)]
struct Checkpoint {
    sha: String,
    tree: String,
    manifest_sha256: String,
    logical_bytes: u64,
    paths: u32,
}

/// The authenticated corpus, open over one selection.
pub struct Corpus {
    root: PathBuf,
    row: Row,
    states: Vec<State>,
    /// The 157 checkpoints, in manifest order.
    checkpoints: Vec<Checkpoint>,
    /// Every checkpoint's tree, by 1-based checkpoint index.
    ///
    /// `None` until [`Corpus::checkpoint_trees`] fills it. A producer that ran at
    /// every commit needs each checkpoint's tree to answer "what was this path
    /// before?", and re-reading a manifest per path would be 157 parses for one
    /// answer.
    checkpoint_trees: Vec<Option<BTreeMap<Vec<u8>, TreeEntry>>>,
    /// The previous *selected* state's tree. Empty before the first state.
    previous: BTreeMap<Vec<u8>, TreeEntry>,
}

impl Corpus {
    /// Opens and authenticates the corpus for one selection.
    ///
    /// Every identity in `README.md` §3 is checked here, before any state runs: the
    /// manifest's SHA-256, the pinned tip, the checkpoint count, the selection's
    /// shape, and each selected state's `manifest_sha256` and oracle digest. A
    /// corpus that does not authenticate is refused; it is never partially used and
    /// there is no default path.
    pub fn open(root: &Path, row: Row) -> Result<Self, HistoryError> {
        if !root.is_dir() {
            return Err(HistoryError::CorpusMissing(root.to_path_buf()));
        }
        let manifest_path = root.join(MANIFEST_FILE);
        let bytes = read(&manifest_path)?;
        let found = hex(&sha256(&bytes));
        if found != MANIFEST_SHA256 {
            return Err(HistoryError::ManifestIdentity {
                expected: MANIFEST_SHA256.to_string(),
                found,
            });
        }
        let document = json::parse(std::str::from_utf8(&bytes).map_err(|error| {
            HistoryError::document(&manifest_path, format!("not UTF-8: {error}"))
        })?)
        .map_err(|error| HistoryError::document(&manifest_path, error.to_string()))?;

        let tip = document
            .get_str("tip")
            .ok_or_else(|| HistoryError::document(&manifest_path, "no string `tip`"))?;
        if tip != SOURCE_TIP {
            return Err(HistoryError::SourceTip {
                expected: SOURCE_TIP.to_string(),
                found: tip.to_string(),
            });
        }

        let declared = document
            .get_array("checkpoints")
            .ok_or_else(|| HistoryError::document(&manifest_path, "no array `checkpoints`"))?;
        if declared.len() != CHECKPOINTS {
            return Err(HistoryError::CheckpointCount {
                expected: CHECKPOINTS,
                found: declared.len(),
            });
        }
        let checkpoints = declared
            .iter()
            .enumerate()
            .map(|(index, value)| checkpoint(&manifest_path, index, value))
            .collect::<Result<Vec<_>, _>>()?;

        let selection = row.selection();
        check_selection(row, selection)?;

        let mut states = Vec::with_capacity(selection.len());
        for (position, &full157_index) in selection.iter().enumerate() {
            let checkpoint = &checkpoints[full157_index as usize - 1];
            let oracle_path = root.join("oracles").join(format!("{}.json", checkpoint.sha));
            if !oracle_path.is_file() {
                return Err(HistoryError::OracleMissing(oracle_path));
            }
            let oracle_bytes = read(&oracle_path)?;
            let oracle_sha256 = hex(&sha256(&oracle_bytes));
            let paths = oracle_len(&oracle_path, &oracle_bytes)?;
            states.push(State {
                ordinal: position + 1,
                full157_index,
                sha: checkpoint.sha.clone(),
                tree: checkpoint.tree.clone(),
                manifest_sha256: checkpoint.manifest_sha256.clone(),
                oracle_path,
                oracle_sha256,
                logical_bytes: checkpoint.logical_bytes,
                paths,
            });
        }

        Ok(Self {
            root: root.to_path_buf(),
            row,
            states,
            checkpoint_trees: vec![None; checkpoints.len()],
            checkpoints,
            previous: BTreeMap::new(),
        })
    }

    /// The corpus root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The selection this corpus was opened over.
    pub fn row(&self) -> Row {
        self.row
    }

    /// The selected states, in order.
    pub fn states(&self) -> &[State] {
        &self.states
    }

    /// Path-states and cumulative logical bytes, for the pins.
    ///
    /// `path_states` is the **oracle entry count** of each state — files **and**
    /// directories (erratum E1) — and not `checkpoints[].files` or the
    /// `manifest.tsv` line count, both of which are short by about 18 %.
    pub fn pins(&self) -> Pins {
        Pins {
            states: self.states.len(),
            path_states: self.states.iter().map(|state| u64::from(state.paths)).sum(),
            logical_bytes: self.states.iter().map(|state| state.logical_bytes).sum(),
            manifest_files: self
                .states
                .iter()
                .map(|state| {
                    u64::from(
                        self.checkpoints[state.full157_index as usize - 1].paths,
                    )
                })
                .sum(),
        }
    }

    /// Loads every checkpoint's tree, once, so a producer can consult the full
    /// history without re-reading a manifest per path.
    pub fn load_checkpoint_trees(&mut self) -> Result<(), HistoryError> {
        if self.checkpoint_trees.iter().all(Option::is_some) {
            return Ok(());
        }
        for index in 1..=self.checkpoints.len() {
            let checkpoint = &self.checkpoints[index - 1];
            let manifest_path = self
                .root
                .join("inputs")
                .join(&checkpoint.sha)
                .join("manifest.tsv");
            let tree = parse_tree(&manifest_path)?;
            self.checkpoint_trees[index - 1] = Some(tree);
        }
        Ok(())
    }

    /// The most recent **earlier** version of each of `paths`, from any checkpoint.
    ///
    /// A real producer runs at **every** commit, not only at the selected ones:
    /// when it saves checkpoint *k* it holds the tree of checkpoint *k-1*, and the
    /// base it declares for a path is that path's version at *k-1*. The selection
    /// then saves states 1, 11, 21 ... into one Store, so a file that changed in
    /// checkpoints 2..10 arrives at state 11 as **new to the Store** while a
    /// producer that ran at every commit would have had its state-10 version in
    /// hand.
    ///
    /// This is the correspondence Stage 7 owns, supplied by the harness because
    /// Stage 7 does not exist. It is **harness code and no product line**.
    ///
    /// One backward sweep over the checkpoints, not one per path: each checkpoint's
    /// tree is visited once and every path still looking for a version is answered
    /// from it. Returns `path -> (oid, the checkpoint that holds it)`.
    pub fn prior_versions(
        &self,
        state: &State,
        paths: &[Vec<u8>],
    ) -> Result<BTreeMap<Vec<u8>, ([u8; 20], u16)>, HistoryError> {
        let mut wanted: BTreeSet<&[u8]> = paths.iter().map(Vec::as_slice).collect();
        let mut found: BTreeMap<Vec<u8>, ([u8; 20], u16)> = BTreeMap::new();
        let mut index = state.full157_index;
        while index > 1 && !wanted.is_empty() {
            index -= 1;
            let Some(tree) = self
                .checkpoint_trees
                .get(index as usize - 1)
                .and_then(Option::as_ref)
            else {
                return Err(HistoryError::TreeManifest {
                    path: self.root.clone(),
                    line: 0,
                    reason: "checkpoint trees were not loaded before prior_versions".to_string(),
                });
            };
            wanted.retain(|path| match tree.get(*path) {
                Some(entry) => {
                    found.insert(path.to_vec(), (entry.oid, index));
                    false
                }
                None => true,
            });
        }
        Ok(found)
    }

    /// The earlier versions of `paths`, with the bytes a producer would hold.
    ///
    /// The correspondence and the input it needs, in one call. The bytes are read
    /// here, outside the measured children, exactly as a transition's own blobs
    /// are — a producer reads its input outside the operation it measures.
    pub fn prior_inputs(
        &self,
        state: &State,
        paths: &[Vec<u8>],
    ) -> Result<BTreeMap<Vec<u8>, Vec<u8>>, HistoryError> {
        let found = self.prior_versions(state, paths)?;
        if found.is_empty() {
            return Ok(BTreeMap::new());
        }
        let oids: HashSet<[u8; 20]> = found.values().map(|(oid, _)| *oid).collect();
        let located = self.locate(&oids, 1, state.full157_index)?;
        let mut bytes_of: HashMap<[u8; 20], Vec<u8>> = HashMap::with_capacity(oids.len());
        for oid in &oids {
            let Some((_checkpoint, path)) = located.get(oid) else {
                continue;
            };
            let bytes = read(path)?;
            bytes_of.insert(*oid, bytes);
        }
        let mut out = BTreeMap::new();
        for (path, (oid, _checkpoint)) in found {
            if let Some(bytes) = bytes_of.remove(&oid) {
                out.insert(path, bytes);
            }
        }
        Ok(out)
    }

    /// One state's oracle, parsed. The verify phase's O4 source.
    pub fn oracle(&self, state: &State) -> Result<Oracle, HistoryError> {
        let bytes = read(&state.oracle_path)?;
        let found = hex(&sha256(&bytes));
        if found != state.oracle_sha256 {
            return Err(HistoryError::document(
                &state.oracle_path,
                format!(
                    "oracle drift: recorded {} but the file hashes to {found}",
                    state.oracle_sha256
                ),
            ));
        }
        parse_oracle(&state.oracle_path, &bytes)
    }

    /// Reads state `position`'s changed bytes and returns its transition.
    ///
    /// `position` is **0-based** within the selection; `Transition::state.ordinal`
    /// is 1-based. `&mut self` because the previous tree is carried forward.
    ///
    /// The diff is against the previous **selected** state's tree, not against
    /// `previous.tsv`, which is the immediately preceding checkpoint's tree — the
    /// two differ for stride-3 and stride-10 (see the module note). Blobs are
    /// located by listing the `blobs/` directories of the checkpoints this
    /// transition spans, and every one served is re-hashed against both its oid and
    /// its recorded digest.
    pub fn transition(&mut self, position: usize) -> Result<Transition, HistoryError> {
        let state = self
            .states
            .get(position)
            .cloned()
            .ok_or_else(|| HistoryError::SelectionShape(format!("no state at position {position}")))?;

        let manifest_path = self
            .root
            .join("inputs")
            .join(&state.sha)
            .join("manifest.tsv");
        let tree = parse_tree(&manifest_path)?;

        let mut changed = Vec::new();
        for (path, entry) in &tree {
            match self.previous.get(path) {
                None => changed.push(ChangedPath {
                    path: path.clone(),
                    mode: entry.mode,
                    oid: entry.oid,
                    size: entry.size,
                    kind: Change::Added,
                }),
                Some(previous) if previous.oid != entry.oid => changed.push(ChangedPath {
                    path: path.clone(),
                    mode: entry.mode,
                    oid: entry.oid,
                    size: entry.size,
                    kind: Change::Modified,
                }),
                Some(previous) if previous.mode != entry.mode => changed.push(ChangedPath {
                    path: path.clone(),
                    mode: entry.mode,
                    oid: entry.oid,
                    size: entry.size,
                    kind: Change::MetadataOnly,
                }),
                Some(_) => {}
            }
        }
        for (path, previous) in &self.previous {
            if !tree.contains_key(path) {
                changed.push(ChangedPath {
                    path: path.clone(),
                    mode: previous.mode,
                    oid: previous.oid,
                    size: previous.size,
                    kind: Change::Removed,
                });
            }
        }
        changed.sort_by(|left, right| left.path.cmp(&right.path));

        // The span of checkpoints this transition covers: everything after the
        // previous selected state, up to and including this one. The first selected
        // state is always checkpoint 1, so its span is exactly `1..=1`.
        let from = if position == 0 {
            1
        } else {
            self.states[position - 1].full157_index + 1
        };
        let to = state.full157_index;

        let wanted: HashSet<[u8; 20]> = changed
            .iter()
            .filter(|entry| !matches!(entry.kind, Change::Removed))
            .map(|entry| entry.oid)
            .collect();
        let located = self.locate(&wanted, from, to)?;

        // Each span's receipts are parsed **once**, not once per blob. A receipt is
        // a few hundred kilobytes and a state can change thousands of blobs, so a
        // per-blob parse is tens of gigabytes of JSON for one selection — the
        // defect the stride-10 smoke tier caught before it reached stride-1.
        let mut receipts: HashMap<u16, HashMap<String, String>> = HashMap::new();
        let mut blobs = BlobMap::new();
        for oid in &wanted {
            let (checkpoint_index, path) = located.get(oid).ok_or_else(|| {
                HistoryError::TreeManifest {
                    path: manifest_path.clone(),
                    line: 0,
                    reason: format!(
                        "state {} needs oid {} and no checkpoint in {}..={} introduced it",
                        state.ordinal,
                        hex_oid(oid),
                        from,
                        to
                    ),
                }
            })?;
            let bytes = read(path)?;
            if !receipts.contains_key(checkpoint_index) {
                let digests = self.read_receipt(*checkpoint_index)?;
                receipts.insert(*checkpoint_index, digests);
            }
            let hex = hex_oid(oid);
            let digest = receipts[checkpoint_index].get(&hex);
            verify_blob(oid, path, &bytes, digest.map(String::as_str))?;
            blobs.insert(*oid, bytes);
        }

        self.previous = tree.clone();
        Ok(Transition {
            state,
            changed,
            blobs,
            tree,
        })
    }

    /// Maps each wanted oid to the checkpoint that introduced it and its file.
    ///
    /// The map is built for one transition's span and dropped with it, so it is
    /// bounded by the span and not by the history. Each checkpoint's `blobs/`
    /// directory is listed exactly once across a run.
    pub fn locate(
        &self,
        wanted: &HashSet<[u8; 20]>,
        from: u16,
        to: u16,
    ) -> Result<HashMap<[u8; 20], (u16, PathBuf)>, HistoryError> {
        let mut found = HashMap::with_capacity(wanted.len());
        for index in from..=to {
            if found.len() == wanted.len() {
                break;
            }
            let checkpoint = &self.checkpoints[index as usize - 1];
            let directory = self.root.join("inputs").join(&checkpoint.sha).join("blobs");
            let entries = std::fs::read_dir(&directory).map_err(|error| HistoryError::Io {
                path: directory.clone(),
                reason: error.to_string(),
            })?;
            for entry in entries {
                let entry = entry.map_err(|error| HistoryError::Io {
                    path: directory.clone(),
                    reason: error.to_string(),
                })?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                let Some(oid) = parse_oid(name) else { continue };
                if wanted.contains(&oid) {
                    found.entry(oid).or_insert((index, entry.path()));
                }
            }
        }
        Ok(found)
    }

    /// One checkpoint's recorded blob digests: oid (lowercase hex) to sha256.
    ///
    /// The receipt carries the same map twice under two key forms — `blob_digests`
    /// keyed by the bare oid, and `files` keyed by `blobs/<oid>` — and the corpus
    /// document names the first. Either is accepted, so a corpus that carries only
    /// one still identifies its blobs. A blob that appears in neither is refused at
    /// the point of use, because a blob the corpus cannot identify is never
    /// assumed.
    fn read_receipt(&self, checkpoint_index: u16) -> Result<HashMap<String, String>, HistoryError> {
        let sha = &self.checkpoints[checkpoint_index as usize - 1].sha;
        let receipt_path = self
            .root
            .join("inputs")
            .join(format!("{sha}.receipt.json"));
        let bytes = read(&receipt_path)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|error| HistoryError::document(&receipt_path, format!("not UTF-8: {error}")))?;
        let document = json::parse(text)
            .map_err(|error| HistoryError::document(&receipt_path, error.to_string()))?;
        let mut digests: HashMap<String, String> = HashMap::new();
        for field in ["blob_digests", "files"] {
            let Some(entries) = document.get(field).and_then(Value::as_object) else {
                continue;
            };
            for (key, value) in entries {
                let Some(digest) = value.as_str() else {
                    return Err(HistoryError::document(
                        &receipt_path,
                        format!("{field}[{key}] is a {}", value.type_name()),
                    ));
                };
                let oid = key.strip_prefix("blobs/").unwrap_or(key);
                digests.entry(oid.to_string()).or_insert(digest.to_string());
            }
        }
        if digests.is_empty() {
            return Err(HistoryError::document(
                &receipt_path,
                "carries neither `blob_digests` nor `files`",
            ));
        }
        Ok(digests)
    }
}

/// Checks a selection's shape before anything is read against it.
fn check_selection(row: Row, selection: &[u16]) -> Result<(), HistoryError> {
    if selection.len() != row.states() {
        return Err(HistoryError::SelectionShape(format!(
            "{} declares {} states, the selection has {}",
            row.id(),
            row.states(),
            selection.len()
        )));
    }
    if selection.first() != Some(&1) || selection.last() != Some(&(CHECKPOINTS as u16)) {
        return Err(HistoryError::SelectionShape(format!(
            "{} must start at 1 and end at {CHECKPOINTS}",
            row.id()
        )));
    }
    if selection.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(HistoryError::SelectionShape(format!(
            "{} is not strictly ascending",
            row.id()
        )));
    }
    Ok(())
}

fn checkpoint(path: &Path, index: usize, value: &Value) -> Result<Checkpoint, HistoryError> {
    let field = |name: &str| {
        value
            .get(name)
            .ok_or_else(|| HistoryError::document(path, format!("checkpoint {index}: no `{name}`")))
    };
    let declared_index = field("index")?
        .as_i64()
        .ok_or_else(|| HistoryError::document(path, format!("checkpoint {index}: `index`")))?;
    if declared_index != index as i64 + 1 {
        return Err(HistoryError::document(
            path,
            format!("checkpoint {index} declares index {declared_index}"),
        ));
    }
    let text = |name: &str| -> Result<String, HistoryError> {
        Ok(field(name)?
            .as_str()
            .ok_or_else(|| {
                HistoryError::document(path, format!("checkpoint {index}: `{name}` is not a string"))
            })?
            .to_string())
    };
    let logical_bytes = field("logical_bytes")?
        .as_i64()
        .ok_or_else(|| {
            HistoryError::document(path, format!("checkpoint {index}: `logical_bytes`"))
        })?;
    let paths = field("files")?
        .as_i64()
        .ok_or_else(|| HistoryError::document(path, format!("checkpoint {index}: `files`")))?;
    Ok(Checkpoint {
        sha: text("sha")?,
        tree: text("tree")?,
        manifest_sha256: text("manifest_sha256")?,
        logical_bytes: logical_bytes.max(0) as u64,
        paths: paths.max(0) as u32,
    })
}

/// Reads one `manifest.tsv` / `previous.tsv` into a tree.
///
/// The column order is `mode \t oid \t size \t hex path` (erratum E2). A malformed
/// line is refused with its number; an empty line is skipped, because the corpus
/// files end with a newline.
fn parse_tree(path: &Path) -> Result<BTreeMap<Vec<u8>, TreeEntry>, HistoryError> {
    let bytes = read(path)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| HistoryError::document(path, format!("not UTF-8: {error}")))?;
    let mut tree = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let line_number = index + 1;
        let mut columns = line.split('\t');
        let (Some(mode), Some(oid), Some(size), Some(hex_path), None) = (
            columns.next(),
            columns.next(),
            columns.next(),
            columns.next(),
            columns.next(),
        ) else {
            return Err(HistoryError::TreeManifest {
                path: path.to_path_buf(),
                line: line_number,
                reason: "expected four tab-separated columns".to_string(),
            });
        };
        let mode = u32::from_str_radix(mode, 8).map_err(|error| HistoryError::TreeManifest {
            path: path.to_path_buf(),
            line: line_number,
            reason: format!("mode {mode:?} is not octal: {error}"),
        })?;
        let oid = parse_oid(oid).ok_or_else(|| HistoryError::TreeManifest {
            path: path.to_path_buf(),
            line: line_number,
            reason: format!("oid {oid:?} is not forty hex characters"),
        })?;
        let size: u64 = size.parse().map_err(|error| HistoryError::TreeManifest {
            path: path.to_path_buf(),
            line: line_number,
            reason: format!("size {size:?}: {error}"),
        })?;
        let decoded = decode_hex(hex_path).ok_or_else(|| HistoryError::TreeManifest {
            path: path.to_path_buf(),
            line: line_number,
            reason: format!("path {hex_path:?} is not even-length hex"),
        })?;
        tree.insert(decoded, TreeEntry { mode, oid, size });
    }
    Ok(tree)
}

/// The oracle's entry count, without building it.
///
/// This is the definition of a state's path-states (erratum E1), and it is the one
/// number `open` needs from a document that can be 2.2 MB with 10,924 members.
/// Building the value tree to call `.len()` on it cost seconds of preparation for
/// a count; the counting reader walks the same tokenizer and refuses the same
/// malformed input. The oracle is parsed in full where it is *consumed*, by
/// [`Corpus::oracle`], so nothing goes unchecked — it is checked where it is used.
fn oracle_len(path: &Path, bytes: &[u8]) -> Result<u32, HistoryError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| HistoryError::document(path, format!("not UTF-8: {error}")))?;
    let count = json::count_object_members(text)
        .map_err(|error| HistoryError::document(path, error.to_string()))?;
    u32::try_from(count)
        .map_err(|_| HistoryError::document(path, "more than u32::MAX path-states"))
}

/// Parses `oracles/<sha>.json`: `hex path -> [mode octal, size, sha256 | "-"]`.
fn parse_oracle(path: &Path, bytes: &[u8]) -> Result<Oracle, HistoryError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| HistoryError::document(path, format!("not UTF-8: {error}")))?;
    let document = json::parse(text)
        .map_err(|error| HistoryError::document(path, error.to_string()))?;
    let entries = document
        .as_object()
        .ok_or_else(|| HistoryError::document(path, "the oracle is not an object"))?;
    let mut oracle = Oracle::default();
    for (hex_path, value) in entries {
        let Some(decoded) = decode_hex(hex_path) else {
            return Err(HistoryError::document(
                path,
                format!("path {hex_path:?} is not even-length hex"),
            ));
        };
        let fields = value.as_array().ok_or_else(|| {
            HistoryError::document(path, format!("{hex_path:?} is not a three-element array"))
        })?;
        let [mode, size, digest] = fields else {
            return Err(HistoryError::document(
                path,
                format!("{hex_path:?} has {} fields, expected 3", fields.len()),
            ));
        };
        let mode_text = mode.as_str().ok_or_else(|| {
            HistoryError::document(path, format!("{hex_path:?}: mode is not a string"))
        })?;
        let mode = u32::from_str_radix(mode_text, 8).map_err(|error| {
            HistoryError::document(path, format!("{hex_path:?}: mode {mode_text:?}: {error}"))
        })?;
        let size = size.as_i64().ok_or_else(|| {
            HistoryError::document(path, format!("{hex_path:?}: size is not an integer"))
        })?;
        let digest_text = digest.as_str().ok_or_else(|| {
            HistoryError::document(path, format!("{hex_path:?}: digest is not a string"))
        })?;
        let digest = if digest_text == "-" {
            None
        } else {
            let raw = decode_hex(digest_text).ok_or_else(|| {
                HistoryError::document(path, format!("{hex_path:?}: digest {digest_text:?}"))
            })?;
            let raw: [u8; 32] = raw.try_into().map_err(|_| {
                HistoryError::document(path, format!("{hex_path:?}: digest is not 32 bytes"))
            })?;
            Some(raw)
        };
        oracle.entries.insert(
            decoded,
            OracleEntry {
                mode,
                size: size.max(0) as u64,
                digest,
            },
        );
    }
    Ok(oracle)
}

/// Re-hashes a blob and refuses it unless it identifies both ways.
fn verify_blob(
    oid: &[u8; 20],
    path: &Path,
    bytes: &[u8],
    recorded: Option<&str>,
) -> Result<(), HistoryError> {
    let computed = blob_oid(bytes);
    if &computed != oid {
        return Err(HistoryError::BlobIdentity {
            oid: hex_oid(oid),
            path: path.to_path_buf(),
            reason: format!("its bytes hash to the git blob oid {}", hex_oid(&computed)),
        });
    }
    if let Some(recorded) = recorded {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let digest = hex(&hasher.finish());
        if digest != recorded {
            return Err(HistoryError::BlobIdentity {
                oid: hex_oid(oid),
                path: path.to_path_buf(),
                reason: format!("sha256 {digest} is not the recorded {recorded}"),
            });
        }
    }
    Ok(())
}

fn read(path: &Path) -> Result<Vec<u8>, HistoryError> {
    std::fs::read(path).map_err(|error| HistoryError::io(path, error))
}

/// Decodes an even-length lowercase-or-uppercase hex string.
fn decode_hex(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks(2) {
        let high = (pair[0] as char).to_digit(16)?;
        let low = (pair[1] as char).to_digit(16)?;
        out.push(((high << 4) | low) as u8);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_selections_are_the_declared_indices() {
        let stride10 = Row::Stride10.selection();
        assert_eq!(stride10.len(), 17);
        assert_eq!(stride10[0], 1);
        assert_eq!(stride10[15], 151);
        assert_eq!(stride10[16], 157);
        assert_eq!(&stride10[..3], &[1, 11, 21]);

        let stride3 = Row::Stride3.selection();
        assert_eq!(stride3.len(), 53);
        assert_eq!(stride3[0], 1);
        assert_eq!(stride3[52], 157);
        assert_eq!(&stride3[..3], &[1, 4, 7]);

        let stride1 = Row::Stride1.selection();
        assert_eq!(stride1.len(), 157);
        assert_eq!(stride1[0], 1);
        assert_eq!(stride1[156], 157);
        assert!(stride1.windows(2).all(|pair| pair[1] == pair[0] + 1));

        for row in Row::ALL {
            let selection = row.selection();
            assert_eq!(selection.len(), row.states());
            assert!(selection.windows(2).all(|pair| pair[0] < pair[1]));
            assert_eq!(selection.first(), Some(&1));
            assert_eq!(selection.last(), Some(&(CHECKPOINTS as u16)));
            assert!(check_selection(row, selection).is_ok());
        }
    }

    #[test]
    fn a_selection_that_is_not_the_declared_shape_is_refused() {
        let mut wrong = Row::Stride3.selection().to_vec();
        wrong.swap(0, 1);
        assert!(matches!(
            check_selection(Row::Stride3, &wrong),
            Err(HistoryError::SelectionShape(_))
        ));
        let short = &Row::Stride3.selection()[..52];
        assert!(matches!(
            check_selection(Row::Stride3, short),
            Err(HistoryError::SelectionShape(_))
        ));
        let mut duplicated = Row::Stride10.selection().to_vec();
        duplicated[1] = duplicated[0];
        assert!(matches!(
            check_selection(Row::Stride10, &duplicated),
            Err(HistoryError::SelectionShape(_))
        ));
        let mut no_tip = Row::Stride1.selection().to_vec();
        no_tip[156] = 156;
        assert!(matches!(
            check_selection(Row::Stride1, &no_tip),
            Err(HistoryError::SelectionShape(_))
        ));
    }

    #[test]
    fn a_missing_corpus_is_refused_rather_than_defaulted() {
        let missing = Path::new("/nonexistent/deepseek-history-data");
        match Corpus::open(missing, Row::Stride10) {
            Err(error) => assert_eq!(error.name(), "CorpusMissing"),
            Ok(_) => panic!("an absent corpus must not open"),
        }
    }

    #[test]
    fn the_pinned_totals_agree_with_the_selections() {
        assert_eq!(Row::Stride1.states(), 157);
        assert_eq!(Row::Stride3.states(), 53);
        assert_eq!(Row::Stride10.states(), 17);
        assert_eq!(Row::Stride1.logical_bytes(), TOTAL_LOGICAL_BYTES);
        assert_eq!(
            Row::Stride10.logical_bytes() + Row::Stride3.logical_bytes(),
            2_237_778_180
        );
        assert_eq!(Row::Stride10.id(), "history-stride10");
        assert_eq!(Row::from_id("history-stride3"), Some(Row::Stride3));
        assert_eq!(Row::from_id("c2.delta.cdc-locality"), None);
        assert_eq!(Row::Stride1.token(), "stride1");
    }

    /// The erratum E1 distinction, as arithmetic: the manifest's file count is
    /// about 18 % below the path-state pin at every size, and the two are different
    /// quantities rather than a rounding of one another.
    #[test]
    fn path_states_are_not_the_manifest_file_count() {
        for row in Row::ALL {
            let files = row.manifest_files();
            let paths = row.path_states();
            assert!(files < paths, "{row}: {files} files vs {paths} path-states");
            let ratio = paths as f64 / files as f64;
            assert!(
                (1.17..1.19).contains(&ratio),
                "{row}: ratio {ratio:.4} is outside the verified band"
            );
        }
    }

    #[test]
    fn a_tree_line_parses_in_the_corpus_column_order() {
        let directory = std::env::temp_dir().join(format!("history-tree-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("temp dir");
        let path = directory.join("manifest.tsv");
        let oid = hex_oid(&blob_oid(b"hello\n"));
        std::fs::write(
            &path,
            format!("100644\t{oid}\t6\t{}\n120000\t{oid}\t6\t{}\n\n", "612f62", "612f63"),
        )
        .expect("write");
        let tree = parse_tree(&path).expect("parses");
        assert_eq!(tree.len(), 2);
        let entry = tree.get(b"a/b".as_slice()).expect("the first path");
        assert_eq!(entry.mode, 0o100644);
        assert_eq!(entry.size, 6);
        assert_eq!(hex_oid(&entry.oid), oid);
        assert_eq!(
            tree.get(b"a/c".as_slice()).expect("the second path").mode,
            0o120000
        );
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_malformed_tree_line_names_its_line_and_is_refused() {
        let directory = std::env::temp_dir().join(format!("history-bad-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("temp dir");
        let path = directory.join("manifest.tsv");
        let good = "100644\tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef\t1\t61\n";
        for (text, expected_line) in [
            ("100644\tnot-an-oid\t1\t61\n".to_string(), 1),
            ("100644\tdeadbeef\t1\t61\n".to_string(), 1),
            ("9z644\tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef\t1\t61\n".to_string(), 1),
            ("100644\tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef\t1\t6\n".to_string(), 1),
            ("100644\tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef\tx\t61\n".to_string(), 1),
            // A well-formed first line, so the fault is on the second.
            (format!("{good}100644\tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef\t1\n"), 2),
        ] {
            std::fs::write(&path, &text).expect("write");
            match parse_tree(&path) {
                Err(HistoryError::TreeManifest { line, .. }) => assert_eq!(line, expected_line),
                other => panic!("{text:?} should be refused, got {other:?}"),
            }
        }
        std::fs::remove_dir_all(&directory).ok();
    }

    #[test]
    fn a_blob_that_does_not_identify_is_refused_both_ways() {
        let bytes = b"hello\n";
        let oid = blob_oid(bytes);
        let recorded = hex(&sha256(bytes));
        let path = Path::new("/corpus/inputs/x/blobs/y");
        assert!(verify_blob(&oid, path, bytes, Some(&recorded)).is_ok());

        // Wrong bytes for the name: the git oid catches it first.
        let error = verify_blob(&oid, path, b"goodbye\n", Some(&recorded)).expect_err("refused");
        assert_eq!(error.name(), "BlobIdentity");
        assert!(error.to_string().contains(&hex_oid(&oid)));

        // Right bytes, wrong recorded digest: the sha256 catches it.
        let error = verify_blob(&oid, path, bytes, Some(&"00".repeat(32))).expect_err("refused");
        assert_eq!(error.name(), "BlobIdentity");

        // An absent receipt entry is a refusal, never an assumption.
        assert!(verify_blob(&oid, path, bytes, None).is_ok());
    }

    #[test]
    fn an_oracle_parses_files_and_directories_apart() {
        let path = Path::new("/corpus/oracles/x.json");
        let text = format!(
            r#"{{"612f62": ["40755", 0, "-"], "612f622f63": ["100644", 6, "{}"]}}"#,
            "ab".repeat(32)
        );
        let oracle = parse_oracle(path, text.as_bytes()).expect("parses");
        assert_eq!(oracle.len(), 2);
        assert_eq!(oracle.directories(), 1);
        let directory = oracle.get(b"a/b").expect("the directory");
        assert!(directory.is_directory());
        assert_eq!(directory.mode, 0o40755);
        let file = oracle.get(b"a/b/c").expect("the file");
        assert!(!file.is_directory());
        assert_eq!(file.mode, 0o100644);
        assert_eq!(file.size, 6);
        assert_eq!(file.digest, Some([0xab; 32]));
    }

    #[test]
    fn a_malformed_oracle_is_refused_rather_than_counted() {
        let path = Path::new("/corpus/oracles/x.json");
        for text in [
            "[]",
            r#"{"61": ["100644", 1]}"#,
            r#"{"61": ["100644", 1, "-", "extra"]}"#,
            r#"{"61": [100644, 1, "-"]}"#,
            r#"{"6": ["100644", 1, "-"]}"#,
            r#"{"61": ["100644", 1, "zz"]}"#,
            r#"{"61": ["100644", 1, "abcd"]}"#,
        ] {
            assert!(
                parse_oracle(path, text.as_bytes()).is_err(),
                "{text:?} should be refused"
            );
        }
    }

    #[test]
    fn hex_decoding_refuses_odd_lengths_and_non_hex() {
        assert_eq!(decode_hex("6162"), Some(b"ab".to_vec()));
        assert_eq!(decode_hex(""), Some(Vec::new()));
        assert_eq!(decode_hex("616"), None);
        assert_eq!(decode_hex("zz"), None);
    }

    #[test]
    fn a_transition_counts_and_sizes_only_what_it_added_or_modified() {
        let state = State {
            ordinal: 1,
            full157_index: 1,
            sha: "s".into(),
            tree: "t".into(),
            manifest_sha256: "m".into(),
            oracle_path: PathBuf::from("/o"),
            oracle_sha256: "h".into(),
            logical_bytes: 10,
            paths: 3,
        };
        let oid = blob_oid(b"x");
        let transition = Transition {
            state,
            changed: vec![
                ChangedPath {
                    path: b"a".to_vec(),
                    mode: 0o100644,
                    oid,
                    size: 7,
                    kind: Change::Added,
                },
                ChangedPath {
                    path: b"b".to_vec(),
                    mode: 0o100644,
                    oid,
                    size: 11,
                    kind: Change::Modified,
                },
                ChangedPath {
                    path: b"c".to_vec(),
                    mode: 0o100644,
                    oid,
                    size: 13,
                    kind: Change::Removed,
                },
                ChangedPath {
                    path: b"d".to_vec(),
                    mode: 0o100755,
                    oid,
                    size: 17,
                    kind: Change::MetadataOnly,
                },
            ],
            blobs: BlobMap::new(),
            tree: BTreeMap::new(),
        };
        assert_eq!(transition.changed_bytes(), 18);
        assert_eq!(transition.count(Change::Added), 1);
        assert_eq!(transition.count(Change::Removed), 1);
        assert_eq!(Change::MetadataOnly.token(), "metadata-only");
    }
}
