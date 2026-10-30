//! What a home's remotes hold, without downloading (#506 E3, E8, E9): one
//! stat of a Store's linked file (#509), one listing of every Store file on
//! a remote, and
//! the providers this build can add. Each request is sent once; a failure
//! is answered with a stable code (Unavailable, Denied, Unknown, …).
use crate::job::{refused, Outcome};
use crate::registry::text;
use crate::sync::driver;
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_bridge::sdk_values::hex;
use layerfs_store_service::remote::storage::RemoteError;
use layerfs_store_service::remote::{self, Checked, Copy};
use std::time::{SystemTime, UNIX_EPOCH};

/// The refusal of a failed remote request.
pub(crate) fn remote_refusal(error: &RemoteError) -> SdkRefusal {
    let code = match error {
        RemoteError::Invalid(_) => "InvalidInput",
        RemoteError::Unavailable(_) => "Unavailable",
        RemoteError::Refused {
            status: 401 | 403, ..
        } => "Denied",
        RemoteError::Refused { status: 404, .. } => "NoSuchBucket",
        RemoteError::Uncertain(_) | RemoteError::PartsLeft { .. } => "Unknown",
        RemoteError::Unsupported(_) => "Unsupported",
        _ => "Failed",
    };
    refused(code, error.to_string())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

fn digest(value: Option<[u8; 32]>) -> SdkValue {
    value.map_or(SdkValue::Null, |d| SdkValue::text(hex(&d)))
}

fn copy_fields(copy: &Copy) -> Vec<(&'static str, SdkValue)> {
    vec![
        ("bytes", SdkValue::Int(copy.bytes as i64)),
        ("sha256", digest(copy.sha256)),
        ("heads", digest(copy.heads)),
        (
            "modified",
            copy.modified
                .map_or(SdkValue::Null, |at| SdkValue::Int(at as i64)),
        ),
    ]
}

fn record(fields: Vec<(&'static str, SdkValue)>) -> SdkValue {
    SdkValue::Map(
        fields
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect(),
    )
}

/// `runtime.store.check { home, store, remote, file, credentials? }`: one
/// stat of the remote file the Store links to: `{ store, remote, file,
/// present, bytes, sha256, heads, modified, checkedAt }` (times in ms since
/// the epoch).
pub(crate) fn check(args: &SdkValue) -> Outcome {
    let store = text(args, "store")?;
    let file = text(args, "file")?;
    let (name, driver) = driver(args)?;
    let checked = remote::check(driver.as_ref(), file).map_err(|e| remote_refusal(&e))?;
    let mut fields = vec![
        ("store", SdkValue::text(store)),
        ("remote", SdkValue::text(&name)),
        ("file", SdkValue::text(file)),
    ];
    match checked {
        Checked::Absent => fields.extend([
            ("present", SdkValue::Bool(false)),
            ("bytes", SdkValue::Null),
            ("sha256", SdkValue::Null),
            ("heads", SdkValue::Null),
            ("modified", SdkValue::Null),
        ]),
        Checked::Present(copy) => {
            fields.push(("present", SdkValue::Bool(true)));
            fields.extend(copy_fields(&copy));
        }
    }
    fields.push(("checkedAt", SdkValue::Int(now_ms())));
    Ok(record(fields))
}

/// `runtime.remote.list_stores { home, remote, credentials? }`: every Store
/// file on the remote, `[{ file, bytes, sha256, heads, modified }]`.
pub(crate) fn list_stores(args: &SdkValue) -> Outcome {
    let (_, driver) = driver(args)?;
    let copies = remote::list_stores(driver.as_ref()).map_err(|e| remote_refusal(&e))?;
    Ok(SdkValue::List(
        copies
            .iter()
            .map(|copy| {
                let mut fields = vec![("file", SdkValue::text(&copy.file))];
                fields.extend(copy_fields(copy));
                record(fields)
            })
            .collect(),
    ))
}

fn field(name: &str, label: &str, kind: &str, required: bool) -> SdkValue {
    SdkValue::map([
        ("name", SdkValue::text(name)),
        ("label", SdkValue::text(label)),
        ("kind", SdkValue::text(kind)),
        ("required", SdkValue::Bool(required)),
    ])
}

fn provider(id: &str, label: &str, available: Result<(), &str>, fields: Vec<SdkValue>) -> SdkValue {
    SdkValue::map([
        ("id", SdkValue::text(id)),
        ("label", SdkValue::text(label)),
        ("available", SdkValue::Bool(available.is_ok())),
        (
            "reason",
            available.err().map_or(SdkValue::Null, SdkValue::text),
        ),
        ("fields", SdkValue::List(fields)),
    ])
}

/// `runtime.remote.providers { home }`: the providers `runtime.remote.add`
/// takes, `[{ id, label, available, reason, fields: [{ name, label, kind,
/// required }] }]`; `kind` is text, url, secret-ref, select or number.
pub(crate) fn providers(_args: &SdkValue) -> Outcome {
    let tls: Result<(), &str> = if cfg!(feature = "tls") {
        Ok(())
    } else {
        Err("this LayerFS build has no TLS; build the binding with --features tls")
    };
    let common = |fields: &mut Vec<SdkValue>| {
        fields.push(field("bucket", "Bucket", "text", true));
        fields.push(field("prefix", "Prefix", "text", false));
        fields.push(field(
            "credentials",
            "Credentials reference",
            "secret-ref",
            false,
        ));
        fields.push(field("partSize", "Part size (bytes)", "number", false));
    };
    let mut minio = vec![field("endpoint", "Endpoint", "url", true)];
    common(&mut minio);
    minio.insert(3, field("region", "Region", "text", false));
    let mut r2 = vec![
        field("account", "Account ID", "text", true),
        field("jurisdiction", "Jurisdiction", "select", false),
    ];
    common(&mut r2);
    Ok(SdkValue::List(vec![
        provider("minio", "MinIO", Ok(()), minio),
        provider("r2", "Cloudflare R2", tls, r2),
        provider(
            "local_folder",
            "Folder",
            Ok(()),
            vec![field("path", "Folder path", "text", true)],
        ),
    ]))
}
