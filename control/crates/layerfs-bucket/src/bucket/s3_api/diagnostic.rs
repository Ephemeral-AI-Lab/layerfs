//! Narrow diagnostic requests: one list page and version-owned temporary probes.
use super::client::{refused, xml_text};
use super::http::Body;
use super::{deadline, sign, S3Client};
use crate::bucket::RemoteError;
use std::collections::BTreeMap;
use std::time::Instant;

/// Reserved diagnostic namespace; none of its objects are Store manifests.
pub const DIAGNOSTIC_PREFIX: &str = "_layerfs_doctor";

/// A successfully created or reconciled immutable object version owned by a probe.
/// Fields cannot be manufactured by callers to authorize deletion of other keys.
pub struct DiagnosticObject {
    name: String,
    version: String,
    token: String,
    binding: (String, String, String),
}
impl DiagnosticObject {
    /// Relative generated key, never an existing backup's key.
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Exact non-null immutable version this probe owns.
    pub fn version(&self) -> &str {
        &self.version
    }
}

impl S3Client {
    fn diagnostic_binding(&self) -> (String, String, String) {
        (
            self.config().endpoint.clone(),
            self.config().bucket.clone(),
            self.config().prefix.clone(),
        )
    }
    fn diagnostic_owner(&self, object: &DiagnosticObject) -> Result<(), RemoteError> {
        if object.binding != self.diagnostic_binding() || object.name != probe_name(&object.token)?
        {
            return Err(RemoteError::Invalid(
                "probe belongs to another destination".into(),
            ));
        }
        Ok(())
    }

    /// Authenticated prefix-scoped ListObjectsV2, one page with max-keys=1.
    /// Returned keys are validated and discarded rather than disclosed.
    pub fn diagnostic_list(&self, until: Instant) -> Result<(), RemoteError> {
        deadline::within(until, || {
            let prefix = self.config().key("");
            let reply = self.send(
                "GET",
                &format!("/{}", self.config().bucket),
                &[
                    ("list-type", "2".into()),
                    ("prefix", prefix.clone()),
                    ("max-keys", "1".into()),
                ],
                BTreeMap::new(),
                Body::Empty,
                sign::hash(&[]),
                None,
            )?;
            if reply.status != 200 {
                return Err(refused(&reply));
            }
            let text = std::str::from_utf8(&reply.body)
                .map_err(|_| RemoteError::Integrity("list response encoding".into()))?;
            if !text.contains("<ListBucketResult")
                || !matches!(
                    xml_text(text, "IsTruncated").as_deref(),
                    Some("true" | "false")
                )
            {
                return Err(RemoteError::Integrity("list response shape".into()));
            }
            let entries: Vec<_> = text.split("<Contents>").skip(1).collect();
            if entries.len() > 1
                || entries
                    .iter()
                    .any(|entry| xml_text(entry, "Key").is_none_or(|key| !key.starts_with(&prefix)))
            {
                return Err(RemoteError::Integrity(
                    "list response exceeded requested prefix or page size".into(),
                ));
            }
            Ok(())
        })
    }

    /// Safe write-test prerequisite: versioning must be positively Enabled.
    /// Suspended/unversioned/denied/unsupported destinations are never written.
    pub fn diagnostic_versioning(&self, until: Instant) -> Result<(), RemoteError> {
        deadline::within(until, || {
            let reply = self.send(
                "GET",
                &format!("/{}", self.config().bucket),
                &[("versioning", String::new())],
                BTreeMap::new(),
                Body::Empty,
                sign::hash(&[]),
                None,
            )?;
            if reply.status != 200 {
                return Err(refused(&reply));
            }
            let text = String::from_utf8_lossy(&reply.body);
            if xml_text(&text, "Status").as_deref() != Some("Enabled") {
                return Err(RemoteError::Unsupported(
                    "safe temporary probe requires enabled immutable object versioning".into(),
                ));
            }
            Ok(())
        })
    }

    /// Generated probe key within the saved namespace; validates a 256-bit token.
    pub fn diagnostic_key(&self, token: &str) -> Result<String, RemoteError> {
        Ok(self.config().key(&probe_name(token)?))
    }

    /// One conditional single-part create; never overwrites or falls back.
    /// Caller must first positively establish enabled versioning.
    pub fn diagnostic_create(
        &self,
        token: &str,
        until: Instant,
    ) -> Result<DiagnosticObject, RemoteError> {
        let name = probe_name(token)?;
        let body = payload(token);
        deadline::within(until, || {
            let reply = self.send(
                "PUT",
                &self.config().object_path(&name)?,
                &[],
                BTreeMap::from([
                    ("if-none-match".into(), "*".into()),
                    ("x-amz-meta-layerfs-doctor".into(), token.into()),
                    ("content-type".into(), "application/octet-stream".into()),
                ]),
                Body::Bytes(&body),
                sign::hash(&body),
                None,
            )?;
            if reply.status == 501 {
                return Err(RemoteError::Unsupported("provider does not implement the conditional probe operation; no fallback attempted".into()));
            }
            if reply.status >= 500 || ((200..300).contains(&reply.status) && reply.status != 200) {
                return Err(RemoteError::Uncertain(
                    "conditional PUT response does not establish whether the object was committed"
                        .into(),
                ));
            }
            if reply.status != 200 {
                return Err(refused(&reply));
            }
            let version = version(&reply.headers)?;
            Ok(DiagnosticObject {
                name,
                version,
                token: token.into(),
                binding: self.diagnostic_binding(),
            })
        })
    }

