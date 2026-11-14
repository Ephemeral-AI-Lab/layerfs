//! Per-file bundle installation preserves unchanged large Sandbox archives.
//! The manifest is published last, so an interrupted copy is never sealed.
//! The Sandbox runtime layer (R5, `runtime/`) is one folder, copied in whole
//! when its hash (`sandbox_runtime.hash`) changes or the guest lacks it; a
//! running Sandbox keeps its own copy, so replacing it ends nothing.
use super::guest::Guest;
use crate::error::{Code, MachineError};
use crate::facts;
use crate::spec::{Bundle, Spec, BUNDLE};

const FILES: [&str; 4] = [
    "layerfs",
    "layerfs-daemon",
    "layerfs-sandbox-helper",
    "sandbox-image.tar",
];

/// `false` is a legacy bundle without file identities; its whole-folder path stays.
pub(crate) fn install(spec: &Spec, guest: &Guest, bundle: &Bundle) -> Result<bool, MachineError> {
    let manifest = match std::fs::read_to_string(bundle.dir.join("bundle.json")) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(MachineError::new(
                Code::InvalidInput,
                format!("reading the bundle manifest: {e}"),
            ))
        }
    };
    let fields = FILES
        .iter()
        .map(|name| file_hash(&manifest, name))
        .collect::<Result<Vec<_>, _>>()?;
    if fields.iter().all(Option::is_none) {
        return Ok(false);
    }
    // Only the image is optional for bundles which supply their own Sandbox image.
    if fields[..3].iter().any(Option::is_none) {
        return Err(MachineError::new(
            Code::InvalidInput,
            "the bundle manifest lacks runtime file identities",
        ));
    }
    let old = guest
        .read_file(&spec.tools, &format!("{BUNDLE}/bundle.json"))?
        .unwrap_or_default();
    let paths: Vec<String> = FILES
        .iter()
        .map(|name| format!("{BUNDLE}/{name}"))
        .collect();
    let args: Vec<&str> = paths.iter().map(String::as_str).collect();
    let present = guest.script(
        &spec.tools,
        "checking installed bundle artifacts",
        r#"for f in "$@"; do test -f "$f" && printf '%s\n' "$f"; done; true"#,
        &args,
    )?;
    let mut changed = false;
    for ((name, wanted), path) in FILES.iter().zip(fields).zip(&paths) {
        let Some(wanted) = wanted else { continue };
        let previous = file_hash(&old, name).ok().flatten();
        if previous.as_deref() != Some(wanted.as_str()) || !present.lines().any(|p| p == path) {
            // Extract into a separate directory, then rename. Never overwrite an
            // executable inode which a worker may still have open (ETXTBSY).
            changed = true;
            let stage = format!("{BUNDLE}/.install");
            guest.copy_in(&spec.tools, &bundle.dir, Some(name), &stage)?;
            guest.script(
                &spec.tools,
                "publishing a bundle artifact",
                r#"mv -f "$1/$2" "$3/$2""#,
                &[&stage, name, BUNDLE],
            )?;
        }
    }
    if let Some(wanted) = runtime_hash(&manifest)? {
        let previous = runtime_hash(&old).ok().flatten();
        let held = guest.script(
            &spec.tools,
            "checking the installed Sandbox runtime layer",
            r#"test -d "$1/runtime/lib" && echo held; true"#,
            &[BUNDLE],
        )?;
        if previous.as_deref() != Some(wanted.as_str()) || held.trim() != "held" {
            changed = true;
            let stage = format!("{BUNDLE}/.install");
            guest.script(
                &spec.tools,
                "clearing a staged Sandbox runtime layer",
                r#"rm -rf "$1/runtime""#,
                &[&stage],
            )?;
            guest.copy_in(&spec.tools, &bundle.dir, Some(RUNTIME), &stage)?;
            guest.script(
                &spec.tools,
                "publishing the Sandbox runtime layer",
                r#"set -eu
rm -rf "$2/runtime.old"
if [ -e "$2/runtime" ]; then mv "$2/runtime" "$2/runtime.old"; fi
mv "$1/runtime" "$2/runtime"
rm -rf "$2/runtime.old""#,
                &[&stage, BUNDLE],
            )?;
        }
    }
    if !changed && old == manifest {
        return Ok(true);
    }
    guest.write_file(&spec.tools, &format!("{BUNDLE}/bundle.json"), &manifest)?;
    guest.write_file(
        &spec.tools,
        &format!("{BUNDLE}/.sealed"),
        &format!("{}\n", bundle.hash),
    )?;
    Ok(true)
}

/// The bundle's Sandbox runtime layer folder.
const RUNTIME: &str = "runtime";

/// The runtime layer's hash (`sandbox_runtime.hash`), when the bundle has one.
fn runtime_hash(manifest: &str) -> Result<Option<String>, MachineError> {
    if manifest.is_empty() {
        return Ok(None);
    }
    let hash = facts::json_string(manifest, &["sandbox_runtime", "hash"]).map_err(|e| {
        MachineError::new(Code::InvalidInput, format!("invalid bundle manifest: {e}"))
    })?;
    if hash
        .as_ref()
        .is_some_and(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(MachineError::new(
            Code::InvalidInput,
            "the bundle's runtime layer identity is not sha256",
        ));
    }
    Ok(hash)
}

fn file_hash(manifest: &str, name: &str) -> Result<Option<String>, MachineError> {
    if manifest.is_empty() {
        return Ok(None);
    }
    let hash = facts::json_string(manifest, &["files", name]).map_err(|e| {
        MachineError::new(Code::InvalidInput, format!("invalid bundle manifest: {e}"))
    })?;
    if hash
        .as_ref()
        .is_some_and(|s| s.len() != 64 || !s.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return Err(MachineError::new(
            Code::InvalidInput,
            "the bundle file identity is not sha256",
        ));
    }
    Ok(hash)
}
