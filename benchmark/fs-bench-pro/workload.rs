use std::error::Error;
use std::ffi::OsString;
use std::fs::Metadata;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};

#[allow(dead_code)]
pub(crate) mod workspace_common;
#[allow(dead_code)]
pub(crate) mod ordinary_workloads;
#[allow(dead_code)]
pub(crate) mod dedup_workloads;
#[allow(dead_code)]
pub(crate) mod workspace_registry;
#[allow(dead_code)]
pub(crate) mod edit_length_changing_capped { include!("families/edit_length_changing_capped.rs"); }
#[allow(dead_code)]
pub(crate) mod reliability_workloads;
#[allow(dead_code)]
pub(crate) mod workspace_reliability { include!("families/workspace_reliability.rs"); }
#[allow(dead_code)]
pub(crate) mod payload_create_read { include!("families/payload_create_read.rs"); }
#[allow(dead_code)]
pub(crate) mod tiny_file_churn { include!("families/tiny_file_churn.rs"); }
#[allow(dead_code)]
pub(crate) mod directory_construction_traversal { include!("families/directory_construction_traversal.rs"); }
#[allow(dead_code)]
pub(crate) mod git_tool_workflow { include!("families/git_tool_workflow.rs"); }
#[allow(dead_code)]
pub(crate) mod namespace_mutation { include!("families/namespace_mutation.rs"); }
#[allow(dead_code)]
pub(crate) mod workspace_change_locality { include!("families/workspace_change_locality.rs"); }
#[allow(dead_code)]
pub(crate) mod mixed_load_bearing { include!("families/mixed_load_bearing.rs"); }
#[allow(dead_code)]
pub(crate) mod dedup_cross_file { include!("families/dedup_cross_file.rs"); }
#[allow(dead_code)]
pub(crate) mod dedup_cdc_locality { include!("families/dedup_cdc_locality.rs"); }
#[allow(dead_code)]
pub(crate) mod dedup_workspace_reuse { include!("families/dedup_workspace_reuse.rs"); }
#[allow(dead_code)]
pub(crate) mod dedup_branch_history { include!("families/dedup_branch_history.rs"); }


#[allow(dead_code)]
pub(crate) mod init_namespace {
    include!("families/init_namespace.rs");
}
pub(crate) use init_namespace::*;
#[allow(dead_code)]
pub(crate) mod edit_same_count {
    include!("families/edit_same_count.rs");
}
#[allow(dead_code)]
pub(crate) mod edit_count_changing {
    include!("families/edit_count_changing.rs");
}
#[allow(dead_code)]
pub(crate) mod sdk_edit_common {
    include!("families/sdk_edit_common.rs");
}
#[allow(dead_code)]
pub(crate) mod edit_length_preserving {
    include!("families/edit_length_preserving.rs");
}
#[allow(dead_code)]
pub(crate) mod edit_length_changing {
    include!("families/edit_length_changing.rs");
}
#[allow(dead_code)]
pub(crate) mod edit_canonical_chunk_count {
    include!("families/edit_canonical_chunk_count.rs");
}
#[allow(dead_code)]
pub(crate) mod store_footprint {
    include!("families/store_footprint.rs");
}

pub(crate) type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PREPEND: &[u8] = b"PREPEND010";
#[allow(dead_code)]
pub(crate) const NAMESPACE_SCRATCH_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub(crate) enum NamespaceClass {
    Empty = 0,
    Tiny = 1,
    Small = 2,
    Medium = 3,
    Anchor = 4,
}

