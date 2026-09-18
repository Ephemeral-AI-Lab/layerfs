//! Pinned expectation constants: the frozen oracle's O1 and O3.
//!
//! `gates_and_oracles.md` section 4 defines **O1 identity** as *"expected root
//! `ObjectId`, from a frozen constant or recomputed off the product path"* and
//! **O3 structural count** as *"chunk counts / page counts / binding counts pinned
//! as constants"*. Until round 5 the drivers gated `replay root == measured root` —
//! self-consistency, which a replay that faithfully reproduced a **wrong** operation
//! would satisfy — and wrote the mechanism counters without comparing them with
//! anything at all.
//!
//! **Where the constants come from.** `tests/golden/expected.tsv` holds them, and
//! they were taken from the round-4c run at `90bbb617d` — the last tree on which all
//! 217 admission rows passed with 0 `FAIL`. Pinning a run's own numbers is only
//! legitimate because of what happened next: the row must reproduce them, and a
//! counter that moves is a `FAIL` rather than a new baseline. That is what makes
//! *"every counter identical to round 4c"* a gate instead of a comparison a reader
//! has to perform.
//!
//! **The table is compiled in.** `include_str!` means the constants are inside the
//! binary, so the harness identity every receipt records (`harness_binary_sha256`)
//! covers them: a table edited without a rebuild is not the table that ran.
//!
//! **A missing pin is not a pass.** A case with no pinned counter, or a pinned
//! counter the row did not publish, is `INCOMPLETE` — never silently skipped. The
//! registry self-check asserts the coverage, so the set cannot shrink unnoticed;
//! that is the same lesson the sealed-oracle parity set records, where an earlier
//! revision under-counted `edit_reference` and the oracle could have silently
//! shrunk with it.

use std::collections::BTreeMap;

use layerfs_content::ObjectId;

use crate::gates::{self, Gate, GateClass};
use crate::registry::{Admission, Case};

/// The pinned table, embedded at build time.
pub const TABLE: &str = include_str!("../../tests/golden/expected.tsv");

/// First line of the table.
pub const FORMAT: &str = "# fs-bench-expected-v1";

/// One pinned expectation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pinned {
    /// An O3 structural count, pinned as a constant.
    Counter(i128),
    /// An O1 identity, pinned as a lowercase hex digest over an identity list.
    Digest(String),
}

/// The pinned expectations of every case, keyed by `(case_id, label)`.
pub struct Expected {
    rows: BTreeMap<(String, String), Pinned>,
}

