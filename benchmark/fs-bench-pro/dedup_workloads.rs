use super::workspace_common::{Case, Content, Entry, EntryKind, SdkEdit};
use super::{Result, Sha256};
use std::collections::BTreeSet;

pub(crate) const MIB: u64 = 1_048_576;
pub(crate) const TIERS: [usize; 4] = [1, 10, 100, 500];

pub(crate) fn cases(family: &'static str, profiles: &[(&'static str, &str)]) -> Vec<Case> {
    profiles
        .iter()
        .flat_map(|&(kind, prefix)| {
            TIERS.map(|tier| Case {
                id: format!("{prefix}-{tier}"),
                family,
                tier,
                kind,
            })
        })
        .collect()
}

pub(crate) fn seed_label(seed: u8) -> Result<String> {
    if !(1..=3).contains(&seed) {
        return Err("dedup seed must be 1, 2 or 3".into());
    }
    Ok(format!("layerfs-v0.1.3-seed-{seed}"))
}

fn frame(hash: &mut Sha256, value: &str) {
    hash.update(&(value.len() as u64).to_le_bytes());
    hash.update(value.as_bytes());
}

pub(crate) fn hash(
    family: &str,
    profile: &str,
    seed: u8,
    ordinal: usize,
    role: &str,
) -> Result<[u8; 32]> {
    let mut hash = Sha256::new();
    for value in ["dedup-input-v1", family, profile, &seed_label(seed)?] {
        frame(&mut hash, value);
    }
    hash.update(&(ordinal as u64).to_le_bytes());
    frame(&mut hash, role);
    Ok(hash.finish())
}

pub(crate) fn content(
    family: &str,
    profile: &str,
    seed: u8,
    ordinal: usize,
    role: &str,
    len: u64,
) -> Result<Content> {
    Ok(Content::Seed {
        seed: u64::from_le_bytes(hash(family, profile, seed, ordinal, role)?[..8].try_into()?),
        len,
    })
}

fn bytes(content: &Content) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    content.write_to(&mut result)?;
    Ok(result)
}

pub(crate) fn offset(family: &str, profile: &str, seed: u8, ordinal: usize) -> Result<u64> {
    Ok(65536
        + u64::from_le_bytes(hash(family, profile, seed, ordinal, "offset")?[..8].try_into()?)
            % 917505)
}

pub(crate) fn variant(
    family: &str,
    profile: &str,
    seed: u8,
    i: usize,
    base: Content,
) -> Result<Content> {
    let at = offset(family, profile, seed, i)?;
    match profile {
        "overwrite" | "local" => {
            let mask = bytes(&content(family, profile, seed, i, "mask", 64)?)?;
            let mut replacement = bytes(&base.slice(at, 64)?)?;
            for (value, mask) in replacement.iter_mut().zip(mask) {
                *value ^= 1 + mask % 255;
            }
            base.splice(at, 64, Content::Literal(replacement))
        }
        "insert" => base.splice(at, 0, content(family, profile, seed, i, "insertion", 4096)?),
        "delete" => base.splice(at, 4096, Content::Literal(Vec::new())),
        "common-body" => Ok(Content::Concat(vec![
            content(family, profile, seed, i, "prefix", 131072)?,
            base.slice(131072, 786432)?,
            content(family, profile, seed, i, "suffix", 131072)?,
        ])),
        "scattered" => {
            let position =
                u64::from_le_bytes(hash(family, profile, seed, i, "position")?[..8].try_into()?)
                    % 4096;
            let mask = bytes(&content(family, profile, seed, i, "mask", 256)?)?;
            let mut parts = Vec::with_capacity(513);
            let mut previous = 0;
            for (block, mask) in mask.into_iter().enumerate() {
                let at = block as u64 * 4096 + position;
                parts.push(base.slice(previous, at - previous)?);
                let value = bytes(&base.slice(at, 1)?)?[0] ^ (1 + mask % 255);
                parts.push(Content::Literal(vec![value]));
                previous = at + 1;
            }
            parts.push(base.slice(previous, base.len() - previous)?);
            Ok(Content::Concat(parts))
        }
        _ => Err("unknown dedup variant".into()),
    }
}

pub(crate) fn directories(names: &[&str]) -> Vec<Entry> {
    std::iter::once(Entry::directory("."))
        .chain(names.iter().map(|name| Entry::directory(*name)))
        .collect()
}

pub(crate) fn validate(case: &Case, family: &str, seed: u8) -> Result<()> {
    seed_label(seed)?;
    if case.family != family || !TIERS.contains(&case.tier) {
        return Err("dedup case family/tier".into());
    }
    Ok(())
}

pub(crate) fn total(entries: &[Entry]) -> u64 {
    entries
        .iter()
        .map(|entry| match &entry.kind {
            EntryKind::File(c) => c.len(),
            _ => 0,
        })
        .sum()
}