impl NamespaceClass {
    #[allow(dead_code)]
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Tiny => "tiny",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Anchor => "anchor",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NamespaceFilePlan {
    pub(crate) relative_path: String,
    pub(crate) class: NamespaceClass,
    pub(crate) role: u64,
    pub(crate) relative_weight: u64,
    pub(crate) size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NamespacePlan {
    pub(crate) scenario: NamespaceScenario,
    pub(crate) files: Vec<NamespaceFilePlan>,
    pub(crate) empty_files: u64,
    pub(crate) tiny_files: u64,
    pub(crate) small_files: u64,
    pub(crate) medium_files: u64,
    pub(crate) anchor_files: u64,
    pub(crate) anchor_bytes: u64,
    pub(crate) edit_path: String,
    pub(crate) edit_size: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PlannedRole {
    class: NamespaceClass,
    role: u64,
    relative_weight: u64,
}

pub(crate) fn namespace_scenario(id: &str) -> Result<NamespaceScenario> {
    init_namespace::namespace_scenario(id).map_err(Into::into)
}

pub(crate) fn namespace_plan(id: &str) -> Result<NamespacePlan> {
    let scenario = namespace_scenario(id)?;
    if scenario.regular_files
        != scenario
            .data_directories
            .checked_mul(NAMESPACE_FILES_PER_DIRECTORY)
            .ok_or("namespace directory multiplication")?
    {
        return Err("namespace files per directory".into());
    }

    let non_anchor = scenario
        .regular_files
        .checked_sub(scenario.anchor_files)
        .ok_or("namespace anchor count")?;
    let classes = [
        NamespaceClass::Empty,
        NamespaceClass::Tiny,
        NamespaceClass::Small,
        NamespaceClass::Medium,
    ];
    let percentages = [1_u64, 79, 15, 5];
    let mut counts = [0_u64; 4];
    let mut remainders = [0_u64; 4];
    let mut allocated = 0_u64;
    for index in 0..classes.len() {
        let numerator = non_anchor
            .checked_mul(percentages[index])
            .ok_or("namespace class count multiplication")?;
        counts[index] = numerator / 100;
        remainders[index] = numerator % 100;
        allocated = allocated
            .checked_add(counts[index])
            .ok_or("namespace class count sum")?;
    }
    let remaining = non_anchor
        .checked_sub(allocated)
        .ok_or("namespace class remainder")?;
    let mut remainder_order = [0_usize, 1, 2, 3];
    remainder_order.sort_by(|left, right| {
        remainders[*right]
            .cmp(&remainders[*left])
            .then_with(|| left.cmp(right))
    });
    for index in remainder_order
        .into_iter()
        .take(usize::try_from(remaining)?)
    {
        counts[index] = counts[index]
            .checked_add(1)
            .ok_or("namespace class remainder increment")?;
    }
    if counts
        != [
            scenario.empty_files,
            scenario.tiny_files,
            scenario.small_files,
            scenario.medium_files,
        ]
    {
        return Err("namespace Hamilton count oracle".into());
    }

    let mut roles = Vec::with_capacity(usize::try_from(scenario.regular_files)?);
    for (class, count) in classes.into_iter().zip(counts) {
        for role in 0..count {
            roles.push(PlannedRole {
                class,
                role,
                relative_weight: namespace_relative_weight(class, role, count)?,
            });
        }
    }
    if roles.len()
        != usize::try_from(
            scenario
                .regular_files
                .checked_sub(scenario.anchor_files)
                .ok_or("namespace non-anchor role count")?,
        )?
    {
        return Err("namespace non-anchor role count".into());
    }
    let mut keyed_roles = roles
        .into_iter()
        .map(|role| (namespace_role_sort_key(scenario, role), role))
        .collect::<Vec<_>>();
    keyed_roles.sort_by(|(left_key, left), (right_key, right)| {
        left_key
            .cmp(right_key)
            .then_with(|| left.class.cmp(&right.class))
            .then_with(|| left.role.cmp(&right.role))
    });

    let mut permuted = vec![None; usize::try_from(scenario.regular_files)?];
    let mut anchor_directories = std::collections::BTreeSet::new();
    for role in 0..scenario.anchor_files {
        let anchor = PlannedRole {
            class: NamespaceClass::Anchor,
            role,
            relative_weight: 0,
        };
        let key = namespace_role_sort_key(scenario, anchor);
        let mut position = usize::try_from(
            u64::from_be_bytes(key[..8].try_into().expect("namespace anchor sort key"))
                % scenario.regular_files,
        )?;
        loop {
            let directory = u64::try_from(position)? % scenario.data_directories;
            if permuted[position].is_none() && !anchor_directories.contains(&directory) {
                permuted[position] = Some(anchor);
                anchor_directories.insert(directory);
                break;
            }
            position = (position + 1) % permuted.len();
        }
    }
    let mut non_anchors = keyed_roles.into_iter().map(|(_, role)| role);
    for slot in &mut permuted {
        if slot.is_none() {
            *slot = Some(non_anchors.next().ok_or("namespace role permutation")?);
        }
    }
    if non_anchors.next().is_some() {
        return Err("namespace role permutation overflow".into());
    }
    let roles = permuted
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or("namespace role permutation gap")?;

    let mut files = Vec::with_capacity(roles.len());
    for (position, role) in roles.into_iter().enumerate() {
        let position = u64::try_from(position)?;
        let directory = position % scenario.data_directories;
        let ordinal = position / scenario.data_directories;
        let global_index = directory
            .checked_mul(NAMESPACE_FILES_PER_DIRECTORY)
            .and_then(|value| value.checked_add(ordinal))
            .ok_or("namespace path index")?;
        files.push(NamespaceFilePlan {
            relative_path: format!("d{directory:04}/f{global_index:06}"),
            class: role.class,
            role: role.role,
            relative_weight: role.relative_weight,
            size: match role.class {
                NamespaceClass::Empty => 0,
                NamespaceClass::Anchor => NAMESPACE_ANCHOR_BYTES,
                _ => 1,
            },
        });
    }

    let positive = counts[1]
        .checked_add(counts[2])
        .and_then(|value| value.checked_add(counts[3]))
        .ok_or("namespace positive count")?;
    let anchor_bytes = scenario
        .anchor_files
        .checked_mul(NAMESPACE_ANCHOR_BYTES)
        .ok_or("namespace anchor bytes")?;
    let distributable = scenario
        .logical_bytes
        .checked_sub(anchor_bytes)
        .and_then(|value| value.checked_sub(positive))
        .ok_or("namespace byte budget")?;
    let weight_sum = files
        .iter()
        .try_fold(0_u64, |total, file| total.checked_add(file.relative_weight))
        .ok_or("namespace weight sum")?;
    if weight_sum == 0 {
        return Err("namespace empty weight sum".into());
    }
    let mut floor_sum = 0_u64;
    let mut size_remainders = Vec::with_capacity(usize::try_from(positive)?);
    for (index, file) in files.iter_mut().enumerate() {
        if file.relative_weight == 0 {
            continue;
        }
        let product = u128::from(distributable)
            .checked_mul(u128::from(file.relative_weight))
            .ok_or("namespace size multiplication")?;
        let floor = u64::try_from(product / u128::from(weight_sum))?;
        let remainder = u64::try_from(product % u128::from(weight_sum))?;
        file.size = 1_u64.checked_add(floor).ok_or("namespace file size")?;
        floor_sum = floor_sum.checked_add(floor).ok_or("namespace floor sum")?;
        size_remainders.push((index, remainder));
    }
    let extra = distributable
        .checked_sub(floor_sum)
        .ok_or("namespace largest remainder")?;
    size_remainders.sort_by(|(left_index, left), (right_index, right)| {
        right.cmp(left).then_with(|| {
            files[*left_index]
                .relative_path
                .as_bytes()
                .cmp(files[*right_index].relative_path.as_bytes())
        })
    });
    if extra > u64::try_from(size_remainders.len())? {
        return Err("namespace largest remainder count".into());
    }
    for (index, _) in size_remainders.into_iter().take(usize::try_from(extra)?) {
        files[index].size = files[index]
            .size
            .checked_add(1)
            .ok_or("namespace largest remainder increment")?;
    }
    files.sort_by(|left, right| {
        left.relative_path
            .as_bytes()
            .cmp(right.relative_path.as_bytes())
    });

    let edit = files
        .iter()
        .filter(|file| {
            file.class != NamespaceClass::Anchor
                && file.class != NamespaceClass::Empty
                && file.size >= u64::try_from(NAMESPACE_EDIT_MARKER.len()).unwrap_or(u64::MAX)
        })
        .min_by(|left, right| {
            left.relative_path
                .as_bytes()
                .cmp(right.relative_path.as_bytes())
        })
        .ok_or("namespace edit target")?;
    let edit_path = edit.relative_path.clone();
    let edit_size = edit.size;
    let plan = NamespacePlan {
        scenario,
        files,
        empty_files: counts[0],
        tiny_files: counts[1],
        small_files: counts[2],
        medium_files: counts[3],
        anchor_files: scenario.anchor_files,
        anchor_bytes,
        edit_path,
        edit_size,
    };
    validate_namespace_plan(&plan)?;
    Ok(plan)
}

fn namespace_relative_weight(class: NamespaceClass, role: u64, count: u64) -> Result<u64> {
    let (lower, upper) = match class {
        NamespaceClass::Empty | NamespaceClass::Anchor => return Ok(0),
        NamespaceClass::Tiny => (1_u64, 8_u64),
        NamespaceClass::Small => (32, 256),
        NamespaceClass::Medium => (1_024, 8_192),
    };
    if count == 0 || role >= count {
        return Err("namespace weight role".into());
    }
    let width = upper
        .checked_sub(lower)
        .and_then(|value| value.checked_add(1))
        .ok_or("namespace weight width")?;
    let numerator = u128::from(
        role.checked_mul(2)
            .and_then(|value| value.checked_add(1))
            .ok_or("namespace weight numerator")?,
    )
    .checked_mul(u128::from(width))
    .ok_or("namespace weight multiplication")?;
    let denominator = u128::from(count.checked_mul(2).ok_or("namespace weight denominator")?);
    lower
        .checked_add(u64::try_from(numerator / denominator)?)
        .ok_or_else(|| "namespace weight".into())
}

fn namespace_role_sort_key(scenario: NamespaceScenario, role: PlannedRole) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"layerfs/fs-bench-pro/namespace-role-permutation/v1\0");
    hash_field(&mut hash, scenario.id.as_bytes());
    hash.update(&[role.class as u8]);
    hash.update(&role.role.to_be_bytes());
    hash.finish()
}

fn validate_namespace_plan(plan: &NamespacePlan) -> Result<()> {
    if plan.files.len() != usize::try_from(plan.scenario.regular_files)?
        || plan.empty_files
            + plan.tiny_files
            + plan.small_files
            + plan.medium_files
            + plan.anchor_files
            != plan.scenario.regular_files
        || plan.anchor_bytes
            != plan
                .anchor_files
                .checked_mul(NAMESPACE_ANCHOR_BYTES)
                .ok_or("namespace anchor validation")?
    {
        return Err("namespace plan count equation".into());
    }
    let mut class_counts = [0_u64; 5];
    let mut logical_bytes = 0_u64;
    let mut directory_counts = vec![0_u64; usize::try_from(plan.scenario.data_directories)?];
    let mut anchor_directories = std::collections::BTreeSet::new();
    let mut previous: Option<&str> = None;
    for file in &plan.files {
        if previous.is_some_and(|path| path.as_bytes() >= file.relative_path.as_bytes()) {
            return Err("namespace path ordering".into());
        }
        previous = Some(&file.relative_path);
        class_counts[file.class as usize] = class_counts[file.class as usize]
            .checked_add(1)
            .ok_or("namespace validation class count")?;
        logical_bytes = logical_bytes
            .checked_add(file.size)
            .ok_or("namespace validation bytes")?;
        let directory = file
            .relative_path
            .split_once('/')
            .ok_or("namespace relative path")?
            .0
            .strip_prefix('d')
            .ok_or("namespace directory path")?
            .parse::<usize>()?;
        let count = directory_counts
            .get_mut(directory)
            .ok_or("namespace directory index")?;
        *count = count
            .checked_add(1)
            .ok_or("namespace directory file count")?;
        match file.class {
            NamespaceClass::Empty if file.size != 0 => {
                return Err("namespace empty file size".into());
            }
            NamespaceClass::Anchor if file.size != NAMESPACE_ANCHOR_BYTES => {
                return Err("namespace anchor file size".into());
            }
            NamespaceClass::Tiny | NamespaceClass::Small | NamespaceClass::Medium
                if file.size == 0 =>
            {
                return Err("namespace positive file size".into());
            }
            _ => {}
        }
        if file.class == NamespaceClass::Anchor {
            anchor_directories.insert(directory);
        }
    }
    if logical_bytes != plan.scenario.logical_bytes
        || class_counts
            != [
                plan.empty_files,
                plan.tiny_files,
                plan.small_files,
                plan.medium_files,
                plan.anchor_files,
            ]
        || directory_counts
            .iter()
            .any(|count| *count != NAMESPACE_FILES_PER_DIRECTORY)
        || (plan.anchor_files > 1
            && anchor_directories.len() != usize::try_from(plan.anchor_files)?)
        || !plan.files.iter().any(|file| {
            file.relative_path == plan.edit_path
                && file.size == plan.edit_size
                && file.class != NamespaceClass::Anchor
                && file.class != NamespaceClass::Empty
                && file.size >= NAMESPACE_EDIT_MARKER.len() as u64
        })
    {
        return Err("namespace plan validation".into());
    }
    Ok(())
}

#[allow(dead_code)]
pub(crate) fn namespace_edit_offset(size: u64) -> Result<u64> {
    size.checked_sub(u64::try_from(NAMESPACE_EDIT_MARKER.len())?)
        .filter(|range| *range != 0)
        .map(|range| 2_654_435_761_u64 % range)
        .ok_or_else(|| "namespace edit size".into())
}

#[allow(dead_code)]
pub(crate) fn namespace_plan_owned_bytes(plan: &NamespacePlan) -> Result<u64> {
    let fixed = plan
        .files
        .capacity()
        .checked_mul(std::mem::size_of::<NamespaceFilePlan>())
        .and_then(|value| value.checked_add(std::mem::size_of::<NamespacePlan>()))
        .and_then(|value| value.checked_add(plan.edit_path.capacity()))
        .ok_or("namespace plan ownership")?;
    let paths = plan.files.iter().try_fold(0_usize, |total, file| {
        total.checked_add(file.relative_path.capacity())
    });
    u64::try_from(
        fixed
            .checked_add(paths.ok_or("namespace path ownership")?)
            .ok_or("namespace plan ownership total")?,
    )
    .map_err(Into::into)
}

pub(crate) struct NamespaceContentStream {
    state: [u64; 4],
    word: [u8; 8],
    used: usize,
}

impl NamespaceContentStream {
    pub(crate) fn new(scenario: NamespaceScenario, file: &NamespaceFilePlan) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"layerfs/fs-bench-pro/namespace-content-stream/v1\0");
        hash_field(&mut hash, scenario.id.as_bytes());
        hash_field(&mut hash, file.relative_path.as_bytes());
        hash.update(&[file.class as u8]);
        hash.update(&file.size.to_be_bytes());
        let seed = hash.finish();
        let mut state = [0_u64; 4];
        for (slot, bytes) in state.iter_mut().zip(seed.chunks_exact(8)) {
            *slot = u64::from_be_bytes(bytes.try_into().expect("SHA-256 seed word"));
        }
        if state == [0; 4] {
            state[0] = 1;
        }
        Self {
            state,
            word: [0; 8],
            used: 8,
        }
    }

    pub(crate) fn fill(&mut self, mut output: &mut [u8]) {
        while !output.is_empty() {
            if self.used == self.word.len() {
                self.word = self.next().to_le_bytes();
                self.used = 0;
            }
            let count = output.len().min(self.word.len() - self.used);
            output[..count].copy_from_slice(&self.word[self.used..self.used + count]);
            self.used += count;
            output = &mut output[count..];
        }
    }

    fn next(&mut self) -> u64 {
        let result = self.state[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let temporary = self.state[1] << 17;
        self.state[2] ^= self.state[0];
        self.state[3] ^= self.state[1];
        self.state[1] ^= self.state[2];
        self.state[0] ^= self.state[3];
        self.state[2] ^= temporary;
        self.state[3] = self.state[3].rotate_left(45);
        result
    }
}

#[allow(dead_code)]
pub(crate) fn namespace_tree_digest(
    plan: &NamespacePlan,
    content_digests: &[Option<[u8; 32]>],
) -> Result<String> {
    if content_digests.len() != plan.files.len() {
        return Err("namespace content digest count".into());
    }
    let mut hash = Sha256::new();
    hash.update(b"layerfs/fs-bench-pro/namespace-file-digest-tree/v2\0");
    hash_field(&mut hash, NAMESPACE_FIXTURE_PROFILE.as_bytes());
    hash_field(&mut hash, NAMESPACE_DIGEST_PROFILE.as_bytes());
    hash_field(&mut hash, plan.scenario.id.as_bytes());
    hash_tree_record(&mut hash, "", b'D', u8::MAX, 0, &[0; 32]);
    for directory in 0..plan.scenario.data_directories {
        hash_tree_record(
            &mut hash,
            &format!("d{directory:04}"),
            b'D',
            u8::MAX,
            0,
            &[0; 32],
        );
        let first = usize::try_from(
            directory
                .checked_mul(NAMESPACE_FILES_PER_DIRECTORY)
                .ok_or("namespace tree directory index")?,
        )?;
        let end = first
            .checked_add(usize::try_from(NAMESPACE_FILES_PER_DIRECTORY)?)
            .ok_or("namespace tree directory range")?;
        for (file, digest) in plan.files[first..end]
            .iter()
            .zip(&content_digests[first..end])
        {
            let digest = digest.as_ref().ok_or("missing namespace content digest")?;
            hash_tree_record(
                &mut hash,
                &file.relative_path,
                b'F',
                file.class as u8,
                file.size,
                digest,
            );
        }
    }
    Ok(hex(&hash.finish()))
}

