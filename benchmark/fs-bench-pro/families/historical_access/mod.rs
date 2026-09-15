// v0.1.6 `historical_access`: six verify-only cases that mount one selected
// retained state of a sealed producer.
//
// `benchmark-families.md` §"historical_access: six additions" declares each case
// by producer identity, retained graph size and selected ordinal. Nothing here
// builds a history: the producer is sealed once during fixture preparation
// (`infra-seal-producer`), and an invocation mounts exactly one retained state
// with one live workspace, creates no commits and reports the reader profile.
//
// The expected bytes of every read come from the *producer's* own declared
// algebra (`dedup_branch_history::expected` or the compact control's ancestral
// sequence), recomputed here from the seed and the selected ordinal. That is the
// original derivation the access verifier compares against; the product never
// reports the value under test.
use super::dedup_branch_history as history;
use super::v016_compact as s;
use super::v016_stages::{self as stages, CompactRole};
use super::workspace_common::{Case, Entry, EntryKind, Receipt};
use super::Result;

pub(crate) const FAMILY: &str = "historical_access";

pub(crate) const BOUNDARY_BEFORE: &str = "v016-access-boundary-before-v1";
pub(crate) const BOUNDARY_AFTER: &str = "v016-access-boundary-after-v1";
pub(crate) const INODE_BEFORE: &str = "v016-access-inode-before-v1";
pub(crate) const INODE_AFTER: &str = "v016-access-inode-after-v1";
pub(crate) const FORK_POINT: &str = "v016-access-fork-point-v1";
pub(crate) const DIVERGENT_HEAD: &str = "v016-access-divergent-head-v1";

/// The producer branch an access case selects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProducerBranch {
    /// The sealed master's own branch, i.e. the single-branch history producers.
    Main,
    Trunk,
    B,
}

impl ProducerBranch {
    pub(crate) fn role(self) -> CompactRole {
        match self {
            Self::Trunk => CompactRole::Trunk,
            Self::B => CompactRole::B,
            Self::Main => CompactRole::Trunk,
        }
    }
    pub(crate) fn of(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Trunk => "trunk",
            Self::B => "B",
        }
    }
}

/// The declared read plan of one access case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReadPlan {
    /// Full read of the boundary producer's below-role file.
    BoundaryBelow,
    /// Full reads of the inode producer's replacement target and its alias, with
    /// stat/nlink equivalence.
    InodePair,
    /// Full read of the replacement target while the alias is gone.
    InodeTarget,
    /// Full read of the first medium file.
    MediumFull,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AccessCase {
    pub(crate) id: &'static str,
    pub(crate) producer: &'static str,
    pub(crate) ordinal: usize,
    pub(crate) roots: usize,
    pub(crate) branch: ProducerBranch,
    pub(crate) plan: ReadPlan,
    pub(crate) declared_bytes: u64,
}

pub(crate) const CASES: [AccessCase; 6] = [
    AccessCase {
        id: BOUNDARY_BEFORE,
        producer: history::BOUNDARY_CYCLE_K100,
        ordinal: 48,
        roots: 101,
        branch: ProducerBranch::Main,
        plan: ReadPlan::BoundaryBelow,
        declared_bytes: 131_071,
    },
    AccessCase {
        id: BOUNDARY_AFTER,
        producer: history::BOUNDARY_CYCLE_K100,
        ordinal: 49,
        roots: 101,
        branch: ProducerBranch::Main,
        plan: ReadPlan::BoundaryBelow,
        declared_bytes: 131_072,
    },
    AccessCase {
        id: INODE_BEFORE,
        producer: history::NAMESPACE_INODE_K100,
        ordinal: 94,
        roots: 101,
        branch: ProducerBranch::Main,
        plan: ReadPlan::InodePair,
        declared_bytes: 8_192,
    },
    AccessCase {
        id: INODE_AFTER,
        producer: history::NAMESPACE_INODE_K100,
        ordinal: 95,
        roots: 101,
        branch: ProducerBranch::Main,
        plan: ReadPlan::InodeTarget,
        declared_bytes: 4_096,
    },
    AccessCase {
        id: FORK_POINT,
        producer: stages::COMPACT_CONVERGENT,
        ordinal: 5,
        roots: 31,
        branch: ProducerBranch::Trunk,
        plan: ReadPlan::MediumFull,
        declared_bytes: 65_536,
    },
    AccessCase {
        id: DIVERGENT_HEAD,
        producer: stages::COMPACT_DESCENDANT,
        ordinal: 10,
        roots: 41,
        branch: ProducerBranch::B,
        plan: ReadPlan::MediumFull,
        declared_bytes: 65_536,
    },
];

