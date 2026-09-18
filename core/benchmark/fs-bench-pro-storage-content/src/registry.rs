//! The frozen case registry: 220 rows = 217 admission + 3 diagnostic.
//!
//! `CONTRACT.md` section 3 freezes twenty families, a sub-lane and a pipeline
//! group, with the cardinality array `[4,4,12,12,32,7,20,12,4,12,8,5,10,20,21,
//! 14,6,4,4,2,4]` summing to 217. Three further `component.primitives` cases are
//! registered, run and receipted but are **excluded from admission and from every
//! count in that section**: under `claim_kind = structural-complexity` their
//! receipt is diagnostic and cannot gate. Folding them into 217 is wrong.
//!
//! Cardinality parsing rule (frozen in `CONTRACT.md` section 3): a bracketed
//! profile list in a case ID is **one** case rendered with a profile chosen by
//! tier, never one case per profile. Read the other way the C1 ID list parses to
//! 175 against a table that sums to 127.
//!
//! This module owns the rows; each family module owns its own case list, so no
//! family can silently gain or lose a case without the cardinality self-check
//! failing.

use std::sync::OnceLock;

use crate::families;

/// Frozen per-family cardinality, in `families::ALL` order.
pub const FROZEN_CARDINALITY: [usize; 21] = [
    4, 4, 12, 12, 32, 7, 20, 12, 4, 12, 8, // C1: 127
    5, 10, 20, 21, 14, 6, 4, 4, 2, // C2: 86
    4, // pipeline: 4
];

/// Registered admission cases.
pub const ADMISSION_CASES: usize = 217;

/// Diagnostic cases excluded from the 217.
pub const DIAGNOSTIC_CASES: usize = 3;

/// Every registered row.
pub const REGISTERED_ROWS: usize = ADMISSION_CASES + DIAGNOSTIC_CASES;

/// `--smoke` selects one tier per family: twenty families, twenty cases.
pub const SMOKE_CASES: usize = 20;

/// A registry row's admission class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Admission {
    /// Counted in the frozen 217. May gate.
    Admission,
    /// Registered, receipted, excluded from admission and from every count.
    Diagnostic,
}

/// Declared cache state of a row. States are never pooled.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheState {
    /// Fixture bytes are in harness memory; no file I/O happens in the timer.
    WarmInProcessFixture,
    /// Prepared base under a declared copy rung, de-warmed before the sample.
    PreparedDewarmed,
    /// Base written by the operation itself inside the timed region.
    CreatedInSample,
}

/// Declared store state of a C2 row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoreState {
    /// Not a C2 row.
    None,
    /// The Store is created inside the timed region.
    CreatedInSample,
    /// The Store is opened over a per-sample byte copy of a prepared master.
    OpenedFromCopy,
}

/// The operation shape one row drives.
///
/// Parameters that vary per row live on [`Case`] (`bytes`, `entries`, `profile`),
/// so this enum stays small and `Copy`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Shape {
    /// C1-1/C1-2: complete-file construction through one of the two entry points.
    Construct(Route),
    /// C1-3: one fixed 64 KiB overwrite classified by its chunk-count effect.
    ChunkCount(CountOp),
    /// C1-4/C1-5: one localized edit of a constructed base.
    Edit(EditOp),
    /// C1-6: a representation transition across the frozen cutoff.
    Transition(Transition),
    /// C1-7: many tiny files.
    ManyTiny(TinyOp),
    /// C1-8: directory construction and traversal.
    Tree(TreeOp),
    /// C1-9: namespace subtree relocation plus delete.
    Namespace,
    /// C1-10: change locality over a constructed tree.
    Locality(LocalityOp),
    /// C1-11: filesystem build scale.
    FsBuild {
        /// Whether the row uses the `-text-v1` fixture variant.
        text: bool,
    },
    /// C2-1: one Store lifecycle step.
    Lifecycle(LifecycleStep),
    /// C2-2: cross-file reuse of supplied objects.
    Reuse(ReuseOp),
    /// C2-3: CDC delta locality over a stored base.
    Delta(DeltaOp),
    /// C2-3 sub-lane: one chunk-grammar boundary length.
    Boundary {
        /// Declared seed of this boundary row.
        seed: u8,
    },
    /// C2-4: workspace reuse.
    Workspace(ReuseOp),
    /// C2-5: footprint accounting.
    Footprint(FootprintOp),
    /// C2-6: the delta policy below the small-file cutoff.
    SmallFile,
    /// C2-7: independent read waves over a stored ladder.
    ReadWave,
    /// C2-8: the pooled metadata lane, cold or warm.
    Pool {
        /// `true` for the cold row, `false` for the warm one.
        cold: bool,
    },
    /// C2-9: the integrated C1 to C2 handoff.
    Pipeline(PipelineOp),
    /// The three diagnostic matched-pair cases.
    Primitives(PrimitiveOp),
}