#[allow(dead_code)]
fn hash_tree_record(
    hash: &mut Sha256,
    path: &str,
    file_type: u8,
    class: u8,
    size: u64,
    content_digest: &[u8; 32],
) {
    hash.update(&[file_type, class]);
    hash_field(hash, path.as_bytes());
    hash.update(&size.to_be_bytes());
    hash.update(
        &if file_type == b'D' {
            NAMESPACE_DIRECTORY_MODE
        } else {
            NAMESPACE_FILE_MODE
        }
        .to_be_bytes(),
    );
    hash.update(&NAMESPACE_MTIME_SECONDS.to_be_bytes());
    hash.update(&NAMESPACE_MTIME_NANOSECONDS.to_be_bytes());
    hash.update(content_digest);
}

pub(crate) fn set_namespace_metadata(path: &Path, directory: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(if directory {
                NAMESPACE_DIRECTORY_MODE
            } else {
                NAMESPACE_FILE_MODE
            }),
        )?;
    }
    #[cfg(not(unix))]
    {
        let _ = directory;
        return Err("namespace mode verification requires Unix".into());
    }
    let modified = std::time::UNIX_EPOCH
        .checked_add(std::time::Duration::from_secs(u64::try_from(
            NAMESPACE_MTIME_SECONDS,
        )?))
        .ok_or("namespace mtime")?;
    File::open(path)?.set_times(FileTimes::new().set_modified(modified))?;
    Ok(())
}

fn validate_namespace_metadata(metadata: &Metadata, directory: bool) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let expected_mode = if directory {
            NAMESPACE_DIRECTORY_MODE
        } else {
            NAMESPACE_FILE_MODE
        };
        if metadata.mode() & 0o7777 != expected_mode
            || metadata.mtime() != NAMESPACE_MTIME_SECONDS
            || metadata.mtime_nsec() != i64::from(NAMESPACE_MTIME_NANOSECONDS)
        {
            return Err("namespace mode or mtime mismatch".into());
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = (metadata, directory);
        Err("namespace mode verification requires Unix".into())
    }
}

fn hash_field(hash: &mut Sha256, bytes: &[u8]) {
    hash.update(&(bytes.len() as u64).to_be_bytes());
    hash.update(bytes);
}

#[derive(Clone)]
struct StoreFixtureRecord {
    path: String,
    size: u64,
    mode: u32,
    mtime_seconds: i64,
    content_digest: [u8; 32],
}

fn store_tree_digest(records: &[StoreFixtureRecord]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"layerfs/fs-bench-pro/store-footprint-tree/v1\0");
    for record in records {
        store_tree_hash_record(&mut hash, record);
    }
    hex(&hash.finish())
}

fn store_tree_hash_record(hash: &mut Sha256, record: &StoreFixtureRecord) {
    hash_field(hash, record.path.as_bytes());
    hash.update(&record.size.to_be_bytes());
    hash.update(&record.mode.to_be_bytes());
    hash.update(&record.mtime_seconds.to_be_bytes());
    hash.update(&record.content_digest);
}

fn store_metadata_seconds(kind: store_footprint::Kind, index: usize) -> i64 {
    if kind == store_footprint::Kind::MetadataCardinality {
        NAMESPACE_MTIME_SECONDS + index as i64 + 1
    } else {
        NAMESPACE_MTIME_SECONDS
    }
}

fn set_store_file_metadata(path: &Path, seconds: i64) -> Result<()> {
    use std::time::{Duration, UNIX_EPOCH};
    let timestamp = UNIX_EPOCH + Duration::from_secs(u64::try_from(seconds)?);
    fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(NAMESPACE_FILE_MODE))?;
    File::open(path)?.set_times(
        FileTimes::new()
            .set_accessed(timestamp)
            .set_modified(timestamp),
    )?;
    Ok(())
}

fn update_store_edited_hash(
    hash: &mut Sha256,
    bytes: &[u8],
    chunk_offset: u64,
    edit_offset: u64,
) -> Result<()> {
    let chunk_end = chunk_offset + bytes.len() as u64;
    let edit_end = edit_offset + NAMESPACE_EDIT_MARKER.len() as u64;
    if chunk_end <= edit_offset || chunk_offset >= edit_end {
        hash.update(bytes);
        return Ok(());
    }
    let overlap_start = chunk_offset.max(edit_offset);
    let overlap_end = chunk_end.min(edit_end);
    let before = usize::try_from(overlap_start - chunk_offset)?;
    let after = usize::try_from(overlap_end - chunk_offset)?;
    let marker_start = usize::try_from(overlap_start - edit_offset)?;
    let marker_end = usize::try_from(overlap_end - edit_offset)?;
    hash.update(&bytes[..before]);
    hash.update(&NAMESPACE_EDIT_MARKER[marker_start..marker_end]);
    hash.update(&bytes[after..]);
    Ok(())
}

fn create_store_footprint_fixture(root: &Path, control_id: &str, tier: u64) -> Result<()> {
    let control = store_footprint::control(control_id)?;
    if root.exists() || ![100, 1_000, 10_000, 100_000].contains(&tier) {
        return Err("Store-footprint fixture arguments".into());
    }
    fs::create_dir(root)?;
    let (scenario, files) = if control.kind == store_footprint::Kind::LargeObject {
        let count = if tier == 100_000 { 100 } else { tier.min(100) };
        let size = if tier == 100_000 { 5_000_000 } else { 1_000_000 };
        let scenario = NamespaceScenario {
            id: "store-footprint-large-object-500m",
            alias: "store-footprint-large-object-500m",
            display_name: "Store footprint large-object fixture",
            regular_files: count,
            data_directories: 1,
            logical_bytes: count * size,
            anchor_files: 0,
            empty_files: 0,
            tiny_files: 0,
            small_files: 0,
            medium_files: count,
        };
        let files = (0..count)
            .map(|index| NamespaceFilePlan {
                relative_path: format!("d0000/file-{index:05}.bin"),
                class: NamespaceClass::Medium,
                role: index,
                relative_weight: 1,
                size,
            })
            .collect();
        (scenario, files)
    } else {
        let namespace_id = match tier {
            100 => "namespace-100",
            1_000 => "namespace-1000",
            10_000 => "namespace-10000",
            100_000 => "namespace-100000",
            _ => unreachable!(),
        };
        let plan = namespace_plan(namespace_id)?;
        (plan.scenario, plan.files)
    };
    for directory in 0..scenario.data_directories {
        fs::create_dir(root.join(format!("d{directory:04}")))?;
    }
    let edit_path = files
        .iter()
        .find(|file| file.size > NAMESPACE_EDIT_MARKER.len() as u64)
        .ok_or("Store-footprint edit target")?
        .relative_path
        .clone();
    let edit_size = files
        .iter()
        .find(|file| file.relative_path == edit_path)
        .ok_or("Store-footprint edit size")?
        .size;
    let edit_offset = namespace_edit_offset(edit_size)?;
    let mut buffer = vec![0_u8; NAMESPACE_SCRATCH_BYTES];
    let mut records = Vec::with_capacity(files.len());
    let mut edited_digest = None;
    for (index, file) in files.iter().enumerate() {
        let path = root.join(&file.relative_path);
        let mut output = File::create(&path)?;
        let mut stream = NamespaceContentStream::new(scenario, file);
        let mut digest = Sha256::new();
        let mut edited = (file.relative_path == edit_path).then(Sha256::new);
        let mut offset = 0_u64;
        while offset < file.size {
            let count = usize::try_from((file.size - offset).min(buffer.len() as u64))?;
            stream.fill(&mut buffer[..count]);
            output.write_all(&buffer[..count])?;
            digest.update(&buffer[..count]);
            if let Some(hash) = edited.as_mut() {
                update_store_edited_hash(hash, &buffer[..count], offset, edit_offset)?;
            }
            offset += count as u64;
        }
        drop(output);
        let mtime_seconds = store_metadata_seconds(control.kind, index);
        set_store_file_metadata(&path, mtime_seconds)?;
        let content_digest = digest.finish();
        if let Some(hash) = edited {
            edited_digest = Some(hash.finish());
        }
        records.push(StoreFixtureRecord {
            path: file.relative_path.clone(),
            size: file.size,
            mode: NAMESPACE_FILE_MODE,
            mtime_seconds,
            content_digest,
        });
    }
    for directory in 0..scenario.data_directories {
        set_namespace_metadata(&root.join(format!("d{directory:04}")), true)?;
    }
    set_namespace_metadata(root, true)?;
    let original = store_tree_digest(&records);
    let target = records
        .iter_mut()
        .find(|record| record.path == edit_path)
        .ok_or("Store-footprint edit record")?;
    target.content_digest = edited_digest.ok_or("Store-footprint edited digest")?;
    target.mtime_seconds = NAMESPACE_MTIME_SECONDS;
    let edited = store_tree_digest(&records);
    let logical_bytes: u64 = records.iter().map(|record| record.size).sum();
    println!(
        "{{\"schema\":\"{}\",\"family_id\":\"{}\",\"control_id\":\"{}\",\"tier\":{},\"reportable\":{},\"regular_files\":{},\"data_directories\":{},\"logical_bytes\":{},\"file_mode\":{},\"directory_mode\":{},\"edit_path\":\"{}\",\"edit_size\":{},\"fixture_digest\":\"{}\",\"edited_fixture_digest\":\"{}\"}}",
        store_footprint::FIXTURE_SCHEMA,
        store_footprint::FAMILY_ID,
        control.id,
        tier,
        tier == 100_000,
        records.len(),
        scenario.data_directories,
        logical_bytes,
        NAMESPACE_FILE_MODE,
        NAMESPACE_DIRECTORY_MODE,
        edit_path,
        edit_size,
        original,
        edited,
    );
    Ok(())
}

