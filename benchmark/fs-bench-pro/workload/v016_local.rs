// Product-free M1 stage driver.
//
// Runs the five declared POSIX stages over one real directory by launching the
// same helper command the container uses, so the static worker and the fixture
// layout can be debugged without a container or a Store. This path never
// produces a benchmark receipt.
use super::v016_common::{self as v016};
use super::v016_stages::{self as stages};
use super::workspace_common as common;
use super::Result;
use std::collections::BTreeMap;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;

/// `v016-local-workload TIER SEED ROOT CYCLES SALT`
pub(crate) fn run_command(args: &[String]) -> Result<()> {
    let [tier, seed, root, cycles, salt] = args else {
        return Err("usage: v016-local-workload TIER SEED ROOT CYCLES SALT".into());
    };
    let tier = if tier == "L100" {
        &v016::L100
    } else if tier == "L500" {
        &v016::L500
    } else {
        return Err("v0.1.6 local tier must be L100 or L500".into());
    };
    let seed: u8 = seed.parse()?;
    let cycles: usize = cycles.parse()?;
    let root = Path::new(root);
    let fixture = v016::load_fixture(tier, seed)?;
    if root.exists() {
        return Err("v0.1.6 local root must be absent".into());
    }
    common::create_fixture(root, &fixture.entries)?;
    let initial = census(root)?;
    let declared = Census {
        names: tier.initial_paths(),
        bytes: tier.initial_bytes(),
        directories: tier.max_dirs(),
        distinct_inodes: tier.initial_paths(),
    };
    if initial != declared {
        return Err(format!(
            "v0.1.6 local fixture census {initial:?} != declared {declared:?}"
        )
        .into());
    }
    let binary = std::env::current_exe()?;
    let tier_name = if tier.data_dirs == v016::L100.data_dirs {
        "L100"
    } else {
        "L500"
    };
    for cycle in 1..=cycles {
        let cycle_base = if cycle % 2 == 1 {
            common::manifest(&fixture.entries)?.len()
        } else {
            0
        };
        for ordinal in 1..=stages::STAGES {
            match run_stage(&binary, root, tier_name, seed, cycle, ordinal, salt) {
                Ok(()) => {}
                Err(error) => {
                    let mut chain = error.to_string();
                    let mut source = error.source();
                    while let Some(cause) = source {
                        chain.push_str(&format!(" <- {cause}"));
                        source = cause.source();
                    }
                    let live = census(root).unwrap_or_default();
                    return Err(format!(
                        "cycle {cycle} stage {ordinal}: {chain} (live census {live:?})"
                    )
                    .into());
                }
            }
        }
        let after = census(root)?;
        if after != declared {
            return Err(format!(
                "v0.1.6 cycle {cycle} finishes at {after:?}, declared {declared:?}"
            )
            .into());
        }
        let _ = cycle_base;
        println!("cycle={cycle} census={after:?} status=pass");
    }
    println!(
        "v016_local_workload=pass cycles={cycles} paths={}",
        fixture.entries.len()
    );
    Ok(())
}

fn run_stage(
    binary: &Path,
    root: &Path,
    tier: &str,
    seed: u8,
    cycle: usize,
    ordinal: usize,
    salt: &str,
) -> Result<()> {
    let output = Command::new(binary)
        .current_dir(root)
        .args([
            "v016-m1-stage",
            tier,
            &seed.to_string(),
            &cycle.to_string(),
            "0",
            salt,
            &ordinal.to_string(),
        ])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "helper exit {}: {}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
struct Census {
    /// Every non-directory name, including hardlink aliases and symlinks.
    names: usize,
    /// `logical_path_bytes`: a hardlink name is charged its referent length.
    bytes: u64,
    directories: usize,
    /// Distinct inodes among the regular names.
    distinct_inodes: usize,
}

/// Walk the tree and produce the declared live census.
fn census(root: &Path) -> Result<Census> {
    let mut result = Census::default();
    let mut inodes = std::collections::BTreeSet::new();
    let mut lengths: BTreeMap<u64, u64> = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    let mut pending: Vec<(std::path::PathBuf, u64)> = Vec::new();
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let metadata = std::fs::symlink_metadata(entry.path())?;
            if metadata.is_dir() {
                result.directories += 1;
                stack.push(entry.path());
                continue;
            }
            result.names += 1;
            if metadata.file_type().is_symlink() {
                result.bytes += std::fs::read_link(entry.path())?.as_os_str().len() as u64;
                continue;
            }
            inodes.insert(metadata.ino());
            lengths.insert(metadata.ino(), metadata.len());
            pending.push((entry.path(), metadata.ino()));
        }
    }
    for (_, inode) in pending {
        result.bytes += lengths[&inode];
    }
    result.distinct_inodes = inodes.len();
    Ok(result)
}