/// The declared default reader profile of every addition.
pub(crate) const READER_PROFILE: &str =
    "fresh-session-application-cold;uncontrolled-os-cache;no-pre-read";

pub(crate) fn access_case(id: &str) -> Result<Option<AccessCase>> {
    Ok(CASES.iter().copied().find(|row| row.id == id))
}

/// The registry rows of the family.
pub(crate) fn cases() -> Vec<Case> {
    CASES
        .iter()
        .map(|row| Case {
            id: row.id.to_owned(),
            family: FAMILY,
            // The selection is an ordinal inside the producer's retained graph,
            // so the display tier is the producer's own regular tier.
            tier: 100,
            kind: "v016-access",
        })
        .collect()
}

/// The producer's registered case.
pub(crate) fn producer_case(access: AccessCase) -> Result<Case> {
    super::workspace_registry::resolve(access.producer)
}

/// The producer's genesis fixture: what the sealed master starts from.
pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    let access = access_case(&case.id)?.ok_or_else(|| format!("unknown historical_access case: {}", case.id))?;
    let producer = producer_case(access)?;
    super::workspace_registry::fixture(&producer, seed)
}

/// The declared state of the selected retained commit, derived from the
/// producer's own declaration.
pub(crate) fn declared_state(access: AccessCase, seed: u8) -> Result<Vec<Entry>> {
    let producer = producer_case(access)?;
    if producer.family == history::FAMILY {
        return history::expected(&producer, seed, access.ordinal);
    }
    let compact = stages::compact_case(&producer.id)?
        .ok_or_else(|| format!("historical_access producer is not a compact control: {}", producer.id))?;
    if access.branch == ProducerBranch::Main {
        return Err("historical_access compact producers select a named branch".into());
    }
    super::branch_development::compact_expected_at(seed, compact, access.branch.role(), access.ordinal)
}

/// A declared read plan: the paths with their declared lengths, and the names
/// the case declares absent.
pub(crate) type DeclaredPlan = (Vec<(String, u64)>, Vec<String>);

/// The declared read plan without any content derivation: the paths, their
/// declared lengths and the declared absences. The container-side reader uses
/// this; the host oracle uses `read_targets`, and the self-check proves the two
/// agree.
pub(crate) fn read_plan(access: AccessCase) -> Result<DeclaredPlan> {
    let rows = match access.plan {
        ReadPlan::BoundaryBelow => vec![(
            s::s_boundary_path("below"),
            if access.ordinal % 2 == 0 {
                s::BELOW_LEN
            } else {
                s::EXACT_LEN
            },
        )],
        ReadPlan::InodePair => {
            let dir = history_dir(access)?;
            vec![
                (format!("{dir}/{}", super::v016_hn::REPLACEMENT), s::TINY_LEN),
                (format!("{dir}/{}", super::v016_hn::ALIAS), s::TINY_LEN),
            ]
        }
        ReadPlan::InodeTarget => {
            let dir = history_dir(access)?;
            vec![(format!("{dir}/{}", super::v016_hn::REPLACEMENT), s::TINY_LEN)]
        }
        ReadPlan::MediumFull => vec![(s::s_medium_path(0), s::MEDIUM_LEN)],
    };
    let absent = match access.plan {
        ReadPlan::InodeTarget => {
            let dir = history_dir(access)?;
            vec![format!("{dir}/{}", super::v016_hn::ALIAS)]
        }
        _ => Vec::new(),
    };
    Ok((rows, absent))
}