impl Expected {
    /// Parses the embedded table. A malformed line is a defect, not a skip.
    pub fn load() -> Result<Self, String> {
        let mut rows = BTreeMap::new();
        let mut lines = TABLE.lines();
        match lines.next() {
            Some(first) if first.trim() == FORMAT => {}
            other => {
                return Err(format!(
                    "expected.tsv does not start with {FORMAT:?}: {other:?}"
                ))
            }
        }
        for (number, line) in lines.enumerate() {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            let [case_id, label, value] = fields.as_slice() else {
                return Err(format!("expected.tsv line {}: {line:?}", number + 2));
            };
            let pinned = if let Some(counter) = label.strip_prefix("counter:") {
                let parsed: i128 = value
                    .parse()
                    .map_err(|_| format!("expected.tsv {case_id} {counter}: {value:?}"))?;
                Pinned::Counter(parsed)
            } else if let Some(name) = label.strip_prefix("digest:") {
                if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(format!(
                        "expected.tsv {case_id} {name}: {value:?} is not a sha256"
                    ));
                }
                Pinned::Digest(value.to_ascii_lowercase())
            } else {
                return Err(format!("expected.tsv {case_id}: unknown label {label:?}"));
            };
            let key = (case_id.to_string(), label.to_string());
            if rows.insert(key, pinned).is_some() {
                return Err(format!("expected.tsv {case_id} {label}: pinned twice"));
            }
        }
        Ok(Self { rows })
    }

    /// Every pinned counter of one case, as `(key, value)`.
    pub fn counters_of(&self, case_id: &str) -> Vec<(String, i128)> {
        self.rows
            .iter()
            .filter(|((id, label), _)| id == case_id && label.starts_with("counter:"))
            .filter_map(|((_, label), pinned)| match pinned {
                Pinned::Counter(value) => Some((label["counter:".len()..].to_string(), *value)),
                Pinned::Digest(_) => None,
            })
            .collect()
    }

    /// One pinned digest, if the table carries it.
    pub fn digest(&self, case_id: &str, name: &str) -> Option<&str> {
        match self
            .rows
            .get(&(case_id.to_string(), format!("digest:{name}")))
        {
            Some(Pinned::Digest(value)) => Some(value.as_str()),
            _ => None,
        }
    }

    /// Whether the table pins anything for this case.
    pub fn covers(&self, case_id: &str) -> bool {
        self.rows.keys().any(|(id, _)| id == case_id)
    }

    /// The number of pinned rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Gates the counters an invocation published against the pinned constants.
    ///
    /// **What this invocation gates is what this invocation published.** A
    /// phase-split row writes its measured counters from the performance invocation
    /// and its `verify.*` counters from the deferred one, so a pinned counter the
    /// invocation did not publish is not a defect here — it is a counter that
    /// belongs to the other invocation. Requiring it here would report a correct
    /// row as `INCOMPLETE`, which is the false failure the first version of this
    /// gate produced for every `c2.delta.cdc-locality` row.
    ///
    /// The coverage that rule gives up is taken back by `runner.py verify`, which
    /// re-reads the row's whole trace — both invocations — and requires every pinned
    /// counter to appear in it. A pinned constant nobody publishes is a defect
    /// there, where the whole record set is visible.
    ///
    /// An invocation that published **none** of its row's pinned counters is still
    /// `INCOMPLETE`: a gate that compares nothing is not a gate.
    pub fn counter_gates(&self, case_id: &str, published: &[(String, i128)]) -> Vec<Gate> {
        let pinned = self.counters_of(case_id);
        if pinned.is_empty() {
            return vec![Gate::incomplete(
                GateClass::Correctness,
                "g1.o3-pinned-counters",
                "no pinned counters for this case",
                "at least one O3 constant pinned for every admission case",
            )];
        }
        let observed: BTreeMap<&str, i128> = published
            .iter()
            .map(|(key, value)| (key.as_str(), *value))
            .collect();
        let mut drifted = Vec::new();
        let mut matched = 0_usize;
        let mut elsewhere = Vec::new();
        for (key, expected) in &pinned {
            match observed.get(key.as_str()) {
                None => elsewhere.push(key.clone()),
                Some(value) if value == expected => matched += 1,
                Some(value) => drifted.push(format!("{key} {expected} -> {value}")),
            }
        }
        let mut gates = Vec::new();
        if matched == 0 && drifted.is_empty() {
            return vec![Gate::incomplete(
                GateClass::Correctness,
                "g1.o3-pinned-counters",
                &format!(
                    "this invocation published none of the row's {} pinned counter(s)",
                    pinned.len()
                ),
                "an invocation that gates nothing is not a gate",
            )];
        }
        gates.push(gates::require(
            GateClass::Correctness,
            "g1.o3-pinned-counters",
            drifted.is_empty(),
            &if drifted.is_empty() {
                format!(
                    "{matched} of {} pinned counters reproduced here; {} published by the other phase",
                    pinned.len(),
                    elsewhere.len()
                )
            } else {
                drifted.join("; ")
            },
            "every O3 structural count equals its pinned constant",
        ));
        gates
    }

    /// Gates one observed identity digest against its pinned constant.
    ///
    /// This is O1: the expected root comes from the frozen table, not from a replay
    /// of the same operation, so a replay that reproduced a wrong result cannot
    /// satisfy it.
    pub fn digest_gate(
        &self,
        case_id: &str,
        name: &str,
        identifier: &'static str,
        observed: &str,
    ) -> Gate {
        match self.digest(case_id, name) {
            None => Gate::incomplete(
                GateClass::Correctness,
                identifier,
                &format!("no pinned {name} for {case_id}"),
                "a pinned expected identity for every row that claims O1",
            ),
            Some(expected) => gates::require(
                GateClass::Correctness,
                identifier,
                expected == observed.to_ascii_lowercase(),
                &format!("{name} {observed}"),
                expected,
            ),
        }
    }

    /// Whether every admission case carries at least one pinned counter.
    ///
    /// The registry self-check calls this, so the pinned set cannot shrink without
    /// the self-check saying so.
    pub fn coverage(&self, cases: &[Case]) -> Vec<String> {
        cases
            .iter()
            .filter(|case| case.admission == Admission::Admission)
            .filter(|case| self.counters_of(case.id).is_empty())
            .map(|case| case.id.to_string())
            .collect()
    }
}

