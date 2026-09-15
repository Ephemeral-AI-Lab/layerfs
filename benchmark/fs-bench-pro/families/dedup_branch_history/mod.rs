use super::dedup_workloads as d;
use super::v016_compact::{self as s};
use super::v016_hn as hn;
use super::workspace_common::{self, Case, Content, Entry, EntryKind, SdkEdit};
use super::Result;
pub(crate) const FAMILY: &str = "dedup_branch_history";

/// The three v0.1.6 history profiles, each at K10 and K100. They extend the
/// family with profiles the inherited depth-10/100 cases do not cover instead
/// of renaming an existing small-hotset or distributed scenario.
pub(crate) const LARGE_HOTSET_K10: &str = "v016-history-large-hotset-k10-v1";
pub(crate) const LARGE_HOTSET_K100: &str = "v016-history-large-hotset-k100-v1";
pub(crate) const NAMESPACE_INODE_K10: &str = "v016-history-namespace-inode-k10-v1";
pub(crate) const NAMESPACE_INODE_K100: &str = "v016-history-namespace-inode-k100-v1";
pub(crate) const BOUNDARY_CYCLE_K10: &str = "v016-history-boundary-cycle-k10-v1";
pub(crate) const BOUNDARY_CYCLE_K100: &str = "v016-history-boundary-cycle-k100-v1";

/// The v0.1.6 additions, in registration order.
pub(crate) const V016: [(&str, &str, usize); 6] = [
    (LARGE_HOTSET_K10, "large-hotset", 10),
    (LARGE_HOTSET_K100, "large-hotset", 100),
    (NAMESPACE_INODE_K10, "namespace-inode", 10),
    (NAMESPACE_INODE_K100, "namespace-inode", 100),
    (BOUNDARY_CYCLE_K10, "boundary-cycle", 10),
    (BOUNDARY_CYCLE_K100, "boundary-cycle", 100),
];

pub(crate) fn v016_case(id: &str) -> Option<(&'static str, usize)> {
    V016.iter()
        .find(|(case, _, _)| *case == id)
        .map(|(_, kind, tier)| (*kind, *tier))
}

pub(crate) fn cases() -> Vec<Case> {
    let mut rows = d::cases(
        FAMILY,
        &[
            ("distributed", "dedup-history-distributed"),
            ("hotset", "dedup-history-hotset"),
            ("recurring", "dedup-history-recurring"),
            ("metadata", "dedup-history-metadata"),
            ("unrelated", "dedup-history-unrelated"),
        ],
    );
    for row in &mut rows {
        if row.kind == "unrelated" && matches!(row.tier, 100 | 500) {
            row.id.push_str("-mixed-v2");
        }
    }
    rows.extend(V016.into_iter().map(|(id, kind, tier)| Case {
        id: id.to_owned(),
        family: FAMILY,
        tier,
        kind,
    }));
    rows
}
/// Routine proof coverage; workload depth and all-parent checks are unchanged.
pub(crate) fn verification_steps(case: &Case) -> Vec<usize> {
    let n = case.tier;
    if n <= 10 {
        return (0..=n).collect();
    }
    let mut steps = match case.kind {
        "distributed" if n > 200 => vec![0, 1, 199, 200, 201, n / 2, n - 1, n],
        "hotset" => vec![0, 1, 7, 8, 9, n / 2, n - 1, n],
        "recurring" => vec![0, 1, 2, 3, n - 1, n],
        // The namespace-inode schedule adds the declared access states: commit
        // n-6 publishes the stage-4 alias (two names, one inode) and commit n-5
        // is the atomic save that replaces the destination and removes the
        // alias. Both are verified with their own complete namespace proof.
        "namespace-inode" => vec![0, 1, 2, n / 2 - 1, n / 2, n - 6, n - 5, n - 1, n],
        _ => vec![0, 1, 2, n / 2 - 1, n / 2, n - 1, n],
    };
    steps.sort_unstable();
    steps.dedup();
    steps
}

pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    d::validate(case, FAMILY, seed)?;
    if let Some((kind, _)) = v016_case(&case.id) {
        return s::fixture(s::profile_of(kind)?, kind, seed);
    }
    if d::history_unrelated_mixed_v2(case) {
        d::mixed_v2_entries(seed)
    } else {
        workspace_common::shards(seed, 1, "")
    }
}
pub(crate) fn edit(case: &Case, seed: u8, step: usize) -> Result<SdkEdit> {
    let mut rows = edits(case, seed, step)?;
    if rows.len() != 1 {
        return Err("history edit cardinality".into());
    }
    Ok(rows.remove(0))
}