/// The declared reader profile of one access invocation, reported with every
/// result.
pub(crate) fn reader_profile() -> &'static str {
    READER_PROFILE
}

/// `v016-access-read CASE SEED PHASE` — one declared read pass inside the live
/// mount. The helper never sees the expected values: it reports what the mount
/// returned, plus the inode facts the case declares, and the host compares all
/// of it with its own derivation.
pub(crate) fn run_command(args: &[String]) -> Result<()> {
    let [case, seed, phase] = args else {
        return Err(format!(
            "usage: v016-access-read CASE SEED reader|verifier (received {} arguments: {args:?})",
            args.len()
        )
        .into());
    };
    if !matches!(phase.as_str(), "reader" | "verifier") {
        return Err("v0.1.6 access phase must be reader or verifier".into());
    }
    let seed: u8 = seed.parse()?;
    super::dedup_workloads::seed_label(seed)?;
    let access = access_case(case)?.ok_or_else(|| format!("unknown access case: {case}"))?;
    let (targets, absent) = read_plan(access)?;
    println!("access_case={}", access.id);
    println!("access_phase={phase}");
    println!("access_reader_profile={READER_PROFILE}");
    println!("access_pre_read=0");
    println!("access_commits=0");
    for (path, declared) in &targets {
        let mut file = std::fs::File::open(path)
            .map_err(|error| format!("v0.1.6 access read {path}: {error}"))?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut bytes)?;
        drop(file);
        let metadata = std::fs::symlink_metadata(path)?;
        let ino = std::os::unix::fs::MetadataExt::ino(&metadata);
        let dev = std::os::unix::fs::MetadataExt::dev(&metadata);
        let nlink = std::os::unix::fs::MetadataExt::nlink(&metadata);
        println!("access_target={path}");
        println!("access_declared_bytes={declared}");
        println!("access_bytes={}", bytes.len());
        println!(
            "access_sha256={}",
            super::sdk_edit_common::sha256_hex(&bytes)
        );
        println!("access_ino={ino}");
        println!("access_dev={dev}");
        println!("access_nlink={nlink}");
        println!(
            "access_payload_hex={}",
            bytes.iter().map(|byte| format!("{byte:02x}")).collect::<String>()
        );
    }
    for path in &absent {
        match std::fs::symlink_metadata(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                println!("access_absent={path}");
            }
            Ok(_) => {
                return Err(format!("v0.1.6 access declared absent path is present: {path}").into())
            }
            Err(error) => {
                return Err(format!("v0.1.6 access absence probe {path}: {error}").into())
            }
        }
    }
    println!("access_status=pass");
    Ok(())
}

/// The declared bytes of one read target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReadTarget {
    pub(crate) path: String,
    pub(crate) len: u64,
    pub(crate) content: Vec<u8>,
}

