// v0.1.6 boundary/size-transition family.
//
// Every case is one branch/one workspace with a fixed number of public SDK
// `edit_workspace_file_range` commits. The oracle is derived from the fixture
// recipe and the declared operation, never from the mutated Store.
use super::v016_common as v016;
use super::workspace_common::{Case, Content, Entry, EntryKind, Receipt};
use super::{dedup_workloads as d, sdk_edit_common, Result};

pub(crate) const FAMILY_ID: &str = "file_size_transition";

pub(crate) const SMALL_CONTROL: &str = "v016-boundary-small-control-v1";
pub(crate) const BELOW: &str = "v016-boundary-below-v1";
pub(crate) const EXACT: &str = "v016-boundary-exact-v1";
pub(crate) const ABOVE: &str = "v016-boundary-above-v1";
pub(crate) const LARGE_CONTROL: &str = "v016-boundary-large-control-v1";
pub(crate) const ROUNDTRIP: &str = "v016-boundary-roundtrip-v1";
pub(crate) const ALIAS_ROUNDTRIP: &str = "v016-boundary-alias-roundtrip-v1";

pub(crate) const TARGET: &str = "data/target.bin";
pub(crate) const WITNESS: &str = "data/witness.bin";
pub(crate) const ALIAS: &str = "data/alias.bin";
pub(crate) const TEMPORARY: &str = "data/.target.bin.tmp";

/// The explicit metadata every declared boundary file carries. The alias plan
/// writes through POSIX names, which would otherwise publish a runtime mtime,
/// and it creates a temporary file whose creation mode would otherwise be a
/// runtime default; both are set to these declared values so the final state is
/// a declaration rather than an observation.
pub(crate) const TARGET_MODE: u32 = 0o640;
pub(crate) const DECLARED_MTIME_SECONDS: i64 = super::workspace_common::MTIME;
pub(crate) const DECLARED_MTIME_NANOSECONDS: u32 = 0;

/// Fixed-overwrite offsets are 0 then 2048, identical across sizes.
pub(crate) const OVERWRITE_OFFSETS: [u64; 2] = [0, 2_048];
pub(crate) const OVERWRITE_LEN: u64 = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BoundaryPlan {
    /// Initial target length.
    pub(crate) initial_len: u64,
    /// Number of public Commits, i.e. one SDK operation each.
    pub(crate) commits: usize,
    pub(crate) alias: bool,
}

pub(crate) fn plan(id: &str) -> Result<BoundaryPlan> {
    let row = match id {
        SMALL_CONTROL => BoundaryPlan { initial_len: 4_096, commits: 2, alias: false },
        BELOW => BoundaryPlan { initial_len: v016::BOUNDARY_BELOW, commits: 2, alias: false },
        EXACT => BoundaryPlan { initial_len: v016::BOUNDARY_EXACT, commits: 2, alias: false },
        ABOVE => BoundaryPlan { initial_len: v016::BOUNDARY_ABOVE, commits: 2, alias: false },
        LARGE_CONTROL => BoundaryPlan { initial_len: v016::MIB, commits: 2, alias: false },
        ROUNDTRIP => BoundaryPlan { initial_len: v016::BOUNDARY_BELOW, commits: 4, alias: false },
        ALIAS_ROUNDTRIP => BoundaryPlan { initial_len: v016::BOUNDARY_BELOW, commits: 5, alias: true },
        other => return Err(format!("unknown {FAMILY_ID} case {other}").into()),
    };
    Ok(row)
}

pub(crate) fn cases() -> Vec<Case> {
    [SMALL_CONTROL, BELOW, EXACT, ABOVE, LARGE_CONTROL, ROUNDTRIP, ALIAS_ROUNDTRIP]
        .into_iter()
        .map(|id| Case { id: id.to_string(), family: FAMILY_ID, tier: 1, kind: "boundary" })
        .collect()
}

pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    let plan = plan(&case.id)?;
    let mut entries = vec![Entry::directory("."), Entry::directory("data")];
    let mut target = Entry::file(
        TARGET,
        d::content(FAMILY_ID, "target", seed, 0, "boundary-target", plan.initial_len)?,
    );
    target.mode = TARGET_MODE;
    target.mtime_seconds = DECLARED_MTIME_SECONDS;
    target.mtime_nanoseconds = DECLARED_MTIME_NANOSECONDS;
    entries.push(target);
    entries.push(Entry::file(
        WITNESS,
        d::content(FAMILY_ID, "witness", seed, 0, "boundary-witness", v016::WITNESS)?,
    ));
    if plan.alias {
        entries.push(Entry::hardlink(ALIAS, TARGET));
    }
    Ok(entries)
}