/// Every declared SDK call of one commit, in order. The inherited profiles make
/// exactly one call per commit; the v0.1.6 boundary-cycle profile makes two.
pub(crate) fn edits(case: &Case, seed: u8, step: usize) -> Result<Vec<SdkEdit>> {
    if let Some((kind, _)) = v016_case(&case.id) {
        return v016_edits(case, kind, seed, step);
    }
    Ok(vec![d::history_edit(case, seed, step, &fixture(case, seed)?)?])
}

/// The declared SDK calls of one v0.1.6 history commit. Both profiles derive
/// every byte from the fixture recipe and the declared visit arithmetic, so a
/// commit is a pure function of its ordinal and never of observed state.
fn v016_edits(case: &Case, kind: &str, seed: u8, step: usize) -> Result<Vec<SdkEdit>> {
    if step >= case.tier {
        return Err("history step outside prefix".into());
    }
    match kind {
        "large-hotset" => {
            // Commit j (1-based) selects file (j-1) mod 2 and region
            // floor((j-1)/2) mod 3, and alternates B/A by the visit count to
            // that exact (file,region); the fixture initialized it to A.
            let file = step % 2;
            let region = (step / 2) % 3;
            let visits = (0..step)
                .filter(|t| t % 2 == file && (t / 2) % 3 == region)
                .count();
            let label = if visits % 2 == 0 {
                d::HOTSET_B
            } else {
                d::HOTSET_A
            };
            let replacement = v016_bytes(
                case,
                kind,
                seed,
                file * 3 + region,
                label,
                d::HOTSET_REGION_LEN,
            )?;
            Ok(vec![SdkEdit {
                path: s::s_large_path(file),
                start: s::hotset_offset(region),
                delete_len: d::HOTSET_REGION_LEN,
                replacement,
            }])
        }
        "boundary-cycle" => {
            // Two single-file SDK calls per commit exchange one byte between the
            // 131,071 B and 131,073 B files, shrinking first and reversing on the
            // next commit. The exchange moves the higher file's last byte *down*
            // into the lower one, so the pair oscillates between
            // (131,071, 131,073) and (131,072, 131,072): every other commit puts
            // both names exactly on the 128 KiB boundary, which is the transition
            // the profile exists to cycle and the state the two
            // `v016-access-boundary-{before,after}-v1` consumers read at commits
            // 48 and 49 (`benchmark-families.md` §historical_access, and
            // `cases.json` `declared_full_read_bytes` 131071 / 131072).
            let below = s::s_boundary_path("below");
            let above = s::s_boundary_path("above");
            let byte = *s::bytes_of(&fixture(case, seed)?, &above)?
                .last()
                .ok_or("boundary-cycle source byte")?;
            // The declared length of each name before this commit follows from
            // the exchange arithmetic: an even commit leaves the pair at its
            // initial sizes, an odd commit leaves them one byte exchanged.
            let (shrink, grow, shrink_len, grow_len) = if step % 2 == 0 {
                // First call shrinks the higher file onto the exact boundary,
                // second call grows the lower file onto it.
                (above, below, s::ABOVE_LEN, s::BELOW_LEN)
            } else {
                // The next commit reverses the exchange: the lower file gives
                // the byte back to the higher one.
                (below, above, s::BELOW_LEN + 1, s::ABOVE_LEN - 1)
            };
            Ok(vec![
                SdkEdit {
                    path: shrink,
                    start: shrink_len - 1,
                    delete_len: 1,
                    replacement: Vec::new(),
                },
                SdkEdit {
                    path: grow,
                    start: grow_len,
                    delete_len: 0,
                    replacement: vec![byte],
                },
            ])
        }
        // HN is a five-stage schedule, not a pure SDK history: four of its five
        // stages are POSIX helper executions the host orchestrator launches, and
        // only stage 2 is the two ordered single-file SDK calls below.
        "namespace-inode" => {
            let (cycle, stage) = hn::stage_of(step + 1)?;
            Ok(if stage == hn::EDITS {
                hn::sdk_edits(seed, cycle)?
            } else {
                Vec::new()
            })
        }
        other => Err(format!("unknown v0.1.6 history profile {other}").into()),
    }
}

