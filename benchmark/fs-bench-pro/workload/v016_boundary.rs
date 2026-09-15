// v0.1.6 boundary alias roundtrip: the declared POSIX stage of
// `v016-boundary-alias-roundtrip-v1`.
//
// `benchmark-families.md` declares five commits for that case: append1 through
// the alias, append1 through the target, truncate1 through the alias, truncate1
// through the target, then an atomic replacement of the target that leaves the
// original alias holding the pre-replacement inode. The host drives one of
// those steps per invocation, so every step is a declared, separately observable
// operation and the host commits in between.
//
// Every byte comes from the family's own recipe, so the workload helper and the
// independent host oracle generate the same content from the same declaration.

use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::io::{Read, Seek, SeekFrom, Write};

use super::file_size_transition as boundary;
use super::sdk_edit_common;
use super::workspace_common::Case;
use super::Result;

fn case() -> Case {
    Case {
        id: boundary::ALIAS_ROUNDTRIP.to_owned(),
        family: boundary::FAMILY_ID,
        tier: 1,
        kind: "boundary",
    }
}

fn declared_operation(step: usize) -> Result<boundary::BoundaryOp> {
    let operations = boundary::operations(&case())?;
    operations
        .get(step - 1)
        .cloned()
        .ok_or_else(|| format!("v0.1.6 boundary alias step {step} outside the declared schedule").into())
}

fn inode_of(path: &str) -> Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(fs::metadata(path)?.ino())
}

fn length(path: &str) -> Result<u64> {
    Ok(fs::metadata(path)?.len())
}

/// Restore the declared metadata of every name this stage touched. A POSIX
/// write publishes a runtime mtime otherwise, and the replacement's temporary
/// file would carry its creation mode instead of the declared one, so both are
/// set explicitly and read back.
fn restore_declared_metadata(paths: &[&str], mode: Option<u32>) -> Result<Vec<String>> {
    let mut readback = Vec::new();
    for path in paths {
        if let Some(mode) = mode {
            fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
        }
        let file = OpenOptions::new().write(true).open(path)?;
        let stamp = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs(
                u64::try_from(boundary::DECLARED_MTIME_SECONDS).map_err(|_| "declared mtime")?,
            )
            + std::time::Duration::from_nanos(u64::from(
                boundary::DECLARED_MTIME_NANOSECONDS,
            ));
        file.set_times(
            std::fs::FileTimes::new()
                .set_accessed(stamp)
                .set_modified(stamp),
        )?;
        drop(file);
        let metadata = fs::metadata(path)?;
        let observed = metadata.permissions().mode() & 0o7777;
        if observed != boundary::TARGET_MODE {
            return Err(format!("v0.1.6 boundary alias declared mode: {path} {observed:o}").into());
        }
        readback.push(format!(
            "{path}\tmode={observed:o}\tmtime={}",
            metadata.modified()?.duration_since(std::time::UNIX_EPOCH)?.as_secs()
        ));
    }
    Ok(readback)
}

