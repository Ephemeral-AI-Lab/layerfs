use super::dedup_workloads as d;
use super::workspace_common::{Case, Entry};
use super::Result;
pub(crate) const FAMILY: &str = "dedup_cdc_locality";
pub(crate) fn cases() -> Vec<Case> {
    d::cases(
        FAMILY,
        &[
            ("overwrite", "dedup-cdc-overwrite"),
            ("insert", "dedup-cdc-insert"),
            ("delete", "dedup-cdc-delete"),
            ("common-body", "dedup-cdc-common-body"),
            ("scattered", "dedup-cdc-scattered"),
        ],
    )
}
pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    d::validate(case, FAMILY, seed)?;
    let base = d::content(FAMILY, case.kind, seed, 0, "reference", d::MIB)?;
    let mut entries = d::directories(&["variants"]);
    entries.push(Entry::file("reference.dat", base.clone()));
    for i in 0..case.tier {
        entries.push(Entry::file(
            format!("variants/v{i:04}.dat"),
            d::variant(FAMILY, case.kind, seed, i, base.clone())?,
        ));
    }
    Ok(entries)
}
pub(crate) fn expected(case: &Case, seed: u8, _step: usize) -> Result<Vec<Entry>> {
    fixture(case, seed)
}
pub(crate) fn boundaries() -> Result<Vec<Entry>> {
    let mut entries = d::directories(&["boundary"]);
    for seed in 1..=3 {
        entries.push(Entry::directory(format!("boundary/s{seed}")));
        for len in [0u64, 1, 8191, 8192, 16384, 32768, 32769] {
            let path = format!("boundary/s{seed}/n{len}");
            entries.push(Entry::directory(&path));
            let base = d::content(
                FAMILY,
                "boundaries",
                seed,
                len as usize,
                "boundary-bytes",
                len,
            )?;
            entries.push(Entry::file(format!("{path}/a.dat"), base.clone()));
            entries.push(Entry::file(format!("{path}/b.dat"), base.clone()));
            if len > 0 {
                entries.push(Entry::file(
                    format!("{path}/changed.dat"),
                    base.xor(len / 2, 1, 1)?,
                ));
            }
        }
    }
    Ok(entries)
}
pub(crate) fn self_check() -> Result<()> {
    d::check_registry(&cases(), 20)?;
    for case in cases() {
        let per = match case.kind {
            "insert" => d::MIB + 4096,
            "delete" => d::MIB - 4096,
            _ => d::MIB,
        };
        if d::total(&fixture(&case, 1)?) != d::MIB + case.tier as u64 * per {
            return Err("CDC size equation".into());
        }
    }
    let count = boundaries()?
        .iter()
        .filter(|e| matches!(e.kind, super::workspace_common::EntryKind::File(_)))
        .count();
    if count != 60 {
        return Err("CDC boundary file count".into());
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
