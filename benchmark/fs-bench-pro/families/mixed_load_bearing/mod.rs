use super::workspace_common::{Case, Entry, Receipt};
use super::{ordinary_workloads, Result};

pub(crate) const FAMILY_ID: &str = "mixed_load_bearing";

pub(crate) fn cases() -> Vec<Case> {
    let mut rows = Vec::new();
    for (kind, prefix, suffix) in [("agent-episodes", "agent-episodes-", "")] {
        for tier in [1, 10, 100, 500] {
            rows.push(Case {
                id: format!("{prefix}{tier}{suffix}{}", if tier<=10 {"-compact-v2"} else {""}),
                family: FAMILY_ID,
                tier,
                kind,
            });
        }
    }
    rows.extend(
        super::v016_stages::family_case_ids(super::v016_stages::Topology::Sequential)
            .into_iter()
            .map(|id| Case {
                id: id.to_string(),
                family: FAMILY_ID,
                tier: if id.contains("-k100-") { 100 } else { 10 },
                kind: "v016-m1-sequential",
            }),
    );
    // The two explicit extended replays share this family's registry identity
    // so one explicit selection can resolve them; they are never a default.
    rows.extend(
        super::v016_stages::EXTENDED_IDS
            .into_iter()
            .filter(|id| !id.contains("workspace-four"))
            .map(|id| Case {
                id: id.to_string(),
                family: FAMILY_ID,
                tier: 100,
                kind: "v016-m1-exhaustive",
            }),
    );
    rows
}

pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    if super::v016_stages::mixed_case(&case.id)?.is_some() {
        return super::v016_common::fixture_for_case(&case.id, seed);
    }
    ordinary_workloads::fixture(case, seed)
}

pub(crate) fn expected(case: &Case, seed: u8, step: usize) -> Result<Vec<Entry>> {
    // The v0.1.6 M1 oracle is stateful and lives in the host orchestrator:
    // `src/v016_oracle.rs` derives every state from the fixture recipe and the
    // declared operation algebra instead of a flat per-step entry list.
    if super::v016_stages::mixed_case(&case.id)?.is_some() {
        return fixture(case, seed);
    }
    ordinary_workloads::expected(case, seed, step)
}

pub(crate) fn apply(case: &Case, seed: u8, step: usize, verify: bool) -> Result<Receipt> {
    if super::v016_stages::mixed_case(&case.id)?.is_some() {
        return Err("v0.1.6 M1 cases run through the host orchestrator".into());
    }
    ordinary_workloads::apply(case, seed, step, verify)
}

pub(crate) fn self_check() -> Result<()> {
    ordinary_workloads::check_cases(&cases()[..4], 4)?;
    let mixed = super::v016_stages::family_case_ids(super::v016_stages::Topology::Sequential);
    if mixed.len() != 4 {
        return Err("mixed_load_bearing must register four v0.1.6 M1 cases".into());
    }
    for id in mixed {
        let case = super::v016_stages::mixed_case(id)?
            .ok_or_else(|| format!("{id} is not a registered M1 case"))?;
        if case.topology != super::v016_stages::Topology::Sequential {
            return Err(format!("{id} must use the sequential topology").into());
        }
    }
    Ok(())
}