fn store_footprint_digest(root: &Path) -> Result<(u64, u64, String)> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    fn file_record(root: &Path, path: &Path, buffer: &mut [u8]) -> Result<StoreFixtureRecord> {
        let relative = path.strip_prefix(root)?;
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        let mut digest = Sha256::new();
        let mut size = 0_u64;
        loop {
            let read = file.read(buffer)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
            size = size.checked_add(read as u64).ok_or("Store digest size")?;
        }
        if size != metadata.len() {
            return Err("Store file changed during digest".into());
        }
        Ok(StoreFixtureRecord {
            path: relative.to_string_lossy().into_owned(),
            size: metadata.len(),
            mode: metadata.permissions().mode() & 0o7777,
            mtime_seconds: metadata.mtime(),
            content_digest: digest.finish(),
        })
    }
    fn collect_records(
        root: &Path,
        path: &Path,
        buffer: &mut [u8],
    ) -> Result<Vec<StoreFixtureRecord>> {
        let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_unstable_by_key(|entry| entry.file_name());
        let mut output = Vec::with_capacity(entries.len());
        for entry in entries {
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                validate_namespace_metadata(&entry.metadata()?, true)?;
                output.extend(collect_records(root, &path, buffer)?);
            } else if file_type.is_file() {
                output.push(file_record(root, &path, buffer)?);
            }
        }
        Ok(output)
    }
    fn append(
        hash: &mut Sha256,
        records: Vec<StoreFixtureRecord>,
        files: &mut u64,
        logical_bytes: &mut u64,
    ) -> Result<()> {
        for record in records {
            *files = files.checked_add(1).ok_or("Store digest file count")?;
            *logical_bytes = logical_bytes
                .checked_add(record.size)
                .ok_or("Store digest logical bytes")?;
            store_tree_hash_record(hash, &record);
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    hash.update(b"layerfs/fs-bench-pro/store-footprint-tree/v1\0");
    validate_namespace_metadata(&fs::metadata(root)?, true)?;
    let (mut files, mut logical_bytes) = (0, 0);
    let mut entries = fs::read_dir(root)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_unstable_by_key(|entry| entry.file_name());
    let mut directories = Vec::new();
    let mut root_buffer = vec![0_u8; NAMESPACE_SCRATCH_BYTES];
    for entry in entries {
        if entry.file_type()?.is_dir() {
            directories.push(entry.path());
        } else {
            append(
                &mut hash,
                vec![file_record(root, &entry.path(), &mut root_buffer)?],
                &mut files,
                &mut logical_bytes,
            )?;
        }
    }
    let workers = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(16);
    let worker_buffer_bytes = (NAMESPACE_SCRATCH_BYTES / workers).max(4096);
    let window = workers.saturating_mul(4).min(directories.len());
    let (task_sender, task_receiver) = mpsc::sync_channel(window);
    let task_receiver = Arc::new(Mutex::new(task_receiver));
    let (sender, receiver) = mpsc::sync_channel(window);
    for index in 0..window {
        task_sender.send(index)?;
    }
    std::thread::scope(|scope| -> Result<()> {
        for _ in 0..workers.min(directories.len()) {
            let task_receiver = Arc::clone(&task_receiver);
            let sender = sender.clone();
            let directories = &directories;
            scope.spawn(move || {
                let mut buffer = vec![0_u8; worker_buffer_bytes];
                loop {
                    let index = match task_receiver.lock().ok().and_then(|tasks| tasks.recv().ok()) {
                        Some(index) => index,
                        None => return,
                    };
                    let result = collect_records(root, &directories[index], &mut buffer)
                        .map_err(|error| error.to_string());
                    if sender.send((index, result)).is_err() {
                        return;
                    }
                }
            });
        }
        drop(sender);
        let mut sent = window;
        let mut next = 0;
        let mut pending = std::collections::BTreeMap::new();
        while next < directories.len() {
            let (index, result) = receiver.recv()?;
            if pending.insert(index, result).is_some() {
                return Err("duplicate Store digest worker result".into());
            }
            while let Some(result) = pending.remove(&next) {
                append(
                    &mut hash,
                    result.map_err(|error| -> Box<dyn Error> { error.into() })?,
                    &mut files,
                    &mut logical_bytes,
                )?;
                next += 1;
                if sent < directories.len() {
                    task_sender.send(sent)?;
                    sent += 1;
                }
            }
        }
        drop(task_sender);
        Ok(())
    })?;
    Ok((files, logical_bytes, hex(&hash.finish())))
}

fn main() {
    if let Err(error) = run() {
        eprintln!("fs-benchmark-workload: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "workspace-verify-fast") {
        let [_, id, seed, step, binding] = args.as_slice() else { return Err("workspace-verify-fast CASE SEED STEP CERT_BINDING".into()); };
        let case = workspace_registry::cases().into_iter().find(|case| &case.id == id).ok_or("unknown fast verifier case")?;
        let seed = seed.parse::<u8>()?;
        let step = step.parse::<usize>()?;
        let entries = ordinary_workloads::expected(&case, seed, step)?;
        let delta = ordinary_workloads::fast_delta_for_entries(&case, seed, step, &entries)?;
        let receipt = workspace_common::verify_native_fast(Path::new("."), &entries, &delta, binding)?;
        for (key, value) in receipt { println!("{key}={value}"); }
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg.starts_with("workspace-")) {
        return workspace_registry::dispatch(&args);
    }
    match args.as_slice() {
        [command] if command == "self-check" => self_check(),
        [command] if command == "family-list" => {
            for scenario in NAMESPACE_SCENARIOS {
                println!("{}\t{}\t{}", scenario.id, scenario.alias, scenario.display_name);
            }
            Ok(())
        }
        [command, case] if command == "family-resolve" => {
            let scenario = namespace_scenario(case)?;
            println!("{}\t{}\t{}", scenario.id, scenario.alias, scenario.display_name);
            Ok(())
        }
        [command] if command == "same-count-list" => {
            for scenario in edit_same_count::SCENARIOS {
                println!("{}\t{}", scenario.id, scenario.display_name);
            }
            Ok(())
        }
        [command] if command == "same-count-self-check" => {
            edit_same_count::self_check()?;
            println!("same-count-self-check=pass");
            Ok(())
        }
        [command, case] if command == "same-count-resolve" => {
            let scenario = edit_same_count::scenario(case)?;
            println!("{}\t{}", scenario.id, scenario.display_name);
            Ok(())
        }
        [command, case] if command == "same-count-control-resolve" => {
            let control = edit_same_count::pair_control(case)?;
            println!("{}\t{}\t{}", control.id, control.operations, control.position.name());
            Ok(())
        }
        [command] if command == "count-changing-list" => {
            for scenario in edit_count_changing::SCENARIOS {
                println!(
                    "{}\t{}\t{}",
                    scenario.id, scenario.display_name, scenario.paired_same_count_control_id
                );
            }
            Ok(())
        }
        [command] if command == "count-changing-scaling-list" => {
            for scenario in edit_count_changing::SCALING_SCENARIOS {
                println!(
                    "{}\t{}\t{}",
                    scenario.id, scenario.display_name, scenario.fixture_bytes
                );
            }
            Ok(())
        }
        [command] if command == "count-changing-self-check" => {
            edit_count_changing::self_check()?;
            println!("count-changing-self-check=pass");
            Ok(())
        }
        [command, case] if command == "count-changing-resolve" => {
            let scenario = edit_count_changing::scenario(case)?;
            println!(
                "{}\t{}\t{}",
                scenario.id, scenario.display_name, scenario.paired_same_count_control_id
            );
            Ok(())
        }
        [command] if command == "store-footprint-list" => {
            for control in store_footprint::CONTROLS {
                println!("{}\t{}", control.id, control.display_name);
            }
            Ok(())
        }
        [command] if command == "store-footprint-self-check" => {
            store_footprint::self_check()?;
            println!("store-footprint-self-check=pass");
            Ok(())
        }
        [command, case] if command == "store-footprint-resolve" => {
            let control = store_footprint::control(case)?;
            println!("{}\t{}", control.id, control.display_name);
            Ok(())
        }
        [command, root, case, tier] if command == "store-footprint-fixture" => {
            create_store_footprint_fixture(Path::new(root), case, tier.parse()?)
        }
        [command, root] if command == "store-footprint-digest" => {
            let (files, logical_bytes, digest) = store_footprint_digest(Path::new(root))?;
            println!("regular_files={files}");
            println!("logical_bytes={logical_bytes}");
            println!("tree_digest={digest}");
            Ok(())
        }
        [command] if command == "noop" => Ok(()),
        [command, path] if command == "digest" => print_digest(path),
        [command, path] if command == "digest-inode" => print_digest_inode(path),
        [command, path] if command == "stat-inode" => {
            use std::os::unix::fs::MetadataExt;
            let metadata = fs::metadata(path)?;
            println!("{}\tstat-only\t{}", metadata.len(), metadata.ino());
            Ok(())
        }
        [command, path] if command == "read" => print_read(path),
        [command, path, scenario] if command == "namespace-verify" => {
            print_namespace(path, scenario)
        }
        [command, fixture, path] if command == "create" => {
            let started = std::time::Instant::now();
            create(fixture, path)?;
            println!("inner_write_ns={}", started.elapsed().as_nanos());
            Ok(())
        }
        [command, path, index, base_size] if command == "edit" => {
            edit(path, index.parse()?, base_size.parse()?)
        }
        [command, path] if command == "namespace-edit" => {
            namespace_edit(path)?;
            println!("attempted_operations=1");
            println!("completed_operations=1");
            println!("final_file_bytes={}", fs::metadata(path)?.len());
            Ok(())
        }
        [command, path] if command == "namespace-edit-normal" => {
            let (seconds, nanoseconds) = namespace_edit_normal(path)?;
            println!("normal_overwrite_mtime_seconds={seconds}");
            println!("normal_overwrite_mtime_nanoseconds={nanoseconds}");
            Ok(())
        }
        [command, path, case, seed] if command == "same-count-edit" => {
            same_count_edit(path, case, seed.parse()?)
        }
        [command, path, cohort, count, seed] if command == "same-count-fragmented" => {
            same_count_fragmented(path, cohort, count.parse()?, seed.parse()?)
        }
        [command, path, case, seed] if command == "same-count-control-edit" => {
            same_count_control_edit(path, case, seed.parse()?)
        }
        [command, path, case, seed] if command == "count-changing-edit" => {
            count_changing_edit(path, case, seed.parse()?)
        }
        [command, path, verifier] if command == "count-changing-proof" => {
            count_changing_proof(path, verifier)
        }
        [command, path] if command == "prepend" => prepend(path),
        [command, path, expected_size, expected_digest] if command == "verify" => {
            let (size, digest) = digest(Path::new(path))?;
            if size != expected_size.parse::<u64>()? || digest != *expected_digest {
                return Err(format!(
                    "verification mismatch: size={size} sha256={digest} expected_size={expected_size} expected_sha256={expected_digest}"
                )
                .into());
            }
            println!("{size}\t{digest}");
            Ok(())
        }
        _ => Err("usage: fs-benchmark-workload self-check | family-list | family-resolve CASE | same-count-self-check | same-count-list | same-count-resolve CASE | same-count-control-resolve CASE | count-changing-self-check | count-changing-list | count-changing-scaling-list | count-changing-resolve CASE | store-footprint-self-check | store-footprint-list | store-footprint-resolve CASE | store-footprint-fixture ROOT CASE TIER | store-footprint-digest ROOT | digest|read PATH | namespace-verify PATH SCENARIO | namespace-edit|namespace-edit-normal PATH | same-count-edit PATH CASE SEED | same-count-control-edit PATH CASE SEED | same-count-fragmented PATH COHORT COUNT SEED | count-changing-edit PATH CASE SEED | count-changing-proof PATH VERIFIER | create FIXTURE PATH | edit PATH INDEX BASE_SIZE | prepend PATH | verify PATH SIZE SHA256".into()),
    }
}

fn create(fixture: impl AsRef<Path>, path: impl AsRef<Path>) -> Result<()> {
    let mut source = BufReader::with_capacity(1024 * 1024, File::open(fixture)?);
    let target = File::create(path)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, target);
    std::io::copy(&mut source, &mut writer)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    Ok(())
}

fn edit(path: impl AsRef<Path>, index: u64, base_size: u64) -> Result<()> {
    if base_size <= 10 {
        return Err("base size must exceed marker length".into());
    }
    let marker = format!("E{:09}", index.checked_add(1).ok_or("edit index overflow")?);
    if marker.len() != 10 {
        return Err("edit index exceeds the 10-byte marker".into());
    }
    let offset = index
        .checked_add(1)
        .and_then(|value| value.checked_mul(2_654_435_761))
        .ok_or("edit offset overflow")?
        % (base_size - marker.len() as u64);
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    write_all_at(&file, marker.as_bytes(), offset)?;
    file.sync_all()?;
    Ok(())
}

fn namespace_edit(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    edit(path, 0, fs::metadata(path)?.len())?;
    set_namespace_metadata(path, false)
}

fn namespace_edit_normal(path: impl AsRef<Path>) -> Result<(i64, i64)> {
    let path = path.as_ref();
    edit(path, 1, fs::metadata(path)?.len())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(path)?;
        let observed = (metadata.mtime(), metadata.mtime_nsec());
        Ok(observed)
    }
    #[cfg(not(unix))]
    {
        Err("normal overwrite mtime requires Unix".into())
    }
}

