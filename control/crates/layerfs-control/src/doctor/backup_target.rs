//! One saved backup alias: bounded signed listing and optional version-owned probe.
use super::target::{Budget, Checks};
use layerfs_bridge::control::SdkValue;
use layerfs_bucket::bucket::{
    s3_api::{DiagnosticObject, S3Client},
    RemoteError,
};
use layerfs_store::backup::record::BucketRecord;
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) fn diagnose(
    home: &Path,
    name: &str,
    write_test: bool,
    budget: &Budget,
    checks: &mut Checks,
) {
    let record = match BucketRecord::of(home, name) {
        Ok(record) => record,
        Err(_) => {
            checks.fail(
                "backup.configuration",
                "saved backup alias is absent, unreadable or invalid",
            );
            blocked_requests(checks, write_test, "backup.configuration");
            return;
        }
    };
    let fields = record.to_record();
    if fields.get_int("v") != Some(layerfs_home::home::RECORD_VERSION)
        || fields.get_text("name") != Some(name)
    {
        checks.fail(
            "backup.configuration",
            "backup alias identity or record version is invalid",
        );
        blocked_requests(checks, write_test, "backup.configuration");
        return;
    }
    if !matches!(fields.get_text("provider"), Some("r2" | "minio")) {
        checks.fail(
            "backup.configuration",
            "backup alias does not name a supported S3-compatible provider",
        );
        blocked_requests(checks, write_test, "backup.configuration");
        return;
    }
    if !cfg!(feature = "tls")
        && (fields.get_text("provider") == Some("r2")
            || fields
                .get_text("endpoint")
                .is_some_and(|endpoint| endpoint.starts_with("https://")))
    {
        checks.unknown("backup.configuration", "saved destination requires TLS, which is unavailable in this build; no request was sent");
        return;
    }
    let client = match record.diagnostic_client() {
        Ok(client) => client.with_diagnostic_resolver(layerfs_bridge::network::resolve_within),
        Err(_) => {
            checks.fail("backup.configuration", "saved endpoint, bucket, prefix, region or credential reference could not be validated; secrets were not displayed");
            blocked_requests(checks, write_test, "backup.configuration");
            return;
        }
    };
    checks.pass(
        "backup.configuration",
        "saved S3 endpoint, region, bucket and prefix configuration is valid",
    );
    checks.pass(
        "backup.credentials",
        "saved credential reference resolved; values were not displayed",
    );
    let until = Instant::now()
        + budget.limit(if write_test {
            Duration::from_secs(30)
        } else {
            Duration::from_secs(10)
        });
    let started = Instant::now();
    match client.diagnostic_list(until) {
        Ok(()) => {
            checks.pass(
                "backup.transport",
                "configured endpoint and HTTP/TLS connection completed a signed request",
            );
            checks.pass("backup.list", "one authenticated ListObjectsV2 page succeeded under the saved prefix with max-keys=1");
        }
        Err(error) => {
            if matches!(error, RemoteError::Refused { .. }) {
                checks.pass("backup.transport", "configured endpoint returned an HTTP response; authentication is not independently established");
            } else {
                checks.unknown("backup.transport", "DNS, connection, configured TLS or reply could not be established within the diagnostic budget");
            }
            failure(checks, "backup.list", &error);
        }
    }
    checks.duration("backup.list", started.elapsed());
    if !write_test {
        for id in [
            "backup.write_capability",
            "backup.put",
            "backup.get",
            "backup.delete",
        ] {
            checks.skip(id, "temporary write test was not requested; list does not establish object permissions");
        }
        return;
    }
    match client.diagnostic_versioning(until) {
        Ok(()) => checks.pass("backup.write_capability", "bucket reports enabled immutable object versioning; conditional create and version-specific cleanup can be attempted"),
        Err(error) => {
            checks.unknown("backup.write_capability", "safe temporary write-test capability could not be established; no object was written");
            error_evidence(checks, "backup.write_capability", &error);
            for id in ["backup.put", "backup.get", "backup.delete"] { checks.blocked(id, "safe immutable-version ownership prerequisite unavailable; no write or delete attempted", "backup.write_capability"); }
            return;
        }
    }
    let token = match layerfs_home::helpers::random() {
        Ok(bytes) => layerfs_bridge::sdk_values::hex(&bytes),
        Err(_) => {
            checks.fail(
                "backup.put",
                "cryptographic probe token could not be generated; no object was written",
            );
            return;
        }
    };
    let key = client
        .diagnostic_key(&token)
        .expect("generated lowercase hex token");
    let started = Instant::now();
    let object = match client.diagnostic_create(&token, until) {
        Ok(object) => {
            checks.pass(
                "backup.put",
                "single-part If-None-Match:* create returned an immutable owned version",
            );
            object
        }
        Err(error @ RemoteError::Uncertain(_)) => {
            failure(checks, "backup.put", &error);
            match client.diagnostic_reconcile(&token, until) {
                Ok(object) => {
                    checks.pass("backup.reconcile", "exact generated key contains this probe's token and bytes at an immutable version; PUT was not resent");
                    object
                }
                Err(reconcile) => {
                    failure(checks, "backup.reconcile", &reconcile);
                    checks.blocked(
                        "backup.get",
                        "no immutable owned version established",
                        "backup.put",
                    );
                    checks.blocked(
                        "backup.delete",
                        "no immutable owned version established; possible probe left untouched",
                        "backup.put",
                    );
                    retained(checks, "backup.delete", &key, None);
                    return;
                }
            }
        }
        Err(error) => {
            failure(checks, "backup.put", &error);
            checks.blocked(
                "backup.get",
                "conditional create did not establish probe ownership",
                "backup.put",
            );
            checks.skip(
                "backup.delete",
                "no owned version; collision/refused keys are never deleted",
            );
            return;
        }
    };
    checks.duration("backup.put", started.elapsed());
    probe(&client, object, &key, until, checks);
}