/// The declared range sequence. SDK-only plans exchange bytes in place;
/// roundtrip plans change length; the alias plan mixes POSIX alias operations
/// with one final same-length atomic replacement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BoundaryOp {
    Overwrite { offset: u64, len: u64, visit: usize },
    Append { len: u64, visit: usize },
    Remove { len: u64, visit: usize },
    /// POSIX append/truncate through the alias, then a same-length atomic
    /// replace of the target.
    AliasAppend { len: u64 },
    AliasTruncate { len: u64 },
    AtomicReplace { len: u64 },
}

pub(crate) fn operations(case: &Case) -> Result<Vec<BoundaryOp>> {
    let plan = plan(&case.id)?;
    let mut rows = Vec::with_capacity(plan.commits);
    if plan.alias {
        rows.push(BoundaryOp::AliasAppend { len: 1 });
        rows.push(BoundaryOp::AliasAppend { len: 1 });
        rows.push(BoundaryOp::AliasTruncate { len: 1 });
        rows.push(BoundaryOp::AliasTruncate { len: 1 });
        rows.push(BoundaryOp::AtomicReplace {
            len: v016::BOUNDARY_EXACT,
        });
        return Ok(rows);
    }
    if plan.initial_len == v016::BOUNDARY_BELOW && plan.commits == 4 {
        rows.push(BoundaryOp::Append { len: 1, visit: 0 });
        rows.push(BoundaryOp::Append { len: 1, visit: 1 });
        rows.push(BoundaryOp::Remove { len: 1, visit: 0 });
        rows.push(BoundaryOp::Remove { len: 1, visit: 1 });
        return Ok(rows);
    }
    for index in 0..plan.commits {
        rows.push(BoundaryOp::Overwrite {
            offset: OVERWRITE_OFFSETS[index],
            len: OVERWRITE_LEN,
            visit: index,
        });
    }
    Ok(rows)
}

/// Replacement bytes come from an independent generator, never from the target.
pub(crate) fn replacement(case: &Case, seed: u8, visit: usize, len: u64) -> Result<Vec<u8>> {
    let bytes = d::content(
        FAMILY_ID,
        "replacement",
        seed,
        visit,
        &case.id,
        len,
    )?;
    let mut out = Vec::with_capacity(len as usize);
    bytes.write_to(&mut out)?;
    Ok(out)
}

/// The bytes of the alias plan's final atomic replacement. The recipe profile
/// and the ordinal are part of the declaration, so the host oracle and the
/// workload helper generate exactly the same replacement from it.
pub(crate) fn atomic_replacement(
    case: &Case,
    seed: u8,
    ordinal: usize,
    len: u64,
) -> Result<Vec<u8>> {
    let bytes = d::content(FAMILY_ID, "atomic-replace", seed, ordinal, &case.id, len)?;
    let mut out = Vec::with_capacity(len as usize);
    bytes.write_to(&mut out)?;
    Ok(out)
}

/// True when the declared pre-replacement state still holds the alias and the
/// target as the same inode class, which the host alias proof re-checks against
/// the independently derived recipe.
pub(crate) fn pre_replacement_shared(case: &Case, seed: u8) -> Result<bool> {
    let plan = plan(&case.id)?;
    if !plan.alias {
        return Ok(true);
    }
    let operations = operations(case)?;
    let before = expected(case, seed, operations.len() - 1)?;
    Ok([TARGET, ALIAS]
        .iter()
        .all(|path| before.iter().any(|entry| entry.path == *path)))
}