/// The declared read targets of one access case, with the bytes the independent
/// derivation requires. Alias absence is declared separately.
pub(crate) fn read_targets(access: AccessCase, seed: u8) -> Result<(Vec<ReadTarget>, Vec<String>)> {
    let entries = declared_state(access, seed)?;
    let bytes_of = |path: &str| -> Result<Vec<u8>> {
        let entry = entries
            .iter()
            .find(|entry| entry.path == path)
            .ok_or_else(|| format!("historical_access declared path absent: {path}"))?;
        let content = match &entry.kind {
            EntryKind::File(content) => content,
            EntryKind::Hardlink(target) => {
                let referent = entries
                    .iter()
                    .find(|entry| entry.path == *target)
                    .ok_or("historical_access alias target absent")?;
                match &referent.kind {
                    EntryKind::File(content) => content,
                    _ => return Err("historical_access alias target kind".into()),
                }
            }
            _ => return Err("historical_access read target is not a file".into()),
        };
        let mut out = Vec::with_capacity(content.len() as usize);
        content.write_to(&mut out)?;
        Ok(out)
    };
    let target = |path: String| -> Result<ReadTarget> {
        let content = bytes_of(&path)?;
        Ok(ReadTarget {
            path,
            len: content.len() as u64,
            content,
        })
    };
    let rows = match access.plan {
        ReadPlan::BoundaryBelow => vec![target(s::s_boundary_path("below"))?],
        ReadPlan::InodePair => {
            let dir = history_dir(access)?;
            vec![
                target(format!("{dir}/{}", super::v016_hn::REPLACEMENT))?,
                target(format!("{dir}/{}", super::v016_hn::ALIAS))?,
            ]
        }
        ReadPlan::InodeTarget => {
            let dir = history_dir(access)?;
            vec![target(format!("{dir}/{}", super::v016_hn::REPLACEMENT))?]
        }
        ReadPlan::MediumFull => vec![target(s::s_medium_path(0))?],
    };
    let absent = match access.plan {
        ReadPlan::InodeTarget => {
            let dir = history_dir(access)?;
            vec![format!("{dir}/{}", super::v016_hn::ALIAS)]
        }
        _ => Vec::new(),
    };
    let total: u64 = rows.iter().map(|row| row.len).sum();
    if total != access.declared_bytes {
        return Err(format!(
            "historical_access declared read bytes {total} != {} for {}",
            access.declared_bytes, access.id
        )
        .into());
    }
    Ok((rows, absent))
}

/// The movable directory of the inode producer at the selected ordinal: stage 3
/// is the rename, so stages 1 and 2 still resolve under the previous name.
fn history_dir(access: AccessCase) -> Result<&'static str> {
    let (cycle, stage) = super::v016_hn::stage_of(access.ordinal)?;
    Ok(if stage >= super::v016_hn::DIRECTORIES {
        super::v016_hn::dir_after(cycle)
    } else {
        super::v016_hn::dir_before(cycle)
    })
}

/// The declared branch name of a producer that publishes named branches. `None`
/// selects the sealed master's own branch.
pub(crate) fn branch_name(access: AccessCase) -> Option<String> {
    match access.branch {
        ProducerBranch::Main => None,
        role => Some(stages::compact_branch_name(access.producer, role.role())),
    }
}

/// The declared number of local commits the selected producer branch holds. The
/// history producers publish one branch of `tier` commits; both compact controls
/// make ten local commits on every branch they fork.
pub(crate) fn producer_local_commits(access: AccessCase) -> Result<usize> {
    let producer = producer_case(access)?;
    if producer.family == history::FAMILY {
        let (kind, tier) = history::v016_case(&producer.id)
            .ok_or("historical_access producer is not a v0.1.6 history profile")?;
        if kind.is_empty() || tier == 0 {
            return Err("historical_access producer tier".into());
        }
        return Ok(tier);
    }
    let _ = stages::compact_case(&producer.id)?.ok_or("historical_access compact producer")?;
    Ok(match access.branch {
        ProducerBranch::Trunk => stages::COMPACT_TRUNK_COMMITS,
        // Every child branch, the descendant included, makes ten local commits.
        ProducerBranch::Main | ProducerBranch::B => stages::COMPACT_CHILD_COMMITS,
    })
}

/// The declared retained-graph roots of the producer.
pub(crate) fn declared_roots(access: AccessCase) -> Result<usize> {
    let producer = producer_case(access)?;
    if producer.family == history::FAMILY {
        let (kind, tier) = history::v016_case(&producer.id)
            .ok_or("historical_access producer is not a v0.1.6 history profile")?;
        if kind.is_empty() || tier == 0 {
            return Err("historical_access producer tier".into());
        }
        return Ok(tier + 1);
    }
    let compact = stages::compact_case(&producer.id)?.ok_or("historical_access compact producer")?;
    Ok(compact.roots())
}