fn same_count_edit(path: impl AsRef<Path>, case: &str, seed: u8) -> Result<()> {
    let scenario = edit_same_count::scenario(case)?;
    let schedule = edit_same_count::schedule(scenario, seed)?;
    same_count_apply(path.as_ref(), &schedule, seed)
}

fn same_count_control_edit(path: impl AsRef<Path>, case: &str, seed: u8) -> Result<()> {
    let control = edit_same_count::pair_control(case)?;
    let schedule = edit_same_count::pair_control_schedule(control, seed)?;
    same_count_apply(path.as_ref(), &schedule, seed)
}

fn same_count_apply(path: &Path, schedule: &[edit_same_count::Edit], seed: u8) -> Result<()> {
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    if file.metadata()?.len() != edit_same_count::FIXTURE_BYTES {
        return Err("same-count fixture length".into());
    }
    let mut coverage = vec![false; edit_same_count::FIXTURE_BYTES as usize];
    let mut supplied = 0_u64;
    let mut identical = 0_u64;
    let started = std::time::Instant::now();
    for (operation, edit) in schedule.iter().copied().enumerate() {
        let bytes = edit_same_count::replacement_bytes(seed, operation, edit);
        let mut before = vec![0; edit.len];
        read_exact_at(&file, &mut before, edit.offset)?;
        identical += before
            .iter()
            .zip(&bytes)
            .filter(|(left, right)| left == right)
            .count() as u64;
        write_all_at(&file, &bytes, edit.offset)?;
        supplied += edit.len as u64;
        coverage[edit.offset as usize..edit.offset as usize + edit.len].fill(true);
    }
    let inner_edit_ns = started.elapsed().as_nanos();
    file.sync_all()?;
    let unique = coverage.into_iter().filter(|covered| *covered).count() as u64;
    println!("attempted_operations={}", schedule.len());
    println!("completed_operations={}", schedule.len());
    println!("final_file_bytes={}", file.metadata()?.len());
    println!("supplied_bytes={supplied}");
    println!("unique_bytes={unique}");
    println!("overlapping_bytes={}", supplied - unique);
    println!("identical_bytes={identical}");
    println!("superseded_bytes={}", supplied - unique);
    println!("inner_edit_ns={inner_edit_ns}");
    Ok(())
}

fn same_count_fragmented(path: impl AsRef<Path>, cohort: &str, count: usize, seed: u8) -> Result<()> {
    let path = path.as_ref();
    let schedule = edit_same_count::fragmented_schedule(cohort, count, seed)?;
    let file = OpenOptions::new().read(true).write(true).open(path)?;
    if file.metadata()?.len() != edit_same_count::FIXTURE_BYTES {
        return Err("same-count fragmented fixture length".into());
    }
    for (operation, edit) in schedule.iter().copied().enumerate() {
        write_all_at(
            &file,
            &edit_same_count::replacement_bytes(seed, operation, edit),
            edit.offset,
        )?;
    }
    file.sync_all()?;
    println!("attempted_operations={}", schedule.len());
    println!("completed_operations={}", schedule.len());
    println!("final_file_bytes={}", file.metadata()?.len());
    Ok(())
}

fn count_changing_edit(path: impl AsRef<Path>, case: &str, seed: u8) -> Result<()> {
    use edit_count_changing::Kind;
    use std::os::unix::fs::MetadataExt;

    let path = path.as_ref();
    let scenario = edit_count_changing::scenario(case)?;
    let schedule = edit_count_changing::schedule(scenario, seed)?;
    if fs::metadata(path)?.len() != scenario.fixture_bytes {
        return Err("count-changing fixture length".into());
    }
    let initial_inode = fs::metadata(path)?.ino();
    let temporary = path.with_extension("count-changing.tmp");
    let mut supplied = 0_u64;
    let mut inserted = 0_u64;
    let mut deleted = 0_u64;
    let mut overlapping = 0_u64;
    let mut superseded = 0_u64;
    let mut logical_zero = 0_u64;
    let mut copied_payload = 0_u64;
    let mut read_payload = 0_u64;
    let started = std::time::Instant::now();
    for (operation, edit) in schedule.iter().copied().enumerate() {
        let replacement = edit_count_changing::replacement_bytes(seed, operation, edit);
        match scenario.kind {
            Kind::Append => {
                let mut file = OpenOptions::new().append(true).open(path)?;
                file.write_all(&replacement)?;
                file.sync_all()?;
            }
            Kind::Truncate => {
                let file = OpenOptions::new().write(true).open(path)?;
                file.set_len(edit.final_len)?;
                file.sync_all()?;
            }
            Kind::Sparse => {
                let file = OpenOptions::new().write(true).open(path)?;
                write_all_at(&file, &replacement, edit.offset)?;
                file.sync_all()?;
            }
            Kind::Prepend | Kind::Insert | Kind::Delete | Kind::Grow | Kind::Shrink => {
                let copied = rewrite_file_range(
                    path,
                    &temporary,
                    edit.offset,
                    edit.deleted as u64,
                    &replacement,
                )?;
                copied_payload = copied_payload
                    .checked_add(copied)
                    .ok_or("count-changing copied payload")?;
                read_payload = read_payload
                    .checked_add(copied.checked_add(edit.deleted as u64).ok_or(
                        "count-changing operation read payload",
                    )?)
                    .ok_or("count-changing read payload")?;
            }
            Kind::FrozenPrepend => return Err("frozen count-changing workload".into()),
        }
        supplied += edit.inserted as u64;
        inserted += edit.inserted as u64;
        deleted += edit.deleted as u64;
        overlapping += edit.deleted.min(edit.inserted) as u64;
        superseded += edit.deleted as u64;
        logical_zero += edit.logical_zero as u64;
    }
    let inner_edit_ns = started.elapsed().as_nanos();
    let final_file_bytes = fs::metadata(path)?.len();
    if final_file_bytes != schedule.last().ok_or("empty count-changing schedule")?.final_len {
        return Err("count-changing final length".into());
    }
    println!("attempted_operations={}", schedule.len());
    println!("completed_operations={}", schedule.len());
    println!("final_file_bytes={final_file_bytes}");
    println!("initial_inode={initial_inode}");
    println!("final_inode={}", fs::metadata(path)?.ino());
    println!("supplied_bytes={supplied}");
    println!("inserted_bytes={inserted}");
    println!("deleted_bytes={deleted}");
    println!("overlapping_bytes={overlapping}");
    println!("superseded_bytes={superseded}");
    println!("logical_zero_bytes={logical_zero}");
    println!("copied_payload_bytes={copied_payload}");
    println!("read_payload_bytes={read_payload}");
    println!("inner_edit_ns={inner_edit_ns}");
    Ok(())
}

fn rewrite_file_range(
    path: &Path,
    temporary: &Path,
    offset: u64,
    deleted: u64,
    replacement: &[u8],
) -> Result<u64> {
    let mut source = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let target = File::create(temporary)?;
    let mut output = BufWriter::with_capacity(1024 * 1024, target);
    let prefix = std::io::copy(&mut source.by_ref().take(offset), &mut output)?;
    if prefix != offset {
        return Err("count-changing rewrite prefix".into());
    }
    output.write_all(replacement)?;
    let mut remaining = deleted;
    while remaining != 0 {
        let available = source.fill_buf()?;
        if available.is_empty() {
            return Err("count-changing rewrite deletion".into());
        }
        let consumed = available.len().min(usize::try_from(remaining)?);
        source.consume(consumed);
        remaining -= consumed as u64;
    }
    let suffix = std::io::copy(&mut source, &mut output)?;
    output.flush()?;
    output.into_inner()?.sync_all()?;
    fs::rename(temporary, path)?;
    prefix
        .checked_add(suffix)
        .ok_or_else(|| "count-changing copied bytes".into())
}

