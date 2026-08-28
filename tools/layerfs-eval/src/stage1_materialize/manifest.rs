use super::contract::EvalResult;
use super::error::{display_error, io_error};
use super::evidence::digest::{
    append_manifest_line, ascii_argument, command_version, digest_file, is_product_source,
    sha256_bytes, sha256_file,
};
use super::prepare::{durable_write, json_escape};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn hash(path: &Path) -> EvalResult<()> {
    let output = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err("shasum failed".to_owned());
    }
    let sha256 = String::from_utf8(output.stdout)
        .map_err(display_error)?
        .split_whitespace()
        .next()
        .ok_or_else(|| "shasum returned no digest".to_owned())?
        .to_owned();
    println!(
        "{{\"path\":\"{}\",\"bytes\":{},\"sha256\":\"{}\",\"blake3\":\"{}\"}}",
        path.display(),
        fs::metadata(path).map_err(io_error)?.len(),
        sha256,
        digest_file(path)?,
    );
    Ok(())
}

pub fn manifest(
    role: &OsStr,
    commit: &OsStr,
    executable: &Path,
    build_target: &Path,
    build_log: &Path,
    output: &Path,
) -> EvalResult<()> {
    let role = ascii_argument(role, "role")?;
    let requested_commit = ascii_argument(commit, "commit")?;
    if output.exists() {
        return Err(format!(
            "refusing to replace source manifest {}",
            output.display()
        ));
    }
    let json =
        source_build_manifest_json(role, requested_commit, executable, build_target, build_log)?;
    durable_write(output, json.as_bytes())?;
    println!(
        "stage1-manifest status=PASS role={} commit={} output={}",
        role,
        resolve_commit(requested_commit)?,
        output.display()
    );
    Ok(())
}

