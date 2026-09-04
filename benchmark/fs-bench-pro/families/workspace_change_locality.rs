use super::workspace_common::{Case, Entry, Receipt};
use super::{ordinary_workloads, Result};

pub(crate) const FAMILY_ID: &str = "workspace_change_locality";

pub(crate) fn cases() -> Vec<Case> {
    let mut rows = Vec::new();
    for (kind, prefix, suffix) in [
        ("workspace-clean-commit", "workspace-clean-commit-", ""),
        ("workspace-fixed-move", "workspace-fixed-move-", ""),
        (
            "workspace-distributed-sdk-edit",
            "workspace-distributed-sdk-edit-",
            "",
        ),
        ("workspace-dense-rewrite", "workspace-dense-rewrite-", ""),
    ] {
        for tier in [1, 10, 100, 500] {
            rows.push(Case {
                id: format!("{prefix}{tier}{suffix}"),
                family: FAMILY_ID,
                tier,
                kind,
            });
        }
    }
    rows
}

pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    ordinary_workloads::fixture(case, seed)
}

pub(crate) fn expected(case: &Case, seed: u8, step: usize) -> Result<Vec<Entry>> {
    ordinary_workloads::expected(case, seed, step)
}

pub(crate) fn apply(case: &Case, seed: u8, step: usize, verify: bool) -> Result<Receipt> {
    ordinary_workloads::apply(case, seed, step, verify)
}

pub(crate) fn self_check() -> Result<()> {
    ordinary_workloads::check_cases(&cases(), 16)
}