/// SHA-256 over an ordered identity list, as the pinned digest form.
///
/// A digest rather than a list, because the identity list of a 500-member delta row
/// is 500 lines and the constant that has to be frozen is one. It commits to the
/// order as well as to the members, so a reordered fixture is a different digest.
pub fn identity_digest(ids: &[ObjectId]) -> String {
    let mut hasher = crate::workload::digest::Sha256::new();
    for id in ids {
        hasher.update(id.as_bytes());
    }
    crate::workload::digest::hex(&hasher.finish())
}

/// Publishes one measured identity so the pinned gate can compare it.
///
/// The record is `oracle`-kind and keyed `identity.<name>`: it is the row's own
/// declaration of the root it produced, and `main` gates it against
/// `tests/golden/expected.tsv` after the driver has returned.
pub fn publish(
    trace: &mut crate::support::trace::TraceWriter,
    name: &str,
    digest: &str,
) -> Result<(), crate::ops::OpError> {
    trace.write(
        crate::support::trace::Kind::Oracle,
        &format!("identity.{name}"),
        digest,
        "sha256",
        "measured result identity; pinned by tests/golden/expected.tsv",
    )?;
    Ok(())
}

/// The distinct identities of a list, in a stable order.
///
/// `Store::contains` answers per **lookup page** (`LOOKUP_PAGE_IDS = 128`), so an
/// input list with repeats — and 500 `delete` members derived from one base are
/// byte-identical, so the list is 500 copies of one identity — comes back with one
/// entry per page that matched. Comparing `present.len()` with the raw list length
/// therefore reported "1 of 500 present" for a Store that held the identity, which
/// is exactly the false failure this helper removes. Both sides are distinct.
pub fn distinct(ids: &[ObjectId]) -> Vec<ObjectId> {
    let mut out = ids.to_vec();
    out.sort_unstable();
    out.dedup();
    out
}

/// Every identity of `ids` the Store holds, asking in waves the Store allows.
///
/// `Store::contains` refuses a demand above its **declared** read ceiling
/// (`StorageCapacities::read_objects`, today 4,096), and the ceiling is read from
/// the Store rather than restated here: a harness constant that happened to match
/// today's policy would silently become a hardcoded limit the day the policy moved.
/// The `c2.footprint` rows supply 27,198 and 100,000 identities, so the first
/// version of the presence gate was refused with `CapacityExceeded` and reported the
/// row `INCOMPLETE` for the harness's own over-ask rather than for anything the
/// product did.
pub fn present_all(
    store: &layerfs_storage::Store,
    ids: &[ObjectId],
    scope: &layerfs_telemetry::timer::TimingScope<'_, layerfs_telemetry::timer::Active>,
) -> Result<Vec<ObjectId>, layerfs_storage::StorageError> {
    let ceiling = store.capacities().read_objects.max(1);
    let mut present = Vec::with_capacity(ids.len());
    for (index, wave) in ids.chunks(ceiling).enumerate() {
        present.extend(store.contains(wave, scope.child("presence"))?);
        let _ = index;
    }
    Ok(present)
}
