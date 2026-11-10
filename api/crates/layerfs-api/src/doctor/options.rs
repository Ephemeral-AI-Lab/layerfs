//! Strict diagnostic options, shared by SDK bindings and CLI.
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_control::doctor::target::{DoctorRequest, DoctorTarget};
use layerfs_home::outcome::refused;
use std::time::{Duration, Instant};

/// Parses a positive whole duration with ms, s, m or h units.
pub fn duration(text: &str) -> Option<Duration> {
    let at = text.find(|c: char| !c.is_ascii_digit())?;
    let n: u64 = text[..at].parse().ok()?;
    let scale = match &text[at..] {
        "ms" => 1,
        "s" => 1000,
        "m" => 60_000,
        "h" => 3_600_000,
        _ => return None,
    };
    let d = Duration::from_millis(n.checked_mul(scale)?);
    (!d.is_zero() && Instant::now().checked_add(d).is_some()).then_some(d)
}

/// Validates options before any home or external probe is read.
/// Empty options select the existing legacy doctor entry.
pub fn doctor_request(options: &SdkValue) -> Result<Option<DoctorRequest>, SdkRefusal> {
    let fields = match options {
        SdkValue::Null => return Ok(None),
        SdkValue::Map(fields) => fields,
        _ => return Err(invalid("options must be named fields")),
    };
    if fields.is_empty() {
        return Ok(None);
    }
    for (key, _) in fields {
        if !matches!(
            key.as_str(),
            "target" | "name" | "timeout" | "quick" | "write_test"
        ) {
            return Err(invalid(&format!("unknown option `{key}`")));
        }
        if fields.iter().filter(|(k, _)| k == key).count() != 1 {
            return Err(invalid("duplicate option"));
        }
    }
    let target = options
        .get_text("target")
        .ok_or_else(|| invalid("target is required"))?;
    let name = || {
        options
            .get_text("name")
            .ok_or_else(|| invalid("name is required for this target"))
    };
    let selected = match target {
        "host" => {
            if options.get("name").is_some() {
                return Err(invalid("host does not take a name"));
            }
            DoctorTarget::Host
        }
        "machine" => DoctorTarget::Machine(name()?.to_owned()),
        "sandbox" => DoctorTarget::Sandbox(name()?.to_owned()),
        "store.integrity" => DoctorTarget::StoreIntegrity(name()?.to_owned()),
        "store.connection" => DoctorTarget::StoreConnection(name()?.to_owned()),
        "backup.connection" => DoctorTarget::BackupConnection(name()?.to_owned()),
        _ => return Err(invalid("unknown diagnostic target")),
    };
    let (kind, entity) = selected.entity();
    if selected != DoctorTarget::Host {
        layerfs_home::names::name(kind, entity).map_err(|_| invalid("invalid target name"))?;
    }
    let boolean = |key: &str| match options.get(key) {
        None => Ok(false),
        Some(SdkValue::Bool(v)) => Ok(*v),
        _ => Err(invalid(&format!("{key} must be a boolean"))),
    };
    let quick = boolean("quick")?;
    let write_test = boolean("write_test")?;
    if options.get("quick").is_some() && !matches!(selected, DoctorTarget::StoreIntegrity(_)) {
        return Err(invalid("quick applies only to store.integrity"));
    }
    if options.get("write_test").is_some() && !matches!(selected, DoctorTarget::BackupConnection(_))
    {
        return Err(invalid("write_test applies only to backup.connection"));
    }
    let timeout = match options.get("timeout") {
        Some(SdkValue::Text(t)) => Some(duration(t).ok_or_else(|| {
            invalid("timeout must be a positive duration such as 500ms, 10s or 2m")
        })?),
        None if matches!(selected, DoctorTarget::StoreIntegrity(_)) => None,
        None if matches!(
            selected,
            DoctorTarget::StoreConnection(_) | DoctorTarget::BackupConnection(_)
        ) =>
        {
            Some(Duration::from_secs(if write_test { 30 } else { 10 }))
        }
        None => Some(Duration::from_secs(30)),
        _ => return Err(invalid("timeout must be a duration string")),
    };
    Ok(Some(DoctorRequest {
        target: selected,
        timeout,
        quick,
        write_test,
    }))
}
fn invalid(message: &str) -> SdkRefusal {
    refused("InvalidInput", format!("doctor: {message}"))
}