fn v016_bytes(
    case: &Case,
    kind: &str,
    seed: u8,
    ordinal: usize,
    role: &str,
    len: u64,
) -> Result<Vec<u8>> {
    let content = d::content(case.family, kind, seed, ordinal, role, len)?;
    let mut out = Vec::with_capacity(len as usize);
    content.write_to(&mut out)?;
    if out.len() as u64 != len {
        return Err("v0.1.6 history replacement length".into());
    }
    Ok(out)
}

/// Apply one declared SDK call to a literal file map.
fn apply_edit(entries: &mut [Entry], edit: &SdkEdit) -> Result<()> {
    let entry = entries
        .iter_mut()
        .find(|entry| entry.path == edit.path)
        .ok_or("history edit target")?;
    let EntryKind::File(content) = &entry.kind else {
        return Err("history edit type".into());
    };
    let mut bytes = Vec::with_capacity(content.len() as usize);
    content.write_to(&mut bytes)?;
    let start = edit.start as usize;
    let end = start
        .checked_add(edit.delete_len as usize)
        .ok_or("history edit overflow")?;
    if end > bytes.len() {
        return Err("history edit bounds".into());
    }
    bytes.splice(start..end, edit.replacement.iter().copied());
    entry.kind = EntryKind::File(Content::Literal(bytes));
    Ok(())
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
    if v016_case(&case.id).is_some_and(|(kind, _)| kind == "namespace-inode") {
        // The HN schedule is not a sequence of SDK edits: its states follow from
        // the declared five-stage algebra (unlink/recreate, SDK pair, directory
        // move, alias plus attributes, atomic save), so the oracle derives them
        // from the declaration rather than replaying a byte-splice transcript.
        return hn::expected(seed, step);
    }
    if v016_case(&case.id).is_some() {
        // The v0.1.6 profiles replay their declared SDK calls over literal
        // bytes, so every state is an exact declaration.
        for ordinal in 0..step {
            for edit in edits(case, seed, ordinal)? {
                apply_edit(&mut entries, &edit)?;
            }
        }
        return Ok(entries);
    }
    if case.kind == "unrelated" {
        let mixed = d::history_unrelated_mixed_v2(case);
        let files = if mixed { d::MIXED_V2_FILES } else { 200 };
        for j in 0..files {
            let path = if mixed {
                d::mixed_v2_path(j)?
            } else {
                d::shard_path(j)
            };
            let entry = entries
                .iter_mut()
                .find(|e| e.path == path)
                .ok_or("history path")?;
            let EntryKind::File(c) = &entry.kind else {
                return Err("history file type".into());
            };
            entry.kind = EntryKind::File(if mixed {
                d::mixed_v2_rewrite(seed, &path, j, step - 1, c.len())?
            } else {
                d::content(
                    FAMILY,
                    "unrelated",
                    seed,
                    200 * (step - 1) + j,
                    "bytes",
                    c.len(),
                )?
            });
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
            // History edits touch at most a 48 KiB file. Keep the independent
            // oracle flat: nested Slice/Concat recipes repeatedly validate the
            // same ancestry and grow exponentially under hot-set/A-B rewrites.
            let mut bytes = Vec::with_capacity(old.len() as usize);
            old.write_to(&mut bytes)?;
            let start = change.start as usize;
            let end = start.checked_add(change.delete_len as usize).ok_or("history edit overflow")?;
            if end > bytes.len() {
                return Err("history edit bounds".into());
            }
            bytes.splice(start..end, change.replacement);
            entry.kind = EntryKind::File(Content::Literal(bytes));
        }
    }
    Ok(entries)
}
pub(crate) fn self_check() -> Result<()> {
    d::check_registry(&cases(), 26)?;
    for case in cases() {
        if let Some((kind, tier)) = v016_case(&case.id) {
            // The v0.1.6 profiles use fixture S: sixteen declared files and
            // 2,658,304 declared bytes, independent of the requested depth.
            if case.tier != tier || v016_envelope(&case, kind)? != 16 {
                return Err("history v0.1.6 profile bound".into());
            }
            if kind != "namespace-inode" {
                v016_check(&case, kind)?;
            }
            continue;
        }
        if d::total(&fixture(&case, 1)?) != d::MIB
            || (case.tier as u64 + 1) * d::MIB >= 1_073_741_824
        {
            return Err("history size bound".into());
        }
    }
    v016_profiles_check()?;
    hn::self_check()?;
    mixed_v2_check()
}

