//! Strict public image input, shared by every SDK frontend.
use crate::family::CreateRequest;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::outcome::refused;

impl CreateRequest {
    /// Parses the image digest or `{ archive, digest? }` and CLI archive alias.
    pub fn image_choice(
        image: Option<&SdkValue>,
        alias: Option<&str>,
    ) -> Result<(Option<String>, Option<String>), SdkRefusal> {
        let invalid = || {
            refused("InvalidInput", "image must be a sha256 digest or { archive: absolute path, digest?: sha256 digest }")
        };
        let (digest, archive) = match image {
            None | Some(SdkValue::Null) => (None, alias.map(str::to_owned)),
            Some(SdkValue::Text(value)) => (Some(value.clone()), alias.map(str::to_owned)),
            Some(SdkValue::Map(fields)) if alias.is_none() => {
                if fields
                    .iter()
                    .any(|(key, _)| !matches!(key.as_str(), "archive" | "digest"))
                {
                    return Err(invalid());
                }
                let archive = image
                    .and_then(|v| v.get_text("archive"))
                    .ok_or_else(invalid)?;
                let digest = match image.and_then(|v| v.get("digest")) {
                    None => None,
                    Some(SdkValue::Text(value)) => Some(value.clone()),
                    _ => return Err(invalid()),
                };
                (digest, Some(archive.to_owned()))
            }
            _ => return Err(invalid()),
        };
        if digest
            .as_deref()
            .is_some_and(|d| !crate::image_artifact::digest(d))
        {
            return Err(invalid());
        }
        if archive
            .as_deref()
            .is_some_and(|p| !std::path::Path::new(p).is_absolute())
        {
            return Err(invalid());
        }
        Ok((digest, archive))
    }
}
