//! The four `store upgrade` guards (#804 §11). Each fails when its rule is
//! broken: the schema guard when the current schema text differs from the
//! last step's frozen target (a schema change without a step), the coverage
//! guard when a version from the floor to latest−1 or an accepted variant
//! has no fixture, the frozen guard when a committed fixture or manifest
//! changed, and the chain guard when the steps have a gap or a step changes
//! acquisition or layout.
use crate::support;

use layerfs_persistence::internals::{
    postgresql_target_and_current_source, postgresql_upgrade_steps, sqlite_decode_user_version,
    sqlite_target_and_current_source, sqlite_upgrade_steps, upgrade_check_chain, StepData,
    UpgradeStep,
};
use layerfs_persistence::upgrade::{UpgradeError, POSTGRESQL_LATEST, SQLITE_LATEST};
use std::path::Path;
use support::upgrade::{fixtures, sha256_file};

#[test]
fn schema_guard_current_schema_is_the_last_steps_frozen_target() {
    for user_version in [56, 57, 58, 62, 63, 64] {
        let (target, current) = sqlite_target_and_current_source(user_version);
        assert!(
            target == current,
            "SQLite user_version {user_version}: the schema changed without an upgrade step \
             (current source differs from step 2->3's frozen target)"
        );
    }
    for acquisition in [true, false] {
        let (target, current) = postgresql_target_and_current_source(acquisition);
        assert!(
            target == current,
            "PostgreSQL (acquisition {acquisition}): the schema changed without an upgrade step \
             (current source differs from step 3->4's frozen target)"
        );
    }
}

/// Every step's base fixture `v<from>/` and each further variant's
/// `v<from>/<variant>/` hold a Store and its manifest.
fn coverage(engine: &str, store: &str, (steps, floor): (Vec<StepData>, i64), latest: i64) {
    for version in floor..latest {
        let step = steps
            .iter()
            .find(|(from, ..)| *from == version)
            .unwrap_or_else(|| panic!("{engine}: no step from format {version}"));
        for (index, variant) in step.3.iter().enumerate() {
            let dir = match index {
                0 => fixtures().join(format!("{engine}/v{version}")),
                _ => fixtures().join(format!("{engine}/v{version}/{variant}")),
            };
            for file in [store, "manifest.json"] {
                assert!(
                    dir.join(file).is_file(),
                    "{engine}: format {version} variant {variant} has no fixture {}",
                    dir.join(file).display()
                );
            }
        }
    }
}

#[test]
fn coverage_guard_every_upgradable_version_and_variant_has_a_fixture() {
    coverage(
        "sqlite",
        "store.sqlite",
        sqlite_upgrade_steps(),
        SQLITE_LATEST,
    );
    coverage(
        "postgresql",
        "store.sql",
        postgresql_upgrade_steps(),
        POSTGRESQL_LATEST,
    );
}

fn files(dir: &Path, base: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, base, out);
        } else {
            out.push(
                path.strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
}

#[test]
fn frozen_guard_fixtures_match_their_hash_list() {
    let base = fixtures();
    let list = std::fs::read_to_string(base.join("SHA256SUMS")).unwrap();
    let mut listed = Vec::new();
    for line in list.lines().filter(|l| !l.trim().is_empty()) {
        let (hash, name) = line.split_once("  ").expect("`<sha256>  <path>` lines");
        assert_eq!(
            sha256_file(&base.join(name)),
            hash,
            "frozen fixture {name} changed"
        );
        listed.push(name.to_owned());
    }
    let mut present = Vec::new();
    files(&base, &base, &mut present);
    present.retain(|name| name != "SHA256SUMS");
    present.sort();
    listed.sort();
    assert_eq!(
        present, listed,
        "every fixture file is in SHA256SUMS and nothing else"
    );
}

fn never(_: &mut ()) -> Result<Option<&'static str>, UpgradeError> {
    Ok(None)
}

fn nothing(_: &mut ()) -> Result<(), UpgradeError> {
    Ok(())
}

fn as_steps(data: &[StepData]) -> Vec<UpgradeStep<()>> {
    data.iter()
        .map(|&(from, to, raw, variants)| UpgradeStep {
            from,
            to,
            raw,
            variants,
            recognize: never,
            apply: nothing,
        })
        .collect()
}

#[test]
fn chain_guard_no_gap_and_no_step_changes_acquisition_or_layout() {
    let (steps, floor) = sqlite_upgrade_steps();
    upgrade_check_chain(
        &as_steps(&steps),
        floor,
        SQLITE_LATEST,
        sqlite_decode_user_version,
    )
    .unwrap_or_else(|rule| panic!("SQLite chain: {rule}"));
    // Every (acquisition, layout) variant of each source version is mapped.
    for (from, _, raw, _) in &steps {
        let mut kept: Vec<i64> = raw
            .iter()
            .map(|(before, _)| sqlite_decode_user_version(*before).unwrap().1)
            .collect();
        kept.sort_unstable();
        kept.dedup();
        assert_eq!(
            kept.len(),
            6,
            "SQLite step from {from} maps every variant once"
        );
    }
    let (steps, floor) = postgresql_upgrade_steps();
    upgrade_check_chain(&as_steps(&steps), floor, POSTGRESQL_LATEST, |v| {
        Some((v, 0))
    })
    .unwrap_or_else(|rule| panic!("PostgreSQL chain: {rule}"));
}
