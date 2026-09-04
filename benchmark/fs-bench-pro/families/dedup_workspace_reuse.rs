use super::dedup_workloads as d;
use super::workspace_common::{Case, Entry};
use super::Result;
pub(crate) const FAMILY: &str = "dedup_workspace_reuse";
pub(crate) fn cases() -> Vec<Case> {
    d::cases(
        FAMILY,
        &[
            ("exact", "dedup-workspace-exact"),
            ("local", "dedup-workspace-local"),
            ("unique", "dedup-workspace-unique"),
        ],
    )
}
pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    d::validate(case, FAMILY, seed)?;
    let mut entries = d::directories(&["base", "added"]);
    for i in 0..128 {
        entries.push(Entry::file(
            format!("base/b{i:04}.dat"),
            d::content(FAMILY, "base", seed, i, "bytes", d::MIB)?,
        ));
    }
    Ok(entries)
}
pub(crate) fn additions(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    d::validate(case, FAMILY, seed)?;
    let mut entries = Vec::with_capacity(case.tier);
    for i in 0..case.tier {
        let base = d::content(FAMILY, "base", seed, i % 128, "bytes", d::MIB)?;
        let content = match case.kind {
            "exact" => base,
            "local" => d::variant(FAMILY, "local", seed, i, base)?,
            "unique" => d::content(FAMILY, "unique", seed, i, "bytes", d::MIB)?,
            _ => return Err("workspace reuse profile".into()),
        };
        entries.push(Entry::file(format!("added/a{i:04}.dat"), content));
    }
    Ok(entries)
}
pub(crate) fn expected(case: &Case, seed: u8, step: usize) -> Result<Vec<Entry>> {
    if step > 1 {
        return Err("workspace reuse step".into());
    }
    let mut entries = fixture(case, seed)?;
    if step == 1 {
        entries.extend(additions(case, seed)?);
    }
    Ok(entries)
}
pub(crate) fn self_check() -> Result<()> {
    d::check_registry(&cases(), 12)?;
    for case in cases() {
        if d::total(&expected(&case, 1, 1)?) != (128 + case.tier as u64) * d::MIB {
            return Err("workspace reuse size".into());
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
