//! One bucket's checks (`backup doctor`): its record, its key read from
//! the home, its address and one signed listing page; with `--full` also a
//! version-owned write, read and delete of one small object. A check that
//! does not apply, or that a failed check of the bucket kept from running,
//! is left out of the report.
use super::target::{Budget, Checks};
use layerfs_protocol::provision::install::SdkValue;
use layerfs_store::backup::{s3_api::{DiagnosticObject, S3Client}, bucket::RemoteError};
use layerfs_home::outcome::shell_word;
use layerfs_store::backup::record::BucketRecord;
use std::path::Path;
use std::time::{Duration, Instant};

/// Evidence naming only the cause word of a finding.
fn cause(word: &str) -> SdkValue {
    SdkValue::map([("cause", SdkValue::text(word))])
}

pub(super) fn diagnose(
    home: &Path,
    name: &str,
    write_test: bool,
    budget: &Budget,
    checks: &mut Checks,
) {
    let record = match BucketRecord::of(home, name) {
        Ok(record) => record,
        Err(refusal) if refusal.code == "NotFound" => {
            checks.fail(
                "bucket.configuration",
                "no backup alias is saved under this name",
            );
            // no record exists to read or preserve: the line is a template
            checks.fix(
                "bucket.configuration",
                format!(
                    "layerfs backup connect {} <url> --access-key <key> --key-stdin",
                    shell_word(name)
                ),
            );
            checks.evidence("bucket.configuration", cause("record_missing"));
            return;
        }
        Err(_) => {
            checks.fail(
                "bucket.configuration",
                "saved backup alias is unreadable or invalid; repair or remove its record by hand before connecting it again",
            );
            // `backup connect` reads an existing record before it can replace
            // it, so it cannot repair the record it cannot read
            checks.evidence("bucket.configuration", cause("record_unreadable"));
            return;
        }
    };
    let fields = record.to_record();
    if fields.get_int("v") != Some(layerfs_home::home::RECORD_VERSION) {
        checks.fail(
            "bucket.configuration",
            "backup alias record version is invalid",
        );
        // rotation rewrites the record through `write_entity`, which writes
        // this build's record version and keeps the Stores' links
        checks.fix("bucket.configuration", record.connect_again());
        checks.evidence(
            "bucket.configuration",
            SdkValue::map([
                ("cause", SdkValue::text("version")),
                (
                    "expected_v",
                    SdkValue::Int(layerfs_home::home::RECORD_VERSION),
                ),
            ]),
        );
        return;
    }
    if fields.get_text("name") != Some(name) {
        checks.fail(
            "bucket.configuration",
            "backup alias record names another bucket; connecting it again keeps that wrong name, so repair the record by hand",
        );
        checks.evidence(
            "bucket.configuration",
            SdkValue::map([("cause", SdkValue::text("identity"))]),
        );
        return;
    }
    if !matches!(fields.get_text("provider"), Some("r2" | "minio")) {
        checks.fail(
            "bucket.configuration",
            "backup alias does not name a supported S3-compatible provider",
        );
        // no fix: connecting the supported form at this name names another
        // place, which `backup connect` refuses; disconnecting would drop
        // the Stores' links
        checks.evidence(
            "bucket.configuration",
            SdkValue::map([
                ("cause", SdkValue::text("unsupported_provider")),
                ("supported", SdkValue::text("r2, minio")),
            ]),
        );
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
            // the connect-again fix the same record's backup calls carry
            // (V20-7, V21-4; RC-A F4)
            let fix = refusal.detail.get_text(layerfs_home::outcome::FIX);
            let fix = fix.map(str::to_owned);
            checks.fail("bucket.credentials", refusal.message);
            if let Some(fix) = fix {
                checks.fix("bucket.credentials", fix);
            }
            checks.evidence("bucket.credentials", cause(&refusal.code));
            return;
        }
    };
    let client = match record.diagnostic_client(credentials) {
        Ok(client) => client.with_diagnostic_resolver(layerfs_os::network::resolve_all_within),
        Err(_) => {
            checks.fail("bucket.configuration", "saved endpoint, bucket, prefix, region or key could not be validated; secrets were not displayed; repair the record by hand or connect the bucket under a new name");
            // no fix: the invalid field is not identified, and reconnecting
            // the same saved values repairs only a bad key
            checks.evidence("bucket.configuration", cause("client_not_validated"));
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
            // a refused key is fixed by connecting again (UI2-3); any other
            // refusal names no cause a command repairs
            if matches!(
                error,
                RemoteError::Refused {
                    status: 401 | 403,
                    ..
                }
            ) {
                checks.fix("bucket.list", record.connect_again());
            }
        }
    }
    // without `--full` the write test is left out of the report
    if !write_test {
        return;
    }
    match client.diagnostic_versioning(until) {
        Ok(()) => checks.pass("bucket.write_capability", "bucket reports enabled immutable object versioning; conditional create and version-specific cleanup can be attempted"),
        Err(error) => {
            checks.unknown("bucket.write_capability", "safe temporary write-test capability could not be established; no object was written");
            error_evidence(checks, "bucket.write_capability", &error);
            return;
        }
    }
    let token = match layerfs_home::helpers::random() {
        Ok(bytes) => layerfs_protocol::sdk::hex(&bytes),
        Err(_) => {
            checks.fail(
                "bucket.put",
                "cryptographic probe token could not be generated; no object was written",
            );
            checks.evidence("bucket.put", cause("entropy_unavailable"));
            return;
        }
    };
    let key = client
        .diagnostic_key(&token)
        .expect("generated lowercase hex token");
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
                    return;
                }
            }
        }
        Err(error) => {
            failure(checks, "bucket.put", &error);
            return;
        }
    };
    probe(&client, object, &key, until, checks);
}

fn probe(
    client: &S3Client,
    object: DiagnosticObject,
    key: &str,
    until: Instant,
    checks: &mut Checks,
) {
    match client.diagnostic_read(&object, until) {
        Ok(()) => checks.pass(
            "bucket.get",
            "owned immutable version returned the exact token and uploaded bytes",
        ),
        Err(error) => failure(checks, "bucket.get", &error),
    }
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