/// Which C1 construction entry point a row uses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    /// `construct_bytes`: the whole payload is an in-memory slice.
    Bytes,
    /// `construct_stream`: the payload arrives through `impl Read`.
    Stream,
}

/// C1-3's three chunk-count effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CountOp {
    /// The edit does not move the chunk count.
    Preserve,
    /// The edit increases the chunk count.
    Increase,
    /// The edit decreases the chunk count.
    Decrease,
}

/// C1-4/C1-5 edit shapes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditOp {
    /// 4 KiB overwrite at the head.
    OverwriteHead,
    /// 4 KiB overwrite in the middle.
    OverwriteMiddle,
    /// 4 KiB overwrite at the tail.
    OverwriteTail,
    /// 4 KiB inserted in the middle.
    InsertMiddle,
    /// 4 KiB deleted from the middle.
    DeleteMiddle,
    /// 4 KiB appended at the tail.
    AppendTail,
    /// 4 KiB prepended at the head.
    PrependHead,
    /// Base grown by 4 KiB.
    Grow,
    /// Base shrunk by 4 KiB.
    Shrink,
    /// Base truncated to a shorter length.
    Truncate,
    /// Base extended with zeros at the tail.
    ZeroExtend,
}

impl EditOp {
    /// The counter equation this op is expected to satisfy.
    pub fn length_delta(self, replacement_len: u64) -> i64 {
        match self {
            Self::OverwriteHead | Self::OverwriteMiddle | Self::OverwriteTail => 0,
            Self::InsertMiddle | Self::AppendTail | Self::PrependHead => replacement_len as i64,
            Self::DeleteMiddle => -(replacement_len as i64),
            Self::Grow | Self::ZeroExtend => replacement_len as i64,
            Self::Shrink | Self::Truncate => -(replacement_len as i64),
        }
    }

    /// Whether the op keeps the logical length.
    pub fn is_length_preserving(self) -> bool {
        matches!(
            self,
            Self::OverwriteHead | Self::OverwriteMiddle | Self::OverwriteTail
        )
    }
}

/// C1-6 transition targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Transition {
    /// 131,071 bytes: below the cutoff, so whole-file.
    SmallControl,
    /// 131,071 bytes built, then grown to the exact cutoff.
    Below,
    /// Exactly 131,072 bytes: the cutoff itself, so chunked.
    Exact,
    /// 131,073 bytes: just above the cutoff.
    Above,
    /// 1 MiB: the large control.
    LargeControl,
    /// A chunked base shrunk below the cutoff: chunked to whole-file.
    Roundtrip,
    /// The same transition reached through an alias root.
    AliasRoundtrip,
}

/// C1-7 tiny-file operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TinyOp {
    /// Create one tiny file per entry.
    Create,
    /// Stat one tiny file per entry.
    Stat,
    /// Unlink one tiny file per entry.
    Unlink,
    /// Create entries in bulk.
    BulkCreate,
    /// Delete entries in bulk.
    BulkDelete,
}

/// C1-8 tree operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TreeOp {
    /// Build the tree.
    Construct,
    /// Read back every inode value.
    MetadataScan,
    /// Read back every file's content.
    ContentScan,
}

/// C1-10 locality operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalityOp {
    /// Re-commit an unchanged tree.
    CleanCommit,
    /// Move one subtree to a new parent.
    FixedMove,
    /// Rewrite every leaf inside a dense region.
    DenseRewrite,
}

/// C2-1 lifecycle steps.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecycleStep {
    /// `Store::create` over a fresh path.
    Create,
    /// `Store::open` over a prepared copy.
    Open,
    /// `begin_save` on an idle Store.
    BeginSave,
    /// `begin_save` then `finish` with nothing accepted.
    FinishEmpty,
    /// `begin_save` then `abort`.
    Abort,
}

/// C2-2/C2-4 reuse profiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReuseOp {
    /// Every member is byte-identical to the first.
    Identical,
    /// Members differ in one localized region.
    Local,
    /// Every member is distinct.
    Unique,
    /// Control: every member is distinct at a 128-file base.
    Base128,
}

/// C2-3 delta locality operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeltaOp {
    /// Overwrite one region.
    Overwrite,
    /// Insert one region.
    Insert,
    /// Delete one region.
    Delete,
    /// Two members sharing an unchanged body.
    CommonBody,
    /// Scattered edits across the base.
    Scattered,
}