/// The declared fixture-S envelope of one v0.1.6 profile: sixteen regular
/// files, 2,658,304 declared bytes and five directories excluding root.
fn v016_envelope(case: &Case, kind: &str) -> Result<u64> {
    let entries = fixture(case, 1)?;
    let inventory = s::declared_inventory();
    let files = entries
        .iter()
        .filter(|entry| matches!(entry.kind, EntryKind::File(_)))
        .count();
    if files != inventory.len()
        || workspace_common::validate_entries(&entries)? != s::S_BYTES
        || entries
            .iter()
            .filter(|entry| matches!(entry.kind, EntryKind::Directory))
            .count()
            != 6
    {
        return Err(format!("{kind} fixture S envelope").into());
    }
    for (path, len) in &inventory {
        let entry = entries
            .iter()
            .find(|entry| entry.path == *path)
            .ok_or("fixture S inventory path")?;
        let EntryKind::File(content) = &entry.kind else {
            return Err("fixture S inventory type".into());
        };
        if content.len() != *len {
            return Err(format!("fixture S inventory length {path}").into());
        }
    }
    Ok(files as u64)
}

/// The declared schedule of one flat v0.1.6 profile: every commit is Created
/// because each edit changes bytes against the state it is applied to, and K10
/// is exactly the first ten commits of K100 for the same profile and seed.
fn v016_check(case: &Case, kind: &str) -> Result<()> {
    for seed in 1..=3u8 {
        let mut previous = fixture(case, seed)?;
        for step in 0..case.tier {
            let mut next = previous.clone();
            for edit in edits(case, seed, step)? {
                apply_edit(&mut next, &edit)?;
            }
            if format!("{next:?}") == format!("{previous:?}") {
                return Err(format!("{kind} step {step} is a no-op").into());
            }
            previous = next;
        }
        if format!("{previous:?}") != format!("{:?}", expected(case, seed, case.tier)?) {
            return Err(format!("{kind} declared replay differs from the oracle").into());
        }
        if kind == "boundary-cycle" && case.tier >= 49 {
            // The two declared `historical_access` consumers read commit 48 and
            // commit 49 of this producer: one byte below the 128 KiB boundary and
            // the same name exactly on it. The exchange therefore has to carry
            // the pair across the boundary, not away from it.
            let length = |entries: &[Entry], path: &str| -> Result<u64> {
                let entry = entries
                    .iter()
                    .find(|entry| entry.path == path)
                    .ok_or("boundary-cycle consumer path")?;
                let EntryKind::File(content) = &entry.kind else {
                    return Err("boundary-cycle consumer kind".into());
                };
                Ok(content.len())
            };
            let below = s::s_boundary_path("below");
            let above = s::s_boundary_path("above");
            let before = expected(case, seed, 48)?;
            let after = expected(case, seed, 49)?;
            if length(&before, &below)? != s::BELOW_LEN
                || length(&before, &above)? != s::ABOVE_LEN
                || length(&after, &below)? != s::EXACT_LEN
                || length(&after, &above)? != s::EXACT_LEN
            {
                return Err(format!("boundary-cycle consumer states at seed {seed}").into());
            }
            for step in [48usize, 49] {
                let state = expected(case, seed, step)?;
                if length(&state, &s::s_boundary_path("exact"))? != s::EXACT_LEN {
                    return Err("boundary-cycle exact control unchanged".into());
                }
            }
        }
        if let Some(counterpart) = match case.id.as_str() {
            LARGE_HOTSET_K10 => Some(LARGE_HOTSET_K100),
            BOUNDARY_CYCLE_K10 => Some(BOUNDARY_CYCLE_K100),
            _ => None,
        } {
            let other = cases()
                .into_iter()
                .find(|row| row.id == counterpart)
                .ok_or("v0.1.6 history counterpart")?;
            for step in 0..=10 {
                if format!("{:?}", expected(case, seed, step)?)
                    != format!("{:?}", expected(&other, seed, step)?)
                {
                    return Err(format!("{kind} K10 prefix of K100 at step {step}").into());
                }
            }
        }
    }
    Ok(())
}

