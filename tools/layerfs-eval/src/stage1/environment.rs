use super::artifact::{command_bytes, command_output, display_error, io_error, json_escape};
use super::model::Environment;
use crate::stage1_fixture::{
    assert_apfs, fixture_root, hash_file, workspace_root, BaseManifest, EvalResult, Master,
    BUFFER_BYTES, FILE_BYTES,
};
use std::fs;
pub(crate) fn base<'a>(master: &'a Master, name: &str) -> EvalResult<&'a BaseManifest> {
    master
        .bases
        .get(name)
        .ok_or_else(|| format!("fixture base {name} missing"))
}
pub(crate) fn environment() -> EvalResult<Environment> {
    let git_commit = command_output("git", &["rev-parse", "HEAD"])?
        .trim()
        .to_owned();
    let (dirty_tree_blake3, source_tree_blake3, source_files) = source_fingerprints()?;
    let executable = std::env::current_exe().map_err(io_error)?;
    Ok(Environment {
        git_commit,
        dirty_tree_blake3,
        source_tree_blake3,
        source_file_count: source_files.len() as u64,
        source_files,
        cargo_lock_blake3: hash_file(&workspace_root().join("Cargo.lock"))?,
        executable_blake3: hash_file(&executable)?,
        build_profile: if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        debug_assertions: cfg!(debug_assertions),
        uname: command_output("uname", &["-a"])?.trim().to_owned(),
        macos: command_output("sw_vers", &[]).unwrap_or_else(|_| "Unavailable".to_owned()),
        apfs_identity: assert_apfs(&fixture_root()).unwrap_or_else(|_| "Unavailable".to_owned()),
    })
}
pub(crate) fn preparation_source_context_json() -> EvalResult<String> {
    let value = environment()?;
    let source_files = value
        .source_files
        .iter()
        .map(|path| format!("\"{}\"", json_escape(path)))
        .collect::<Vec<_>>()
        .join(",");
    Ok(format!(
        concat!(
            "{{\"git_commit\":\"{}\",\"dirty_tree_blake3\":\"{}\",",
            "\"source_tree_blake3\":\"{}\",\"source_files\":[{}],",
            "\"cargo_lock_blake3\":\"{}\",\"executable_blake3\":\"{}\",",
            "\"build_profile\":\"{}\",\"debug_assertions\":{}}}"
        ),
        json_escape(&value.git_commit),
        value.dirty_tree_blake3,
        value.source_tree_blake3,
        source_files,
        value.cargo_lock_blake3,
        value.executable_blake3,
        value.build_profile,
        value.debug_assertions,
    ))
}
pub(crate) fn source_fingerprints() -> EvalResult<(String, String, Vec<String>)> {
    let diff = command_bytes("git", &["diff", "--binary", "HEAD"])?;
    let untracked = command_bytes("git", &["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut paths = untracked
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).map_err(display_error))
        .collect::<EvalResult<Vec<_>>>()?;
    paths.sort();
    let mut dirty = blake3::Hasher::new();
    dirty.update(&diff);
    for path in &paths {
        dirty.update(path.as_bytes());
        dirty.update(&[0]);
        let bytes = fs::read(workspace_root().join(path)).map_err(io_error)?;
        dirty.update(blake3::hash(&bytes).as_bytes());
    }
    let tracked = command_bytes(
        "git",
        &[
            "ls-files",
            "-co",
            "--exclude-standard",
            "-z",
            "--",
            "*.rs",
            "Cargo.toml",
            "Cargo.lock",
        ],
    )?;
    let mut source_paths = tracked
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).map_err(display_error))
        .collect::<EvalResult<Vec<_>>>()?;
    source_paths.retain(|path| workspace_root().join(path).is_file());
    source_paths.sort();
    source_paths.dedup();
    let mut source = blake3::Hasher::new();
    for path in &source_paths {
        source.update(path.as_bytes());
        source.update(&[0]);
        source.update(&fs::read(workspace_root().join(path)).map_err(io_error)?);
    }
    Ok((
        dirty.finalize().to_hex().to_string(),
        source.finalize().to_hex().to_string(),
        source_paths,
    ))
}
pub(crate) fn environment_json(value: &Environment) -> String {
    let source_files = value
        .source_files
        .iter()
        .map(|path| format!("\"{}\"", json_escape(path)))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1-environment-v2\",\"git_commit\":\"{}\",",
            "\"dirty_tree_blake3\":\"{}\",\"source_tree_blake3\":\"{}\",",
            "\"source_file_count\":{},\"source_files\":[{}],\"cargo_lock_blake3\":\"{}\",",
            "\"executable_blake3\":\"{}\",\"build_profile\":\"{}\",",
            "\"debug_assertions\":{},\"maximum_user_regular_file_bytes\":{},",
            "\"largest_product_buffer_bytes\":{},\"uname\":\"{}\",\"macos\":\"{}\",",
            "\"apfs_identity\":\"{}\",",
            "\"build_command\":\"cargo build -p layerfs-eval --release\",",
            "\"command\":\"layerfs-eval stage1 run single-file <run-directory>\"}}\n"
        ),
        json_escape(&value.git_commit),
        value.dirty_tree_blake3,
        value.source_tree_blake3,
        value.source_file_count,
        source_files,
        value.cargo_lock_blake3,
        value.executable_blake3,
        value.build_profile,
        value.debug_assertions,
        FILE_BYTES,
        BUFFER_BYTES,
        json_escape(&value.uname),
        json_escape(&value.macos),
        json_escape(&value.apfs_identity),
    )
}
