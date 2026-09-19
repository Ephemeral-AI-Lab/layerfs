//! One module per family, each owning its own case list.
//!
//! `docs/general/benchmark_rules.md` section 7 requires one canonical definition
//! module and one thin runner per family. A row in a case table is a canonical
//! definition but is not a module, so each family here holds its rows and nothing
//! else: the operation, fixture and oracle bodies live in `ops.rs`, `fixture.rs`
//! and `workload/oracle.rs`. Collapsing the layer into the golden TSV would need
//! an owner waiver that was not granted.

pub mod c1_cdc;
pub mod c1_construct;
pub mod c1_edit;
pub mod c1_fs_build;
pub mod c1_locality;
pub mod c1_many_tiny;
pub mod c1_transition;
pub mod c1_tree;
pub mod c2_delta;
pub mod c2_delta_small_file;
pub mod c2_footprint;
pub mod c2_lifecycle;
pub mod c2_pool;
pub mod c2_read;
pub mod c2_reuse;
pub mod component_primitives;
pub mod history;
pub mod pipeline;

use crate::registry::{Admission, CacheState, Case, Preparation, Shape, StoreState};

/// Registry groups, in frozen cardinality order.
///
/// Twenty-one groups: eleven C1 families, nine C2 rows (`c2.delta.boundaries` is
/// the registered sub-lane of C2-3, counted separately) and `pipeline.*`.
pub const ALL: [fn() -> Vec<Case>; 21] = [
    c1_construct::whole_file,
    c1_construct::chunked,
    c1_cdc::cases,
    c1_edit::length_preserving,
    c1_edit::length_changing,
    c1_transition::cases,
    c1_many_tiny::cases,
    c1_tree::construct_traverse,
    c1_tree::namespace_mutation,
    c1_locality::cases,
    c1_fs_build::cases,
    c2_lifecycle::cases,
    c2_reuse::cross_file,
    c2_delta::cdc_locality,
    c2_delta::boundaries,
    c2_reuse::workspace,
    c2_footprint::cases,
    c2_delta_small_file::cases,
    c2_read::cases,
    c2_pool::cases,
    pipeline::cases,
];

/// Group identifiers, aligned one-for-one with [`ALL`].
///
/// A family's own module declares its identifier; this array is the single list
/// that pins the correspondence, and `registry::self_check` fails if a row names a
/// family that is not here.
pub const GROUP_IDS: [&str; 21] = [
    c1_construct::WHOLE_FILE,
    c1_construct::CHUNKED,
    c1_cdc::FAMILY,
    c1_edit::LENGTH_PRESERVING,
    c1_edit::LENGTH_CHANGING,
    c1_transition::FAMILY,
    c1_many_tiny::FAMILY,
    c1_tree::CONSTRUCT_TRAVERSE,
    c1_tree::NAMESPACE_MUTATION,
    c1_locality::FAMILY,
    c1_fs_build::FAMILY,
    c2_lifecycle::FAMILY,
    c2_reuse::CROSS_FILE,
    c2_delta::CDC_LOCALITY,
    c2_delta::BOUNDARIES,
    c2_reuse::WORKSPACE,
    c2_footprint::FAMILY,
    c2_delta_small_file::FAMILY,
    c2_read::FAMILY,
    c2_pool::FAMILY,
    pipeline::GROUP,
];

/// Registered groups that are **not** one of the twenty families.
///
/// `component.primitives` is registered, receipted and excluded from admission and
/// from every count in `CONTRACT.md` section 3. It is listed here so the
/// self-check can distinguish "a registered diagnostic group" from "a family name
/// that no module declares".
pub const DIAGNOSTIC_GROUPS: [&str; 1] = [component_primitives::GROUP];

/// The byte ladder every byte-size family uses: 1 / 10 / 100 / 500 MiB.
pub const BYTE_LADDER: [(u64, &str); 4] = [
    (1 << 20, "1m"),
    (10 << 20, "10m"),
    (100 << 20, "100m"),
    (500 << 20, "500m"),
];