/// `v016-boundary-alias SEED STEP` — one declared step, 1-based.
pub(crate) fn run_command(args: &[String]) -> Result<()> {
    let [seed, step] = args else {
        return Err(format!(
            "usage: v016-boundary-alias SEED STEP (received {} arguments: {args:?})",
            args.len()
        )
        .into());
    };
    let seed: u8 = seed.parse()?;
    let step: usize = step.parse()?;
    if step == 0 || step > 5 {
        return Err("v0.1.6 boundary alias steps are 1 to 5".into());
    }
    let target = boundary::TARGET;
    let alias = boundary::ALIAS;
    let temporary = boundary::TEMPORARY;
    let operation = declared_operation(step)?;
    match operation {
        // Append one declared byte through the alias (step 1) or the target
        // (step 2).
        boundary::BoundaryOp::AliasAppend { len } => {
            if len != 1 {
                return Err("v0.1.6 boundary alias append length".into());
            }
            let path = if step == 1 { alias } else { target };
            let before = length(path)?;
            let bytes = boundary::replacement(&case(), seed, step - 1, len)?;
            let mut file = OpenOptions::new().append(true).open(path)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            let after = length(path)?;
            if after != before + 1 {
                return Err(
                    format!("v0.1.6 boundary alias append {path}: {before} -> {after}").into(),
                );
            }
            println!("step={step}");
            println!("operation=append");
            println!("path={path}");
            println!("before_len={before}");
            println!("after_len={after}");
            println!("payload_sha256={}", sha(&bytes));
            println!("target_ino={}", inode_of(target)?);
            println!("alias_ino={}", inode_of(alias)?);
            // The append touched the shared inode, so both names carry the
            // declared metadata afterwards.
            for row in restore_declared_metadata(&[target, alias], None)? {
                println!("declared_metadata={row}");
            }
        }
        // Truncate one declared byte through the alias (step 3) or the target
        // (step 4). The removed byte must be the previously declared append, so
        // the truncation is checked against the recipe before it happens.
        boundary::BoundaryOp::AliasTruncate { len } => {
            if len != 1 {
                return Err("v0.1.6 boundary alias truncate length".into());
            }
            // Step 3 removes the byte step 2 appended, step 4 the byte step 1
            // appended: the two appends are undone tail-first.
            let visit = if step == 3 { 1 } else { 0 };
            let path = if step == 3 { alias } else { target };
            let before = length(path)?;
            let expected = boundary::replacement(&case(), seed, visit, len)?;
            let mut reader = File::open(path)?;
            reader.seek(SeekFrom::Start(before - 1))?;
            let mut tail = vec![0u8; len as usize];
            reader.read_exact(&mut tail)?;
            drop(reader);
            if tail != expected {
                return Err("v0.1.6 boundary alias truncate does not match the recipe".into());
            }
            let file = OpenOptions::new().write(true).open(path)?;
            file.set_len(before - len)?;
            file.sync_all()?;
            drop(file);
            let after = length(path)?;
            if after + len != before {
                return Err(
                    format!("v0.1.6 boundary alias truncate {path}: {before} -> {after}").into(),
                );
            }
            println!("step={step}");
            println!("operation=truncate");
            println!("path={path}");
            println!("before_len={before}");
            println!("after_len={after}");
            println!("removed_sha256={}", sha(&expected));
            println!("target_ino={}", inode_of(target)?);
            println!("alias_ino={}", inode_of(alias)?);
            for row in restore_declared_metadata(&[target, alias], None)? {
                println!("declared_metadata={row}");
            }
        }
        // The atomic replacement: the new content is written to a temporary
        // name, made durable and closed, and only then renamed onto the target,
        // so the target becomes a new inode while the alias keeps the old one.
        boundary::BoundaryOp::AtomicReplace { len } => {
            let before_target = length(target)?;
            let before_alias = length(alias)?;
            let shared_ino = inode_of(target)?;
            let alias_ino = inode_of(alias)?;
            if shared_ino != alias_ino {
                return Err(format!(
                    "v0.1.6 boundary alias replacement requires one shared inode, observed {shared_ino} and {alias_ino}"
                )
                .into());
            }
            let bytes = boundary::atomic_replacement(&case(), seed, step - 1, len)?;
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            // The replacement inherits the temporary file's inode, so the
            // declared mode is applied before the rename publishes it.
            restore_declared_metadata(&[temporary], Some(boundary::TARGET_MODE))?;
            if length(temporary)? != len {
                return Err("v0.1.6 boundary alias temporary length".into());
            }
            fs::rename(temporary, target)?;
            // A fresh handle proves the replacement is visible by name.
            let after_target = length(target)?;
            let after_alias = length(alias)?;
            let after_ino = inode_of(target)?;
            if after_target != len {
                return Err(format!(
                    "v0.1.6 boundary alias replacement target length {after_target}, declared {len}"
                )
                .into());
            }
            if after_ino == alias_ino {
                return Err(
                    "v0.1.6 boundary alias replacement did not split the inode".into(),
                );
            }
            if after_alias != before_alias {
                return Err("v0.1.6 boundary alias replacement changed the alias".into());
            }
            println!("step={step}");
            println!("operation=atomic-replace");
            println!("path={target}");
            println!("temporary={temporary}");
            println!("before_target_len={before_target}");
            println!("before_alias_len={before_alias}");
            println!("after_target_len={after_target}");
            println!("after_alias_len={after_alias}");
            println!("pre_replacement_ino={alias_ino}");
            println!("target_ino={after_ino}");
            println!("alias_ino={}", inode_of(alias)?);
            println!("content_sha256={}", sha(&bytes));
            for row in restore_declared_metadata(&[target, alias], None)? {
                println!("declared_metadata={row}");
            }
        }
        other => {
            return Err(
                format!("v0.1.6 boundary alias step {step} is {other:?}, not a POSIX alias step")
                    .into(),
            )
        }
    }
    Ok(())
}

fn sha(bytes: &[u8]) -> String {
    sdk_edit_common::sha256_hex(bytes)
}