/// The independent oracle: the declared length sequence and the resulting
/// content for every retained state, derived from the fixture recipe.
pub(crate) fn expected(case: &Case, seed: u8, step: usize) -> Result<Vec<Entry>> {
    let plan = plan(&case.id)?;
    let operations = operations(case)?;
    if step > operations.len() {
        return Err("boundary expected step outside the declared schedule".into());
    }
    let mut entries = fixture(case, seed)?;
    let mut previous_bytes: Option<Vec<u8>> = None;
    let mut bytes = {
        let mut out = Vec::new();
        d::content(FAMILY_ID, "target", seed, 0, "boundary-target", plan.initial_len)?
            .write_to(&mut out)?;
        out
    };
    for (index, operation) in operations.iter().take(step).enumerate() {
        match operation {
            BoundaryOp::Overwrite { offset, len, visit } => {
                let replacement = replacement(case, seed, *visit, *len)?;
                let start = *offset as usize;
                let end = start + *len as usize;
                if end > bytes.len() {
                    return Err("boundary overwrite bounds".into());
                }
                bytes[start..end].copy_from_slice(&replacement);
            }
            BoundaryOp::Append { len, visit } => {
                let mut tail = replacement(case, seed, *visit, *len)?;
                bytes.append(&mut tail);
            }
            // One real removal from the end of the file.
            BoundaryOp::Remove { len, visit } => {
                let _ = visit;
                let keep = bytes.len() - *len as usize;
                bytes.truncate(keep);
            }
            BoundaryOp::AliasAppend { len } => {
                let mut tail = replacement(case, seed, index, *len)?;
                bytes.append(&mut tail);
            }
            BoundaryOp::AliasTruncate { len } => {
                let _ = index;
                let keep = bytes.len() - *len as usize;
                bytes.truncate(keep);
            }
            BoundaryOp::AtomicReplace { len } => {
                let out = atomic_replacement(case, seed, index, *len)?;
                previous_bytes = Some(bytes);
                bytes = out;
            }
        }
    }
    let length = bytes.len() as u64;
    // The atomic replacement replaces only the target name. When the plan has
    // an alias, the pre-replacement inode survives under that name: the alias
    // keeps the previous bytes and the target becomes a separate inode with the
    // new content, which is what the final inode classes record.
    if plan.alias {
        // Uniform representation for the verifier: the alias is always a
        // regular file, holding the shared content before the replacement and
        // the surviving pre-replacement inode afterwards.
        let content = previous_bytes.unwrap_or_else(|| bytes.clone());
        let alias = entries
            .iter_mut()
            .find(|entry| entry.path == ALIAS)
            .ok_or("boundary alias entry")?;
        alias.kind = EntryKind::File(Content::Literal(content));
    }
    let target = entries
        .iter_mut()
        .find(|entry| entry.path == TARGET)
        .ok_or("boundary target entry")?;
    target.kind = EntryKind::File(Content::Literal(bytes.clone()));
    let witness = entries
        .iter()
        .find(|entry| entry.path == WITNESS)
        .ok_or("boundary witness entry")?;
    match &witness.kind {
        EntryKind::File(content) if content.len() == v016::WITNESS => {}
        _ => return Err("boundary witness changed".into()),
    }
    if length == 0 {
        return Err("boundary target became empty".into());
    }
    Ok(entries)
}

/// The target length immediately before `step`, derived from the declared
/// recipe rather than from the live Store.
pub(crate) fn declared_length(case: &Case, step: usize) -> Result<u64> {
    let plan = plan(&case.id)?;
    let mut length = plan.initial_len;
    for operation in operations(case)?.iter().take(step) {
        match operation {
            BoundaryOp::Overwrite { .. } => {}
            BoundaryOp::Append { len, .. } | BoundaryOp::AliasAppend { len } => {
                length += *len;
            }
            BoundaryOp::Remove { len, .. } | BoundaryOp::AliasTruncate { len } => {
                length -= *len;
            }
            BoundaryOp::AtomicReplace { len } => length = *len,
        }
    }
    Ok(length)
}

/// Verifier shape for one case. The alias plan ends with the target and the
/// alias in two different inode classes, so the generic oracle verifies the
/// declared classes (target as a replacement-class file, alias as the surviving
/// pre-replacement class) and the host adds the explicit separation proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VerifierShape {
    /// Declared paths whose inode class splits from their declared pair.
    pub(crate) split_classes: bool,
}

pub(crate) fn verifier_shape(case: &Case) -> Result<VerifierShape> {
    Ok(VerifierShape { split_classes: plan(&case.id)?.alias })
}

/// Boundary cases run entirely in the host SDK orchestrator: the workload
/// helper has no POSIX stage except the alias plan, which is driven from the
/// host too. `apply` therefore never executes.
pub(crate) fn apply(case: &Case, _seed: u8, _step: usize, _verify: bool) -> Result<Receipt> {
    Err(format!(
        "v0.1.6 {} runs through the host SDK orchestrator",
        case.id
    )
    .into())
}