/// The entry ladder: 1 / 10 / 100 / 500 entries.
pub const ENTRY_LADDER: [(u32, &str); 4] = [(1, "1"), (10, "10"), (100, "100"), (500, "500")];

/// Leaks one assembled ID. `Case` owns `&'static str` fields and the registry is
/// built once per process, so this is a bounded, one-time cost.
pub(crate) fn leak(text: String) -> &'static str {
    Box::leak(text.into_boxed_str())
}

/// Builder for one row, so a family module states only what is specific to it.
pub struct CaseSpec {
    case: Case,
}

impl CaseSpec {
    /// Starts a row with the defaults every family shares.
    pub fn new(id: &'static str, family: &'static str, shape: Shape) -> Self {
        Self {
            case: Case {
                id,
                family,
                admission: Admission::Admission,
                tier: u8::MAX,
                tier_label: "",
                profile: "",
                bytes: 0,
                entries: 0,
                smoke: false,
                cache: CacheState::WarmInProcessFixture,
                store: StoreState::None,
                prepared: Preparation::InProcess,
                shape,
            },
        }
    }

    /// Position in a byte ladder.
    pub fn byte_tier(mut self, index: usize, bytes: u64, label: &'static str) -> Self {
        self.case.tier = index as u8;
        self.case.tier_label = label;
        self.case.bytes = bytes;
        self
    }

    /// Position in an entry ladder.
    pub fn entry_tier(mut self, index: usize, entries: u32, label: &'static str) -> Self {
        self.case.tier = index as u8;
        self.case.tier_label = label;
        self.case.entries = entries;
        self
    }

    /// A fixed-configuration row: no position on a tier ladder.
    ///
    /// `u8::MAX` is the convention every family already uses for a row that is not
    /// on a ladder (`CaseSpec::new` sets it), and it is restored here because
    /// [`CaseSpec::byte_tier`] and [`CaseSpec::entry_tier`] each write `tier` — a
    /// row that carries both axes would otherwise be filed at ladder position 0.
    pub fn fixed(mut self) -> Self {
        self.case.tier = u8::MAX;
        self.case.tier_label = "";
        self
    }

    /// Fixture-profile variant, when the row carries one.
    pub fn profile(mut self, profile: &'static str) -> Self {
        self.case.profile = profile;
        self
    }

    /// Declared cache state.
    pub fn cache(mut self, cache: CacheState) -> Self {
        self.case.cache = cache;
        self
    }

    /// Declared Store state.
    pub fn store(mut self, store: StoreState) -> Self {
        self.case.store = store;
        self
    }

    /// Declared preparation: acquired once, or built inside the row's invocation.
    pub fn prepared(mut self, prepared: Preparation) -> Self {
        self.case.prepared = prepared;
        self
    }

    /// Marks the row as the family's `--smoke` representative.
    pub fn smoke(mut self) -> Self {
        self.case.smoke = true;
        self
    }

    /// Marks the row as the `--smoke` representative when `flag` holds.
    pub fn smoke_if(mut self, flag: bool) -> Self {
        self.case.smoke = flag;
        self
    }

    /// Marks the row as a registered diagnostic outside the 217.
    pub fn diagnostic(mut self) -> Self {
        self.case.admission = Admission::Diagnostic;
        self
    }

    /// Finishes the row.
    pub fn build(self) -> Case {
        self.case
    }
}

/// Tier-to-profile rule for a bracketed profile list.
///
/// `c1-families.md` section 3.1 writes `[-compact-v2|-mixed-v4]` and the frozen
/// cardinality parsing rule says that is **one** case rendered with a
/// tier-selected profile. The rule is fixed here, once: the two small tiers take
/// the mixed variant and the two large tiers the compact one, so the ladder spans
/// both profiles without changing the row count. The golden TSV names the profile
/// each row actually got, so the ruling is visible in a diff.
pub fn profile_for_tier(index: usize, mixed: &'static str, compact: &'static str) -> &'static str {
    if index < 2 {
        mixed
    } else {
        compact
    }
}
