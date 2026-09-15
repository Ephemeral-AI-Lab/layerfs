use super::workspace_common::{Case, Entry, Receipt};
use super::Result;

pub(crate) const FAMILY_ID: &str = "multi_workspace_development";

pub(crate) fn cases() -> Vec<Case> {
    let mut rows: Vec<Case> =
        super::v016_stages::family_case_ids(super::v016_stages::Topology::Concurrent)
            .into_iter()
            .map(|id| Case {
                id: id.to_string(),
                family: FAMILY_ID,
                tier: if id.contains("-k100-") { 100 } else { 10 },
                kind: "v016-m1-concurrent",
            })
            .collect();
    // The explicit four-workspace extension shares this family's registry
    // identity so one explicit selection can resolve it.
    rows.push(Case {
        id: super::v016_stages::EXTENDED_IDS[2].to_string(),
        family: FAMILY_ID,
        tier: 100,
        kind: "v016-m1-four",
    });
    rows
}

pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    super::v016_common::fixture_for_case(&case.id, seed)
}

pub(crate) fn self_check() -> Result<()> {
    let rows = cases();
    if rows.len() != 5 {
        return Err("multi_workspace_development must register four regular cases and one extended case".into());
    }
    for case in &rows {
        if super::v016_stages::mixed_case(&case.id)?.is_none() {
            return Err(format!("{} is not a registered M1 case", case.id).into());
        }
    }
    Ok(())
}

pub(crate) fn expected(case: &Case, seed: u8, _step: usize) -> Result<Vec<Entry>> {
    // The v0.1.6 M1 oracle is stateful and lives in the host orchestrator:
    // `src/v016_oracle.rs` derives every state from the fixture recipe and the
    // declared operation algebra instead of a flat per-step entry list.
    fixture(case, seed)
}

pub(crate) fn apply(_case: &Case, _seed: u8, _step: usize, _verify: bool) -> Result<Receipt> {
    Err("v0.1.6 M1 cases run through the host orchestrator".into())
}