/// The namespace-inode profile is registered but its five-stage compact
/// schedule is not implemented yet: report it instead of pretending to run it.
fn v016_profiles_check() -> Result<()> {
    for (id, kind, tier) in V016 {
        let case = Case {
            id: id.to_owned(),
            family: FAMILY,
            tier,
            kind,
        };
        if v016_case(&case.id) != Some((kind, tier))
            || s::profile_of(kind).is_err()
            || cases().iter().filter(|row| row.id == id).count() != 1
        {
            return Err("v0.1.6 history registration".into());
        }
    }
    Ok(())
}

fn mixed_v2_check() -> Result<()> {
    use std::collections::BTreeMap;
    let rows = cases();
    if rows.len() != 26
        || rows
            .iter()
            .filter(|row| row.id.ends_with("-mixed-v2"))
            .count()
            != 2
        || rows
            .iter()
            .any(|row| row.kind != "unrelated" && row.id.ends_with("-mixed-v2"))
        || rows.iter().any(|row| {
            row.kind == "unrelated"
                && matches!(row.tier, 1 | 10)
                && (row.id.ends_with("-mixed-v2") || d::history_unrelated_mixed_v2(row))
        })
    {
        return Err("history-unrelated-mixed-v2 membership".into());
    }
    if d::mixed_v2_peak_bytes() >= workspace_common::mixed_v4_container_bound() {
        return Err("history-unrelated-mixed-v2 peak bytes exceed 2 GiB container".into());
    }
    let wanted = BTreeMap::from([
        (d::MIXED_V2_LARGE, 1usize),
        (d::MIXED_V2_SMALL, 6),
        (d::MIXED_V2_MEDIUM, 3),
    ]);
    let one_hundred = rows
        .iter()
        .find(|row| row.id == "dedup-history-unrelated-100-mixed-v2")
        .ok_or("history-unrelated-mixed-v2 100")?;
    let five_hundred = rows
        .iter()
        .find(|row| row.id == "dedup-history-unrelated-500-mixed-v2")
        .ok_or("history-unrelated-mixed-v2 500")?;
    for seed in 1..=3 {
        let genesis = fixture(one_hundred, seed)?;
        if format!("{genesis:?}") != format!("{:?}", fixture(five_hundred, seed)?) {
            return Err("history-unrelated-mixed-v2 shared genesis".into());
        }
        let mut sizes = BTreeMap::<u64, usize>::new();
        let mut files = 0;
        for entry in &genesis {
            if let EntryKind::File(content) = &entry.kind {
                files += 1;
                *sizes.entry(content.len()).or_default() += 1;
            }
        }
        if files != d::MIXED_V2_FILES
            || d::total(&genesis) != d::MIB
            || sizes != wanted
            || workspace_common::validate_entries(&genesis)? != d::MIB
        {
            return Err(format!("history-unrelated-mixed-v2 distribution seed {seed}").into());
        }
        for ordinal in 0..d::MIXED_V2_FILES {
            let path = d::mixed_v2_path(ordinal)?;
            let EntryKind::File(content) = &genesis
                .iter()
                .find(|entry| entry.path == path)
                .ok_or("history-unrelated-mixed-v2 path")?
                .kind
            else {
                return Err("history-unrelated-mixed-v2 file kind".into());
            };
            if content.len() != d::mixed_v2_len(ordinal)? {
                return Err("history-unrelated-mixed-v2 ordinal assignment".into());
            }
        }
        let shard = workspace_common::shards(seed, 1, "")?;
        let mixed_files: BTreeMap<_, _> = genesis
            .iter()
            .filter(|entry| matches!(entry.kind, EntryKind::File(_)))
            .map(|entry| (entry.path.as_str(), entry))
            .collect();
        if mixed_files
            .keys()
            .any(|path| shard.iter().any(|entry| entry.path == *path && matches!(entry.kind, EntryKind::File(_))))
        {
            return Err("history-unrelated-mixed-v2 path collided with 200-file shard".into());
        }
        for step in 0..=100 {
            if format!("{:?}", expected(one_hundred, seed, step)?)
                != format!("{:?}", expected(five_hundred, seed, step)?)
            {
                return Err("history-unrelated-mixed-v2 prefix 100 subset 500".into());
            }
        }
        let sample = d::history_sample(one_hundred, seed)?;
        sample.validate()?;
        let large_path = d::mixed_v2_path(0)?;
        let small_first = d::mixed_v2_path(1)?;
        let small_last = d::mixed_v2_path(6)?;
        let medium_first = d::mixed_v2_path(7)?;
        let medium_last = d::mixed_v2_path(9)?;
        if sample.ranges.len() != 1
            || sample.ranges.get(&large_path).map(Vec::len) != Some(3)
            || !sample.entries.iter().any(|entry| entry.path == large_path)
            || !sample.entries.iter().any(|entry| entry.path == small_first)
            || !sample.entries.iter().any(|entry| entry.path == small_last)
            || !sample.entries.iter().any(|entry| entry.path == medium_first)
            || !sample.entries.iter().any(|entry| entry.path == medium_last)
        {
            return Err("history-unrelated-mixed-v2 sampled large/small/medium coverage".into());
        }
        let final_entries = expected(one_hundred, seed, 100)?;
        for entry in &sample.entries {
            if matches!(entry.kind, EntryKind::File(_))
                && !final_entries
                    .iter()
                    .any(|other| format!("{entry:?}") == format!("{other:?}"))
            {
                return Err("history-unrelated-mixed-v2 sample recipe".into());
            }
        }
        let unrelated_one = rows
            .iter()
            .find(|row| row.id == "dedup-history-unrelated-1")
            .ok_or("unrelated-1")?;
        if fixture(unrelated_one, seed)?
            .iter()
            .filter(|entry| matches!(entry.kind, EntryKind::File(_)))
            .count()
            != 200
        {
            return Err("unrelated-1/10 must keep the 200-file shard".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn mixed_v2_contract() {
        super::self_check().unwrap();
    }
}

pub(crate) fn apply(
    case: &Case,
    seed: u8,
    step: usize,
    verify: bool,
) -> Result<super::workspace_common::Receipt> {
    d::apply(case, seed, step, verify)
}

#[cfg(test)]
mod checkpoint_tests {
    #[test]
    fn flat_history_oracle_matches_small_recipe_and_handles_deep_rewrites() {
        use super::*;
        for case in cases().into_iter().filter(|case| case.tier == 500 && matches!(case.kind, "distributed" | "hotset" | "recurring")) {
            let genesis = fixture(&case, 1).unwrap();
            let mut recursive = genesis.clone();
            for step in 0..8 {
                let edit = d::history_edit(&case, 1, step, &genesis).unwrap();
                let target = recursive.iter_mut().find(|entry| entry.path == edit.path).unwrap();
                let EntryKind::File(content) = &target.kind else { panic!("file") };
                target.kind = EntryKind::File(content.splice(edit.start, edit.delete_len, Content::Literal(edit.replacement)).unwrap());
            }
            let flat = expected(&case, 1, 8).unwrap();
            for (old, new) in recursive.iter().zip(&flat) {
                if let (EntryKind::File(a), EntryKind::File(b)) = (&old.kind, &new.kind) {
                    assert_eq!(a.digest().unwrap(), b.digest().unwrap());
                }
            }
            let deep = expected(&case, 1, 500).unwrap();
            assert_eq!(workspace_common::validate_entries(&deep).unwrap(), d::MIB);
            assert!(deep.iter().all(|entry| !matches!(&entry.kind, EntryKind::File(Content::Concat(_)))));
        }
    }

    #[test]
    fn history_samples_cover_cycles_and_final_states() {
        for case in super::cases() {
            let steps = super::verification_steps(&case);
            assert_eq!(steps.first(), Some(&0));
            assert_eq!(steps.last(), Some(&case.tier));
            assert!(steps.windows(2).all(|pair| pair[0] < pair[1]));
            if case.tier > 10 {
                if case.kind == "namespace-inode" {
                    // The HN selection adds the producer's own declared access
                    // states: commit 94 publishes the stage-4 alias and commit
                    // 95 is the atomic save that replaces the destination and
                    // removes the alias.
                    assert_eq!(steps.len(), 9);
                    assert!(steps.contains(&94) && steps.contains(&95));
                } else {
                    assert!((6..=8).contains(&steps.len()));
                }
                assert!(steps.contains(&(case.tier - 1)));
                if case.kind == "hotset" { assert!(steps.contains(&8) && steps.contains(&9)); }
                if case.kind == "distributed" && case.tier > 200 {
                    assert!(steps.contains(&199) && steps.contains(&200) && steps.contains(&201));
                }
            }
        }
    }
}