pub(in crate::stage1_materialize) fn source_build_manifest_json(
    role: &str,
    requested_commit: &str,
    executable: &Path,
    build_target: &Path,
    build_log: &Path,
) -> EvalResult<String> {
    let executable = executable.canonicalize().map_err(io_error)?;
    let running_executable = std::env::current_exe()
        .map_err(io_error)?
        .canonicalize()
        .map_err(io_error)?;
    let build_target = build_target.canonicalize().map_err(io_error)?;
    let expected_executable = build_target
        .join("release/layerfs-eval")
        .canonicalize()
        .map_err(io_error)?;
    if executable != running_executable || executable != expected_executable {
        return Err("manifest executable is not the running clean-build output".to_owned());
    }
    let (commit, workspace_root) = clean_head_custody()?;
    let resolved_commit = resolve_commit(requested_commit)?;
    if resolved_commit != commit {
        return Err(format!(
            "manifest commit {resolved_commit} is not current HEAD {commit}"
        ));
    }
    let build_log = build_log.canonicalize().map_err(io_error)?;
    let build_log_bytes = fs::read(&build_log).map_err(io_error)?;
    let build_log_text = String::from_utf8_lossy(&build_log_bytes);
    let build_command = format!(
        "CARGO_NET_OFFLINE=true CARGO_TARGET_DIR={} cargo build --release --locked -p layerfs-eval",
        build_target.display()
    );
    let required_build_log = [
        "schema=layerfs-build-log-v1".to_owned(),
        format!("source_head_before={commit}"),
        "source_status_before=clean".to_owned(),
        format!("build_command={build_command}"),
        "build_exit_code=0".to_owned(),
        format!("source_head_after={commit}"),
        "source_status_after=clean".to_owned(),
        "Finished `release` profile".to_owned(),
    ];
    if required_build_log
        .iter()
        .any(|required| !build_log_text.contains(required))
    {
        return Err("build log does not contain the exact successful release command".to_owned());
    }
    let listed = Command::new("git")
        .args(["ls-tree", "-r", "--name-only", "-z", &commit])
        .output()
        .map_err(io_error)?;
    if !listed.status.success() {
        return Err("git ls-tree failed".to_owned());
    }
    let mut paths = listed
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| String::from_utf8(path.to_vec()).map_err(display_error))
        .collect::<EvalResult<Vec<_>>>()?;
    paths.retain(|path| {
        path.ends_with(".rs")
            || path == "Cargo.toml"
            || path == "Cargo.lock"
            || path.ends_with("/Cargo.toml")
    });
    paths.sort();
    let mut entries = Vec::with_capacity(paths.len());
    let mut aggregate = Vec::new();
    let mut product_aggregate = Vec::new();
    let mut product_files = 0_u64;
    for path in paths {
        let shown = Command::new("git")
            .args(["show", &format!("{commit}:{path}")])
            .output()
            .map_err(io_error)?;
        if !shown.status.success() {
            return Err(format!("git show failed for {path}"));
        }
        let sha256 = sha256_bytes(&shown.stdout)?;
        let blake3 = blake3::hash(&shown.stdout).to_hex().to_string();
        append_manifest_line(&mut aggregate, &path, shown.stdout.len(), &sha256, &blake3);
        let product = is_product_source(&path);
        if product {
            product_files = product_files
                .checked_add(1)
                .ok_or_else(|| "product source count overflow".to_owned())?;
            append_manifest_line(
                &mut product_aggregate,
                &path,
                shown.stdout.len(),
                &sha256,
                &blake3,
            );
        }
        entries.push(format!(
            "{{\"path\":\"{}\",\"bytes\":{},\"sha256\":\"{}\",\"blake3\":\"{}\",\"product\":{}}}",
            json_escape(&path),
            shown.stdout.len(),
            sha256,
            blake3,
            product,
        ));
    }
    let executable_sha256 = sha256_file(&executable)?;
    let executable_blake3 = digest_file(&executable)?;
    let build_log_sha256 = sha256_bytes(&build_log_bytes)?;
    let build_log_blake3 = blake3::hash(&build_log_bytes).to_hex().to_string();
    let rustc = command_version("rustc")?;
    let cargo = command_version("cargo")?;
    let json = format!(
        concat!(
            "{{\"schema\":\"layerfs-stage1-source-build-manifest-v2\",",
            "\"status\":\"PASS\",\"role\":\"{}\",\"commit\":\"{}\",",
            "\"head_matches_commit\":true,\"dirty_tree\":false,",
            "\"workspace_root\":\"{}\",\"file_count\":{},",
            "\"aggregate_sha256\":\"{}\",\"aggregate_blake3\":\"{}\",",
            "\"product_file_count\":{},\"product_aggregate_sha256\":\"{}\",",
            "\"product_aggregate_blake3\":\"{}\",",
            "\"executable_path\":\"{}\",\"executable_sha256\":\"{}\",",
            "\"executable_blake3\":\"{}\",",
            "\"build_target\":\"{}\",\"build_command\":\"{}\",",
            "\"build_log_path\":\"{}\",\"build_log_bytes\":{},",
            "\"build_log_sha256\":\"{}\",\"build_log_blake3\":\"{}\",",
            "\"deterministic_build_claim\":false,",
            "\"executable\":{{\"path\":\"{}\",\"bytes\":{},",
            "\"sha256\":\"{}\",\"blake3\":\"{}\"}},",
            "\"build\":{{\"cwd\":\"{}\",",
            "\"environment\":{{\"CARGO_NET_OFFLINE\":\"true\",",
            "\"CARGO_TARGET_DIR\":\"{}\"}},",
            "\"argv\":[\"cargo\",\"build\",\"--release\",\"--locked\",",
            "\"-p\",\"layerfs-eval\"],\"log_sha256\":\"{}\"}},",
            "\"rustc\":\"{}\",\"cargo\":\"{}\",\"files\":[{}]}}\n"
        ),
        json_escape(role),
        json_escape(&commit),
        json_escape(&workspace_root.display().to_string()),
        entries.len(),
        sha256_bytes(&aggregate)?,
        blake3::hash(&aggregate).to_hex(),
        product_files,
        sha256_bytes(&product_aggregate)?,
        blake3::hash(&product_aggregate).to_hex(),
        json_escape(&executable.display().to_string()),
        executable_sha256,
        executable_blake3,
        json_escape(&build_target.display().to_string()),
        json_escape(&build_command),
        json_escape(&build_log.display().to_string()),
        build_log_bytes.len(),
        build_log_sha256,
        build_log_blake3,
        json_escape(&executable.display().to_string()),
        fs::metadata(&executable).map_err(io_error)?.len(),
        executable_sha256,
        executable_blake3,
        json_escape(&workspace_root.display().to_string()),
        json_escape(&build_target.display().to_string()),
        build_log_sha256,
        json_escape(&rustc),
        json_escape(&cargo),
        entries.join(","),
    );
    Ok(json)
}

pub(in crate::stage1_materialize) fn git_stdout(arguments: &[&str]) -> EvalResult<String> {
    let output = Command::new("git")
        .args(arguments)
        .output()
        .map_err(io_error)?;
    if !output.status.success() {
        return Err(format!("git {} failed", arguments.join(" ")));
    }
    String::from_utf8(output.stdout)
        .map(|value| value.trim().to_owned())
        .map_err(display_error)
}

pub(in crate::stage1_materialize) fn resolve_commit(commit: &str) -> EvalResult<String> {
    git_stdout(&["rev-parse", "--verify", &format!("{commit}^{{commit}}")])
}

pub(in crate::stage1_materialize) fn clean_head_custody() -> EvalResult<(String, PathBuf)> {
    let workspace_root = PathBuf::from(git_stdout(&["rev-parse", "--show-toplevel"])?);
    let current = std::env::current_dir()
        .map_err(io_error)?
        .canonicalize()
        .map_err(io_error)?;
    let workspace_root = workspace_root.canonicalize().map_err(io_error)?;
    if current != workspace_root {
        return Err("source/build custody must run at the clean workspace root".to_owned());
    }
    let status = git_stdout(&[
        "status",
        "--porcelain=v1",
        "--untracked-files=all",
        "--ignore-submodules=none",
    ])?;
    if !status.is_empty() {
        return Err("source/build custody requires a completely clean worktree".to_owned());
    }
    Ok((resolve_commit("HEAD")?, workspace_root))
}
