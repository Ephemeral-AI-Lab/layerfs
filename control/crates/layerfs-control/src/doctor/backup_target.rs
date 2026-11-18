//! One bucket's checks (`backup doctor`): its record, its key read from
//! the home, its address and one signed listing page; with `--full` also a
//! version-owned write, read and delete of one small object. A check that
//! does not apply, or that a failed check of the bucket kept from running,
//! is left out of the report.
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
                "bucket.configuration",
                "saved backup alias is absent, unreadable or invalid",
            );
            blocked_requests(checks, write_test, "bucket.configuration");
            return;
        }
    };
    let fields = record.to_record();
    if fields.get_int("v") != Some(layerfs_home::home::RECORD_VERSION)
        || fields.get_text("name") != Some(name)
    {
        checks.fail(
            "bucket.configuration",
            "backup alias identity or record version is invalid",
        );
        blocked_requests(checks, write_test, "bucket.configuration");
        return;
    }
    if !matches!(fields.get_text("provider"), Some("r2" | "minio")) {
        checks.fail(
            "bucket.configuration",
            "backup alias does not name a supported S3-compatible provider",
        );
        blocked_requests(checks, write_test, "bucket.configuration");
        return;
    }
    if !cfg!(feature = "tls")
        && (fields.get_text("provider") == Some("r2")
            || fields
                .get_text("endpoint")
                .is_some_and(|endpoint| endpoint.starts_with("https://")))
    {
        checks.unknown("bucket.configuration", "saved destination requires TLS, which is unavailable in this build; no request was sent");
        return;
    }
    let credentials = match record.credentials(home) {
        Ok(credentials) => credentials,
        Err(refusal) => {
            checks.pass(
                "bucket.configuration",
                "saved S3 endpoint, region, bucket and prefix configuration is valid",
            );
            checks.fail("bucket.credentials", refusal.message);
            blocked_requests(checks, write_test, "bucket.credentials");
            return;
        }
    };
    let client = match record.diagnostic_client(credentials) {
        Ok(client) => client.with_diagnostic_resolver(layerfs_bridge::network::resolve_all_within),
        Err(_) => {
            checks.fail("bucket.configuration", "saved endpoint, bucket, prefix, region or key could not be validated; secrets were not displayed");
            blocked_requests(checks, write_test, "bucket.configuration");
            return;
        }
    };
    checks.pass(
        "bucket.configuration",
        "saved S3 endpoint, region, bucket and prefix configuration is valid",
    );
    checks.pass(
        "bucket.credentials",
        "its key was read from the home; values were not displayed",
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
                "bucket.transport",
                "configured endpoint and HTTP/TLS connection completed a signed request",
            );
            checks.pass("bucket.list", "one authenticated ListObjectsV2 page succeeded under the saved prefix with max-keys=1");
        }
        Err(error) => {
            if matches!(error, RemoteError::Refused { .. }) {
                checks.pass("bucket.transport", "configured endpoint returned an HTTP response; authentication is not independently established");
            } else {
                checks.lost("bucket.transport", "DNS, connection, configured TLS or reply could not be established within the diagnostic budget");
            }
            failure(checks, "bucket.list", &error);
        }
    }
    checks.duration("bucket.list", started.elapsed());
    // without `--full` the write test is left out of the report
    if !write_test {
        return;
    }
    match client.diagnostic_versioning(until) {
        Ok(()) => checks.pass("bucket.write_capability", "bucket reports enabled immutable object versioning; conditional create and version-specific cleanup can be attempted"),
        Err(error) => {
            checks.unknown("bucket.write_capability", "safe temporary write-test capability could not be established; no object was written");
            error_evidence(checks, "bucket.write_capability", &error);
            for id in ["bucket.put", "bucket.get", "bucket.delete"] { checks.blocked(id, "safe immutable-version ownership prerequisite unavailable; no write or delete attempted", "bucket.write_capability"); }
            return;
        }
    }
    let token = match layerfs_home::helpers::random() {
        Ok(bytes) => layerfs_bridge::sdk_values::hex(&bytes),
        Err(_) => {
            checks.fail(
                "bucket.put",
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
                "bucket.put",
                "single-part If-None-Match:* create returned an immutable owned version",
            );
            object
        }
        Err(error @ RemoteError::Uncertain(_)) => {
            failure(checks, "bucket.put", &error);
            match client.diagnostic_reconcile(&token, until) {
                Ok(object) => {
                    checks.pass("bucket.reconcile", "exact generated key contains this probe's token and bytes at an immutable version; PUT was not resent");
                    object
                }
                Err(reconcile) => {
                    failure(checks, "bucket.reconcile", &reconcile);
                    checks.blocked(
                        "bucket.get",
                        "no immutable owned version established",
                        "bucket.put",
                    );
                    checks.blocked(
                        "bucket.delete",
                        "no immutable owned version established; possible probe left untouched",
                        "bucket.put",
                    );
                    retained(checks, "bucket.delete", &key, None);
                    return;
                }
            }
        }
        Err(error) => {
            failure(checks, "bucket.put", &error);
            checks.blocked(
                "bucket.get",
                "conditional create did not establish probe ownership",
                "bucket.put",
            );
            checks.skip(
                "bucket.delete",
                "no owned version; collision/refused keys are never deleted",
            );
            return;
        }
    };
    checks.duration("bucket.put", started.elapsed());
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
            "bucket.get",
            "owned immutable version returned the exact token and uploaded bytes",
        ),
        Err(error) => failure(checks, "bucket.get", &error),
    }
    checks.duration("bucket.get", started.elapsed());
    let started = Instant::now();
    match client.diagnostic_delete(&object, until) {
        Ok(()) => checks.pass(
            "bucket.delete",
            "only the owned immutable version was deleted; replacement versions were not targeted",
        ),
        Err(error) => {
            failure(checks, "bucket.delete", &error);
            retained(checks, "bucket.delete", key, Some(object.version()));
        }
    }
    checks.duration("bucket.delete", started.elapsed());
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
            // guidance, not a command: a finding's `fix` is a runnable line
            // or absent (conformance R4 F6)
            (
                "note",
                SdkValue::text("inspect only this key and owned version; do not delete by prefix or retry an uncertain mutation"),
            ),
        ]),
    );
}
fn failure(checks: &mut Checks, id: &str, error: &RemoteError) {
    match error {
        RemoteError::Refused { status, .. } => checks.fail(id, format!("S3 request refused with HTTP {status}; authentication or permission failure is not inferred more precisely than the response")),
        RemoteError::Integrity(_) => checks.fail(id, "S3 response failed diagnostic prefix, version, token or byte-integrity checks"),
        RemoteError::Uncertain(_) => checks.lost(id, "sent request outcome is uncertain; no mutation was resent"),
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
    for id in ["bucket.transport", "bucket.list"] {
        checks.blocked(id, "saved connection configuration unavailable", dependency);
    }
    if write {
        for id in [
            "bucket.write_capability",
            "bucket.put",
            "bucket.get",
            "bucket.delete",
        ] {
            checks.blocked(id, "saved connection configuration unavailable", dependency);
        }
    }
}