fn count_changing_proof(path: impl AsRef<Path>, verifier: &str) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    const MIB: u64 = 1024 * 1024;
    let path = path.as_ref();
    let temporary = path.with_extension("count-changing-proof.tmp");
    if fs::metadata(path)?.len() != 8 * MIB {
        return Err("count-changing proof fixture".into());
    }
    let initial_inode = fs::metadata(path)?.ino();
    match verifier {
        "insert-middle-4k-on-8m-proof" => {
            let bytes = (0..4096)
                .map(|index| ((index * 17 + 3) % 251) as u8)
                .collect::<Vec<_>>();
            rewrite_file_range(path, &temporary, 4 * MIB, 0, &bytes)?;
        }
        "delete-middle-4k-on-8m-proof" => {
            rewrite_file_range(path, &temporary, 4 * MIB - 2048, 4096, &[])?;
        }
        "rewrite-full-grow-8m-to-12m-proof" | "rewrite-full-shrink-8m-to-4m-proof" => {
            let final_len = if verifier.contains("grow") {
                12 * MIB
            } else {
                4 * MIB
            };
            let mut output = BufWriter::with_capacity(64 * 1024, File::create(&temporary)?);
            let mut offset = 0_u64;
            let mut buffer = vec![0_u8; 64 * 1024];
            while offset < final_len {
                let count = usize::try_from((final_len - offset).min(buffer.len() as u64))?;
                for (index, byte) in buffer[..count].iter_mut().enumerate() {
                    *byte = (((offset + index as u64) * 31 + final_len / MIB) % 251) as u8;
                }
                output.write_all(&buffer[..count])?;
                offset += count as u64;
            }
            output.flush()?;
            output.into_inner()?.sync_all()?;
            fs::rename(&temporary, path)?;
        }
        _ => return Err("unknown count-changing proof".into()),
    }
    println!("attempted_operations=1");
    println!("completed_operations=1");
    println!("final_file_bytes={}", fs::metadata(path)?.len());
    println!("initial_inode={initial_inode}");
    println!("final_inode={}", fs::metadata(path)?.ino());
    Ok(())
}

#[cfg(unix)]
fn read_exact_at(file: &File, bytes: &mut [u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.read_exact_at(bytes, offset)
}

#[cfg(not(unix))]
fn read_exact_at(file: &File, bytes: &mut [u8], offset: u64) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut file = file;
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(bytes)
}

#[cfg(unix)]
fn write_all_at(file: &File, bytes: &[u8], offset: u64) -> std::io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.write_all_at(bytes, offset)
}

#[cfg(not(unix))]
fn write_all_at(file: &File, bytes: &[u8], offset: u64) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut file = file;
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(bytes)
}

fn prepend(path: impl AsRef<Path>) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let path = path.as_ref();
    let before = fs::metadata(path)?;
    let temporary = path.with_extension("bin.prepend.tmp");
    let mut source = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let target = File::create(&temporary)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, target);
    writer.write_all(PREPEND)?;
    std::io::copy(&mut source, &mut writer)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    fs::rename(temporary, path)?;
    let after = fs::metadata(path)?;
    println!("attempted_operations=1");
    println!("completed_operations=1");
    println!("final_file_bytes={}", after.len());
    println!("initial_inode={}", before.ino());
    println!("final_inode={}", after.ino());
    Ok(())
}

fn print_digest(path: impl AsRef<Path>) -> Result<()> {
    let (size, digest) = digest(path.as_ref())?;
    println!("{size}\t{digest}");
    Ok(())
}

fn print_digest_inode(path: impl AsRef<Path>) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let path = path.as_ref();
    let (size, digest) = digest(path)?;
    println!("{size}\t{digest}\t{}", fs::metadata(path)?.ino());
    Ok(())
}

fn print_read(path: impl AsRef<Path>) -> Result<()> {
    let mut input = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let bytes = std::io::copy(&mut input, &mut std::io::sink())?;
    println!("read_bytes={bytes}");
    Ok(())
}

fn print_namespace(path: impl AsRef<Path>, scenario: &str) -> Result<()> {
    let plan = namespace_plan(scenario)?;
    let summary = namespace_digest_with_plan(path.as_ref(), &plan)?;
    println!("regular_files={}", summary.regular_files);
    println!("data_directories={}", summary.data_directories);
    println!("logical_bytes={}", summary.logical_bytes);
    println!("empty_files={}", summary.empty_files);
    println!("tiny_files={}", summary.tiny_files);
    println!("small_files={}", summary.small_files);
    println!("medium_files={}", summary.medium_files);
    println!("anchor_files={}", summary.anchor_files);
    println!("anchor_bytes={}", summary.anchor_bytes);
    println!("namespace_digest={}", summary.digest);
    println!(
        "maximum_verifier_buffer_bytes={}",
        summary.maximum_verifier_buffer_bytes
    );
    println!("verifier_worker_count={}", summary.verifier_worker_count);
    println!("verifier_plan_bytes={}", summary.verifier_plan_bytes);
    println!(
        "verifier_path_state_peak_bytes={}",
        summary.verifier_path_state_peak_bytes
    );
    println!(
        "verifier_digest_state_peak_bytes={}",
        summary.verifier_digest_state_peak_bytes
    );
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct NamespaceSummary {
    regular_files: u64,
    data_directories: u64,
    logical_bytes: u64,
    empty_files: u64,
    tiny_files: u64,
    small_files: u64,
    medium_files: u64,
    anchor_files: u64,
    anchor_bytes: u64,
    digest: String,
    maximum_verifier_buffer_bytes: u64,
    verifier_worker_count: u64,
    verifier_plan_bytes: u64,
    verifier_path_state_peak_bytes: u64,
    verifier_digest_state_peak_bytes: u64,
}

struct NamespacePathEntry {
    name: OsString,
    metadata: Metadata,
}

fn namespace_digest_with_plan(root: &Path, plan: &NamespacePlan) -> Result<NamespaceSummary> {
    let root_metadata = fs::symlink_metadata(root)?;
    if !root_metadata.file_type().is_dir() {
        return Err("namespace root is not a directory".into());
    }
    validate_namespace_metadata(&root_metadata, true)?;
    let verifier_plan_bytes = namespace_plan_owned_bytes(plan)?;
    let mut directories_seen = vec![0_u8; usize::try_from(plan.scenario.data_directories)?];
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_str().ok_or("namespace path is not UTF-8")?;
        let directory = name
            .strip_prefix('d')
            .ok_or("unexpected namespace root path")?
            .parse::<usize>()?;
        if name != format!("d{directory:04}") {
            return Err("unexpected namespace root path".into());
        }
        let seen = directories_seen
            .get_mut(directory)
            .ok_or("unexpected namespace directory")?;
        if std::mem::replace(seen, 1) != 0 {
            return Err("duplicate namespace directory".into());
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.file_type().is_dir() {
            return Err("namespace directory type mismatch".into());
        }
        validate_namespace_metadata(&metadata, true)?;
    }
    if directories_seen.contains(&0) {
        return Err("missing namespace directory".into());
    }
    let directory_state_bytes = directories_seen
        .capacity()
        .checked_mul(std::mem::size_of::<u8>())
        .ok_or("namespace verifier directory ownership")?;
    let mut verifier_path_state_peak_bytes = directory_state_bytes;
    for directory in 0..plan.scenario.data_directories {
        let first = usize::try_from(
            directory
                .checked_mul(NAMESPACE_FILES_PER_DIRECTORY)
                .ok_or("namespace verifier directory index")?,
        )?;
        let end = first
            .checked_add(usize::try_from(NAMESPACE_FILES_PER_DIRECTORY)?)
            .ok_or("namespace verifier directory range")?;
        let directory_path = root.join(format!("d{directory:04}"));
        let mut entries = namespace_directory_entries(
            &directory_path,
            usize::try_from(NAMESPACE_FILES_PER_DIRECTORY)?,
        )?;
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        verifier_path_state_peak_bytes = verifier_path_state_peak_bytes.max(
            directory_state_bytes
                .checked_add(namespace_entry_state_bytes(&entries)?)
                .ok_or("namespace verifier path ownership")?,
        );
        if entries.len() != end - first {
            return Err("missing namespace file".into());
        }
        for (entry, file) in entries.iter().zip(&plan.files[first..end]) {
            let expected_name = file
                .relative_path
                .split_once('/')
                .ok_or("namespace planned file path")?
                .1;
            if entry.name != expected_name
                || !entry.metadata.file_type().is_file()
                || entry.metadata.len() != file.size
            {
                return Err("namespace file path, type, or size mismatch".into());
            }
            validate_namespace_metadata(&entry.metadata, false)?;
        }
    }
    drop(directories_seen);

    let workers = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(16)
        .min(plan.files.len().max(1));
    let worker_buffer_bytes = NAMESPACE_SCRATCH_BYTES / workers;
    let maximum_verifier_buffer_bytes = worker_buffer_bytes
        .checked_mul(workers)
        .ok_or("namespace verifier buffer bytes")?;
    let next = Arc::new(AtomicUsize::new(0));
    let (sender, receiver) = mpsc::sync_channel::<std::result::Result<(usize, [u8; 32]), String>>(
        workers.saturating_mul(2),
    );
    std::thread::scope(|scope| -> Result<NamespaceSummary> {
        for _ in 0..workers {
            let next = Arc::clone(&next);
            let sender = sender.clone();
            let plan = &plan;
            scope.spawn(move || {
                let mut buffer = vec![0_u8; worker_buffer_bytes];
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(file) = plan.files.get(index) else {
                        return;
                    };
                    let result = (|| -> std::result::Result<_, String> {
                        let mut input = File::open(root.join(&file.relative_path))
                            .map_err(|error| error.to_string())?;
                        let mut hash = Sha256::new();
                        let mut size = 0_u64;
                        loop {
                            let read =
                                input.read(&mut buffer).map_err(|error| error.to_string())?;
                            if read == 0 {
                                break;
                            }
                            size = size
                                .checked_add(read as u64)
                                .ok_or_else(|| "namespace verifier size overflow".to_owned())?;
                            hash.update(&buffer[..read]);
                        }
                        if size != file.size {
                            return Err("namespace file changed during verification".to_owned());
                        }
                        Ok((index, hash.finish()))
                    })();
                    if sender.send(result).is_err() {
                        return;
                    }
                }
            });
        }
        drop(sender);
        let mut digests = vec![None; plan.files.len()];
        let verifier_digest_state_peak_bytes = digests
            .capacity()
            .checked_mul(std::mem::size_of::<Option<[u8; 32]>>())
            .ok_or("namespace verifier digest ownership")?;
        for _ in 0..plan.files.len() {
            let (index, digest) = receiver
                .recv()
                .map_err(|_| "namespace file reader stopped")?
                .map_err(|error| -> Box<dyn Error> { error.into() })?;
            let slot = digests
                .get_mut(index)
                .ok_or("namespace file result index")?;
            if slot.replace(digest).is_some() {
                return Err("duplicate namespace file result".into());
            }
        }
        let digest = namespace_tree_digest(plan, &digests)?;
        Ok(NamespaceSummary {
            regular_files: plan.scenario.regular_files,
            data_directories: plan.scenario.data_directories,
            logical_bytes: plan.scenario.logical_bytes,
            empty_files: plan.empty_files,
            tiny_files: plan.tiny_files,
            small_files: plan.small_files,
            medium_files: plan.medium_files,
            anchor_files: plan.anchor_files,
            anchor_bytes: plan.anchor_bytes,
            digest,
            maximum_verifier_buffer_bytes: u64::try_from(maximum_verifier_buffer_bytes)?,
            verifier_worker_count: u64::try_from(workers)?,
            verifier_plan_bytes,
            verifier_path_state_peak_bytes: u64::try_from(verifier_path_state_peak_bytes)?,
            verifier_digest_state_peak_bytes: u64::try_from(verifier_digest_state_peak_bytes)?,
        })
    })
}

