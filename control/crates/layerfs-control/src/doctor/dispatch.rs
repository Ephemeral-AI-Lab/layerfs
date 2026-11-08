//! Explicit diagnostic routing; never opens or converts the home.
use super::target::{Budget, Checks, DoctorRequest, DoctorTarget};
use layerfs_bridge::control::SdkValue;
use layerfs_home::outcome::{refused, Outcome};
use std::path::Path;
use std::time::{Duration, Instant};

/// Diagnoses exactly one target without ordinary call dispatch or repair.
pub fn doctor_target(home: &Path, request: &DoctorRequest) -> Outcome {
    if !home.is_absolute() {
        return Err(refused("InvalidInput", "doctor home must be absolute"));
    }
    if request.quick && !matches!(request.target, DoctorTarget::StoreIntegrity(_)) {
        return Err(refused(
            "InvalidInput",
            "quick applies only to Store integrity",
        ));
    }
    if request.write_test && !matches!(request.target, DoctorTarget::BackupConnection(_)) {
        return Err(refused(
            "InvalidInput",
            "write_test applies only to backup connection",
        ));
    }
    let (kind, name) = request.target.entity();
    if request.target != DoctorTarget::Host {
        layerfs_home::names::name(kind, name)
            .map_err(|_| refused("InvalidInput", "invalid doctor target name"))?;
    }
    let timeout = request.timeout.or_else(|| match request.target {
        DoctorTarget::StoreIntegrity(_) => None,
        DoctorTarget::StoreConnection(_) | DoctorTarget::BackupConnection(_) => {
            Some(Duration::from_secs(if request.write_test {
                30
            } else {
                10
            }))
        }
        _ => Some(Duration::from_secs(30)),
    });
    if timeout.is_some_and(|d| d.is_zero() || Instant::now().checked_add(d).is_none()) {
        return Err(refused(
            "InvalidInput",
            "doctor timeout must be a positive representable duration",
        ));
    }
    let budget = Budget::new(timeout);
    let mut checks = Checks::new(kind, name);
    match &request.target {
        DoctorTarget::Host => super::host_target::diagnose(home, &budget, &mut checks),
        DoctorTarget::Machine(name) => {
            super::machine_target::diagnose(home, name, &budget, &mut checks)
        }
        DoctorTarget::Sandbox(name) => {
            super::sandbox_target::diagnose(home, name, &budget, &mut checks)
        }
        DoctorTarget::StoreIntegrity(name) => {
            super::store_target::diagnose_integrity(home, name, request.quick, &budget, &mut checks)
        }
        DoctorTarget::StoreConnection(name) => {
            super::store_target::diagnose_connection(home, name, &budget, &mut checks)
        }
        DoctorTarget::BackupConnection(name) => {
            super::backup_target::diagnose(home, name, request.write_test, &budget, &mut checks)
        }
    }
    if budget.expired() {
        checks.unknown(
            "diagnostic.deadline",
            "Overall diagnostic deadline expired; coverage is incomplete",
        );
    }
    let mut report = checks.to_sdk();
    if let SdkValue::Map(fields) = &mut report {
        fields.push((
            "home".to_owned(),
            SdkValue::text(home.display().to_string()),
        ));
        fields.push((
            "operation".to_owned(),
            SdkValue::text(request.target.operation()),
        ));
        fields.push((
            "mode".to_owned(),
            SdkValue::text(
                if matches!(request.target, DoctorTarget::StoreIntegrity(_)) {
                    if request.quick {
                        "quick"
                    } else {
                        "full"
                    }
                } else if request.write_test {
                    "write-test"
                } else {
                    "observation"
                },
            ),
        ));
    }
    Ok(report)
}
