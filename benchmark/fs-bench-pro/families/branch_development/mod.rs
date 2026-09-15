use super::v016_compact as s;
use super::v016_stages::{self as stages, CompactCase, CompactRole};
use super::workspace_common::{Case, Entry, EntryKind, Receipt};
use super::Result;

pub(crate) const FAMILY_ID: &str = "branch_development";

pub(crate) fn cases() -> Vec<Case> {
    let mut rows = super::v016_stages::family_case_ids(super::v016_stages::Topology::Branch)
        .into_iter()
        .map(|id| Case {
            id: id.to_string(),
            family: FAMILY_ID,
            tier: if id.contains("-k100-") { 100 } else { 10 },
            kind: "v016-m1-branch",
        })
        .collect::<Vec<_>>();
    // The two compact controls are fixture-S graph controls: each of their
    // branches makes ten local commits, so the display tier is 10 for both while
    // the declared totals are 30 and 40 commits.
    rows.extend(stages::COMPACT_IDS.into_iter().map(|id| Case {
        id: id.to_owned(),
        family: FAMILY_ID,
        tier: 10,
        kind: "v016-compact-control",
    }));
    rows
}

pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    if stages::compact_case(&case.id)?.is_some() {
        return s::fixture(s::SProfile::BranchControl, stages::COMPACT_PROFILE, seed);
    }
    super::v016_common::fixture_for_case(&case.id, seed)
}

/// The declared state of one compact branch after `local` of its own commits:
/// fixture S with the first medium file's two declared regions rewritten by the
/// branch's ancestral sequence (its inherited prefix, then its own commits).
pub(crate) fn compact_expected_at(
    seed: u8,
    compact: CompactCase,
    role: CompactRole,
    local: usize,
) -> Result<Vec<Entry>> {
    let mut entries = s::fixture(s::SProfile::BranchControl, stages::COMPACT_PROFILE, seed)?;
    let path = s::s_medium_path(0);
    let slot = entries
        .iter()
        .position(|entry| entry.path == path)
        .ok_or("v0.1.6 compact control target path")?;
    let EntryKind::File(base) = &entries[slot].kind else {
        return Err("v0.1.6 compact control target is not a regular file".into());
    };
    let mut content = base.clone();
    let mut sequence: Vec<(usize, &str)> = Vec::new();
    for ancestral in 1..=compact.fork_ancestry(role) {
        sequence.push((ancestral, ""));
    }
    let salt = stages::compact_salt(compact, role);
    for step in stages::compact_steps(compact)
        .iter()
        .filter(|row| row.role == role)
        .take(local)
    {
        sequence.push((step.ancestral, salt));
    }
    for (ancestral, salt) in sequence {
        content = content.splice(
            stages::compact_offset(ancestral)?,
            super::dedup_workloads::BRANCH_REGION_LEN,
            stages::compact_payload(FAMILY_ID, seed, ancestral, salt)?,
        )?;
    }
    entries[slot].kind = EntryKind::File(content);
    Ok(entries)
}

/// The declared state of one compact branch head.
pub(crate) fn compact_expected(
    seed: u8,
    compact: CompactCase,
    role: CompactRole,
) -> Result<Vec<Entry>> {
    let depth = if role == CompactRole::Trunk {
        stages::COMPACT_TRUNK_COMMITS
    } else {
        stages::COMPACT_CHILD_COMMITS
    };
    compact_expected_at(seed, compact, role, depth)
}

/// The declared content digest of one branch's first medium file.
pub(crate) fn compact_content_digest(
    seed: u8,
    compact: CompactCase,
    role: CompactRole,
) -> Result<String> {
    let entries = compact_expected(seed, compact, role)?;
    let path = s::s_medium_path(0);
    let entry = entries
        .iter()
        .find(|entry| entry.path == path)
        .ok_or("v0.1.6 compact control target path")?;
    let EntryKind::File(content) = &entry.kind else {
        return Err("v0.1.6 compact control target kind".into());
    };
    content.digest()
}

pub(crate) fn self_check() -> Result<()> {
    let rows = cases();
    if rows.len() != 6 {
        return Err("branch_development must register six cases".into());
    }
    let mut compact_rows = Vec::new();
    for case in &rows {
        if stages::compact_case(&case.id)?.is_some() {
            compact_rows.push(case.id.clone());
            continue;
        }
        if stages::mixed_case(&case.id)?.is_none() {
            return Err(format!("{} is not a registered M1 case", case.id).into());
        }
    }
    if compact_rows.len() != 2 {
        return Err("branch_development must register both compact controls".into());
    }
    for seed in 1..=3u8 {
        // Every compact commit must change the bytes it is applied to, and the
        // prefix relationship must hold: the trunk's state after five commits is
        // the state both children inherit.
        for id in stages::COMPACT_IDS {
            let compact = stages::compact_case(id)?.ok_or("v0.1.6 compact control")?;
            let fixture = fixture(
                &Case {
                    id: id.to_owned(),
                    family: FAMILY_ID,
                    tier: 10,
                    kind: "v016-compact-control",
                },
                seed,
            )?;
            let fixture_digest = content_digest(&fixture)?;
            let mut digests = Vec::new();
            for role in compact.roles() {
                let digest = compact_content_digest(seed, compact, role)?;
                let expected_commits = if role == CompactRole::Trunk {
                    stages::COMPACT_TRUNK_COMMITS
                } else {
                    stages::COMPACT_CHILD_COMMITS
                };
                let steps = stages::compact_steps(compact)
                    .iter()
                    .filter(|row| row.role == role)
                    .count();
                if expected_commits != steps {
                    return Err("v0.1.6 compact branch depth".into());
                }
                if digest == fixture_digest {
                    return Err(format!("v0.1.6 compact branch {id} changed nothing").into());
                }
                digests.push((role, digest));
            }
            // Convergent children must have equal file-content IDs; the
            // descendant control must salt B and C by branch.
            let digest_of = |role: CompactRole| {
                digests
                    .iter()
                    .find(|(candidate, _)| *candidate == role)
                    .map(|(_, digest)| digest.clone())
                    .ok_or("v0.1.6 compact branch digest")
            };
            let a = digest_of(CompactRole::A)?;
            let b = digest_of(CompactRole::B)?;
            if compact.descendant {
                let c = digest_of(CompactRole::C)?;
                if a == b || a == c || b == c {
                    return Err("v0.1.6 descendant control must salt B/C by branch".into());
                }
            } else if a != b {
                return Err("v0.1.6 convergent children must share file-content IDs".into());
            }
        }
    }
    Ok(())
}

/// The declared content digest of one state's first medium file.
fn content_digest(entries: &[Entry]) -> Result<String> {
    let path = s::s_medium_path(0);
    let entry = entries
        .iter()
        .find(|entry| entry.path == path)
        .ok_or("v0.1.6 compact control target path")?;
    let EntryKind::File(content) = &entry.kind else {
        return Err("v0.1.6 compact control target kind".into());
    };
    content.digest()
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