    /// Reconciles only this generated key after an uncertain PUT, never resends it.
    /// Exact token, bytes and a non-null response version establish ownership.
    pub fn diagnostic_reconcile(
        &self,
        token: &str,
        until: Instant,
    ) -> Result<DiagnosticObject, RemoteError> {
        let name = probe_name(token)?;
        deadline::within(until, || {
            let reply = self.send(
                "GET",
                &self.config().object_path(&name)?,
                &[],
                BTreeMap::new(),
                Body::Empty,
                sign::hash(&[]),
                None,
            )?;
            if reply.status != 200 {
                return Err(refused(&reply));
            }
            if reply.body != payload(token)
                || reply
                    .headers
                    .get("x-amz-meta-layerfs-doctor")
                    .map(String::as_str)
                    != Some(token)
            {
                return Err(RemoteError::Integrity(
                    "generated key does not contain this probe's token and bytes; left untouched"
                        .into(),
                ));
            }
            Ok(DiagnosticObject {
                name,
                version: version(&reply.headers)?,
                token: token.into(),
                binding: self.diagnostic_binding(),
            })
        })
    }

    /// Reads only the acknowledged/reconciled immutable version and compares bytes.
    pub fn diagnostic_read(
        &self,
        object: &DiagnosticObject,
        until: Instant,
    ) -> Result<(), RemoteError> {
        self.diagnostic_owner(object)?;
        deadline::within(until, || {
            let reply = self.send(
                "GET",
                &self.config().object_path(&object.name)?,
                &[("versionId", object.version.clone())],
                BTreeMap::new(),
                Body::Empty,
                sign::hash(&[]),
                None,
            )?;
            if reply.status != 200 {
                return Err(refused(&reply));
            }
            if version(&reply.headers)? != object.version
                || reply.body != payload(&object.token)
                || reply.headers.get("x-amz-meta-layerfs-doctor") != Some(&object.token)
            {
                return Err(RemoteError::Integrity(
                    "probe version, token or downloaded bytes differ".into(),
                ));
            }
            Ok(())
        })
    }

    /// Deletes only the immutable owned version. No HEAD/unconditional delete.
    /// Lost responses remain uncertain and are never automatically resent.
    pub fn diagnostic_delete(
        &self,
        object: &DiagnosticObject,
        until: Instant,
    ) -> Result<(), RemoteError> {
        self.diagnostic_owner(object)?;
        deadline::within(until, || {
            let reply = self.send(
                "DELETE",
                &self.config().object_path(&object.name)?,
                &[("versionId", object.version.clone())],
                BTreeMap::new(),
                Body::Empty,
                sign::hash(&[]),
                None,
            )?;
            if reply.status == 501 {
                return Err(RemoteError::Unsupported(
                    "provider does not implement version-specific cleanup; no fallback attempted"
                        .into(),
                ));
            }
            if reply.status >= 500
                || ((200..300).contains(&reply.status) && !matches!(reply.status, 200 | 204))
            {
                return Err(RemoteError::Uncertain(
                    "DELETE response does not establish whether the owned version was removed"
                        .into(),
                ));
            }
            if !matches!(reply.status, 200 | 204) {
                return Err(refused(&reply));
            }
            if version(&reply.headers)? != object.version {
                return Err(RemoteError::Uncertain(
                    "delete reply did not identify the owned immutable version".into(),
                ));
            }
            Ok(())
        })
    }
}

fn probe_name(token: &str) -> Result<String, RemoteError> {
    if token.len() != 64
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(RemoteError::Invalid(
            "diagnostic token must contain 256 random bits as lowercase hex".into(),
        ));
    }
    Ok(format!("{DIAGNOSTIC_PREFIX}/{token}/probe"))
}
fn payload(token: &str) -> Vec<u8> {
    format!("LayerFS diagnostic probe v1\n{token}\n").into_bytes()
}
fn version(headers: &BTreeMap<String, String>) -> Result<String, RemoteError> {
    headers
        .get("x-amz-version-id")
        .filter(|version| {
            !version.is_empty() && version.as_str() != "null" && version.len() <= 1024
        })
        .cloned()
        .ok_or_else(|| {
            RemoteError::Uncertain(
                "reply supplied no immutable non-null probe version; no deletion authorized".into(),
            )
        })
}