pub(crate) fn self_check() -> Result<()> {
    let rows = cases();
    if rows.len() != 7 {
        return Err("file_size_transition must register seven cases".into());
    }
    let ids: std::collections::BTreeSet<&str> = rows.iter().map(|case| case.id.as_str()).collect();
    if ids.len() != 7 {
        return Err("file_size_transition case IDs must be unique".into());
    }
    for case in &rows {
        let plan = plan(&case.id)?;
        let operations = operations(case)?;
        if operations.len() != plan.commits {
            return Err(format!("{} operation count", case.id).into());
        }
        if !plan.alias && plan.commits == 2 {
            let offsets: Vec<u64> = operations
                .iter()
                .map(|operation| match operation {
                    BoundaryOp::Overwrite { offset, .. } => *offset,
                    _ => u64::MAX,
                })
                .collect();
            if offsets != OVERWRITE_OFFSETS {
                return Err(format!("{} fixed-overwrite offsets", case.id).into());
            }
        }
        // The declared length sequence, checked against an independent model.
        let expected_lengths: Vec<u64> = match case.id.as_str() {
            ROUNDTRIP => vec![131_071, 131_072, 131_073, 131_072, 131_071],
            ALIAS_ROUNDTRIP => vec![131_071, 131_072, 131_073, 131_072, 131_071, 131_072],
            SMALL_CONTROL => vec![4_096, 4_096, 4_096],
            BELOW => vec![131_071, 131_071, 131_071],
            EXACT => vec![131_072, 131_072, 131_072],
            ABOVE => vec![131_073, 131_073, 131_073],
            LARGE_CONTROL => vec![1_048_576, 1_048_576, 1_048_576],
            other => return Err(format!("unregistered boundary case {other}").into()),
        };
        if expected_lengths.len() != plan.commits + 1 {
            return Err(format!("{} declared length sequence", case.id).into());
        }
        for (seed, _) in [(1u8, ()), (2, ()), (3, ())] {
            for (step, length) in expected_lengths.iter().enumerate() {
                let entries = expected(case, seed, step)?;
                let target = entries
                    .iter()
                    .find(|entry| entry.path == TARGET)
                    .ok_or("boundary target")?;
                let observed = match &target.kind {
                    EntryKind::File(content) => content.len(),
                    _ => return Err("boundary target kind".into()),
                };
                if observed != *length {
                    return Err(format!(
                        "{} seed {seed} step {step}: {observed} != {length}",
                        case.id
                    )
                    .into());
                }
            }
        }
    }
    // The alias plan preserves the original alias name and separates the two
    // inode classes after the final atomic replacement.
    let alias = cases()
        .into_iter()
        .find(|case| case.id == ALIAS_ROUNDTRIP)
        .ok_or("alias roundtrip case")?;
    let operations = operations(&alias)?;
    let before = expected(&alias, 1, operations.len() - 1)?;
    let after = expected(&alias, 1, operations.len())?;
    let _ = (&before, &after);
    let length_of = |entries: &[Entry], path: &str| -> Result<u64> {
        match &entries
            .iter()
            .find(|entry| entry.path == path)
            .ok_or("alias roundtrip path")?
            .kind
        {
            EntryKind::File(content) => Ok(content.len()),
            _ => Err("alias roundtrip kind".into()),
        }
    };
    if !after.iter().any(|entry| entry.path == ALIAS)
        || after
            .iter()
            .filter(|entry| matches!(entry.kind, EntryKind::File(_)))
            .count()
            != 3
    {
        return Err("alias roundtrip final path count".into());
    }
    // Before the replacement the alias shares the target inode; afterwards the
    // two classes hold different lengths.
    // Before the replacement the alias shares the target inode, so the two
    // names agree; afterwards the surviving alias keeps the pre-replacement
    // bytes and the target holds the new content.
    let previous = length_of(&before, TARGET)?;
    let replaced = length_of(&after, TARGET)?;
    if length_of(&before, ALIAS)? != previous
        || length_of(&after, ALIAS)? != previous
        || replaced == previous
    {
        return Err(format!(
            "alias roundtrip must separate the replaced inode: before {}/{}, after {}/{}",
            previous,
            length_of(&before, ALIAS)?,
            replaced,
            length_of(&after, ALIAS)?
        )
        .into());
    }
    let _ = sdk_edit_common::sha256_hex(b"v016-boundary-self-check");
    Ok(())
}

#[cfg(test)]
mod tests {






    #[test]
    fn boundary_contract() {
        super::self_check().unwrap();
    }
}
