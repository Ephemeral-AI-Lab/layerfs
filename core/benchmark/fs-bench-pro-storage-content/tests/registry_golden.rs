//! The registry must match its generated golden table row for row.
//!
//! The golden file is a *rendering* of the registry, never its source of truth: a
//! hand-edited table compares equal to nothing, because this test recomputes the
//! string from the registry and compares it with `include_str!`. A self-referential
//! hash that no test recomputes would be decoration; a golden diff names the
//! drifted row.

use fs_bench_storage_content::registry;

#[test]
fn golden_table_matches_the_registry() {
    let rendered = registry::render_tsv();
    let golden = include_str!("golden/registry.tsv");
    assert_eq!(
        rendered, golden,
        "tests/golden/registry.tsv drifted from the registry; regenerate it with\n  \
         fs-bench-storage-content --emit-registry-tsv tests/golden/registry.tsv"
    );
}

#[test]
fn golden_table_names_every_row_once() {
    let golden = include_str!("golden/registry.tsv");
    let ids: Vec<&str> = golden
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty())
        .map(|line| line.split('\t').next().unwrap_or_default())
        .collect();
    assert_eq!(ids.len(), registry::REGISTERED_ROWS);
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "duplicate case ID in the golden table");
}
