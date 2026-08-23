use std::env;
use std::fs;
use std::path::PathBuf;

fn replace_once(source: String, from: &str, to: &str) -> String {
    assert_eq!(source.matches(from).count(), 1, "transport overlay drift");
    source.replacen(from, to, 1)
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let repository = manifest.ancestors().nth(6).unwrap();
    let source = repository.join("crates/layerfs-engine/src/bin/phase4_create_edit_benchmark.rs");
    let g3 = repository.join("crates/layerfs-engine/src/bin/phase4_g3_materialization.rs");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    let retained = fs::read_to_string(&source).unwrap();
    let retained = retained
        .lines()
        .enumerate()
        .map(|(index, line)| {
            if index < 4 {
                line.strip_prefix("//!")
                    .map_or_else(|| line.to_owned(), |rest| format!("//{rest}"))
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let retained = replace_once(
        retained,
        r#"    #[cfg(test)]
    AfterCommitDifferentHead,
    #[cfg(test)]
    AfterCommitUnavailable,"#,
        r#"    AfterCommitDifferentHead,
    AfterCommitUnavailable,"#,
    );
    let retained = replace_once(
        retained,
        r#"    record.sync_all()?;
    println!(
        "fixture={} size_bytes={size} raw_fingerprint={fingerprint} cdc_references={references} cdc_sequence={sequence}",
        source.display(),
    );
    Ok(())
}

fn prepare_fixed_radix_acceptance_fixtures"#,
        r#"    record.sync_all()?;
    Ok(())
}

fn prepare_fixed_radix_acceptance_fixtures"#,
    );
    let retained = replace_once(
        retained,
        "    #[cfg(test)]\n    fn install_different_complete_head_after_commit(",
        "    fn install_different_complete_head_after_commit(",
    );
    let retained = replace_once(
        retained,
        r#"        #[cfg(test)]
        {
            lost_ack |= matches!(
                fault,
                Some(PublishFault::AfterCommitDifferentHead | PublishFault::AfterCommitUnavailable)
            );
        }"#,
        r#"        lost_ack |= matches!(
            fault,
            Some(PublishFault::AfterCommitDifferentHead | PublishFault::AfterCommitUnavailable)
        );"#,
    );
    let retained = replace_once(
        retained,
        "            #[cfg(test)]\n            if fault == Some(PublishFault::AfterCommitDifferentHead) {",
        "            if fault == Some(PublishFault::AfterCommitDifferentHead) {",
    );
    let retained = replace_once(
        retained,
        "            #[cfg(test)]\n            let unavailable_path = if fault == Some(PublishFault::AfterCommitUnavailable) {",
        "            let unavailable_path = if fault == Some(PublishFault::AfterCommitUnavailable) {",
    );
    let retained = replace_once(
        retained,
        "            #[cfg(test)]\n            if let Some(hidden) = unavailable_path {",
        "            if let Some(hidden) = unavailable_path {",
    );
    fs::write(output.join("retained_control.rs"), retained).unwrap();
    fs::copy(&g3, output.join("phase4_g3_materialization.rs")).unwrap();

    println!("cargo:rerun-if-changed={}", source.display());
    println!("cargo:rerun-if-changed={}", g3.display());
}