fn probe(
    client: &S3Client,
    object: DiagnosticObject,
    key: &str,
    until: Instant,
    checks: &mut Checks,
) {
    let started = Instant::now();
    match client.diagnostic_read(&object, until) {
        Ok(()) => checks.pass(
            "backup.get",
            "owned immutable version returned the exact token and uploaded bytes",
        ),
        Err(error) => failure(checks, "backup.get", &error),
    }
    checks.duration("backup.get", started.elapsed());
    let started = Instant::now();
    match client.diagnostic_delete(&object, until) {
        Ok(()) => checks.pass(
            "backup.delete",
            "only the owned immutable version was deleted; replacement versions were not targeted",
        ),
        Err(error) => {
            failure(checks, "backup.delete", &error);
            retained(checks, "backup.delete", key, Some(object.version()));
        }
    }
    checks.duration("backup.delete", started.elapsed());
}
fn retained(checks: &mut Checks, id: &str, key: &str, version: Option<&str>) {
    checks.evidence(
        id,
        SdkValue::map([
            ("possible_retained_key", SdkValue::text(key)),
            (
                "owned_version",
                version.map_or(SdkValue::Null, SdkValue::text),
            ),
        ]),
    );
    checks.fix(id, "Inspect only the exact generated diagnostic key and owned version, if known; do not delete by prefix or retry an uncertain mutation automatically.");
}
fn failure(checks: &mut Checks, id: &str, error: &RemoteError) {
    match error {
        RemoteError::Refused { status, .. } => checks.fail(id, format!("S3 request refused with HTTP {status}; authentication or permission failure is not inferred more precisely than the response")),
        RemoteError::Integrity(_) => checks.fail(id, "S3 response failed diagnostic prefix, version, token or byte-integrity checks"),
        RemoteError::Uncertain(_) => checks.unknown(id, "sent request outcome is uncertain; no mutation was resent"),
        RemoteError::Unsupported(_) => checks.unknown(id, "required safe diagnostic capability is unsupported; no fallback was attempted"),
        _ => checks.unknown(id, "request could not complete within its configured diagnostic path or budget"),
    }
    error_evidence(checks, id, error);
}
fn error_evidence(checks: &mut Checks, id: &str, error: &RemoteError) {
    let (kind, status) = match error {
        RemoteError::Refused { status, .. } => ("refused", SdkValue::Int(i64::from(*status))),
        RemoteError::Unavailable(_) => ("unavailable_before_send", SdkValue::Null),
        RemoteError::Uncertain(_) => ("uncertain_after_send", SdkValue::Null),
        RemoteError::Unsupported(_) => ("unsupported", SdkValue::Null),
        RemoteError::Integrity(_) => ("integrity", SdkValue::Null),
        _ => ("invalid_or_local", SdkValue::Null),
    };
    let phase = match error {
        RemoteError::Unavailable(why) if why.starts_with("DNS:") => "dns",
        RemoteError::Unavailable(why) if why.starts_with("TCP:") => "tcp",
        RemoteError::Unavailable(why) if why.starts_with("TLS") => "tls",
        RemoteError::Unavailable(_) => "connection_before_send",
        RemoteError::Uncertain(_) => "exchange_after_send",
        RemoteError::Refused { .. } => "http_response",
        _ => "local_or_validation",
    };
    checks.evidence(
        id,
        SdkValue::map([
            ("cause", SdkValue::text(kind)),
            ("http_status", status),
            ("phase", SdkValue::text(phase)),
            (
                "s3_code",
                match error {
                    RemoteError::Refused { code, .. }
                        if matches!(
                            code.as_str(),
                            "AccessDenied"
                                | "SignatureDoesNotMatch"
                                | "InvalidAccessKeyId"
                                | "AuthorizationHeaderMalformed"
                                | "PermanentRedirect"
                                | "NoSuchBucket"
                                | "PreconditionFailed"
                                | "ConditionalRequestConflict"
                                | "InvalidRequest"
                                | "NotImplemented"
                                | "RequestTimeTooSkewed"
                                | "ExpiredToken"
                                | "InvalidToken"
                                | "SlowDown"
                        ) =>
                    {
                        SdkValue::text(code)
                    }
                    _ => SdkValue::Null,
                },
            ),
        ]),
    );
}

fn blocked_requests(checks: &mut Checks, write: bool, dependency: &str) {
    for id in ["backup.transport", "backup.list"] {
        checks.blocked(id, "saved connection configuration unavailable", dependency);
    }
    for id in [
        "backup.write_capability",
        "backup.put",
        "backup.get",
        "backup.delete",
    ] {
        if write {
            checks.blocked(id, "saved connection configuration unavailable", dependency);
        } else {
            checks.skip(id, "temporary write test was not requested");
        }
    }
}