/// C2-5 footprint controls.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FootprintOp {
    /// Many distinct small objects.
    Unique,
    /// High metadata cardinality.
    MetadataCardinality,
    /// One large object.
    LargeObject,
}

/// C2-9 integrated cases.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PipelineOp {
    /// Edit the small shape end to end.
    EditsSmall,
    /// Edit the chunked shape end to end.
    EditsChunked,
    /// A large base edited down to a small result.
    EditsLargeToSmall,
    /// Build a filesystem and save every emitted object.
    FilesystemBuild,
}

/// The three diagnostic matched-pair shapes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimitiveOp {
    /// Whole-file payload primitive.
    Payload,
    /// Filesystem primitive.
    Filesystem,
    /// Localized-edit primitive.
    Edit,
}

/// One registry row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Case {
    /// Globally unique case ID.
    pub id: &'static str,
    /// Owning family, or `pipeline.*` / `component.primitives`.
    pub family: &'static str,
    /// Admission class.
    pub admission: Admission,
    /// Position in the family's tier ladder; `u8::MAX` for fixed-config rows.
    pub tier: u8,
    /// Tier label as the specification writes it (`1m`, `10`, or empty).
    pub tier_label: &'static str,
    /// Fixture-profile variant, empty when the row has none.
    pub profile: &'static str,
    /// Primary byte axis, when the family's ladder is a byte ladder.
    pub bytes: u64,
    /// Primary count axis, when the family's ladder is an entry ladder.
    pub entries: u32,
    /// Selected by `--smoke` (one tier per family).
    pub smoke: bool,
    /// Declared cache state.
    pub cache: CacheState,
    /// Declared store state.
    pub store: StoreState,
    /// The operation this row drives.
    pub shape: Shape,
}

/// Every registered row, in family order.
///
/// Built once per process. The IDs are assembled from their family's ladder, so
/// the family modules stay thin; leaking those ~220 short strings once keeps
/// `Case` `Copy` and every other module free of lifetime plumbing.
pub fn cases() -> &'static [Case] {
    static REGISTRY: OnceLock<Vec<Case>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut all = Vec::with_capacity(REGISTERED_ROWS);
        for family in families::ALL {
            all.extend(family());
        }
        // Registered, receipted, and outside the 217: these three rows carry
        // `Admission::Diagnostic` and are never counted as admission cases.
        all.extend(families::component_primitives::cases());
        all
    })
}

/// Registered admission rows only.
pub fn admission_cases() -> Vec<Case> {
    cases()
        .iter()
        .copied()
        .filter(|case| case.admission == Admission::Admission)
        .collect()
}

/// The `--smoke` lane: one tier per family.
pub fn smoke_cases() -> Vec<Case> {
    cases().iter().copied().filter(|case| case.smoke).collect()
}

/// Per-family case counts in `families::ALL` order.
pub fn cardinality() -> Vec<usize> {
    families::GROUP_IDS
        .iter()
        .map(|group| cases().iter().filter(|case| case.family == *group).count())
        .collect()
}

/// One self-check failure, with the number that disagreed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mismatch {
    /// What was checked.
    pub what: &'static str,
    /// The frozen expectation.
    pub expected: String,
    /// What the registry actually holds.
    pub actual: String,
}

/// Asserts the frozen cardinality, the sums, uniqueness and lane sizes.
///
/// This is the check `CONTRACT.md` section 3 names. It fails closed: every
/// disagreement is returned, never the first one only.
pub fn self_check() -> Vec<Mismatch> {
    let mut problems = Vec::new();
    let rows = cases();
    let counts = cardinality();
    if counts != FROZEN_CARDINALITY {
        problems.push(Mismatch {
            what: "frozen cardinality array",
            expected: format!("{FROZEN_CARDINALITY:?}"),
            actual: format!("{counts:?}"),
        });
    }
    let admission = rows
        .iter()
        .filter(|case| case.admission == Admission::Admission)
        .count();
    if admission != ADMISSION_CASES {
        problems.push(Mismatch {
            what: "admission case count",
            expected: ADMISSION_CASES.to_string(),
            actual: admission.to_string(),
        });
    }
    if rows.len() != REGISTERED_ROWS {
        problems.push(Mismatch {
            what: "registered row count",
            expected: REGISTERED_ROWS.to_string(),
            actual: rows.len().to_string(),
        });
    }
    let diagnostic = rows
        .iter()
        .filter(|case| case.admission == Admission::Diagnostic)
        .count();
    if diagnostic != DIAGNOSTIC_CASES {
        problems.push(Mismatch {
            what: "diagnostic case count",
            expected: DIAGNOSTIC_CASES.to_string(),
            actual: diagnostic.to_string(),
        });
    }
    for case in rows {
        if !families::GROUP_IDS.contains(&case.family)
            && !families::DIAGNOSTIC_GROUPS.contains(&case.family)
        {
            problems.push(Mismatch {
                what: "family identifier is registered",
                expected: "one of the twenty-one registered groups".to_string(),
                actual: case.family.to_string(),
            });
            break;
        }
    }
    let sum: usize = counts.iter().sum();
    if sum != ADMISSION_CASES {
        problems.push(Mismatch {
            what: "cardinality sum",
            expected: ADMISSION_CASES.to_string(),
            actual: sum.to_string(),
        });
    }
    let smoke = rows.iter().filter(|case| case.smoke).count();
    if smoke != SMOKE_CASES {
        problems.push(Mismatch {
            what: "smoke lane size",
            expected: SMOKE_CASES.to_string(),
            actual: smoke.to_string(),
        });
    }
    let mut ids: Vec<&str> = rows.iter().map(|case| case.id).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    if ids.len() != before {
        problems.push(Mismatch {
            what: "case ID uniqueness",
            expected: before.to_string(),
            actual: ids.len().to_string(),
        });
    }
    problems
}