fn namespace_directory_entries(directory: &Path, limit: usize) -> Result<Vec<NamespacePathEntry>> {
    let mut output = Vec::with_capacity(limit);
    for entry in fs::read_dir(directory)? {
        if output.len() == limit {
            return Err("extra namespace path".into());
        }
        let entry = entry?;
        output.push(NamespacePathEntry {
            name: entry.file_name(),
            metadata: fs::symlink_metadata(entry.path())?,
        });
    }
    Ok(output)
}

fn namespace_entry_state_bytes(entries: &Vec<NamespacePathEntry>) -> Result<usize> {
    entries
        .capacity()
        .checked_mul(std::mem::size_of::<NamespacePathEntry>())
        .and_then(|fixed| {
            entries.iter().try_fold(fixed, |total, entry| {
                total.checked_add(entry.name.capacity())
            })
        })
        .ok_or_else(|| "namespace verifier path ownership".into())
}

fn digest(path: &Path) -> Result<(u64, String)> {
    let mut input = BufReader::with_capacity(1024 * 1024, File::open(path)?);
    let mut hash = Sha256::new();
    let mut size = 0_u64;
    let mut bytes = [0_u8; 1024 * 1024];
    loop {
        let read = input.read(&mut bytes)?;
        if read == 0 {
            break;
        }
        size = size.checked_add(read as u64).ok_or("file size overflow")?;
        hash.update(&bytes[..read]);
    }
    Ok((size, hex(&hash.finish())))
}

fn self_check() -> Result<()> {
    init_namespace::self_check()?;
    edit_same_count::self_check()?;
    edit_count_changing::self_check()?;
    store_footprint::self_check()?;
    let root = std::env::temp_dir().join(format!(
        "fs-benchmark-pro-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir(&root)?;
    let checked = (|| -> Result<()> {
        let fixture = root.join("fixture.bin");
        let payload = root.join("payload.bin");
        let expected = (0_u16..8192).map(|value| value as u8).collect::<Vec<_>>();
        fs::write(&fixture, &expected)?;
        create(&fixture, &payload)?;
        let mut read = BufReader::with_capacity(1024, File::open(&payload)?);
        assert_eq!(
            std::io::copy(&mut read, &mut std::io::sink())?,
            expected.len() as u64
        );
        edit(&payload, 0, expected.len() as u64)?;
        prepend(&payload)?;
        let mut expected = expected;
        let marker = b"E000000001";
        let offset = 2_654_435_761_u64 % (expected.len() as u64 - marker.len() as u64);
        expected[offset as usize..offset as usize + marker.len()].copy_from_slice(marker);
        let expected = [PREPEND, &expected].concat();
        if fs::read(&payload)? != expected {
            return Err("workload byte oracle mismatch".into());
        }
        let same_count = root.join("same-count.bin");
        fs::write(&same_count, edit_same_count::fixture_bytes())?;
        same_count_edit(&same_count, "overwrite-middle-4k-ops-1", 1)?;
        if fs::metadata(&same_count)?.len() != edit_same_count::FIXTURE_BYTES {
            return Err("same-count workload length".into());
        }
        let count_changing = root.join("count-changing.bin");
        fs::write(&count_changing, edit_count_changing::fixture_bytes())?;
        count_changing_edit(&count_changing, "append-tail-4k-ops-1", 1)?;
        if fs::metadata(&count_changing)?.len() != edit_count_changing::FIXTURE_BYTES + 4096 {
            return Err("count-changing workload length".into());
        }
        let count_delete = root.join("count-delete.bin");
        let mut expected_delete = edit_count_changing::fixture_bytes();
        let edit = edit_count_changing::schedule(
            edit_count_changing::scenario("delete-middle-2k-ops-1")?,
            1,
        )?[0];
        fs::write(&count_delete, &expected_delete)?;
        count_changing_edit(&count_delete, "delete-middle-2k-ops-1", 1)?;
        expected_delete.drain(edit.offset as usize..edit.offset as usize + edit.deleted);
        if fs::read(&count_delete)? != expected_delete {
            return Err("count-changing delete byte oracle".into());
        }
        let (size, actual) = digest(&payload)?;
        let mut expected_hash = Sha256::new();
        expected_hash.update(&expected);
        if size != expected.len() as u64 || actual != hex(&expected_hash.finish()) {
            return Err("workload digest oracle mismatch".into());
        }
        if hex(&Sha256::digest(b"abc"))
            != "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        {
            return Err("SHA-256 known vector".into());
        }
        let normal_mtime = root.join("normal-mtime.bin");
        fs::write(&normal_mtime, [0_u8; 16])?;
        set_namespace_metadata(&normal_mtime, false)?;
        let observed_mtime = namespace_edit_normal(&normal_mtime)?;
        if observed_mtime
            == (
                NAMESPACE_MTIME_SECONDS,
                i64::from(NAMESPACE_MTIME_NANOSECONDS),
            )
        {
            return Err("normal overwrite mtime self-check".into());
        }
        let mut segmented = Sha256::new();
        segmented.update(b"a");
        segmented.update(b"b");
        segmented.update(b"c");
        if hex(&segmented.finish())
            != "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        {
            return Err("segmented SHA-256 known vector".into());
        }
        let expected_counts = [
            ("namespace-100", [1, 78, 15, 5, 1], 125_000_000),
            ("namespace-1000", [10, 789, 150, 50, 1], 200_000_000),
            ("namespace-10000", [100, 7_899, 1_500, 500, 1], 300_000_000),
            (
                "namespace-100000",
                [1_000, 78_998, 15_000, 5_000, 2],
                500_000_000,
            ),
        ];
        for (scenario, counts, logical_bytes) in expected_counts {
            let first = namespace_plan(scenario)?;
            if [
                first.empty_files,
                first.tiny_files,
                first.small_files,
                first.medium_files,
                first.anchor_files,
            ] != counts
                || first.scenario.logical_bytes != logical_bytes
                || (scenario == "namespace-100" && first != namespace_plan(scenario)?)
            {
                return Err("namespace-v2 plan oracle".into());
            }
        }
        let plan = namespace_plan("namespace-100")?;
        let edit = plan
            .files
            .iter()
            .find(|file| file.relative_path == plan.edit_path)
            .ok_or("namespace-v2 self-check edit")?;
        let other = plan
            .files
            .iter()
            .find(|file| file.relative_path != edit.relative_path && file.size != 0)
            .ok_or("namespace-v2 self-check other file")?;
        let mut first = [0_u8; 64];
        let mut second = [0_u8; 64];
        let mut different = [0_u8; 64];
        NamespaceContentStream::new(plan.scenario, edit).fill(&mut first);
        NamespaceContentStream::new(plan.scenario, edit).fill(&mut second);
        NamespaceContentStream::new(plan.scenario, other).fill(&mut different);
        if first != second || first == different {
            return Err("namespace-v2 content stream oracle".into());
        }
        if NAMESPACE_FIXTURE_PROFILE != "synthetic-small-heavy-v2"
            || NAMESPACE_DIGEST_PROFILE != "namespace-file-digest-tree-v2"
            || NAMESPACE_EDIT_CONTRACT != "content-only-normalized-mtime-v1"
            || NAMESPACE_LIFECYCLE_PROFILE != "commit-head-exact-reopen-v2"
            || NAMESPACE_INIT_DIAGNOSTIC_PROFILE != "initialization-only-diagnostic-v1"
        {
            return Err("namespace-v2 evidence identity".into());
        }
        let store_digest_root = root.join("store-digest");
        fs::create_dir_all(store_digest_root.join("d0000"))?;
        fs::create_dir_all(store_digest_root.join("d0001"))?;
        let store_files = [
            ("d0000/a", b"first".as_slice()),
            ("d0000/b", b"second".as_slice()),
            ("d0001/a", b"third".as_slice()),
        ];
        let mut store_records = Vec::new();
        for (path, bytes) in store_files {
            let output = store_digest_root.join(path);
            fs::write(&output, bytes)?;
            set_store_file_metadata(&output, NAMESPACE_MTIME_SECONDS)?;
            store_records.push(StoreFixtureRecord {
                path: path.to_owned(),
                size: bytes.len() as u64,
                mode: NAMESPACE_FILE_MODE,
                mtime_seconds: NAMESPACE_MTIME_SECONDS,
                content_digest: Sha256::digest(bytes),
            });
        }
        set_namespace_metadata(&store_digest_root.join("d0000"), true)?;
        set_namespace_metadata(&store_digest_root.join("d0001"), true)?;
        set_namespace_metadata(&store_digest_root, true)?;
        let (store_files, store_bytes, store_digest) = store_footprint_digest(&store_digest_root)?;
        if store_files != 3
            || store_bytes != 16
            || store_digest != store_tree_digest(&store_records)
        {
            return Err("Store multi-directory digest merge".into());
        }
        #[cfg(test)]
        miniature_namespace_self_check(&root.join("namespace-v2"))?;
        if namespace_v1_bridge_digest()?
            != "b272176a494debb3ae07c22f48eb04e198cf2f09ceead334631b0e7fd646f34d"
        {
            return Err("namespace-v1 bridge digest".into());
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(root);
    checked?;
    println!("{{\"schema\":\"fs-benchmark-pro-workload-self-check-v3\",\"status\":\"pass\"}}");
    Ok(())
}

#[cfg(test)]
pub(crate) fn miniature_namespace_self_check(root: &Path) -> Result<()> {
    let mut plan = namespace_plan("namespace-100")?;
    let edit_path = plan.edit_path.clone();
    for file in &mut plan.files {
        file.size = if file.class == NamespaceClass::Empty {
            0
        } else if file.relative_path == edit_path {
            16
        } else {
            1
        };
    }
    plan.edit_size = 16;
    plan.anchor_bytes = plan
        .files
        .iter()
        .filter(|file| file.class == NamespaceClass::Anchor)
        .try_fold(0_u64, |total, file| total.checked_add(file.size))
        .ok_or("miniature namespace anchor bytes")?;
    plan.scenario.logical_bytes = plan
        .files
        .iter()
        .try_fold(0_u64, |total, file| total.checked_add(file.size))
        .ok_or("miniature namespace logical bytes")?;

    fs::create_dir(root)?;
    for directory in 0..plan.scenario.data_directories {
        fs::create_dir(root.join(format!("d{directory:04}")))?;
    }
    let mut content_digests = Vec::with_capacity(plan.files.len());
    for file in &plan.files {
        let bytes = miniature_namespace_bytes(plan.scenario, file)?;
        write_miniature_namespace_file(&root.join(&file.relative_path), &bytes)?;
        content_digests.push(Some(Sha256::digest(&bytes)));
    }
    for directory in 0..plan.scenario.data_directories {
        set_namespace_metadata(&root.join(format!("d{directory:04}")), true)?;
    }
    set_namespace_metadata(root, true)?;

    let original_digest = namespace_tree_digest(&plan, &content_digests)?;
    if namespace_digest_with_plan(root, &plan)?.digest != original_digest {
        return Err("namespace-v2 original custody digest".into());
    }
    let edit_index = plan
        .files
        .iter()
        .position(|file| file.relative_path == plan.edit_path)
        .ok_or("miniature namespace edit index")?;
    let edit_file = &plan.files[edit_index];
    let mut edited_bytes = miniature_namespace_bytes(plan.scenario, edit_file)?;
    let edit_offset = usize::try_from(namespace_edit_offset(edit_file.size)?)?;
    edited_bytes[edit_offset..edit_offset + NAMESPACE_EDIT_MARKER.len()]
        .copy_from_slice(NAMESPACE_EDIT_MARKER);
    namespace_edit(root.join(&edit_file.relative_path))?;
    content_digests[edit_index] = Some(Sha256::digest(&edited_bytes));
    let edited_digest = namespace_tree_digest(&plan, &content_digests)?;
    if edited_digest == original_digest
        || namespace_digest_with_plan(root, &plan)?.digest != edited_digest
    {
        return Err("namespace-v2 edited custody digest".into());
    }

    let file_path = root.join(&edit_file.relative_path);
    let directory_path = file_path.parent().ok_or("miniature namespace parent")?;
    fs::remove_file(&file_path)?;
    set_namespace_metadata(directory_path, true)?;
    expect_namespace_rejection(root, &plan, "missing")?;
    write_miniature_namespace_file(&file_path, &edited_bytes)?;
    set_namespace_metadata(directory_path, true)?;

    let extra_path = directory_path.join("extra");
    write_miniature_namespace_file(&extra_path, b"x")?;
    set_namespace_metadata(directory_path, true)?;
    expect_namespace_rejection(root, &plan, "extra")?;
    fs::remove_file(extra_path)?;
    set_namespace_metadata(directory_path, true)?;

    fs::remove_file(&file_path)?;
    fs::create_dir(&file_path)?;
    set_namespace_metadata(&file_path, true)?;
    set_namespace_metadata(directory_path, true)?;
    expect_namespace_rejection(root, &plan, "type")?;
    fs::remove_dir(&file_path)?;
    write_miniature_namespace_file(&file_path, &edited_bytes)?;
    set_namespace_metadata(directory_path, true)?;

    OpenOptions::new()
        .append(true)
        .open(&file_path)?
        .write_all(b"x")?;
    set_namespace_metadata(&file_path, false)?;
    expect_namespace_rejection(root, &plan, "size")?;
    write_miniature_namespace_file(&file_path, &edited_bytes)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            &file_path,
            fs::Permissions::from_mode(NAMESPACE_FILE_MODE ^ 0o100),
        )?;
    }
    expect_namespace_rejection(root, &plan, "mode")?;
    set_namespace_metadata(&file_path, false)?;

    let changed_mtime = std::time::UNIX_EPOCH
        .checked_add(std::time::Duration::from_secs(u64::try_from(
            NAMESPACE_MTIME_SECONDS + 1,
        )?))
        .ok_or("miniature namespace mtime")?;
    File::open(&file_path)?.set_times(FileTimes::new().set_modified(changed_mtime))?;
    expect_namespace_rejection(root, &plan, "mtime")?;
    set_namespace_metadata(&file_path, false)?;

    if namespace_digest_with_plan(root, &plan)?.digest != edited_digest {
        return Err("namespace-v2 rejection restore".into());
    }
    Ok(())
}

#[cfg(test)]
fn miniature_namespace_bytes(
    scenario: NamespaceScenario,
    file: &NamespaceFilePlan,
) -> Result<Vec<u8>> {
    let mut bytes = vec![0; usize::try_from(file.size)?];
    NamespaceContentStream::new(scenario, file).fill(&mut bytes);
    Ok(bytes)
}

#[cfg(test)]
fn write_miniature_namespace_file(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes)?;
    set_namespace_metadata(path, false)
}

