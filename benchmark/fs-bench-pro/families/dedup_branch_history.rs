use super::dedup_workloads as d;
use super::workspace_common::{self, Case, Content, Entry, EntryKind, SdkEdit};
use super::Result;
pub(crate) const FAMILY: &str = "dedup_branch_history";
pub(crate) fn cases() -> Vec<Case> {
    d::cases(
        FAMILY,
        &[
            ("distributed", "dedup-history-distributed"),
            ("hotset", "dedup-history-hotset"),
            ("recurring", "dedup-history-recurring"),
            ("metadata", "dedup-history-metadata"),
            ("unrelated", "dedup-history-unrelated"),
        ],
    )
}
pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    d::validate(case, FAMILY, seed)?;
    workspace_common::shards(seed, 1, "")
}
pub(crate) fn edit(case: &Case, seed: u8, step: usize) -> Result<SdkEdit> {
    d::history_edit(case, seed, step, &fixture(case, seed)?)
}
// step is the number of completed Created commits; zero denotes genesis.
pub(crate) fn expected(case: &Case, seed: u8, step: usize) -> Result<Vec<Entry>> {
    if step > case.tier {
        return Err("history expected step outside prefix".into());
    }
    let genesis = fixture(case, seed)?;
    let mut entries = genesis.clone();
    if step == 0 {
        return Ok(entries);
    }
    if case.kind == "unrelated" {
        for j in 0..200 {
            let path = d::shard_path(j);
            let entry = entries
                .iter_mut()
                .find(|e| e.path == path)
                .ok_or("history path")?;
            let EntryKind::File(c) = &entry.kind else {
                return Err("history file type".into());
            };
            entry.kind = EntryKind::File(d::content(
                FAMILY,
                "unrelated",
                seed,
                200 * (step - 1) + j,
                "bytes",
                c.len(),
            )?);
        }
    } else if case.kind == "metadata" {
        entries
            .iter_mut()
            .find(|e| e.path == d::shard_path(0))
            .ok_or("metadata path")?
            .mode = if step % 2 == 1 { 0o600 } else { 0o640 };
    } else {
        for k in 0..step {
            let change = d::history_edit(case, seed, k, &genesis)?;
            let entry = entries
                .iter_mut()
                .find(|e| e.path == change.path)
                .ok_or("edit target")?;
            let EntryKind::File(old) = &entry.kind else {
                return Err("edit file type".into());
            };
            entry.kind = EntryKind::File(old.splice(
                change.start,
                change.delete_len,
                Content::Literal(change.replacement),
            )?);
        }
    }
    Ok(entries)
}
pub(crate) fn self_check() -> Result<()> {
    d::check_registry(&cases(), 20)?;
    for case in cases() {
        if d::total(&fixture(&case, 1)?) != d::MIB
            || (case.tier as u64 + 1) * d::MIB >= 1_073_741_824
        {
            return Err("history size bound".into());
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