/// Renders the whole registry as the golden TSV this crate ships.
///
/// The table is a rendering of the registry, never its source of truth: a
/// hand-edited golden file would compare equal to nothing, because the test
/// recomputes this string and compares it with `include_str!`.
pub fn render_tsv() -> String {
    let mut out = String::from(
        "id\tfamily\tadmission\ttier\ttier_label\tprofile\tbytes\tentries\tsmoke\tcache\tstore\tshape\n",
    );
    for case in cases() {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            case.id,
            case.family,
            match case.admission {
                Admission::Admission => "admission",
                Admission::Diagnostic => "diagnostic",
            },
            if case.tier == u8::MAX {
                "-".to_string()
            } else {
                case.tier.to_string()
            },
            if case.tier_label.is_empty() {
                "-"
            } else {
                case.tier_label
            },
            if case.profile.is_empty() { "-" } else { case.profile },
            case.bytes,
            case.entries,
            if case.smoke { "yes" } else { "no" },
            match case.cache {
                CacheState::WarmInProcessFixture => "warm-in-process-fixture",
                CacheState::PreparedDewarmed => "prepared-dewarmed",
                CacheState::CreatedInSample => "created-in-sample",
            },
            match case.store {
                StoreState::None => "-",
                StoreState::CreatedInSample => "created-in-sample",
                StoreState::OpenedFromCopy => "opened-from-copy",
            },
            render_shape(case.shape),
        ));
    }
    out
}

/// Renders one shape as a stable token, so a drifted row is visible in a diff.
fn render_shape(shape: Shape) -> String {
    match shape {
        Shape::Construct(route) => format!(
            "construct:{}",
            match route {
                Route::Bytes => "bytes",
                Route::Stream => "stream",
            }
        ),
        Shape::ChunkCount(op) => format!(
            "chunk-count:{}",
            match op {
                CountOp::Preserve => "preserve",
                CountOp::Increase => "increase",
                CountOp::Decrease => "decrease",
            }
        ),
        Shape::Edit(op) => format!("edit:{op:?}").to_lowercase(),
        Shape::Transition(target) => format!("transition:{target:?}").to_lowercase(),
        Shape::ManyTiny(op) => format!("many-tiny:{op:?}").to_lowercase(),
        Shape::Tree(op) => format!("tree:{op:?}").to_lowercase(),
        Shape::Namespace => "namespace".to_string(),
        Shape::Locality(op) => format!("locality:{op:?}").to_lowercase(),
        Shape::FsBuild { text } => format!("fs-build:{}", if text { "text-v1" } else { "binary" }),
        Shape::Lifecycle(step) => format!("lifecycle:{step:?}").to_lowercase(),
        Shape::Reuse(op) => format!("reuse:{op:?}").to_lowercase(),
        Shape::Delta(op) => format!("delta:{op:?}").to_lowercase(),
        Shape::Boundary { seed } => format!("boundary:seed{seed}"),
        Shape::Workspace(op) => format!("workspace:{op:?}").to_lowercase(),
        Shape::Footprint(op) => format!("footprint:{op:?}").to_lowercase(),
        Shape::SmallFile => "small-file".to_string(),
        Shape::ReadWave => "read-wave".to_string(),
        Shape::Pool { cold } => format!("pool:{}", if cold { "cold" } else { "warm" }),
        Shape::Pipeline(op) => format!("pipeline:{op:?}").to_lowercase(),
        Shape::Primitives(op) => format!("primitives:{op:?}").to_lowercase(),
    }
}
