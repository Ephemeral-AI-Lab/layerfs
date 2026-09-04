use super::dedup_workloads as d;
use super::workspace_common::{Case, Entry};
use super::Result;
pub(crate) const FAMILY: &str = "dedup_cross_file";
pub(crate) fn cases() -> Vec<Case> {
    let mut rows = vec![Case {
        id: "dedup-cross-file-anchor-1".into(),
        family: FAMILY,
        tier: 1,
        kind: "anchor",
    }];
    rows.extend(
        d::cases(
            FAMILY,
            &[
                ("unique", "dedup-cross-file-unique"),
                ("identical", "dedup-cross-file-identical"),
                ("mixed", "dedup-cross-file-mixed"),
            ],
        )
        .into_iter()
        .filter(|c| c.tier != 1),
    );
    rows
}
pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    d::validate(case, FAMILY, seed)?;
    let mut entries = d::directories(&["files"]);
    for i in 0..case.tier {
        let base = match case.kind {
            "anchor" | "identical" => 0,
            "unique" => i,
            "mixed" => 3 * (i / 4) + (i % 4).saturating_sub(1),
            _ => return Err("cross-file profile".into()),
        };
        entries.push(Entry::file(
            format!("files/f{i:04}.dat"),
            d::content(FAMILY, "base", seed, base, "bytes", d::MIB)?,
        ));
    }
    Ok(entries)
}
pub(crate) fn expected(case: &Case, seed: u8, _step: usize) -> Result<Vec<Entry>> {
    fixture(case, seed)
}
pub(crate) fn self_check() -> Result<()> {
    d::check_registry(&cases(), 10)?;
    for case in cases() {
        if d::total(&fixture(&case, 1)?) != case.tier as u64 * d::MIB {
            return Err("cross-file bytes".into());
        }
    }
    for (n, want) in [(10, 7), (100, 75), (500, 375)] {
        let count = (0..n)
            .map(|i: usize| 3 * (i / 4) + (i % 4).saturating_sub(1))
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        if count != want {
            return Err("mixed unique count".into());
        }
    }
    Ok(())
}

pub(crate) fn apply(
    case: &Case,
    seed: u8,
    step: usize,
    verify: bool,
) -> Result<super::workspace_common::Receipt> {
    d::apply(case, seed, step, verify)
}