#[cfg(test)]
fn expect_namespace_rejection(root: &Path, plan: &NamespacePlan, mutation: &str) -> Result<()> {
    if namespace_digest_with_plan(root, plan).is_ok() {
        return Err(format!("namespace-v2 accepted {mutation} mutation").into());
    }
    Ok(())
}

fn namespace_v1_bridge_digest() -> Result<String> {
    let mut hash = Sha256::new();
    legacy_sha256_update(&mut hash, b"layerfs/fs-bench-pro/namespace-tree/v1\0");
    legacy_sha256_update(&mut hash, b"D\0");
    legacy_sha256_update(&mut hash, b"d0000");
    legacy_sha256_update(&mut hash, b"\0");
    let mut buffer = [0_u8; 2_500];
    for index in 0..100_u64 {
        let path = format!("d0000/f{index:06}");
        let mut seed = b"layerfs/fs-bench-pro/namespace-content/v1\0".to_vec();
        seed.extend_from_slice(path.as_bytes());
        seed.push(0);
        legacy_sha256_update(&mut hash, b"F\0");
        legacy_sha256_update(&mut hash, path.as_bytes());
        legacy_sha256_update(&mut hash, b"\0");
        legacy_sha256_update(&mut hash, b"2500");
        legacy_sha256_update(&mut hash, b"\0");
        for (absolute, slot) in buffer.iter_mut().enumerate() {
            *slot =
                seed[absolute % seed.len()] ^ ((absolute / seed.len()) as u8).wrapping_mul(0x9d);
        }
        legacy_sha256_update(&mut hash, &buffer);
        legacy_sha256_update(&mut hash, b"\0");
    }
    Ok(hex(&hash.finish()))
}

// Namespace-v1 evidence was sealed with this update-boundary bug. Keep it
// local to the bridge so historical fixture identities remain reproducible.
fn legacy_sha256_update(hash: &mut Sha256, mut input: &[u8]) {
    hash.bytes = hash
        .bytes
        .checked_add(input.len() as u64)
        .expect("SHA-256 input size");
    if hash.buffered != 0 {
        let take = (64 - hash.buffered).min(input.len());
        hash.block[hash.buffered..hash.buffered + take].copy_from_slice(&input[..take]);
        hash.buffered += take;
        input = &input[take..];
        if hash.buffered == 64 {
            compress(&mut hash.state, &hash.block);
            hash.buffered = 0;
        }
    }
    while input.len() >= 64 {
        let block: &[u8; 64] = input[..64].try_into().expect("block length");
        compress(&mut hash.state, block);
        input = &input[64..];
    }
    hash.block[..input.len()].copy_from_slice(input);
    hash.buffered = input.len();
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut value, "{byte:02x}").expect("String formatting");
    }
    value
}

pub(crate) struct Sha256 {
    state: [u32; 8],
    block: [u8; 64],
    buffered: usize,
    bytes: u64,
}

impl Sha256 {
    pub(crate) fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            block: [0; 64],
            buffered: 0,
            bytes: 0,
        }
    }

    pub(crate) fn update(&mut self, mut input: &[u8]) {
        self.bytes = self
            .bytes
            .checked_add(input.len() as u64)
            .expect("SHA-256 input size");
        if self.buffered != 0 {
            let take = (64 - self.buffered).min(input.len());
            self.block[self.buffered..self.buffered + take].copy_from_slice(&input[..take]);
            self.buffered += take;
            input = &input[take..];
            if self.buffered != 64 {
                return;
            }
            compress(&mut self.state, &self.block);
            self.buffered = 0;
        }
        while input.len() >= 64 {
            let block: &[u8; 64] = input[..64].try_into().expect("block length");
            compress(&mut self.state, block);
            input = &input[64..];
        }
        self.block[..input.len()].copy_from_slice(input);
        self.buffered = input.len();
    }

    pub(crate) fn finish(mut self) -> [u8; 32] {
        let bit_length = self.bytes.checked_mul(8).expect("SHA-256 bit length");
        self.block[self.buffered] = 0x80;
        self.buffered += 1;
        if self.buffered > 56 {
            self.block[self.buffered..].fill(0);
            compress(&mut self.state, &self.block);
            self.block = [0; 64];
        } else {
            self.block[self.buffered..56].fill(0);
        }
        self.block[56..].copy_from_slice(&bit_length.to_be_bytes());
        compress(&mut self.state, &self.block);
        let mut output = [0_u8; 32];
        for (chunk, word) in output.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        output
    }

    pub(crate) fn digest(bytes: &[u8]) -> [u8; 32] {
        let mut hash = Self::new();
        hash.update(bytes);
        hash.finish()
    }
}

fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut words = [0_u32; 64];
    for (word, bytes) in words.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_be_bytes(bytes.try_into().expect("word length"));
    }
    for index in 16..64 {
        let s0 = words[index - 15].rotate_right(7)
            ^ words[index - 15].rotate_right(18)
            ^ (words[index - 15] >> 3);
        let s1 = words[index - 2].rotate_right(17)
            ^ words[index - 2].rotate_right(19)
            ^ (words[index - 2] >> 10);
        words[index] = words[index - 16]
            .wrapping_add(s0)
            .wrapping_add(words[index - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for (word, constant) in words.into_iter().zip(K) {
        let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let choose = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(sum1)
            .wrapping_add(choose)
            .wrapping_add(constant)
            .wrapping_add(word);
        let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let majority = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = sum0.wrapping_add(majority);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }
    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn namespace_metadata_custody_is_exact() {
        super::self_check().unwrap();
    }
}