pub(crate) fn self_check() -> Result<()> {
    for access in CASES {
        let producer = producer_case(access)?;
        if declared_roots(access)? != access.roots {
            return Err(format!(
                "historical_access retained roots for {}: declared {}, producer {}",
                access.id,
                access.roots,
                declared_roots(access)?
            )
            .into());
        }
        if access.ordinal == 0 || access.ordinal > access.roots {
            return Err("historical_access selected ordinal outside the graph".into());
        }
        if access.ordinal > producer_local_commits(access)? {
            return Err(format!(
                "historical_access selected ordinal {} outside the producer's local commits",
                access.ordinal
            )
            .into());
        }
        for seed in 1..=3u8 {
            let fixture = fixture(
                &Case {
                    id: access.id.to_owned(),
                    family: FAMILY,
                    tier: 100,
                    kind: "v016-access",
                },
                seed,
            )?;
            if fixture.is_empty() {
                return Err("historical_access producer genesis".into());
            }
            let (rows, absent) = read_targets(access, seed)?;
            if rows.is_empty() {
                return Err("historical_access has no declared read target".into());
            }
            if rows.iter().any(|row| row.len == 0) {
                return Err("historical_access declared an empty read".into());
            }
            if rows.iter().map(|row| row.len).sum::<u64>() != access.declared_bytes {
                return Err(format!("historical_access declared bytes for {}", access.id).into());
            }
            if access.plan == ReadPlan::InodePair
                && (rows.len() != 2 || rows[0].content != rows[1].content || !absent.is_empty())
            {
                return Err("historical_access alias pair declaration".into());
            }
            if access.plan == ReadPlan::InodeTarget {
                if rows.len() != 1 || absent.len() != 1 {
                    return Err("historical_access replacement declaration".into());
                }
                // The replacement payload is the stage-5 generation of cycle 19.
                let mut expected = Vec::new();
                super::v016_hn::replacement_content(seed, 19)?.write_to(&mut expected)?;
                if rows[0].content != expected {
                    return Err("historical_access replacement payload".into());
                }
            }
            let (planned, planned_absent) = read_plan(access)?;
            if planned.len() != rows.len()
                || planned_absent != absent
                || planned
                    .iter()
                    .zip(&rows)
                    .any(|((path, len), row)| *path != row.path || *len != row.len)
            {
                return Err(format!(
                    "historical_access declared plan disagrees with the derived targets for {}",
                    access.id
                )
                .into());
            }
            if access.plan == ReadPlan::BoundaryBelow {
                let want = match access.ordinal {
                    48 => s::BELOW_LEN,
                    49 => s::EXACT_LEN,
                    _ => return Err("historical_access boundary ordinal".into()),
                };
                if rows[0].len != want {
                    return Err(format!(
                        "historical_access boundary length at commit {}: {} != {want}",
                        access.ordinal, rows[0].len
                    )
                    .into());
                }
            }
            if access.plan == ReadPlan::MediumFull && rows[0].len != s::MEDIUM_LEN {
                return Err("historical_access medium read length".into());
            }
        }
        // The producer of a named branch must be a compact control; the two
        // single-branch histories select the sealed master's own branch.
        if branch_name(access).is_some() != (producer.family == super::branch_development::FAMILY_ID) {
            return Err("historical_access producer branch role".into());
        }
    }
    if cases().len() != 6 {
        return Err("historical_access must register six cases".into());
    }
    Ok(())
}

/// The registry oracle entry point: the selected retained state is this case's
/// declared state, and the route reads exactly it.
pub(crate) fn expected(case: &Case, seed: u8, _step: usize) -> Result<Vec<Entry>> {
    let access = access_case(&case.id)?.ok_or_else(|| format!("unknown historical_access case: {}", case.id))?;
    declared_state(access, seed)
}

pub(crate) fn apply(_case: &Case, _seed: u8, _step: usize, _verify: bool) -> Result<Receipt> {
    Err("historical_access creates no commits; the route mounts a sealed producer state".into())
}
