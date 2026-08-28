use crate::{Result, RuntimeLeases, RuntimeObservation};
use std::time::Duration;

pub(crate) fn establish(leases: &RuntimeLeases, timeout: Duration) -> Result<RuntimeObservation> {
    leases.close_and_wait(timeout)
}
