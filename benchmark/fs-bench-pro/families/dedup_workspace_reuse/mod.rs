use super::dedup_workloads as d;
use super::workspace_common::{Case, Entry, EntryKind};
use super::Result;
pub(crate) const FAMILY: &str = "dedup_workspace_reuse";
pub(crate) fn cases() -> Vec<Case> {
    let mut cases = d::cases(
        FAMILY,
        &[
            ("exact", "dedup-workspace-exact"),
            ("local", "dedup-workspace-local"),
            ("unique", "dedup-workspace-unique"),
        ],
    );
    for case in &mut cases {
        if case.tier <= 10 {
            case.id.push_str("-compact-v2");
        }
    }
    cases
}
pub(crate) fn base_file_count(case: &Case) -> Result<usize> {
    match case.tier {
        1 | 10 => Ok(case.tier),
        100 | 500 => Ok(128),
        _ => Err("workspace reuse tier".into()),
    }
}
pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    d::validate(case, FAMILY, seed)?;
    let mut entries = d::directories(&["base", "added"]);
    for i in 0..base_file_count(case)? {
        entries.push(Entry::file(
            format!("base/b{i:04}.dat"),
            d::content(FAMILY, "base", seed, i, "bytes", d::MIB)?,
        ));
    }
    Ok(entries)
}
pub(crate) fn additions(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    d::validate(case, FAMILY, seed)?;
    let base_files = base_file_count(case)?;
    let mut entries = Vec::with_capacity(case.tier);
    for i in 0..case.tier {
        let base = d::content(FAMILY, "base", seed, i % base_files, "bytes", d::MIB)?;
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
        let base_files = base_file_count(&case)?;
        if (case.tier <= 10) != case.id.ends_with("-compact-v2") {
            return Err("workspace reuse compact case identity".into());
        }
        if d::total(&expected(&case, 1, 1)?)
            != (base_files as u64 + case.tier as u64) * d::MIB
        {
            return Err("workspace reuse size".into());
        }
        if case.tier <= 10 {
            for step in 0..=1 {
                let state = expected(&case, 1, step)?;
                let files = state
                    .iter()
                    .filter(|entry| matches!(&entry.kind, EntryKind::File(_)))
                    .count();
                if d::total(&state) > 50 * d::MIB || files > 1_000 {
                    return Err("workspace reuse compact state bound".into());
                }
            }
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