pub(crate) fn history_rank(
    seed: u8,
    profile: &str,
    start: usize,
    count: usize,
) -> Result<Vec<usize>> {
    let seed = seed_label(seed)?;
    let domain = format!("dedup-history-{profile}");
    let mut ranked: Vec<_> = (start..start + count)
        .map(|i| {
            let mut h = Sha256::new();
            frame(&mut h, &seed);
            frame(&mut h, &domain);
            h.update(&(i as u64).to_le_bytes());
            (h.finish(), i)
        })
        .collect();
    ranked.sort();
    Ok(ranked.into_iter().map(|(_, i)| i).collect())
}

pub(crate) fn shard_path(j: usize) -> String {
    if j < 64 {
        format!("wide/s000-f{j:03}.dat")
    } else if j < 199 {
        format!("regular/s000/f{j:03}.dat")
    } else {
        format!(
            "spine/{}/s000.dat",
            (1..=128)
                .map(|i| format!("d{i:03}"))
                .collect::<Vec<_>>()
                .join("/")
        )
    }
}

pub(crate) fn history_edit(
    case: &Case,
    seed: u8,
    step: usize,
    genesis: &[Entry],
) -> Result<SdkEdit> {
    if step >= case.tier {
        return Err("history step outside prefix".into());
    }
    let ordinal = match case.kind {
        "distributed" => history_rank(seed, case.kind, 0, 200)?[step % 200],
        "hotset" => history_rank(seed, case.kind, 192, 8)?[step % 8],
        "recurring" => 192,
        _ => return Err("history profile is not an SDK edit".into()),
    };
    let path = shard_path(ordinal);
    let base = genesis
        .iter()
        .find_map(|e| {
            if e.path == path {
                if let EntryKind::File(c) = &e.kind {
                    Some(c)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .ok_or("missing history target")?;
    let recurring = case.kind == "recurring";
    let replacement = if recurring && step % 2 == 1 {
        bytes(base)?
    } else {
        bytes(&content(
            case.family,
            case.kind,
            seed,
            if recurring { 0 } else { step },
            if recurring { "B" } else { "replacement" },
            if recurring { 49152 } else { 64 },
        )?)?
    };
    Ok(SdkEdit {
        path,
        start: if recurring { 0 } else { base.len() / 2 - 32 },
        delete_len: if recurring { 49152 } else { 64 },
        replacement,
    })
}

pub(crate) fn check_registry(cases: &[Case], count: usize) -> Result<()> {
    if cases.len() != count || cases.iter().map(|c| &c.id).collect::<BTreeSet<_>>().len() != count {
        return Err("dedup registry cardinality".into());
    }
    Ok(())
}

pub(crate) fn is_sdk(case: &Case) -> bool {
    case.family == "dedup_branch_history"
        && matches!(case.kind, "distributed" | "hotset" | "recurring")
}
pub(crate) fn sdk_edits(case: &Case, seed: u8, step: usize) -> Result<Vec<SdkEdit>> {
    if !is_sdk(case) {
        return Err("dedup case is not SDK".into());
    }
    Ok(vec![super::dedup_branch_history::edit(case, seed, step)?])
}

struct AcknowledgedWrite<'a, W>(&'a mut W, &'a mut u64);
impl<W: std::io::Write> std::io::Write for AcknowledgedWrite<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = self.0.write(bytes)?;
        *self.1 += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

pub(crate) fn apply(
    case: &Case,
    seed: u8,
    step: usize,
    _verify: bool,
) -> Result<super::workspace_common::Receipt> {
    use super::workspace_common::{self as common, Receipt};
    use std::fs::{File, OpenOptions};
    use std::path::Path;
    use std::time::Instant;
    if is_sdk(case) {
        return Err("SDK history edits must use the public host SDK route".into());
    }
    if matches!(case.family, "dedup_cross_file" | "dedup_cdc_locality") {
        return Err(
            "dedup imports must use public initialize_layerstack, not a filesystem apply".into(),
        );
    }
    let (files, mut directories, create) = match case.family {
        "dedup_workspace_reuse" => {
            if step != 0 {
                return Err("reuse has one workload step".into());
            }
            (
                super::dedup_workspace_reuse::additions(case, seed)?,
                vec![Entry::directory("added"), Entry::directory(".")],
                true,
            )
        }
        "dedup_branch_history" if matches!(case.kind, "metadata" | "unrelated") => {
            if step >= case.tier {
                return Err("history workload step".into());
            }
            let entries = super::dedup_branch_history::expected(case, seed, step + 1)?;
            let dirs = entries
                .iter()
                .filter(|e| case.kind == "unrelated" && matches!(e.kind, EntryKind::Directory))
                .cloned()
                .collect::<Vec<_>>();
            let files = entries
                .into_iter()
                .filter(|e| {
                    matches!(e.kind, EntryKind::File(_))
                        && (case.kind == "unrelated" || e.path == shard_path(0))
                })
                .collect();
            (files, dirs, false)
        }
        _ => return Err("unsupported dedup native workload".into()),
    };
    directories.sort_by(|a, b| {
        b.path
            .matches('/')
            .count()
            .cmp(&a.path.matches('/').count())
            .then_with(|| b.path.cmp(&a.path))
    });
    let started = Instant::now();
    let mut sync_ns = 0u128;
    let mut writes = 0u64;
    let mut attempted = 0u64;
    let mut completed = 0u64;
    let mut sync_attempts = 0u64;
    let mut syncs = 0u64;
    let mut directory_attempts = 0u64;
    let mut directory_completed = 0u64;
    let mut phase = "file-open";
    let mut path_index = 0;
    let result = (|| -> Result<()> {
        for (index, entry) in files.iter().enumerate() {
            path_index = index;
            attempted += 1;
            phase = "file-open";
            if case.kind == "metadata" {
                use std::os::unix::fs::PermissionsExt;
                phase = "file-chmod";
                std::fs::set_permissions(&entry.path, std::fs::Permissions::from_mode(entry.mode))?;
                phase = "file-open";
                let file = File::open(&entry.path)?;
                phase = "file-sync";
                sync_attempts += 1;
                let sync = Instant::now();
                let result = file.sync_all();
                sync_ns += sync.elapsed().as_nanos();
                result?;
                syncs += 1;
            } else {
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(create)
                    .truncate(!create)
                    .open(&entry.path)?;
                let EntryKind::File(content) = &entry.kind else {
                    return Err("native dedup file kind".into());
                };
                phase = "file-write";
                content.write_to(&mut AcknowledgedWrite(&mut file, &mut writes))?;
                phase = "file-metadata";
                common::set_metadata(Path::new(&entry.path), entry)?;
                phase = "file-sync";
                sync_attempts += 1;
                let sync = Instant::now();
                let result = file.sync_all();
                sync_ns += sync.elapsed().as_nanos();
                result?;
                syncs += 1;
            }
            completed += 1;
        }
        for (index, entry) in directories.iter().enumerate() {
            path_index = index;
            directory_attempts += 1;
            phase = "directory-metadata";
            common::set_metadata(Path::new(&entry.path), entry)?;
            phase = "directory-open";
            let file = File::open(&entry.path)?;
            phase = "directory-sync";
            sync_attempts += 1;
            let sync = Instant::now();
            let result = file.sync_all();
            sync_ns += sync.elapsed().as_nanos();
            result?;
            syncs += 1;
            directory_completed += 1;
        }
        Ok(())
    })();
    let inner_workload_ns = started.elapsed().as_nanos();
    let mut receipt = Receipt::new();
    receipt.insert("scenario_id".into(), case.id.clone());
    for (key, value) in [
        ("seed", seed as u128),
        ("benchmark_injection_count", 0),
        ("benchmark_reopen_count", 0),
        ("benchmark_verifier_count", 0),
        ("inner_workload_ns", inner_workload_ns),
        ("sync_ns", sync_ns),
        ("attempted_operations", attempted as u128),
        ("completed_operations", completed as u128),
        ("successful_write_bytes", writes as u128),
        (
            "file_write_count",
            if case.kind == "metadata" {
                0
            } else {
                completed as u128
            },
        ),
        ("attempted_sync_count", sync_attempts as u128),
        ("sync_count", syncs as u128),
        (
            "attempted_directory_operation_count",
            directory_attempts as u128,
        ),
        (
            "completed_directory_operation_count",
            directory_completed as u128,
        ),
    ] {
        receipt.insert(key.into(), value.to_string());
    }
    if let Err(error) = result {
        for (key, value) in &receipt {
            eprintln!("partial_{key}={value}");
        }
        let entries = if phase.starts_with("directory-") {
            &directories
        } else {
            &files
        };
        eprintln!("partial_failure_phase={phase}");
        if let Some(entry) = entries.get(path_index) {
            eprintln!("partial_failure_path={}", entry.path);
        }
        return Err(error);
    }
    Ok(receipt)
}

pub(crate) fn self_check() -> Result<()> {
    super::dedup_cross_file::self_check()?;
    super::dedup_cdc_locality::self_check()?;
    super::dedup_workspace_reuse::self_check()?;
    super::dedup_branch_history::self_check()?;
    for seed in 1..=3 {
        for profile in ["overwrite", "insert", "delete", "common-body", "scattered"] {
            for i in 0..500 {
                if !(65536..=983040).contains(&offset("dedup_cdc_locality", profile, seed, i)?) {
                    return Err("dedup offset bound".into());
                }
            }
        }
        for (profile, start, count) in [("distributed", 0, 200), ("hotset", 192, 8)] {
            if history_rank(seed, profile, start, count)?
                .into_iter()
                .collect::<BTreeSet<_>>()
                != (start..start + count).collect()
            {
                return Err("history ranking not permutation".into());
            }
        }
    }
    if seed_label(0).is_ok() || seed_label(4).is_ok() {
        return Err("invalid seed accepted".into());
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn acknowledged_bytes_survive_a_later_write_failure() {
    use std::io::{self, Write};
    struct Short(usize);
    impl Write for Short {
        fn write(&mut self, input: &[u8]) -> io::Result<usize> {
            if self.0 == 2 {
                return Err(io::Error::other("injected later write failure"));
            }
            self.0 += 1;
            Ok(input.len().min(3))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut target = Short(0);
    let mut acknowledged = 0;
    assert!(AcknowledgedWrite(&mut target, &mut acknowledged)
        .write_all(b"abcdefgh")
        .is_err());
    assert_eq!(acknowledged, 6);
}
