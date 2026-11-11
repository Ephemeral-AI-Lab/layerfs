//! The Engine owns this content-to-image mapping, shared by all LayerFS homes.
//! The archive's config digest is not necessarily the ID returned by Docker.
use super::power::load_image;
use super::unix::run;
use crate::error::{Code, MachineError};
use crate::spec::Spec;
use std::path::Path;

pub(crate) fn cached_image(
    spec: &Spec,
    archive: &Path,
    content: &str,
    previous: Option<&str>,
) -> Result<String, MachineError> {
    let hex = content
        .strip_prefix("sha256:")
        .filter(|s| digest(s))
        .ok_or_else(|| {
            MachineError::new(
                Code::InvalidInput,
                "the Sandbox image content ID is not sha256",
            )
        })?;
    let tag = format!("layerfs-cache/sandbox:{hex}");
    if let Some(id) = inspect(spec, &tag)? {
        return Ok(id);
    }
    // Upgrade a previous per-home record without loading its archive again.
    let id = match previous {
        Some(id) => inspect(spec, id)?,
        None => None,
    };
    let id = match id {
        Some(id) => id,
        None => load_image(spec, archive)?,
    };
    run(
        spec,
        &["image", "tag", &id, &tag],
        "caching the Sandbox image",
    )?
    .answer("caching the Sandbox image")?;
    Ok(id)
}

fn digest(hex: &str) -> bool {
    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

fn inspect(spec: &Spec, image: &str) -> Result<Option<String>, MachineError> {
    let out = run(
        spec,
        &["image", "inspect", "--format", "{{.Id}}", image],
        "checking the cached Sandbox image",
    )?;
    if !out.ok() {
        let said = out.stderr.to_ascii_lowercase();
        if said.contains("no such image") || said.contains("no such object") {
            return Ok(None);
        }
        return Err(MachineError::new(
            Code::Failed,
            format!("checking the cached Sandbox image: {}", out.said()),
        ));
    }
    let id = out.stdout.trim();
    if !id.strip_prefix("sha256:").is_some_and(digest) {
        return Err(MachineError::new(
            Code::Failed,
            "Docker image inspect returned no image ID",
        ));
    }
    Ok(Some(id.to_owned()))
}
